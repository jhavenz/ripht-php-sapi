use std::path::Path;

use super::message::ExecutionMessage;
use super::result::ExecutionResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OutputAction {
    Buffer,
    Handled,
}

pub struct NoOpHooks;

impl ExecutionHooks for NoOpHooks {}

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

pub trait ExecutionHooks {
    fn on_context_created(&mut self) {}
    fn on_request_starting(&mut self) {}
    fn on_request_started(&mut self) {}

    fn on_script_executing(&mut self, script_path: &Path) {
        let _ = script_path;
    }

    fn on_script_executed(&mut self, success: bool) {
        let _ = success;
    }

    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        let _ = data;
        OutputAction::Buffer
    }

    fn on_flush(&mut self) {}

    fn on_header(&mut self, name: &str, value: &str) -> bool {
        let _ = (name, value);
        true
    }

    fn on_status(&mut self, code: u16) {
        let _ = code;
    }

    fn on_php_message(&mut self, message: &ExecutionMessage) {
        let _ = message;
    }

    fn is_connection_alive(&self) -> bool {
        true
    }

    fn on_request_finishing(&mut self) {}

    fn on_request_finished(&mut self, result: &ExecutionResult) {
        let _ = result;
    }
}
