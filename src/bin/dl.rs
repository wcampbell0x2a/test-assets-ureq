use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::fs;
use std::time::Duration;
use test_assets_ureq::{dl_test_files_backoff_with_progress, ProgressCallbacks, TestAsset};

#[derive(Parser, Debug)]
struct Cli {
    /// Path to the TOML file to read
    #[arg(value_name = "FILE")]
    file: String,

    /// Base path to write downloaded files
    #[arg(value_name = "PATH")]
    out: String,

    /// List of specific asset names to download (downloads all if not specified)
    #[arg(long, value_delimiter = ',')]
    assets: Option<Vec<String>>,
}

pub fn sha_matched(pb: &ProgressBar, s: &str) {
    let blue_bold: console::Style = console::Style::new().blue().bold();
    let line = format!("{:>16} {}", blue_bold.apply_to("SHA Matched"), s,);
    pb.println(line);
}

pub fn sha_not_matched(pb: &ProgressBar, s: &str) {
    let blue_bold: console::Style = console::Style::new().blue().bold();
    let line = format!("{:>16} {}", blue_bold.apply_to("SHA Not Matched"), s,);
    pb.println(line);
}

pub fn downloaded(pb: &ProgressBar, s: &str) {
    let blue_bold: console::Style = console::Style::new().blue().bold();
    let line = format!("{:>16} {}", blue_bold.apply_to("Downloaded"), s,);
    pb.println(line);
}

pub fn downloading_failed(pb: &ProgressBar, s: &str) {
    let error: console::Style = console::Style::new().red().bold();
    let line = format!("{:>16} {}", error.apply_to("Downloading Failed"), s,);
    pb.println(line);
}

pub fn finished(pb: &ProgressBar, s: &str) {
    let blue_bold: console::Style = console::Style::new().blue().bold();
    let line = format!("{:>16} {}", blue_bold.apply_to("Finished"), s,);
    pb.println(line);
}

fn main() {
    let cli = Cli::parse();

    let file_content = fs::read_to_string(&cli.file).unwrap();

    let parsed: TestAsset = toml::de::from_str(&file_content).unwrap();
    let mut assets = parsed.values();

    // Filter assets if --assets flag is provided
    if let Some(asset_names) = &cli.assets {
        let asset_names_set: std::collections::HashSet<_> = asset_names.iter().collect();
        assets.retain(|asset| {
            asset_names_set.iter().any(|name| asset.filepath.contains(name.as_str()))
        });
    }

    let start = std::time::Instant::now();
    let multi_progress = MultiProgress::new();
    let pb_download = multi_progress.add(ProgressBar::new(0));
    pb_download.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40.cyan/blue}] {pos}/{len} files downloaded")
            .unwrap()
            .progress_chars("#>-"),
    );

    let pb_msg = multi_progress.add(ProgressBar::new_spinner());

    let pb_msg_clone1 = pb_msg.clone();
    let sha_matched_fn = move |filepath: &str| {
        sha_matched(&pb_msg_clone1, filepath);
    };

    let pb_msg_clone2 = pb_msg.clone();
    let sha_not_matched_fn = move |filepath: &str| {
        sha_not_matched(&pb_msg_clone2, filepath);
    };

    let pb_msg_clone3 = pb_msg.clone();
    let downloaded_fn = move |filepath: &str| {
        downloaded(&pb_msg_clone3, filepath);
    };

    let pb_msg_clone4 = pb_msg.clone();
    let downloading_failed_fn = move |filepath: &str| {
        downloading_failed(&pb_msg_clone4, filepath);
    };

    let pb_msg_clone5 = pb_msg.clone();
    let finished_fn = move |msg: &str| {
        finished(&pb_msg_clone5, msg);
    };

    let pb_msg_clone6 = pb_msg.clone();
    let progress_update_fn = move |msg: &str| {
        pb_msg_clone6.set_message(msg.to_string());
    };

    let pb_download_clone = pb_download.clone();
    let pb_initialized = std::sync::Arc::new(std::sync::Mutex::new(false));
    let pb_init_clone = pb_initialized.clone();
    let download_progress_fn = move |completed: usize, total: usize| {
        let mut initialized = pb_init_clone.lock().unwrap();
        if !*initialized && total > 0 {
            pb_download_clone.set_length(total as u64);
            *initialized = true;
        }
        drop(initialized);
        pb_download_clone.set_position(completed as u64);
    };

    let callbacks = ProgressCallbacks {
        sha_matched_fn: &sha_matched_fn,
        sha_not_matched_fn: &sha_not_matched_fn,
        downloaded_fn: &downloaded_fn,
        downloading_failed_fn: &downloading_failed_fn,
        finished_fn: &finished_fn,
        progress_update_fn: &progress_update_fn,
        download_progress_fn: &download_progress_fn,
    };

    dl_test_files_backoff_with_progress(&assets, &cli.out, Duration::from_secs(1), &callbacks)
        .unwrap();

    pb_download.finish_and_clear();

    let elapsed = start.elapsed();
    finished(
        &pb_msg,
        &format!("Downloaded {} asset(s) in {:.2} seconds", assets.len(), elapsed.as_secs_f64()),
    );
}
