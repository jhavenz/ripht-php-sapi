use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use ripht_php_sapi::{
    ExecutionHooks, ExecutionOptions, ExecutionReport, ExecutionResult,
    OutputAction, RiphtSapi, WebRequest,
};

use crate::{
    GauntletCase, HeaderValue, HttpMethod, LifecycleEvent, RecordingSink,
    ReportMetadata, RuntimeAdapter, RuntimeFailure, RuntimeFailureKind,
    RuntimeMessage, RuntimeMode, RuntimeResult,
};

pub struct RiphtBufferedAdapter {
    sapi: RiphtSapi,
}

impl RiphtBufferedAdapter {
    pub fn new() -> Self {
        Self {
            sapi: RiphtSapi::instance(),
        }
    }
}

impl Default for RiphtBufferedAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for RiphtBufferedAdapter {
    fn name(&self) -> &'static str {
        "ripht"
    }

    fn mode(&self) -> RuntimeMode {
        RuntimeMode::RiphtBuffered
    }

    fn execute(&mut self, case: &GauntletCase) -> RuntimeResult {
        let started_at = Instant::now();
        let ctx = match build_request(case) {
            Ok(ctx) => ctx,
            Err(err) => return request_failure(self, case, started_at, err),
        };

        match self.sapi.execute(ctx) {
            Ok(result) => result_runtime(
                self.name(),
                self.mode(),
                case,
                started_at,
                result,
                None,
                None,
            ),
            Err(err) => execute_failure(self, case, started_at, err),
        }
    }
}

pub struct RiphtStreamingAdapter {
    sapi: RiphtSapi,
}

impl RiphtStreamingAdapter {
    pub fn new() -> Self {
        Self {
            sapi: RiphtSapi::instance(),
        }
    }
}

impl Default for RiphtStreamingAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for RiphtStreamingAdapter {
    fn name(&self) -> &'static str {
        "ripht"
    }

    fn mode(&self) -> RuntimeMode {
        RuntimeMode::RiphtStreaming
    }

    fn execute(&mut self, case: &GauntletCase) -> RuntimeResult {
        let started_at = Instant::now();
        let ctx = match build_request(case) {
            Ok(ctx) => ctx,
            Err(err) => return request_failure(self, case, started_at, err),
        };

        let body = Rc::new(RefCell::new(Vec::new()));
        let captured_body = Rc::clone(&body);

        let execution = self
            .sapi
            .execute_streaming(ctx, move |chunk| {
                captured_body
                    .borrow_mut()
                    .extend_from_slice(chunk);
            });

        match execution {
            Ok(result) => result_runtime(
                self.name(),
                self.mode(),
                case,
                started_at,
                result,
                Some(body.borrow().clone()),
                None,
            ),
            Err(err) => execute_failure(self, case, started_at, err),
        }
    }
}

pub struct RiphtHooksAdapter {
    sapi: RiphtSapi,
}

impl RiphtHooksAdapter {
    pub fn new() -> Self {
        Self {
            sapi: RiphtSapi::instance(),
        }
    }
}

impl Default for RiphtHooksAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for RiphtHooksAdapter {
    fn name(&self) -> &'static str {
        "ripht"
    }

    fn mode(&self) -> RuntimeMode {
        RuntimeMode::RiphtHooks
    }

    fn execute(&mut self, case: &GauntletCase) -> RuntimeResult {
        let started_at = Instant::now();
        let ctx = match build_request(case) {
            Ok(ctx) => ctx,
            Err(err) => return request_failure(self, case, started_at, err),
        };

        let hooks = RecordingHooks::default();
        let state = hooks.state();
        let execution = self
            .sapi
            .execute_with_hooks(ctx, hooks);

        match execution {
            Ok(result) => {
                let state = state.borrow();

                result_runtime(
                    self.name(),
                    self.mode(),
                    case,
                    started_at,
                    result,
                    Some(state.body.clone()),
                    Some(state.events.clone()),
                )
            }
            Err(err) => execute_failure(self, case, started_at, err),
        }
    }
}

pub struct RiphtSinkAdapter {
    sapi: RiphtSapi,
}

impl RiphtSinkAdapter {
    pub fn new() -> Self {
        Self {
            sapi: RiphtSapi::instance(),
        }
    }
}

impl Default for RiphtSinkAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for RiphtSinkAdapter {
    fn name(&self) -> &'static str {
        "ripht"
    }

    fn mode(&self) -> RuntimeMode {
        RuntimeMode::RiphtSink
    }

    fn execute(&mut self, case: &GauntletCase) -> RuntimeResult {
        let started_at = Instant::now();
        let ctx = match build_request(case) {
            Ok(ctx) => ctx,
            Err(err) => return request_failure(self, case, started_at, err),
        };

        let sink = RecordingSink::new();
        let events = sink.clone();
        let execution = self
            .sapi
            .execute_with_sink(ctx, sink);

        match execution {
            Ok(report) => report_runtime(
                self.name(),
                self.mode(),
                case,
                started_at,
                report,
                events.events(),
            ),
            Err(err) => execute_failure(self, case, started_at, err),
        }
    }
}

pub struct RiphtSinkWithOptionsAdapter {
    sapi: RiphtSapi,
}

impl RiphtSinkWithOptionsAdapter {
    pub fn new() -> Self {
        Self {
            sapi: RiphtSapi::instance(),
        }
    }
}

impl Default for RiphtSinkWithOptionsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAdapter for RiphtSinkWithOptionsAdapter {
    fn name(&self) -> &'static str {
        "ripht"
    }

    fn mode(&self) -> RuntimeMode {
        RuntimeMode::RiphtSinkWithOptions
    }

    fn execute(&mut self, case: &GauntletCase) -> RuntimeResult {
        let started_at = Instant::now();
        let ctx = match build_request(case) {
            Ok(ctx) => ctx,
            Err(err) => return request_failure(self, case, started_at, err),
        };

        let sink = RecordingSink::new();
        let events = sink.clone();
        let execution = self
            .sapi
            .execute_with_sink_and_options(
                ctx,
                sink,
                ExecutionOptions::default(),
            );

        match execution {
            Ok(report) => report_runtime(
                self.name(),
                self.mode(),
                case,
                started_at,
                report,
                events.events(),
            ),
            Err(err) => execute_failure(self, case, started_at, err),
        }
    }
}

pub fn ripht_mode_adapters() -> Vec<Box<dyn RuntimeAdapter>> {
    vec![
        Box::new(RiphtBufferedAdapter::new()),
        Box::new(RiphtStreamingAdapter::new()),
        Box::new(RiphtHooksAdapter::new()),
        Box::new(RiphtSinkAdapter::new()),
        Box::new(RiphtSinkWithOptionsAdapter::new()),
    ]
}

fn build_request(
    case: &GauntletCase,
) -> Result<ripht_php_sapi::ExecutionContext, String> {
    let mut request = request_builder(case);

    if let Some(body) = &case.body {
        request = request.with_body(body.clone());
    }

    if let Some(content_type) = case.content_type {
        request = request.with_content_type(content_type);
    }

    request
        .build(case.script_path())
        .map_err(|err| err.to_string())
}

fn request_failure(
    adapter: &dyn RuntimeAdapter,
    case: &GauntletCase,
    started_at: Instant,
    message: String,
) -> RuntimeResult {
    RuntimeResult::failure(
        adapter.name(),
        adapter.mode(),
        case.name,
        started_at.elapsed(),
        RuntimeFailure::new(RuntimeFailureKind::BuildRequest, message),
    )
}

fn execute_failure<E: std::fmt::Display>(
    adapter: &dyn RuntimeAdapter,
    case: &GauntletCase,
    started_at: Instant,
    err: E,
) -> RuntimeResult {
    RuntimeResult::failure(
        adapter.name(),
        adapter.mode(),
        case.name,
        started_at.elapsed(),
        RuntimeFailure::new(RuntimeFailureKind::Execute, err.to_string()),
    )
}

fn result_runtime(
    runtime: &str,
    mode: RuntimeMode,
    case: &GauntletCase,
    started_at: Instant,
    result: ExecutionResult,
    body: Option<Vec<u8>>,
    events: Option<Vec<LifecycleEvent>>,
) -> RuntimeResult {
    RuntimeResult {
        runtime: runtime.to_string(),
        mode,
        case: case.name.to_string(),
        status_code: Some(result.status_code()),
        exit_status: Some(result.exit_status()),
        headers: result_headers(&result),
        body: body.unwrap_or_else(|| result.body()),
        messages: result_messages(&result),
        report: None,
        events: events.unwrap_or_default(),
        duration_ms: started_at
            .elapsed()
            .as_millis(),
        artifact_path: None,
        failure: None,
    }
}

fn report_runtime(
    runtime: &str,
    mode: RuntimeMode,
    case: &GauntletCase,
    started_at: Instant,
    report: ExecutionReport,
    events: Vec<LifecycleEvent>,
) -> RuntimeResult {
    RuntimeResult {
        runtime: runtime.to_string(),
        mode,
        case: case.name.to_string(),
        status_code: Some(report.status_code),
        exit_status: Some(report.exit_status),
        headers: event_headers(&events),
        body: event_body(&events),
        messages: report_messages(&report),
        report: Some(report_metadata(&report)),
        events,
        duration_ms: started_at
            .elapsed()
            .as_millis(),
        artifact_path: None,
        failure: None,
    }
}

fn result_headers(result: &ExecutionResult) -> Vec<HeaderValue> {
    result
        .all_headers()
        .map(|header| HeaderValue::new(header.name(), header.value()))
        .collect()
}

fn result_messages(result: &ExecutionResult) -> Vec<RuntimeMessage> {
    result
        .all_messages()
        .map(|message| RuntimeMessage {
            level: message.level.to_string(),
            message: message.message.clone(),
        })
        .collect()
}

fn report_messages(report: &ExecutionReport) -> Vec<RuntimeMessage> {
    report
        .messages
        .iter()
        .map(|message| RuntimeMessage {
            level: message.level.to_string(),
            message: message.message.clone(),
        })
        .collect()
}

fn report_metadata(report: &ExecutionReport) -> ReportMetadata {
    ReportMetadata {
        status_code: report.status_code,
        exit_status: report.exit_status,
        php_success: report.php_success,
        finalized_early: report.finalized_early,
        aborted: report.aborted,
        client_closed: report.client_closed,
        timed_out: report.timed_out,
        post_finish_duration_ms: report
            .post_finish_duration
            .map(|duration| duration.as_millis()),
        abort_reason: report
            .abort_reason
            .map(|reason| format!("{reason:?}")),
    }
}

fn event_headers(events: &[LifecycleEvent]) -> Vec<HeaderValue> {
    events
        .iter()
        .find_map(|event| match event {
            LifecycleEvent::Headers { headers, .. } => Some(headers.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

fn event_body(events: &[LifecycleEvent]) -> Vec<u8> {
    let mut body = Vec::new();

    for event in events {
        if let LifecycleEvent::Write { bytes } = event {
            body.extend_from_slice(bytes);
        }
    }

    body
}

fn request_builder(case: &GauntletCase) -> WebRequest {
    match case.method {
        HttpMethod::Get => WebRequest::get(),
        HttpMethod::Post => WebRequest::post(),
        HttpMethod::Put => WebRequest::put(),
        HttpMethod::Delete => WebRequest::delete(),
        HttpMethod::Patch => WebRequest::patch(),
        HttpMethod::Head => WebRequest::head(),
        HttpMethod::Options => WebRequest::options(),
    }
}

#[derive(Default)]
struct RecordingHooks {
    state: Rc<RefCell<HookState>>,
}

impl RecordingHooks {
    fn state(&self) -> Rc<RefCell<HookState>> {
        Rc::clone(&self.state)
    }
}

impl ExecutionHooks for RecordingHooks {
    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        let mut state = self.state.borrow_mut();

        state
            .body
            .extend_from_slice(data);
        state
            .events
            .push(LifecycleEvent::Write {
                bytes: data.to_vec(),
            });

        OutputAction::Continue
    }

    fn on_flush(&mut self) {
        self.state
            .borrow_mut()
            .events
            .push(LifecycleEvent::Flush);
    }
}

#[derive(Default)]
struct HookState {
    body: Vec<u8>,
    events: Vec<LifecycleEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ripht_mode_adapters_cover_all_modes() {
        let modes: Vec<_> = ripht_mode_adapters()
            .into_iter()
            .map(|adapter| adapter.mode())
            .collect();

        assert_eq!(
            modes,
            vec![
                RuntimeMode::RiphtBuffered,
                RuntimeMode::RiphtStreaming,
                RuntimeMode::RiphtHooks,
                RuntimeMode::RiphtSink,
                RuntimeMode::RiphtSinkWithOptions,
            ]
        );
    }

    #[test]
    fn body_is_reconstructed_from_write_events() {
        let events = vec![
            LifecycleEvent::Write {
                bytes: b"alpha".to_vec(),
            },
            LifecycleEvent::Flush,
            LifecycleEvent::Write {
                bytes: b"omega".to_vec(),
            },
        ];

        assert_eq!(event_body(&events), b"alphaomega");
    }
}
