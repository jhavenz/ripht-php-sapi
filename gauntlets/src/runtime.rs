use std::path::PathBuf;
use std::time::Duration;

use crate::{GauntletCase, HeaderValue, LifecycleEvent};

pub trait RuntimeAdapter {
    fn name(&self) -> &'static str;
    fn mode(&self) -> RuntimeMode;
    fn execute(&mut self, case: &GauntletCase) -> RuntimeResult;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    RiphtBuffered,
    RiphtStreaming,
    RiphtHooks,
    RiphtSink,
    RiphtSinkWithOptions,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeResult {
    pub runtime: String,
    pub mode: RuntimeMode,
    pub case: String,
    pub status_code: Option<u16>,
    pub exit_status: Option<i32>,
    pub headers: Vec<HeaderValue>,
    pub body: Vec<u8>,
    pub messages: Vec<RuntimeMessage>,
    pub report: Option<ReportMetadata>,
    pub events: Vec<LifecycleEvent>,
    pub duration_ms: u128,
    pub artifact_path: Option<PathBuf>,
    pub failure: Option<RuntimeFailure>,
}

impl RuntimeResult {
    pub fn failure(
        runtime: impl Into<String>,
        mode: RuntimeMode,
        case: impl Into<String>,
        duration: Duration,
        failure: RuntimeFailure,
    ) -> Self {
        Self {
            runtime: runtime.into(),
            mode,
            case: case.into(),
            status_code: None,
            exit_status: None,
            headers: Vec::new(),
            body: Vec::new(),
            messages: Vec::new(),
            report: None,
            events: Vec::new(),
            duration_ms: duration.as_millis(),
            artifact_path: None,
            failure: Some(failure),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeMessage {
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReportMetadata {
    pub status_code: u16,
    pub exit_status: i32,
    pub php_success: bool,
    pub finalized_early: bool,
    pub aborted: bool,
    pub client_closed: bool,
    pub timed_out: bool,
    pub post_finish_duration_ms: Option<u128>,
    pub abort_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RuntimeFailure {
    pub kind: RuntimeFailureKind,
    pub message: String,
}

impl RuntimeFailure {
    pub fn new(kind: RuntimeFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeFailureKind {
    BuildRequest,
    Execute,
    Assertion,
    Artifact,
    Skipped,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SmokeReport {
    pub generated_unix_epoch_secs: u64,
    pub passed: bool,
    pub result: RuntimeResult,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModesReport {
    pub generated_unix_epoch_secs: u64,
    pub passed: bool,
    pub case: String,
    pub results: Vec<RuntimeResult>,
}
