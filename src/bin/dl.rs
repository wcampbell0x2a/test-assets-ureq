use clap::Parser;
use std::fs;
use std::time::Duration;
use test_assets_ureq::{dl_test_files_backoff, TestAsset};

#[derive(Parser, Debug)]
struct Cli {
    /// Path to the TOML file to read
    #[arg(value_name = "FILE")]
    file: String,

    /// Base path to write downloaded files
    #[arg(value_name = "PATH")]
    out: String,

    /// Only grab matching test names
    #[arg(long)]
    filter: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let file_content = fs::read_to_string(&cli.file).unwrap();

    let parsed: TestAsset = toml::de::from_str(&file_content).unwrap();
    dl_test_files_backoff(&parsed, &cli.filter, &cli.out, true, Duration::from_secs(1)).unwrap();
}
