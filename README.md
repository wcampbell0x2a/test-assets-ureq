# Test Assets 

[<img alt="github" src="https://img.shields.io/badge/github-wcampbell0x2a/test_assets_ureq-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/wcampbell0x2a/test-assets-ureq)
[<img alt="crates.io" src="https://img.shields.io/crates/v/test-assets-ureq.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/test-assets-ureq)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-test_assets_ureq-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/test-assets-ureq)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/wcampbell0x2a/test-assets-ureq/main.yml?branch=master&style=for-the-badge" height="20">](https://github.com/wcampbell0x2a/test-assets-ureq/actions?query=branch%3Amaster)

Download test assets, managing them outside of git.

Changes from being a fork of [test-assets](https://github.com/est31/test-assets):
* Use rust library `ureq` and avoid compiling curl for test binaries
* Includes backoff support

## Library
*Compiler support: requires rustc 1.74.1+*

Add the following to your `Cargo.toml` file:
```toml
[dependencies]
test-assets-ureq = "0.5.0"
```

For example, add the following information into the project `toml` file.
```toml
[test_assets.test_00]
filename = "out.squashfs"
hash = "976c1638d8c1ba8014de6c64b196cbd70a5acf031be10a8e7f649536193c8e78"
url = "https://wcampbell.dev/squashfs/testing/test_00/out.squashfs"
```

In your rust code, add the following to download using that previous file.
```rust,no_run
let file_content = fs::read_to_string("test.toml").unwrap();
let parsed: TestAsset = toml::de::from_str(&file_content).unwrap();
let assets = parsed.values();
dl_test_files_backoff(&assets, "test-assets", true, Duration::from_secs(1)).unwrap();
```

## Binary
If test-assets are needed outside of the Rust code, a binary is provided to download them.
```console
$ curl -L https://github.com/wcampbell0x2a/test-assets-ureq/releases/download/v0.5.0/dl-v0.5.0-x86_64-unknown-linux-musl.tar.gz -o dl.tar.gz
$ tar -xvf dl.tar.gz
$ ./dl test-assets.toml
```
