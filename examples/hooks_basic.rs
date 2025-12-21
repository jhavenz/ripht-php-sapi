//! Basic ExecutionHooks usage demonstrating lifecycle callbacks.
//!
//! This example shows how to implement `ExecutionHooks` to observe the PHP
//! request lifecycle without modifying behavior.
//!
//! Run: `cargo run --example hooks_basic`

use std::path::{Path, PathBuf};

use ripht_php_sapi::{
    ExecutionHooks, ExecutionMessage, ExecutionResult, OutputAction, RiphtSapi,
    WebRequest,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("hello.php");

    let exec = WebRequest::get().build(&script_path)?;

    let hooks = LifecycleObserver::new();

    println!("Executing PHP script with lifecycle hooks...\n");

    let result = sapi.execute_with_hooks(exec, hooks)?;

    println!("Execution complete!");
    println!("Status: {}", result.status);
    println!("Body: {}", result.body_string());

    Ok(())
}



struct LifecycleObserver {
    events: Vec<String>,
}

impl LifecycleObserver {
    fn new() -> Self {
        Self { events: Vec::new() }
    }

    fn log(&mut self, event: &str) {
        self.events
            .push(event.to_string());
    }
}

impl ExecutionHooks for LifecycleObserver {
    fn on_context_created(&mut self) {
        self.log("context_created");
    }

    fn on_request_starting(&mut self) {
        self.log("request_starting");
    }

    fn on_request_started(&mut self) {
        self.log("request_started");
    }

    fn on_script_executing(&mut self, script_path: &Path) {
        self.log(&format!("script_executing: {}", script_path.display()));
    }

    fn on_script_executed(&mut self, success: bool) {
        self.log(&format!("script_executed: success={}", success));
    }

    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        self.log(&format!("on_output: {} bytes", data.len()));
        OutputAction::Buffer
    }

    fn on_header(&mut self, name: &str, value: &str) -> bool {
        self.log(&format!("on_header: {}: {}", name, value));
        true
    }

    fn on_status(&mut self, code: u16) {
        self.log(&format!("on_status: {}", code));
    }

    fn on_php_message(&mut self, message: &ExecutionMessage) {
        self.log(&format!(
            "on_php_message: {:?} - {}",
            message.level, message.message
        ));
    }

    fn on_request_finishing(&mut self) {
        self.log("request_finishing");
    }

    fn on_request_finished(&mut self, result: &ExecutionResult) {
        self.log(&format!(
            "request_finished: status={}, body_len={}",
            result.status,
            result.body.len()
        ));
    }
}