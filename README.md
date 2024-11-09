# Test Assets 

[<img alt="github" src="https://img.shields.io/badge/github-wcampbell0x2a/test_assets_ureq-8da0cb?style=for-the-badge&labelColor=555555&logo=github" height="20">](https://github.com/wcampbell0x2a/test-assets-ureq)
[<img alt="crates.io" src="https://img.shields.io/crates/v/test-assets-ureq.svg?style=for-the-badge&color=fc8d62&logo=rust" height="20">](https://crates.io/crates/test-assets-ureq)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-test_assets_ureq-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/test-assets-ureq)
[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/wcampbell0x2a/test-assets-ureq/main.yml?branch=master&style=for-the-badge" height="20">](https://github.com/wcampbell0x2a/test-assets-ureq/actions?query=branch%3Amaster)

Download test assets, managing them outside of git.

Changes from being a fork of [test-assets](https://github.com/est31/test-assets):
* Use rust library `ureq` and avoid compiling curl for test binaries
* Includes backoff support

*Compiler support: requires rustc 1.70.0+*
