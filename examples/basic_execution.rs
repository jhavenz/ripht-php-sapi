//! Basic PHP script execution demonstrating simple GET request handling.
//!
//! Run: `cargo run --example basic_execution`

use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("hello.php");

    let exec = WebRequest::get().build(&script_path)?;

    let result = sapi.execute(exec)?;

    println!("Body: {}", result.body_string());
    println!("Status: {}", result.status_code());

    if result.has_errors() {
        eprintln!("PHP errors occurred:");
        for error in result.errors() {
            eprintln!("  {:?}: {}", error.level, error.message);
        }
    }

    Ok(())
}
