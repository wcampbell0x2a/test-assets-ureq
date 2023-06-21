// Copyright (c) 2016 est31 <MTest31@outlook.com>
// and contributors. All rights reserved.
// Licensed under MIT license, or Apache 2 license,
// at your option. Please see the LICENSE file
// attached to this source distribution for details.

#![forbid(unsafe_code)]

/*!
Download test assets, managing them outside of git.

This library downloads test assets using http(s),
and ensures integrity by comparing those assets to a hash.

By managing the download separately, you can keep them
out of VCS and don't make them bloat your repository.

Usage example:

```
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
    test_assets::download_test_files(&asset_defs,
        "test-assets", true).unwrap();
    // use your files here
    // with path under test-assets/file_a.png and test-assets/file_b.png
}
```

If you have run the test once, it will re-use the files
instead of re-downloading them.
*/

mod hash_list;

use hash_list::HashList;
use sha2::digest::Digest;
use sha2::Sha256;
use std::fs::{create_dir_all, File};
use std::io::{self, Write};
use ureq::Agent;

/// Definition for a test file
///
///
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
    #[must_use] pub fn from_digest(sha: Sha256) -> Self {
        let sha = sha.finalize();
        let bytes = sha[..].try_into().unwrap();
        Self(bytes)
    }

    /// Converts the hexadecimal string to a hash value
    pub fn from_hex(s: &str) -> Result<Self, ()> {
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
    #[must_use] pub fn to_hex(&self) -> String {
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
    dbg!(&tfile.url);
    let resp = match agent
        .get(&tfile.url)
        .timeout(std::time::Duration::from_secs(300))
        .call()
    {
        Ok(resp) => resp,
        Err(e) => {
            println!("{e:?}");
            return Err(TaError::DownloadFailed);
        }
    };

    let len = resp
        .header("Content-Length")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap();
    let mut reader = resp.into_reader();
    let mut buffer = vec![];
    let read_len = reader.read_to_end(&mut buffer).unwrap();
    if (buffer.len() != read_len) && (buffer.len() != len) {
        return Err(TaError::DownloadFailed);
    }

    let file = File::create(format!("{}/{}", dir, tfile.filename))?;
    let mut writer = io::BufWriter::new(file);
    writer.write_all(&buffer).unwrap();

    let mut hasher = Sha256::new();
    hasher.update(&buffer);

    Ok(DownloadOutcome::WithHash(Sha256Hash::from_digest(hasher)))
}

/// Downloads the test files into the passed directory.
pub fn download_test_files(defs: &[TestAssetDef], dir: &str, verbose: bool) -> Result<(), TaError> {
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
        if hash_list
            .get_hash(&tfile.filename)
            .map_or(false, |h| h == &tfile_hash)
        {
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
            print!("Fetching file {} ...", tfile.filename);
        }
        let outcome = download_test_file(&mut agent, tfile, dir)?;
        match outcome {
            DownloadOutcome::WithHash(ref hash) => hash_list.add_entry(&tfile.filename, hash),
        }
        if verbose {
            print!("  => ");
            match outcome {
                DownloadOutcome::WithHash(ref found_hash) => {
                    if found_hash == &tfile_hash {
                        println!("Success")
                    } else {
                        println!(
                            "Hash mismatch: found {}, expected {}",
                            found_hash.to_hex(),
                            tfile.hash
                        )
                    }
                }
            }
        }
    }
    hash_list.to_file(&hash_list_path)?;
    Ok(())
}
