//! Tests memory allocation patterns and tracks PHP memory usage under load.
//!
//! Run: `cargo run --example memory_pressure`

use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("memory_intensive.php");

    println!("=== PHP Memory Pressure ===\n");

    let cases = [
        ("Baseline", "action=report"),
        ("Post-stress baseline", "action=report"),
        ("100-level recursion", "action=recursive&size=100"),
        ("5000 objects", "action=objects&size=5000"),
        ("5000 strings (~5MB)", "action=allocate&size=5000"),
        ("10000 strings (~10MB)", "action=allocate&size=10000"),
    ];

    for (name, query) in cases {
        let exec = WebRequest::get()
            .with_uri(format!("/?{}", query))
            .build(&script)?;
        let result = sapi.execute(exec)?;

        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&result.body) {
            let peak = json["peak_after"]
                .as_u64()
                .unwrap_or(0) as f64
                / 1024.0
                / 1024.0;

            println!("{:.<30} peak={:.1}MB", name, peak);
        } else {
            println!("{:.<30} {}", name, result.status);
        }
    }

    println!("\nRepeated allocation cycles:");
    
    for i in 1..=3 {
        let exec = WebRequest::get()
            .with_uri("/?action=allocate&size=3000")
            .build(&script)?;
        let result = sapi.execute(exec)?;

        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&result.body) {
            let peak = json["peak_after"]
                .as_u64()
                .unwrap_or(0) as f64
                / 1024.0
                / 1024.0;

            println!("  Cycle {}: peak={:.1}MB", i, peak);
        }
    }

    Ok(())
}
