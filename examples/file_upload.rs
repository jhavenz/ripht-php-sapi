//! Handles multipart/form-data file uploads from Rust to PHP.
//!
//! Run: `cargo run --example file_upload`

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn generate_boundary() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("----multipart-form-boundary-{:x}", timestamp)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("file_upload.php");

    let boundary = generate_boundary();
    let file_content = b"Hello, this is a test file uploaded from Rust!";
    let filename = "test.txt";

    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         {}\r\n\
         --{boundary}--\r\n",
        String::from_utf8_lossy(file_content),
        boundary = boundary,
        filename = filename
    )
    .into_bytes();

    let exec = WebRequest::post()
        .with_content_type(format!(
            "multipart/form-data; boundary={}",
            boundary
        ))
        .with_body(body)
        .build(&script_path)?;

    let result = sapi.execute(exec)?;

    if result.status_code() == 200 {
        println!("File uploaded successfully");
        println!("Response: {}", result.body_string());
    } else {
        eprintln!("Upload failed with status: {}", result.status_code());
    }

    Ok(())
}
