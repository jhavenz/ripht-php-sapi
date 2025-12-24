use ripht_php_sapi::{
    ExecutionHooks, ExecutionMessage, ExecutionResult, OutputAction, RiphtSapi,
    WebRequest,
};
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sapi = RiphtSapi::instance();

    println!("==== Env and Ini demo ====");

    let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join("env_and_ini.php");

    let exec = WebRequest::get()
        .with_env("FOO", "bar")
        .with_env("HELLO", "world")
        .with_ini("display_errors", "1")
        .with_ini("memory_limit", "64M")
        .build(&script_path)?;

    let result = sapi.execute_with_hooks(exec, ValidationHooks)?;

    println!("Status: {}", result.status_code());
    
    println!("\nOutput from PHP:");
    println!("{}", result.body_string());

    Ok(())
}

struct ValidationHooks;

impl ExecutionHooks for ValidationHooks {
    fn on_script_executing(&mut self, script_path: &Path) {
        println!("[Hook] Executing: {}", script_path.display());
    }

    fn on_script_executed(&mut self, success: bool) {
        println!("[Hook] Script executed successfully: {}", success);
    }

    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        println!("[Hook] Output received: {} bytes", data.len());
        OutputAction::Continue
    }

    fn on_status(&mut self, code: u16) {
        println!("[Hook] Response status: {}", code);
    }

    fn on_php_message(&mut self, message: &ExecutionMessage) {
        println!(
            "[Hook] PHP message: {:?} - {}",
            message.level, message.message
        );
    }

    fn on_request_finished(&mut self, _result: &ExecutionResult) {
        println!("[Hook] Request finished\n");
    }
}
