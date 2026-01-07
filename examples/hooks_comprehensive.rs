//! Shows all the [`ExecutionHooks`] lifecycle methods in action.
//!
//! Runs errors.php which demos PHP's quirky logging behavior - `error_log()`
//! actually sends messages at LOG_NOTICE level, not LOG_ERR. Go figure.
//!
//! Run: `cargo run --example hooks_comprehensive`

use std::path::{Path, PathBuf};

use ripht_php_sapi::{
    ExecutionHooks, ExecutionMessage, ExecutionResult, OutputAction, RiphtSapi,
    WebRequest,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("errors.php");

    let exec = WebRequest::get().build(&script_path)?;

    let hooks = ComprehensiveHooks::new();

    println!("=== Comprehensive Hooks Example ===\n");
    println!("Running errors.php to show all hook callbacks.");
    println!("We'll filter out X-Powered-By header as a demo.\n");
    println!("--- Lifecycle Events ---\n");

    let result = sapi.execute_with_hooks(exec, hooks)?;
    let headers = result.all_headers();

    println!("\n--- Final Result ---\n");
    println!("Status: {}", result.status_code());
    let headers_vec: Vec<_> = headers.collect();
    println!("Headers ({}):", headers_vec.len());
    for header in &headers_vec {
        println!("  {}: {}", header.name(), header.value());
    }
    println!("\nBody:\n{}", result.body_string());

    Ok(())
}

struct ComprehensiveHooks {
    start_time: std::time::Instant,
    connection_alive: bool,
    filtered_headers: Vec<String>,
    error_count: usize,
    warning_count: usize,
}

impl ComprehensiveHooks {
    fn new() -> Self {
        Self {
            start_time: std::time::Instant::now(),
            connection_alive: true,
            filtered_headers: vec!["X-Powered-By".to_string()],
            error_count: 0,
            warning_count: 0,
        }
    }

    fn elapsed_ms(&self) -> u128 {
        self.start_time
            .elapsed()
            .as_micros()
    }
}

impl ExecutionHooks for ComprehensiveHooks {
    fn on_context_created(&mut self) {
        println!("[{:>6}μs] Context created", self.elapsed_ms());
    }

    fn on_request_starting(&mut self) {
        println!("[{:>6}μs] Request starting", self.elapsed_ms());
    }

    fn on_request_started(&mut self) {
        println!("[{:>6}μs] Request started", self.elapsed_ms());
    }

    fn on_script_executing(&mut self, script_path: &Path) {
        println!(
            "[{:>6}μs] Script executing: {}",
            self.elapsed_ms(),
            script_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        );
    }

    fn on_script_executed(&mut self, success: bool) {
        println!(
            "[{:>6}μs] Script executed: {}",
            self.elapsed_ms(),
            if success { "SUCCESS" } else { "FAILED" }
        );
    }

    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        let preview = String::from_utf8_lossy(&data[..data.len().min(50)]);
        let ellipsis = if data.len() > 50 { "..." } else { "" };

        println!(
            "[{:>6}μs] Output received: {} bytes (preview: \"{}{}\")",
            self.elapsed_ms(),
            data.len(),
            preview.trim(),
            ellipsis
        );

        OutputAction::Continue
    }

    fn on_flush(&mut self) {
        println!("[{:>6}μs] Flush called", self.elapsed_ms());
    }

    fn on_header(&mut self, name: &str, value: &str) -> bool {
        let should_include = !self
            .filtered_headers
            .iter()
            .any(|h| h.eq_ignore_ascii_case(name));

        if should_include {
            println!(
                "[{:>6}μs] Header (included): {}: {}",
                self.elapsed_ms(),
                name,
                value
            );
        } else {
            println!(
                "[{:>6}μs] Header (filtered): {}: {}",
                self.elapsed_ms(),
                name,
                value
            );
        }

        should_include
    }

    fn on_status(&mut self, code: u16) {
        println!("[{:>6}μs] HTTP Status: {}", self.elapsed_ms(), code);
    }

    fn on_php_message(&mut self, message: &ExecutionMessage) {
        // just show the raw syslog level - PHP's error_log() sends LOG_NOTICE
        // which might seem weird but that's how it works
        if message
            .level
            .is_error_or_worse()
        {
            self.error_count += 1;
        } else if message
            .level
            .is_warning_or_worse()
        {
            self.warning_count += 1;
        }

        println!(
            "[{:>6}μs] PHP [{}]: {}",
            self.elapsed_ms(),
            message.level,
            message.message
        );
    }

    fn is_connection_alive(&self) -> bool {
        self.connection_alive
    }

    fn on_request_finishing(&mut self) {
        println!(
            "[{:>6}μs] Request finishing (php_request_shutdown about to be called)",
            self.elapsed_ms()
        );
    }

    fn on_request_finished(&mut self, result: &ExecutionResult) {
        println!(
            "[{:>6}μs] Request finished: status={}, body={} bytes, headers={}, messages={}",
            self.elapsed_ms(),
            result.status_code(),
            result.body().len(),
            result.all_headers().count(),
            result.all_messages().count()
        );

        if self.error_count > 0 || self.warning_count > 0 {
            println!(
                "\n  Summary: {} errors, {} warnings",
                self.error_count, self.warning_count
            );
        }
    }
}
