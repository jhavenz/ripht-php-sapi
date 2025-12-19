//! Streams PHP output in real-time using the streaming execution API.
//!
//! Run: `cargo run --example streaming_output`

use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("streaming.php");

    let exec = WebRequest::get().build(&script_path)?;

    println!("Executing PHP script with streaming output...\n");

    let _result = sapi.execute_streaming(exec, |chunk| {
        print!("{}", String::from_utf8_lossy(chunk));
        std::io::Write::flush(&mut std::io::stdout()).ok();
    })?;

    println!("\n\nStreaming complete");

    Ok(())
}
