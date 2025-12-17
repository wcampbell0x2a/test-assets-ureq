use rstest::rstest;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn get_dl_binary() -> String {
    env!("CARGO_BIN_EXE_dl").to_string()
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let path = PathBuf::from(format!("test-output/{}", name));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path_str(&self) -> &str {
        self.path.to_str().unwrap()
    }

    fn toml_path(&self) -> PathBuf {
        self.path.join("test_assets.toml")
    }

    fn file_path(&self, filename: &str) -> PathBuf {
        self.path.join(filename)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[rstest]
#[case(
    r#"
[test_assets.packages]
filename = "packages.txt"
hash = "e70ee73a8fa703ef0e47c8224b4275db2f951249b883a9f9e30d4ac8e9a676eb"
url = "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"
"#,
    vec!["packages.txt"],
    true
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
    vec!["packages1.txt", "packages2.txt"],
    true
)]
#[case(
    r#"
[test_assets.bad]
filename = "packages.txt"
hash = "0000000000000000000000000000000000000000000000000000000000000000"
url = "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"
"#,
    vec![],
    false
)]
fn test_binary_downloads(
    #[case] toml_content: &str,
    #[case] expected_files: Vec<&str>,
    #[case] should_succeed: bool,
) {
    let test_name = format!("binary-test-{}", expected_files.join("-").replace(".txt", ""));
    let test_dir = TestDir::new(&test_name);

    let mut file = fs::File::create(test_dir.toml_path()).unwrap();
    file.write_all(toml_content.as_bytes()).unwrap();
    drop(file);

    let output = Command::new(get_dl_binary())
        .args(&[test_dir.toml_path().to_str().unwrap(), test_dir.path_str()])
        .output()
        .expect("Failed to execute binary");

    if should_succeed {
        assert!(
            output.status.success(),
            "Binary failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        for filename in expected_files {
            assert!(
                test_dir.file_path(filename).exists(),
                "Expected file {} does not exist",
                filename
            );
        }
    } else {
        assert!(!output.status.success(), "Binary should have failed due to hash mismatch");
    }
}

#[test]
fn test_binary_cached_download() {
    let test_dir = TestDir::new("binary-cached-test");

    let toml_content = r#"
[test_assets.cached]
filename = "packages.txt"
hash = "e70ee73a8fa703ef0e47c8224b4275db2f951249b883a9f9e30d4ac8e9a676eb"
url = "https://downloads.openwrt.org/releases/23.05.0/packages/x86_64/base/Packages"
"#;

    let mut file = fs::File::create(test_dir.toml_path()).unwrap();
    file.write_all(toml_content.as_bytes()).unwrap();
    drop(file);

    let output1 = Command::new(get_dl_binary())
        .args(&[test_dir.toml_path().to_str().unwrap(), test_dir.path_str()])
        .output()
        .expect("Failed to execute binary");

    assert!(output1.status.success(), "First run failed");

    let output2 = Command::new(get_dl_binary())
        .args(&[test_dir.toml_path().to_str().unwrap(), test_dir.path_str()])
        .output()
        .expect("Failed to execute binary");

    assert!(output2.status.success(), "Second run failed");

    let stdout = String::from_utf8_lossy(&output2.stdout);
    assert!(
        stdout.contains("matching hash") || stdout.contains("skipping"),
        "Expected cache hit message in output"
    );
}
