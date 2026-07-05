use super::{AbortReason, ExecutionMessage};

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub status_code: u16,
    pub exit_status: i32,
    pub php_success: bool,
    pub finalized_early: bool,
    pub aborted: bool,
    pub abort_reason: Option<AbortReason>,
    pub messages: Vec<ExecutionMessage>,
}

impl ExecutionReport {
    pub(crate) fn new(
        status_code: u16,
        exit_status: i32,
        php_success: bool,
        finalized_early: bool,
        aborted: bool,
        abort_reason: Option<AbortReason>,
        messages: Vec<ExecutionMessage>,
    ) -> Self {
        Self {
            status_code,
            exit_status,
            php_success,
            finalized_early,
            aborted,
            abort_reason,
            messages,
        }
    }
}
