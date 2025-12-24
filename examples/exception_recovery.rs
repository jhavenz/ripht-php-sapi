//! Tests SAPI recovery after various PHP error conditions (exit, die, fatal, exceptions).
//!
//! Run: `cargo run --example exception_recovery`

use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();
    let shutdown = php_script_path("shutdown_behavior.php");
    let exception = php_script_path("exception_handling.php");
    let hello = php_script_path("hello.php");

    println!("=== PHP Error Recovery ===\n");

    let cases = [
        ("Normal execution", &shutdown, "action=normal"),
        ("exit() with code", &shutdown, "action=exit_code"),
        ("die() with message", &shutdown, "action=die_message"),
        ("return from script", &shutdown, "action=return"),
        ("Exception thrown", &exception, "throw=true"),
        ("No exception", &exception, "throw=false"),
        ("Fatal error", &shutdown, "action=fatal"),
    ];

    for (name, script, query) in cases {
        let exec = WebRequest::get()
            .with_uri(format!("/?{}", query))
            .build(script)?;
        let result = sapi.execute(exec)?;
        println!(
            "{:.<30} {} ({})",
            name,
            result.status_code(),
            if result.status_code() == 200 { "OK" } else { "ERR" }
        );
    }

    println!("\nRecovery check:");
    let exec = WebRequest::get().build(&hello)?;
    let result = sapi.execute(exec)?;
    println!(
        "  Post-error request: {} - {}",
        result.status_code(),
        result.body_string().trim()
    );

    Ok(())
}

fn php_script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join(name)
}
