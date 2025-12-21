//! Demonstrates how to enable and view tracing logs from the SAPI.
//!
//! To see logs, run with `RUST_LOG` set to a level (info, debug, trace):
//! `RUST_LOG=debug cargo run --example tracing_demo --features tracing`

#[cfg(feature = "tracing")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;
    use ripht_php_sapi::{RiphtSapi, WebRequest};
    use tracing::info;

    // 1. Initialize a tracing subscriber.
    // This is required to actually see any output. The `fmt` subscriber
    // reads the RUST_LOG environment variable.
    tracing_subscriber::fmt::init();

    info!("Initializing SAPI...");
    let sapi = RiphtSapi::instance();
    
    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("hello.php");

    info!("Executing request...");
    let exec = WebRequest::get().build(&script_path)?;

    // The SAPI will emit traces during execution based on the RUST_LOG level.
    // - INFO: Lifecycle milestones
    // - DEBUG: Request details and status
    // - TRACE: Internal plumbing and data flow
    let result = sapi.execute(exec)?;

    info!("Done. Status: {}", result.status);
    
    Ok(())
}

#[cfg(not(feature = "tracing"))]
fn main() {
    println!("This example requires the 'tracing' feature.");
    println!("Run with: cargo run --example tracing_demo --features tracing");
}
