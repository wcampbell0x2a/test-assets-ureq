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
            filepath : format!("file_a.png"),
            hash : format!("<sha256 here>"),
            url : format!("https://url/to/a.png"),
        },
        TestAssetDef {
            filepath : format!("subdir/file_b.png"),
            hash : format!("<sha256 here>"),
            url : format!("https://url/to/b.png"),
        },
    ];
    test_assets::dl_test_files(&asset_defs, "test-assets").unwrap();
    // use your files here
    // with path under test-assets/file_a.png and test-assets/subdir/file_b.png
}
```

Optionally, a `toml` can also be used.

```toml, no_run
[test_assets.test_00]
filepath = "out.squashfs"
hash = "976c1638d8c1ba8014de6c64b196cbd70a5acf031be10a8e7f649536193c8e78"
url = "https://wcampbell.dev/squashfs/testing/test_00/out.squashfs"
```
```rust,no_run
use test_assets_ureq::{TestAsset, dl_test_files_backoff};
use std::fs;
use std::time::Duration;

let file_content = fs::read_to_string("test.toml").unwrap();
let parsed: TestAsset = toml::de::from_str(&file_content).unwrap();
let assets = parsed.values();
dl_test_files_backoff(&assets, "test-assets", Duration::from_secs(1)).unwrap();
```

If you have run the test once, it will re-use the files
instead of re-downloading them.
*/

mod hash_list;

use backon::BlockingRetryable;
use backon::ExponentialBuilder;
use hash_list::HashList;
use rayon::prelude::*;
use serde::Deserialize;
use sha2::digest::Digest;
use sha2::Sha256;
use std::collections::HashSet;
use std::fs::{create_dir_all, File};
use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use ureq::Agent;

#[derive(Debug, Deserialize)]
pub struct TestAsset {
    #[serde(rename = "test_assets")]
    pub assets: std::collections::BTreeMap<String, TestAssetDef>,
}

impl TestAsset {
    #[must_use]
    pub fn values(&self) -> Vec<TestAssetDef> {
        self.assets.values().cloned().collect()
    }
}

/// Definition for a test file
#[derive(Debug, Deserialize, Clone)]
pub struct TestAssetDef {
    /// Path of the file on disk relative to the output directory. Can include subdirectories.
    pub filepath: String,
    /// Sha256 hash of the file's data in hexadecimal lowercase representation
    pub hash: String,
    /// The url the test file can be obtained from
    pub url: String,
}

impl TestAssetDef {
    /// Get the filename (last component of the filepath)
    #[must_use]
    pub fn filename(&self) -> &str {
        std::path::Path::new(&self.filepath)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&self.filepath)
    }
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

/// Callbacks for download progress and status updates
pub struct ProgressCallbacks<'a> {
    pub sha_matched_fn: &'a (dyn Fn(&str) + Send + Sync),
    pub sha_not_matched_fn: &'a (dyn Fn(&str) + Send + Sync),
    pub downloaded_fn: &'a (dyn Fn(&str) + Send + Sync),
    pub downloading_failed_fn: &'a (dyn Fn(&str) + Send + Sync),
    pub finished_fn: &'a (dyn Fn(&str) + Send + Sync),
    pub progress_update_fn: &'a (dyn Fn(&str) + Send + Sync),
    pub download_progress_fn: &'a (dyn Fn(usize, usize) + Send + Sync),
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

struct DownloadContext<'a> {
    bytes_downloaded: &'a Arc<Mutex<u64>>,
    total_size: u64,
    downloading: &'a Arc<Mutex<HashSet<String>>>,
    println_fn: &'a (dyn Fn(&str) + Send + Sync),
    update_progress_fn: &'a (dyn Fn(&str) + Send + Sync),
}

fn download_test_file(
    agent: &mut Agent,
    tfile: &TestAssetDef,
    dir: &str,
    context: &DownloadContext,
) -> Result<DownloadOutcome, TaError> {
    let resp = match agent.get(&tfile.url).call() {
        Ok(resp) => resp,
        Err(e) => {
            (context.println_fn)(&format!("{e:?}"));
            return Err(TaError::DownloadFailed);
        }
    };

    let len: usize = resp.header("Content-Length").and_then(|s| s.parse().ok()).unwrap_or(0);

    let mut bytes: Vec<u8> = Vec::with_capacity(len);
    let mut reader = resp.into_reader().take(10_000_000_000);

    let mut buffer = vec![0; 8192];
    let mut bytes_since_update = 0u64;
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..n]);

        let mut downloaded = context.bytes_downloaded.lock().unwrap();
        *downloaded += n as u64;
        bytes_since_update += n as u64;

        if bytes_since_update >= 262_144 {
            bytes_since_update = 0;
            let dl = context.downloading.lock().unwrap();
            (context.update_progress_fn)(&format!(
                "{} / {} - {}",
                format_bytes(*downloaded),
                format_bytes(context.total_size),
                dl.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }

    let read_len = bytes.len();

    if (bytes.len() != read_len) && (bytes.len() != len) {
        return Err(TaError::DownloadFailed);
    }

    let filepath = format!("{}/{}", dir, tfile.filepath);
    if let Some(parent) = std::path::Path::new(&filepath).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(&filepath)?;
    let mut writer = io::BufWriter::new(file);
    writer.write_all(&bytes)?;
    writer.flush()?;

    let mut hasher = Sha256::new();
    hasher.update(&bytes);

    Ok(DownloadOutcome::WithHash(Sha256Hash::from_digest(hasher)))
}

/// Downloads the test files into the passed directory with progress callbacks.
pub fn dl_test_files_with_progress(
    defs: &[TestAssetDef],
    dir: &str,
    callbacks: &ProgressCallbacks,
) -> Result<(), TaError> {
    use std::io::ErrorKind;

    let hash_list_path = format!("{dir}/hash_list");
    let hash_list = match HashList::from_file(&hash_list_path) {
        Ok(l) => l,
        Err(TaError::Io(ref e)) if e.kind() == ErrorKind::NotFound => HashList::new(),
        e => {
            e?;
            unreachable!()
        }
    };
    create_dir_all(dir)?;

    let sha_matched_count = Arc::new(Mutex::new(0u64));

    let files_to_download: Vec<_> = defs
        .iter()
        .filter(|tfile| {
            let tfile_hash = match Sha256Hash::from_hex(&tfile.hash) {
                Ok(h) => h,
                Err(_) => {
                    return true;
                }
            };

            let filepath = format!("{}/{}", dir, tfile.filepath);

            if hash_list.get_hash(&tfile.filepath) == Some(&tfile_hash) {
                match File::open(&filepath) {
                    Ok(mut file) => {
                        let mut hasher = Sha256::new();
                        let mut buffer = vec![0; 8192];
                        loop {
                            match file.read(&mut buffer) {
                                Ok(0) => break,
                                Ok(n) => hasher.update(&buffer[..n]),
                                Err(_e) => {
                                    return true; // Error reading, download it
                                }
                            }
                        }
                        let file_hash = Sha256Hash::from_digest(hasher);
                        if file_hash == tfile_hash {
                            *sha_matched_count.lock().unwrap() += 1;
                            (callbacks.sha_matched_fn)(&tfile.filepath);
                            return false;
                        }
                        (callbacks.sha_not_matched_fn)(&tfile.filepath);
                    }
                    Err(_e) => {}
                }
            }
            true
        })
        .collect();

    if files_to_download.is_empty() {
        (callbacks.finished_fn)("All files SHA matched");
        return Ok(());
    }

    let total_size: u64 = files_to_download
        .iter()
        .filter_map(|tfile| {
            let agent = ureq::agent();
            agent
                .head(&tfile.url)
                .call()
                .ok()
                .and_then(|resp| resp.header("Content-Length").map(|s| s.to_string()))
                .and_then(|len| len.parse::<u64>().ok())
        })
        .sum();

    let hash_list = Arc::new(Mutex::new(hash_list));
    let downloading = Arc::new(Mutex::new(HashSet::new()));
    let bytes_downloaded = Arc::new(Mutex::new(0u64));
    let downloads_completed = Arc::new(Mutex::new(0usize));
    let total_to_download = files_to_download.len();

    let results: Vec<_> = files_to_download
        .par_iter()
        .map(|tfile| {
            let mut agent = ureq::agent();
            let tfile_hash =
                Sha256Hash::from_hex(&tfile.hash).map_err(|_| TaError::BadHashFormat)?;

            let mut dl = downloading.lock().unwrap();
            dl.insert(tfile.filepath.clone());
            drop(dl);

            let println_fn = |msg: &str| {
                (callbacks.downloading_failed_fn)(msg);
            };

            let update_progress_fn_local = |msg: &str| {
                (callbacks.progress_update_fn)(msg);
            };

            let context = DownloadContext {
                bytes_downloaded: &bytes_downloaded,
                total_size,
                downloading: &downloading,
                println_fn: &println_fn,
                update_progress_fn: &update_progress_fn_local,
            };

            let outcome = download_test_file(&mut agent, tfile, dir, &context);

            let mut dl = downloading.lock().unwrap();
            dl.remove(&tfile.filepath);
            drop(dl);

            let outcome = match outcome {
                Ok(o) => {
                    (callbacks.downloaded_fn)(&tfile.filepath);
                    let mut completed = downloads_completed.lock().unwrap();
                    *completed += 1;
                    (callbacks.download_progress_fn)(*completed, total_to_download);
                    Ok(o)
                }
                Err(e) => {
                    (callbacks.downloading_failed_fn)(&tfile.filepath);
                    let mut completed = downloads_completed.lock().unwrap();
                    *completed += 1;
                    (callbacks.download_progress_fn)(*completed, total_to_download);
                    Err(e)
                }
            };

            let outcome = outcome?;

            match outcome {
                DownloadOutcome::WithHash(ref hash) => {
                    let mut hash_list = hash_list.lock().unwrap();
                    hash_list.add_entry(&tfile.filepath, hash);
                }
            }

            match outcome {
                DownloadOutcome::WithHash(ref found_hash) => {
                    if found_hash == &tfile_hash {
                        Ok(())
                    } else {
                        Err(TaError::HashMismatch(found_hash.to_hex(), tfile.hash.clone()))
                    }
                }
            }
        })
        .collect();

    for result in results {
        result?;
    }

    let hash_list = match Arc::try_unwrap(hash_list) {
        Ok(mutex) => match mutex.into_inner() {
            Ok(list) => list,
            Err(_) => panic!("Failed to unlock Mutex"),
        },
        Err(_) => panic!("Failed to unwrap Arc"),
    };
    hash_list.to_file(&hash_list_path)?;
    Ok(())
}

/// Download test-assets with backoff retries and progress callbacks
pub fn dl_test_files_backoff_with_progress(
    assets_defs: &[TestAssetDef],
    test_path: &str,
    max_delay: Duration,
    callbacks: &ProgressCallbacks,
) -> Result<(), TaError> {
    let strategy = ExponentialBuilder::default().with_max_delay(max_delay);

    (|| dl_test_files_with_progress(assets_defs, test_path, callbacks))
        .retry(strategy)
        .call()
        .unwrap();

    Ok(())
}

/// Download test files
pub fn dl_test_files(defs: &[TestAssetDef], dir: &str) -> Result<(), TaError> {
    let callbacks = ProgressCallbacks {
        sha_matched_fn: &|_| {},
        sha_not_matched_fn: &|_| {},
        downloaded_fn: &|_| {},
        downloading_failed_fn: &|_| {},
        finished_fn: &|_| {},
        progress_update_fn: &|_| {},
        download_progress_fn: &|_, _| {},
    };
    dl_test_files_with_progress(defs, dir, &callbacks)
}

/// Download test files with backoff retries
pub fn dl_test_files_backoff(
    defs: &[TestAssetDef],
    dir: &str,
    max_delay: Duration,
) -> Result<(), TaError> {
    let callbacks = ProgressCallbacks {
        sha_matched_fn: &|_| {},
        sha_not_matched_fn: &|_| {},
        downloaded_fn: &|_| {},
        downloading_failed_fn: &|_| {},
        finished_fn: &|_| {},
        progress_update_fn: &|_| {},
        download_progress_fn: &|_, _| {},
    };
    dl_test_files_backoff_with_progress(defs, dir, max_delay, &callbacks)
}
