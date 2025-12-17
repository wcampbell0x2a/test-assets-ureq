use rstest::rstest;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use test_assets_ureq::{dl_test_files, dl_test_files_backoff, TestAsset, TestAssetDef};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let path = PathBuf::from(format!("test-output/{}", name));
        let _ = fs::remove_dir_all(&path);
        Self { path }
    }

    fn path_str(&self) -> &str {
        self.path.to_str().unwrap()
    }

    fn file_path(&self, filename: &str) -> PathBuf {
        self.path.join(filename)
    }

    fn toml_path(&self) -> PathBuf {
        self.path.join("test_assets.toml")
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn valid_asset_def(filename: &str) -> TestAssetDef {
    TestAssetDef {
        filename: filename.to_string(),
        hash: "e70ee73a8fa703ef0e47c8224b4275db2f951249b883a9f9e30d4ac8e9a676eb".to_string(),
        url: "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"
            .to_string(),
    }
}

fn invalid_hash_asset_def(filename: &str) -> TestAssetDef {
    TestAssetDef {
        filename: filename.to_string(),
        hash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        url: "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"
            .to_string(),
    }
}

#[rstest]
#[case(vec![valid_asset_def("packages.txt")], vec!["packages.txt"], true)]
#[case(vec![valid_asset_def("packages1.txt"), valid_asset_def("packages2.txt")], vec!["packages1.txt", "packages2.txt"], true)]
#[case(vec![invalid_hash_asset_def("wrong_hash.txt")], vec![], false)]
fn test_library_download(
    #[case] asset_defs: Vec<TestAssetDef>,
    #[case] expected_files: Vec<&str>,
    #[case] should_succeed: bool,
) {
    let test_name = format!("library-test-{}", expected_files.join("-").replace(".txt", ""));
    let test_dir = TestDir::new(&test_name);

    let result = dl_test_files(&asset_defs, test_dir.path_str(), !should_succeed);

    if should_succeed {
        assert!(result.is_ok(), "Failed to download test files: {:?}", result.err());

        for filename in expected_files {
            assert!(
                test_dir.file_path(filename).exists(),
                "Downloaded file {} does not exist",
                filename
            );
        }

        // Test that second download also works (cached)
        let result2 = dl_test_files(&asset_defs, test_dir.path_str(), true);
        assert!(result2.is_ok(), "Failed on second download attempt");
    } else {
        assert!(result.is_err(), "Expected hash mismatch error");
    }
}

#[test]
fn test_library_download_with_backoff() {
    let test_dir = TestDir::new("library-backoff-test");

    let asset_defs = vec![valid_asset_def("packages.txt")];

    let result =
        dl_test_files_backoff(&asset_defs, test_dir.path_str(), true, Duration::from_secs(60));
    assert!(result.is_ok(), "Failed to download with backoff: {:?}", result.err());

    assert!(test_dir.file_path("packages.txt").exists(), "Downloaded file does not exist");
}

#[rstest]
#[case(
    r#"
[test_assets.packages]
filename = "packages.txt"
hash = "e70ee73a8fa703ef0e47c8224b4275db2f951249b883a9f9e30d4ac8e9a676eb"
url = "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"
"#,
    1,
    vec!["packages.txt"]
)]
#[case(
    r#"
[test_assets.packages1]
filename = "packages1.txt"
hash = "e70ee73a8fa703ef0e47c8224b4275db2f951249b883a9f9e30d4ac8e9a676eb"
url = "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"

[test_assets.packages2]
filename = "packages2.txt"
hash = "e70ee73a8fa703ef0e47c8224b4275db2f951249b883a9f9e30d4ac8e9a676eb"
url = "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"
"#,
    2,
    vec!["packages1.txt", "packages2.txt"]
)]
fn test_library_toml_parsing_and_download(
    #[case] toml_content: &str,
    #[case] expected_count: usize,
    #[case] expected_files: Vec<&str>,
) {
    let test_name = format!("library-toml-test-{}", expected_files.join("-").replace(".txt", ""));
    let test_dir = TestDir::new(&test_name);
    fs::create_dir_all(test_dir.path_str()).unwrap();

    let parsed: TestAsset = toml::de::from_str(toml_content).expect("Failed to parse TOML");
    let assets = parsed.values();

    assert_eq!(assets.len(), expected_count, "Should have exactly {} asset(s)", expected_count);

    let result = dl_test_files(&assets, test_dir.path_str(), true);
    assert!(result.is_ok(), "Failed to download from TOML definition: {:?}", result.err());

    for filename in expected_files {
        assert!(
            test_dir.file_path(filename).exists(),
            "Downloaded file {} does not exist",
            filename
        );
    }
}

#[test]
fn test_library_toml_from_file() {
    let test_dir = TestDir::new("library-toml-file-test");
    fs::create_dir_all(test_dir.path_str()).unwrap();

    let toml_content = r#"
[test_assets.file_test]
filename = "packages.txt"
hash = "e70ee73a8fa703ef0e47c8224b4275db2f951249b883a9f9e30d4ac8e9a676eb"
url = "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"
"#;

    let mut file = fs::File::create(test_dir.toml_path()).unwrap();
    file.write_all(toml_content.as_bytes()).unwrap();
    drop(file);

    let file_content = fs::read_to_string(test_dir.toml_path()).expect("Failed to read TOML file");
    let parsed: TestAsset =
        toml::de::from_str(&file_content).expect("Failed to parse TOML from file");
    let assets = parsed.values();

    let result = dl_test_files(&assets, test_dir.path_str(), true);
    assert!(result.is_ok(), "Failed to download from TOML file: {:?}", result.err());

    assert!(test_dir.file_path("packages.txt").exists(), "Downloaded file does not exist");
}

#[test]
fn test_library_toml_with_backoff() {
    let test_dir = TestDir::new("library-toml-backoff-test");
    fs::create_dir_all(test_dir.path_str()).unwrap();

    let toml_content = r#"
[test_assets.backoff_test]
filename = "packages.txt"
hash = "e70ee73a8fa703ef0e47c8224b4275db2f951249b883a9f9e30d4ac8e9a676eb"
url = "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"
"#;

    let parsed: TestAsset = toml::de::from_str(toml_content).expect("Failed to parse TOML");
    let assets = parsed.values();

    let result = dl_test_files_backoff(&assets, test_dir.path_str(), true, Duration::from_secs(1));
    assert!(result.is_ok(), "Failed to download from TOML with backoff: {:?}", result.err());

    assert!(test_dir.file_path("packages.txt").exists(), "Downloaded file does not exist");
}
