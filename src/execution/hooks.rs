use std::path::Path;

use super::message::ExecutionMessage;
use super::result::ExecutionResult;

/// What to do with PHP output data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OutputAction {
    /// Accumulate in the result body.
    Buffer,
    /// Already handled (e.g., streamed to client).
    Handled,
}

/// No-op implementation of `ExecutionHooks`.
pub struct NoOpHooks;

impl ExecutionHooks for NoOpHooks {}

/// Wraps a closure as an `ExecutionHooks` implementation for streaming output.
pub struct StreamingCallback<F> {
    output_fn: F,
}

impl<F> StreamingCallback<F>
where
    F: FnMut(&[u8]),
{
    pub fn new(output_fn: F) -> Self {
        Self { output_fn }
    }
}

impl<F> ExecutionHooks for StreamingCallback<F>
where
    F: FnMut(&[u8]),
{
    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        (self.output_fn)(data);
        OutputAction::Handled
    }
}

/// Callbacks invoked during PHP request execution.
///
/// All methods have default implementations that do nothing or return
/// sensible defaults. Override only what you need.
pub trait ExecutionHooks {
    /// Called after ServerContext is created.
    fn on_context_created(&mut self) {}
    /// Called before php_request_startup.
    fn on_request_starting(&mut self) {}
    /// Called after php_request_startup succeeds.
    fn on_request_started(&mut self) {}

    /// Called before script execution begins.
    fn on_script_executing(&mut self, script_path: &Path) {
        let _ = script_path;
    }

    /// Called after script execution completes.
    fn on_script_executed(&mut self, success: bool) {
        let _ = success;
    }

    /// Called when PHP writes output. Return `Handled` to suppress buffering.
    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        let _ = data;
        OutputAction::Buffer
    }

    /// Called when PHP flushes output.
    fn on_flush(&mut self) {}

    /// Called for each response header. Return false to suppress the header.
    fn on_header(&mut self, name: &str, value: &str) -> bool {
        let _ = (name, value);
        true
    }

    /// Called when HTTP status code is set.
    fn on_status(&mut self, code: u16) {
        let _ = code;
    }

    /// Called for PHP errors, warnings, and notices.
    fn on_php_message(&mut self, message: &ExecutionMessage) {
        let _ = message;
    }

    /// Return false to abort execution (e.g., client disconnected).
    fn is_connection_alive(&self) -> bool {
        true
    }

    /// Called before php_request_shutdown.
    fn on_request_finishing(&mut self) {}

    /// Called after request completes with the final result.
    fn on_request_finished(&mut self, result: &ExecutionResult) {
        let _ = result;
    }
}
