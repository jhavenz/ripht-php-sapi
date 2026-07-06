use std::time::Duration;

use super::{AbortReason, ExecutionMessage};

#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ExecutionReport {
    pub status_code: u16,
    pub exit_status: i32,
    pub php_success: bool,
    pub finalized_early: bool,
    pub aborted: bool,
    pub client_closed: bool,
    pub timed_out: bool,
    pub post_finish_duration: Option<Duration>,
    pub abort_reason: Option<AbortReason>,
    pub messages: Vec<ExecutionMessage>,
}

impl ExecutionReport {
    pub(crate) fn new(parts: ExecutionReportParts) -> Self {
        Self {
            status_code: parts.status_code,
            exit_status: parts.exit_status,
            php_success: parts.php_success,
            finalized_early: parts.finalized_early,
            aborted: parts.aborted,
            client_closed: parts.client_closed,
            timed_out: parts.timed_out,
            post_finish_duration: parts.post_finish_duration,
            abort_reason: parts.abort_reason,
            messages: parts.messages,
        }
    }
}

pub(crate) struct ExecutionReportParts {
    pub(crate) status_code: u16,
    pub(crate) exit_status: i32,
    pub(crate) php_success: bool,
    pub(crate) finalized_early: bool,
    pub(crate) aborted: bool,
    pub(crate) client_closed: bool,
    pub(crate) timed_out: bool,
    pub(crate) post_finish_duration: Option<Duration>,
    pub(crate) abort_reason: Option<AbortReason>,
    pub(crate) messages: Vec<ExecutionMessage>,
}
