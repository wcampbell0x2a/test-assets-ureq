/*!
Download test assets, managing them outside of git

This library downloads test assets using http(s),
and ensures integrity by comparing those assets to a hash.
By managing the download separately, you can keep them
out of VCS and don't make them bloat your repository.

Usage example:

```rust, no_run
#[test]
fn some_awesome_test() {
    let asset_defs = [
        TestAssetDef {
            filename : format!("file_a.png"),
            hash : format!("<sha256 here>"),
            url : format!("https://url/to/a.png"),
        },
        TestAssetDef {
            filename : format!("file_b.png"),
            hash : format!("<sha256 here>"),
            url : format!("https://url/to/a.png"),
        },
    ];
    test_assets::dl_test_files(&asset_defs,
        "test-assets", true).unwrap();
    // use your files here
    // with path under test-assets/file_a.png and test-assets/file_b.png
}
```

If you have run the test once, it will re-use the files
instead of re-downloading them.
*/

mod hash_list;

use backon::BlockingRetryable;
use backon::ExponentialBuilder;
use hash_list::HashList;
use serde::Deserialize;
use sha2::digest::Digest;
use sha2::Sha256;
use std::fs::{create_dir_all, File};
use std::io::{self, Read, Write};
use std::time::Duration;
use ureq::Agent;

#[derive(Debug, Deserialize)]
pub struct TestAsset {
    #[serde(rename = "test_assets")]
    pub assets: std::collections::BTreeMap<String, TestAssetDef>,
}

impl TestAsset {
    pub fn values(&self) -> Vec<TestAssetDef> {
        self.assets.values().cloned().collect()
    }
}

/// Definition for a test file
///
///
#[derive(Debug, Deserialize, Clone)]
pub struct TestAssetDef {
    /// Name of the file on disk. This should be unique for the file.
    pub filename: String,
    /// Sha256 hash of the file's data in hexadecimal lowercase representation
    pub hash: String,
    /// The url the test file can be obtained from
    pub url: String,
}

/// A type for a Sha256 hash value
///
/// Provides conversion functionality to hex representation and back
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct Sha256Hash([u8; 32]);

impl Sha256Hash {
    #[must_use]
    pub fn from_digest(sha: Sha256) -> Self {
        let sha = sha.finalize();
        let bytes = sha[..].try_into().unwrap();
        Self(bytes)
    }

    /// Converts the hexadecimal string to a hash value
    fn from_hex(s: &str) -> Result<Self, ()> {
        let mut res = Self([0; 32]);
        let mut idx = 0;
        let mut iter = s.chars();
        loop {
            let upper = match iter.next().and_then(|c| c.to_digit(16)) {
                Some(v) => v as u8,
                None => return Err(()),
            };
            let lower = match iter.next().and_then(|c| c.to_digit(16)) {
                Some(v) => v as u8,
                None => return Err(()),
            };
            res.0[idx] = (upper << 4) | lower;
            idx += 1;
            if idx == 32 {
                break;
            }
        }
        Ok(res)
    }
    /// Converts the hash value to hexadecimal
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut res = String::with_capacity(64);
        for v in &self.0 {
            use std::char::from_digit;
            res.push(from_digit(u32::from(*v) >> 4, 16).unwrap());
            res.push(from_digit(u32::from(*v) & 15, 16).unwrap());
        }
        res
    }
}

#[derive(Debug)]
pub enum TaError {
    Io(io::Error),
    DownloadFailed,
    HashMismatch(String, String),
    BadHashFormat,
}

impl From<io::Error> for TaError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

enum DownloadOutcome {
    WithHash(Sha256Hash),
}

fn download_test_file(
    agent: &mut Agent,
    tfile: &TestAssetDef,
    dir: &str,
) -> Result<DownloadOutcome, TaError> {
    let resp = match agent.get(&tfile.url).call() {
        Ok(resp) => resp,
        Err(e) => {
            println!("{e:?}");
            return Err(TaError::DownloadFailed);
        }
    };

    let len: usize = resp.header("Content-Length").unwrap().parse().unwrap();

    let mut bytes: Vec<u8> = Vec::with_capacity(len);
    let read_len = resp.into_reader().take(10_000_000_000).read_to_end(&mut bytes)?;

    if (bytes.len() != read_len) && (bytes.len() != len) {
        return Err(TaError::DownloadFailed);
    }

    let file = File::create(format!("{}/{}", dir, tfile.filename))?;
    let mut writer = io::BufWriter::new(file);
    writer.write_all(&bytes).unwrap();

    let mut hasher = Sha256::new();
    hasher.update(&bytes);

    Ok(DownloadOutcome::WithHash(Sha256Hash::from_digest(hasher)))
}

/// Downloads the test files into the passed directory.
pub fn dl_test_files(defs: &[TestAssetDef], dir: &str, verbose: bool) -> Result<(), TaError> {
    let mut agent = ureq::agent();

    use std::io::ErrorKind;

    let hash_list_path = format!("{dir}/hash_list");
    let mut hash_list = match HashList::from_file(&hash_list_path) {
        Ok(l) => l,
        Err(TaError::Io(ref e)) if e.kind() == ErrorKind::NotFound => HashList::new(),
        e => {
            e?;
            unreachable!()
        }
    };
    create_dir_all(dir)?;
    for tfile in defs.iter() {
        let tfile_hash = Sha256Hash::from_hex(&tfile.hash).map_err(|_| TaError::BadHashFormat)?;
        if hash_list.get_hash(&tfile.filename) == Some(&tfile_hash) {
            // Hash match
            if verbose {
                println!(
                    "File {} has matching hash inside hash list, skipping download",
                    tfile.filename
                );
            }
            continue;
        }
        if verbose {
            println!("Fetching file {} ...", tfile.filename);
        }
        let outcome = download_test_file(&mut agent, tfile, dir)?;
        match outcome {
            DownloadOutcome::WithHash(ref hash) => hash_list.add_entry(&tfile.filename, hash),
        }
        if verbose {
            print!("  => ");
        }
        match outcome {
            DownloadOutcome::WithHash(ref found_hash) => {
                if found_hash == &tfile_hash {
                    if verbose {
                        println!("Success")
                    }
                } else {
                    // if the hash mismatches after download, return error
                    return Err(TaError::HashMismatch(found_hash.to_hex(), tfile.hash.clone()));
                }
            }
        }
    }
    hash_list.to_file(&hash_list_path)?;
    Ok(())
}

/// Download test-assets with backoff retries
pub fn dl_test_files_backoff(
    assets_defs: &[TestAssetDef],
    test_path: &str,
    verbose: bool,
    max_delay: Duration,
) -> Result<(), TaError> {
    let strategy = ExponentialBuilder::default().with_max_delay(max_delay);

    (|| dl_test_files(assets_defs, test_path, verbose)).retry(strategy).call().unwrap();

    Ok(())
}
