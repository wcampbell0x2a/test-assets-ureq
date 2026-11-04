use rstest::rstest;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::thread;
use test_assets_ureq::{dl_test_files_with_progress, ProgressCallbacks, TestAssetDef};

const BODY_LEN: usize = 1024 * 1024;

/// How the test server answers a HEAD request
#[derive(Clone, Copy, Debug)]
enum HeadReply {
    /// 200 with the correct Content-Length
    CorrectSize,
    /// 200 with Content-Length: 0, as a redirect can give
    ZeroSize,
    /// 403, as a presigned URL that allows only GET gives
    Forbidden,
}

/// Serves `body` on a local port and returns the URL.
fn serve(body: Vec<u8>, head_reply: HeadReply) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }

            if request_line.starts_with("HEAD") {
                let header = match head_reply {
                    HeadReply::CorrectSize => {
                        format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n", body.len())
                    }
                    HeadReply::ZeroSize => "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n".to_string(),
                    HeadReply::Forbidden => {
                        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n".to_string()
                    }
                };
                write!(stream, "{header}Connection: close\r\n\r\n").unwrap();
            } else {
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
        }
    });
    format!("http://{addr}/asset.bin")
}

#[rstest]
#[case::head_correct_size(HeadReply::CorrectSize)]
#[case::head_zero_size(HeadReply::ZeroSize)]
#[case::head_forbidden(HeadReply::Forbidden)]
fn test_progress_total_is_file_size(#[case] head_reply: HeadReply) {
    let body: Vec<u8> = (0..BODY_LEN).map(|i| (i % 251) as u8).collect();
    let hash = format!("{:x}", Sha256::digest(&body));
    let url = serve(body, head_reply);
    let dir = format!("test-output/progress_{head_reply:?}");
    let _ = fs::remove_dir_all(&dir);

    let messages = Mutex::new(Vec::new());
    let record = |msg: &str| messages.lock().unwrap().push(msg.to_string());
    let callbacks = ProgressCallbacks {
        sha_matched_fn: &|_| {},
        sha_not_matched_fn: &|_| {},
        downloaded_fn: &|_| {},
        downloading_failed_fn: &|_| {},
        finished_fn: &|_| {},
        progress_update_fn: &record,
        download_progress_fn: &|_, _| {},
    };
    let defs = [TestAssetDef { filepath: "asset.bin".to_string(), hash, url }];

    let result = dl_test_files_with_progress(&defs, &dir, &callbacks);
    let _ = fs::remove_dir_all(&dir);

    assert!(result.is_ok(), "{result:?}");
    let messages = messages.into_inner().unwrap();
    assert!(!messages.is_empty());
    for msg in &messages {
        assert!(msg.contains(" / 1.00 MB - asset.bin"), "unexpected progress message: {msg}");
    }
}
