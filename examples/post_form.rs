//! Submits URL-encoded form data via POST request.
//!
//! Run: `cargo run --example post_form`

use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("post_form.php");

    let body =
        "name=John%20Doe&email=john%40example.com&message=Hello%20from%20Rust!";
    let exec = WebRequest::post()
        .with_content_type("application/x-www-form-urlencoded")
        .with_body(body.as_bytes().to_vec())
        .build(&script_path)?;

    let result = sapi.execute(exec)?;

    if result.status_code() == 200 {
        println!("Form submitted successfully");
        println!("Response: {}", result.body_string());
    } else {
        eprintln!("Request failed with status: {}", result.status_code());
    }

    Ok(())
}
