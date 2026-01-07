//! Using the [`StreamingCallback`] helper for output handling with execute_with_hooks.
//!
//! [`StreamingCallback`] is a convenience wrapper that implements [`ExecutionHooks`]
//! with a closure for [`on_output`], returning [`OutputAction::Done`]. This is
//! useful when you only need to handle output but want to use [`execute_with_hooks`]
//! for other hook functionality.
//!
//! Note: For simple streaming use cases, [`execute_streaming()`] is more direct.
//! Use [`StreamingCallback`] when you need hooks AND output handling together.
//!
//! Run: `cargo run --example hooks_streaming_callback`

use std::path::PathBuf;

use ripht_php_sapi::{RiphtSapi, StreamingCallback, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("hello.php");

    let exec = WebRequest::get().build(&script_path)?;

    println!("=== StreamingCallback Helper Demo ===\n");
    println!(
        "Using StreamingCallback to wrap a closure for output handling.\n"
    );

    let mut chunk_count = 0;
    let hooks = StreamingCallback::new(move |chunk: &[u8]| {
        chunk_count += 1;
        let content = String::from_utf8_lossy(chunk);
        println!("[Chunk {}] {} bytes: {}", chunk_count, chunk.len(), content);
    });

    let result = sapi.execute_with_hooks(exec, hooks)?;

    println!("\n--- Result ---");
    println!("Status: {}", result.status_code());
    println!("Body length: {}", result.body().len());
    println!("Headers: {}", result.all_headers().count());

    Ok(())
}
