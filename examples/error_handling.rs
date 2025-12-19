//! Demonstrates capturing and categorizing PHP errors and warnings.
//!
//! Run: `cargo run --example error_handling`

use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, SyslogLevel, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("errors.php");

    let exec = WebRequest::get().build(&script_path)?;
    let result = sapi.execute(exec)?;

    println!("Status: {}", result.status);
    println!("Body: {}", result.body_string());

    if result.has_errors() {
        eprintln!("\nPHP Errors:");
        for error in result.errors() {
            eprintln!("  [{:?}] {}", error.level, error.message);
        }
    }

    let warnings: Vec<_> = result
        .messages
        .iter()
        .filter(|m| matches!(m.level, SyslogLevel::Warning))
        .collect();

    if !warnings.is_empty() {
        eprintln!("\nPHP Warnings:");
        for warning in warnings {
            eprintln!("  [{:?}] {}", warning.level, warning.message);
        }
    }

    if result.messages.is_empty() {
        println!("\nNo PHP errors or warnings detected");
    }

    Ok(())
}
