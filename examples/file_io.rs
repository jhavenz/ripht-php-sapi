//! Demonstrates PHP file system operations and SQLite database interactions.
//!
//! Run: `cargo run --example file_io`

use std::{path::PathBuf};

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("file_io.php");

    println!("=== PHP File I/O Example ===\n");

    let reqs = [
        (
            WebRequest::get().with_uri("/?action=readwrite"),
            "Read/Write cycle:",
        ),
        (
            WebRequest::get().with_uri("/?action=many_files&count=20"),
            "Multiple files:",
        ),
        (
            WebRequest::get().with_uri("/?action=large_file&size=512"),
            "Large file (512KB):",
        ),
        (
            WebRequest::get().with_uri("/?action=sqlite&rows=500"),
            "SQLite (500 rows):",
        ),
    ];

    for (req, msg) in reqs {
        let ctx = req.build(&script)?;
        let result = sapi.execute(ctx)?;

        println!("{msg}");
        println!("{}\n", result.body_string());
    }

    Ok(())
}
