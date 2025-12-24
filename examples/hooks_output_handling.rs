//! Hooks with output handling - processing the buffered output after execution.
//!
//! This example demonstrates using `on_output` to handle the complete response
//! body after PHP execution, transforming or forwarding it as needed.
//!
//! Run: `cargo run --example hooks_output_handling`

use std::path::PathBuf;

use ripht_php_sapi::{ExecutionHooks, OutputAction, RiphtSapi, WebRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("hello.php");

    let exec = WebRequest::get().build(&script_path)?;

    let hooks = OutputTransformer::new();

    println!("Executing with output transformation hooks...\n");

    let result = sapi.execute_with_hooks(exec, hooks)?;

    println!("\nExecution complete!");
    println!("Status: {}", result.status_code());

    println!(
        "Body (empty because we returned Handled): {} bytes",
        result.body().len()
    );

    Ok(())
}

struct OutputTransformer {
    captured_output: Vec<u8>,
}

impl OutputTransformer {
    fn new() -> Self {
        Self {
            captured_output: Vec::new(),
        }
    }
}

impl ExecutionHooks for OutputTransformer {
    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        self.captured_output = data.to_vec();

        let output_str = String::from_utf8_lossy(data);
        let transformed = output_str.to_uppercase();

        println!("[OutputTransformer] Original: {}", output_str.trim());
        println!("[OutputTransformer] Transformed: {}", transformed.trim());

        OutputAction::Done
    }

    fn on_status(&mut self, code: u16) {
        println!("[OutputTransformer] HTTP Status: {}", code);
    }

    fn on_header(&mut self, name: &str, value: &str) -> bool {
        println!("[OutputTransformer] Header: {}: {}", name, value);
        true
    }
}
