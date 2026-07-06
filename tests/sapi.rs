use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ripht_php_sapi::{
    AbortReason, ExecutionContext, ExecutionControl, ExecutionError,
    ExecutionHooks, ExecutionOptions, OutputAction, ResponseHeader,
    ResponseSink, RiphtSapi, SinkResult, WebRequest,
};

fn php_script_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/php_scripts")
        .join(name)
}

fn sidecar_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "ripht-{}-{}.json",
        std::process::id(),
        name
    ))
}

fn execute_streaming_collect(
    php: &RiphtSapi,
    exec: ExecutionContext,
) -> (ripht_php_sapi::ExecutionResult, Vec<u8>) {
    let chunks = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
    let chunks_clone = Arc::clone(&chunks);

    let result = php
        .execute_streaming(exec, move |chunk| {
            chunks_clone
                .lock()
                .unwrap()
                .push(chunk.to_vec());
        })
        .expect("streaming request execution failed");

    let body = chunks
        .lock()
        .unwrap()
        .iter()
        .flatten()
        .copied()
        .collect();

    (result, body)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SinkEvent {
    Headers(u16, Vec<(String, String)>),
    Write(String),
    Flush,
    Finish { marker_exists: Option<bool> },
    Abort(AbortReason),
}

struct RecordingSink {
    events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
    marker_path: Option<PathBuf>,
    write_result: SinkResult,
    finish_result: SinkResult,
    finished: bool,
}

impl RecordingSink {
    fn new(events: Arc<std::sync::Mutex<Vec<SinkEvent>>>) -> Self {
        Self {
            events,
            marker_path: None,
            write_result: SinkResult::Continue,
            finish_result: SinkResult::Continue,
            finished: false,
        }
    }

    fn with_write_result(
        events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
        write_result: SinkResult,
    ) -> Self {
        Self {
            events,
            marker_path: None,
            write_result,
            finish_result: SinkResult::Continue,
            finished: false,
        }
    }

    fn with_finish_result(
        events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
        finish_result: SinkResult,
    ) -> Self {
        Self {
            events,
            marker_path: None,
            write_result: SinkResult::Continue,
            finish_result,
            finished: false,
        }
    }

    fn with_marker_probe(
        events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
        marker_path: PathBuf,
    ) -> Self {
        Self {
            events,
            marker_path: Some(marker_path),
            write_result: SinkResult::Continue,
            finish_result: SinkResult::Continue,
            finished: false,
        }
    }
}

impl ResponseSink for RecordingSink {
    fn send_headers(
        &mut self,
        status: u16,
        headers: &[ResponseHeader],
    ) -> SinkResult {
        let headers = headers
            .iter()
            .map(|header| {
                (header.name().to_string(), header.value().to_string())
            })
            .collect();

        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Headers(status, headers));

        SinkResult::Continue
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Write(
                String::from_utf8_lossy(bytes).into_owned(),
            ));

        self.write_result
    }

    fn flush(&mut self) -> SinkResult {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Flush);

        SinkResult::Continue
    }

    fn finish(&mut self) -> SinkResult {
        self.finished = true;

        let marker_exists = self
            .marker_path
            .as_ref()
            .map(|path| path.exists());

        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Finish { marker_exists });

        self.finish_result
    }

    fn abort(&mut self, reason: AbortReason) {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Abort(reason));
    }

    fn is_finished(&self) -> bool {
        self.finished
    }
}

struct CancellingSink {
    events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
    control: Arc<ExecutionControl>,
    cancel_on_finish: bool,
    finished: bool,
}

struct DeadliningSink {
    events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
    control: Arc<ExecutionControl>,
    finished: bool,
}

impl DeadliningSink {
    fn new(
        events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
        control: Arc<ExecutionControl>,
    ) -> Self {
        Self {
            events,
            control,
            finished: false,
        }
    }
}

struct ClosingCancellingSink {
    events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
    control: Arc<ExecutionControl>,
}

impl ClosingCancellingSink {
    fn new(
        events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
        control: Arc<ExecutionControl>,
    ) -> Self {
        Self { events, control }
    }
}

impl CancellingSink {
    fn new(
        events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
        control: Arc<ExecutionControl>,
    ) -> Self {
        Self {
            events,
            control,
            cancel_on_finish: false,
            finished: false,
        }
    }

    fn on_finish(
        events: Arc<std::sync::Mutex<Vec<SinkEvent>>>,
        control: Arc<ExecutionControl>,
    ) -> Self {
        Self {
            events,
            control,
            cancel_on_finish: true,
            finished: false,
        }
    }
}

impl ResponseSink for CancellingSink {
    fn send_headers(
        &mut self,
        status: u16,
        headers: &[ResponseHeader],
    ) -> SinkResult {
        let headers = headers
            .iter()
            .map(|header| {
                (header.name().to_string(), header.value().to_string())
            })
            .collect();

        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Headers(status, headers));

        SinkResult::Continue
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Write(
                String::from_utf8_lossy(bytes).into_owned(),
            ));

        if !self.cancel_on_finish {
            self.control.cancel();
        }

        SinkResult::Continue
    }

    fn flush(&mut self) -> SinkResult {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Flush);

        SinkResult::Continue
    }

    fn finish(&mut self) -> SinkResult {
        self.finished = true;
        if self.cancel_on_finish {
            self.control.cancel();
        }
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Finish {
                marker_exists: None,
            });

        SinkResult::Continue
    }

    fn abort(&mut self, reason: AbortReason) {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Abort(reason));
    }

    fn is_finished(&self) -> bool {
        self.finished
    }
}

impl ResponseSink for DeadliningSink {
    fn send_headers(
        &mut self,
        status: u16,
        headers: &[ResponseHeader],
    ) -> SinkResult {
        let headers = headers
            .iter()
            .map(|header| {
                (header.name().to_string(), header.value().to_string())
            })
            .collect();

        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Headers(status, headers));

        SinkResult::Continue
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Write(
                String::from_utf8_lossy(bytes).into_owned(),
            ));

        SinkResult::Continue
    }

    fn flush(&mut self) -> SinkResult {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Flush);

        SinkResult::Continue
    }

    fn finish(&mut self) -> SinkResult {
        self.finished = true;
        self.control
            .set_deadline(Instant::now() - Duration::from_secs(1));
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Finish {
                marker_exists: None,
            });

        SinkResult::Continue
    }

    fn abort(&mut self, reason: AbortReason) {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Abort(reason));
    }

    fn is_finished(&self) -> bool {
        self.finished
    }
}

impl ResponseSink for ClosingCancellingSink {
    fn send_headers(
        &mut self,
        status: u16,
        headers: &[ResponseHeader],
    ) -> SinkResult {
        let headers = headers
            .iter()
            .map(|header| {
                (header.name().to_string(), header.value().to_string())
            })
            .collect();

        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Headers(status, headers));

        SinkResult::Continue
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Write(
                String::from_utf8_lossy(bytes).into_owned(),
            ));
        self.control.cancel();

        SinkResult::Closed
    }

    fn flush(&mut self) -> SinkResult {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Flush);

        SinkResult::Continue
    }

    fn finish(&mut self) -> SinkResult {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Finish {
                marker_exists: None,
            });

        SinkResult::Continue
    }

    fn abort(&mut self, reason: AbortReason) {
        self.events
            .lock()
            .unwrap()
            .push(SinkEvent::Abort(reason));
    }

    fn is_finished(&self) -> bool {
        false
    }
}

struct OutputCaptureHooks {
    outputs: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
    action: OutputAction,
}

impl ExecutionHooks for OutputCaptureHooks {
    fn on_output(&mut self, data: &[u8]) -> OutputAction {
        self.outputs
            .lock()
            .unwrap()
            .push(data.to_vec());

        self.action
    }
}

struct FlushCaptureHooks {
    flushes: Arc<std::sync::Mutex<usize>>,
}

impl ExecutionHooks for FlushCaptureHooks {
    fn on_flush(&mut self) {
        let mut flushes = self.flushes.lock().unwrap();

        *flushes += 1;
    }
}

#[test]
fn execute_hello_php() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php.execute(exec);

    match result {
        Ok(resp) => {
            assert!(resp
                .body_string()
                .contains("Hello"));
        }
        Err(e) => {
            panic!("Failed to execute script: {}", e);
        }
    }
}

#[test]
fn fastcgi_finish_request_finalizes_response_once() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish.php");
    let result_path = std::env::temp_dir().join(format!(
        "ripht-fastcgi-finish-{}-{}.json",
        std::process::id(),
        "finalizes-response-once"
    ));
    let _ = std::fs::remove_file(&result_path);

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_FINISH_RESULT",
            result_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("fastcgi_finish.php execution failed");

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("finalized response body should be valid JSON");

    assert_eq!(json["available"], true);
    assert_eq!(json["pre"], true);
    assert!(result
        .body_string()
        .contains("available"));
    assert!(!result
        .body_string()
        .contains("after"));
    assert_eq!(result.header_val("X-Ripht-Finalized"), Some("yes"));

    let returns: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&result_path)
            .expect("fastcgi_finish.php should write return-value sidecar"),
    )
    .expect("return-value sidecar should be valid JSON");

    assert_eq!(returns["first"], true);
    assert_eq!(returns["second"], false);

    let _ = std::fs::remove_file(result_path);
}

#[test]
fn fastcgi_finish_request_continues_after_finish() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish_marker.php");
    let marker_path = std::env::temp_dir().join(format!(
        "ripht-fastcgi-finish-{}-{}.json",
        std::process::id(),
        "continues-after-finish"
    ));
    let _ = std::fs::remove_file(&marker_path);

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_MARKER_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("fastcgi_finish_marker.php execution failed");

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("finalized response body should be valid JSON");
    assert_eq!(json["pre"], true);

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path)
            .expect("fastcgi_finish_marker.php should write marker sidecar"),
    )
    .expect("marker sidecar should be valid JSON");

    assert_eq!(marker["finished"], true);
    assert_eq!(marker["marker"], "after-finish");

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn fastcgi_finish_request_discards_late_output_and_headers() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish_late_output.php");
    let marker_path = std::env::temp_dir().join(format!(
        "ripht-fastcgi-finish-{}-{}.json",
        std::process::id(),
        "late-output"
    ));
    let _ = std::fs::remove_file(&marker_path);

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_LATE_OUTPUT_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("fastcgi_finish_late_output.php execution failed");

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("finalized response body should be valid JSON");
    assert_eq!(json["before"], true);
    assert!(!result
        .body_string()
        .contains("after"));
    assert_eq!(result.header_val("X-Ripht-Before-Finish"), Some("yes"));
    assert_eq!(result.header_val("X-Ripht-After-Finish"), None);

    let marker: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&marker_path).expect(
            "fastcgi_finish_late_output.php should write marker sidecar",
        ))
        .expect("marker sidecar should be valid JSON");

    assert_eq!(marker["finished"], true);
    assert_eq!(marker["marker"], "late-output-complete");

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn fastcgi_finish_request_finalizes_output_handlers() {
    let php = RiphtSapi::instance();
    let script_path =
        php_script_path("fastcgi_finish_final_output_handler.php");
    let marker_path = std::env::temp_dir().join(format!(
        "ripht-fastcgi-finish-{}-{}.json",
        std::process::id(),
        "final-output-handler"
    ));
    let _ = std::fs::remove_file(&marker_path);

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_FINAL_HANDLER_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("fastcgi_finish_final_output_handler.php execution failed");

    assert_eq!(result.body_string(), "before|final");

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path).expect(
            "fastcgi_finish_final_output_handler.php should write marker sidecar",
        ),
    )
    .expect("marker sidecar should be valid JSON");

    assert_eq!(marker["finished"], true);
    assert_eq!(marker["marker"], "final-handler-complete");

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn duplicate_fastcgi_finish_does_not_drain_new_buffers() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish_duplicate_buffers.php");
    let marker_path = sidecar_path("fastcgi-finish-duplicate-buffers");
    let _ = std::fs::remove_file(&marker_path);

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_FINISH_RESULT",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("fastcgi_finish_duplicate_buffers.php execution failed");

    assert_eq!(result.body_string(), "before");

    let marker: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&marker_path).expect(
            "fastcgi_finish_duplicate_buffers.php should write marker sidecar",
        ))
        .expect("marker sidecar should contain valid JSON");

    assert_eq!(marker["first"], true);
    assert_eq!(marker["second"], false);
    assert_eq!(marker["post_finish_buffer"], "after");

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn host_sink_observes_headers_body_flush_finish_in_order() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink(exec, RecordingSink::new(Arc::clone(&events)))
        .expect("execute_with_sink() failed");

    assert_eq!(report.status_code, 200);
    assert!(report.php_success);
    assert!(!report.finalized_early);
    assert!(!report.aborted);
    assert!(!report.client_closed);
    assert!(!report.timed_out);
    assert_eq!(report.post_finish_duration, None);
    assert_eq!(report.abort_reason, None);

    let events = events.lock().unwrap();

    assert!(matches!(events.first(), Some(SinkEvent::Headers(200, _))));
    assert!(events
        .iter()
        .any(|event| matches!(event, SinkEvent::Flush)));
    assert!(matches!(events.last(), Some(SinkEvent::Finish { .. })));

    let body = events
        .iter()
        .filter_map(|event| match event {
            SinkEvent::Write(bytes) => Some(bytes.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert_eq!(body, "alphaomega");
}

#[test]
fn host_sink_observes_finish_before_post_finish_marker() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish_marker.php");
    let marker_path = std::env::temp_dir().join(format!(
        "ripht-fastcgi-finish-{}-{}.json",
        std::process::id(),
        "sink-finish-before-marker"
    ));
    let _ = std::fs::remove_file(&marker_path);

    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_MARKER_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink(
            exec,
            RecordingSink::with_marker_probe(
                Arc::clone(&events),
                marker_path.clone(),
            ),
        )
        .expect("execute_with_sink() failed");

    assert!(report.finalized_early);

    let events = events.lock().unwrap();
    assert!(events.iter().any(|event| {
        matches!(
            event,
            SinkEvent::Finish {
                marker_exists: Some(false)
            }
        )
    }));

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path)
            .expect("fastcgi_finish_marker.php should write marker sidecar"),
    )
    .expect("marker sidecar should be valid JSON");

    assert_eq!(marker["marker"], "after-finish");

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn execute_with_sink_reports_early_finish_and_final_output() {
    let php = RiphtSapi::instance();
    let script_path =
        php_script_path("fastcgi_finish_final_output_handler.php");
    let marker_path = std::env::temp_dir().join(format!(
        "ripht-fastcgi-finish-{}-{}.json",
        std::process::id(),
        "sink-final-output-handler"
    ));
    let _ = std::fs::remove_file(&marker_path);

    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_FINAL_HANDLER_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink(exec, RecordingSink::new(Arc::clone(&events)))
        .expect("execute_with_sink() failed");

    assert!(report.php_success);
    assert!(report.finalized_early);
    assert!(!report.aborted);
    assert!(!report.timed_out);
    assert!(report
        .post_finish_duration
        .is_some());

    let events = events.lock().unwrap();
    let body = events
        .iter()
        .filter_map(|event| match event {
            SinkEvent::Write(bytes) => Some(bytes.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert_eq!(body, "before|final");
    assert!(matches!(events.last(), Some(SinkEvent::Finish { .. })));

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn host_sink_discards_late_output_and_headers_after_finish() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish_late_output.php");
    let marker_path = std::env::temp_dir().join(format!(
        "ripht-fastcgi-finish-{}-{}.json",
        std::process::id(),
        "sink-late-output"
    ));
    let _ = std::fs::remove_file(&marker_path);

    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_LATE_OUTPUT_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink(exec, RecordingSink::new(Arc::clone(&events)))
        .expect("execute_with_sink() failed");

    assert!(report.finalized_early);
    assert!(report
        .post_finish_duration
        .is_some());

    let events = events.lock().unwrap();
    let body = events
        .iter()
        .filter_map(|event| match event {
            SinkEvent::Write(bytes) => Some(bytes.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert!(body.contains("\"before\":true"));
    assert!(!body.contains("after"));

    let header_names = events
        .iter()
        .flat_map(|event| match event {
            SinkEvent::Headers(_, headers) => headers.as_slice(),
            _ => &[],
        })
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();

    assert!(header_names.contains(&"X-Ripht-Before-Finish"));
    assert!(!header_names.contains(&"X-Ripht-After-Finish"));

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn execute_with_sink_preserves_php_messages() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("errors.php");
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink(exec, RecordingSink::new(Arc::clone(&events)))
        .expect("execute_with_sink() failed");

    assert!(report.php_success);
    assert!(!report.messages.is_empty());
    assert!(!report.finalized_early);
    assert!(!report.aborted);
    assert!(!report.client_closed);
    assert!(!report.timed_out);
    assert_eq!(report.post_finish_duration, None);
}

#[test]
fn host_sink_closed_write_reports_client_closed_and_aborts_sink() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let control = Arc::new(ExecutionControl::default());

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink_and_options(
            exec,
            RecordingSink::with_write_result(
                Arc::clone(&events),
                SinkResult::Closed,
            ),
            ExecutionOptions::with_control(Arc::clone(&control)),
        )
        .expect("execute_with_sink_and_options() failed");

    assert!(report.php_success);
    assert!(!report.aborted);
    assert!(report.client_closed);
    assert!(!report.timed_out);
    assert_eq!(report.post_finish_duration, None);
    assert_eq!(report.abort_reason, None);
    assert!(control.is_client_closed());

    let events = events.lock().unwrap();
    assert!(matches!(
        events.as_slice(),
        [
            SinkEvent::Headers(200, _),
            SinkEvent::Write(_),
            SinkEvent::Abort(AbortReason::ClientClosed)
        ]
    ));
    assert!(!events
        .iter()
        .any(|event| matches!(event, SinkEvent::Finish { .. })));
}

#[test]
fn execute_with_sink_and_options_uses_default_options() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink_and_options(
            exec,
            RecordingSink::new(Arc::clone(&events)),
            ExecutionOptions::default(),
        )
        .expect("execute_with_sink_and_options() failed");

    assert!(report.php_success);
    assert!(!report.aborted);
    assert!(!report.client_closed);
    assert!(!report.timed_out);
    assert_eq!(report.post_finish_duration, None);
    assert_eq!(report.abort_reason, None);
    assert!(events
        .lock()
        .unwrap()
        .iter()
        .any(|event| matches!(event, SinkEvent::Finish { .. })));
}

#[test]
fn reused_execution_control_returns_error_before_sink_delivery() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");
    let first_events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let second_events =
        Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let control = Arc::new(ExecutionControl::default());

    let first_exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build first WebRequest");

    let first_report = php
        .execute_with_sink_and_options(
            first_exec,
            RecordingSink::new(Arc::clone(&first_events)),
            ExecutionOptions::with_control(Arc::clone(&control)),
        )
        .expect("first execute_with_sink_and_options() failed");

    assert!(first_report.php_success);

    let second_exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build second WebRequest");

    let error = php
        .execute_with_sink_and_options(
            second_exec,
            RecordingSink::new(Arc::clone(&second_events)),
            ExecutionOptions::with_control(Arc::clone(&control)),
        )
        .expect_err("reused ExecutionControl should fail");

    assert!(matches!(error, ExecutionError::ControlAlreadyUsed));
    assert!(second_events
        .lock()
        .unwrap()
        .is_empty());
}

#[test]
fn script_not_found_does_not_consume_execution_control() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");
    let missing_path = php_script_path("missing-control-preflight.php");
    let missing_events =
        Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let valid_events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let control = Arc::new(ExecutionControl::default());

    let missing_error = php
        .execute_with_sink_and_options(
            ExecutionContext::script(&missing_path),
            RecordingSink::new(Arc::clone(&missing_events)),
            ExecutionOptions::with_control(Arc::clone(&control)),
        )
        .expect_err("missing script should fail before claiming control");

    assert!(matches!(missing_error, ExecutionError::ScriptNotFound(_)));
    assert!(missing_events
        .lock()
        .unwrap()
        .is_empty());

    let valid_exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build valid WebRequest");

    let report = php
        .execute_with_sink_and_options(
            valid_exec,
            RecordingSink::new(Arc::clone(&valid_events)),
            ExecutionOptions::with_control(control),
        )
        .expect("valid request should still be able to use control");

    assert!(report.php_success);
    assert!(valid_events
        .lock()
        .unwrap()
        .iter()
        .any(|event| matches!(event, SinkEvent::Finish { .. })));
}

#[test]
fn delivery_callback_can_cancel_request() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("control_probe.php");
    let marker_path = sidecar_path("delivery-callback-can-cancel-request");
    let _ = std::fs::remove_file(&marker_path);
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let control = Arc::new(ExecutionControl::default());

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_CONTROL_SHUTDOWN_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink_and_options(
            exec,
            CancellingSink::new(Arc::clone(&events), Arc::clone(&control)),
            ExecutionOptions::with_control(Arc::clone(&control)),
        )
        .expect("execute_with_sink_and_options() failed");

    assert!(report.php_success);
    assert!(report.aborted);
    assert!(!report.client_closed);
    assert!(!report.timed_out);
    assert_eq!(report.post_finish_duration, None);
    assert_eq!(report.abort_reason, Some(AbortReason::HostAbort));
    assert!(control.is_cancelled());
    assert_eq!(
        std::fs::read_to_string(&marker_path)
            .expect("control_probe.php should write shutdown sidecar"),
        "shutdown"
    );

    let events = events.lock().unwrap();
    assert!(events.iter().any(
        |event| matches!(event, SinkEvent::Write(body) if body == "alpha")
    ));
    assert!(events
        .iter()
        .any(|event| matches!(
            event,
            SinkEvent::Abort(AbortReason::HostAbort)
        )));
    assert!(!events.iter().any(
        |event| matches!(event, SinkEvent::Write(body) if body == "omega")
    ));

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn deadline_exceeded_sets_deadline_abort_reason() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let control = Arc::new(ExecutionControl::default());

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink_and_options(
            exec,
            RecordingSink::new(Arc::clone(&events)),
            ExecutionOptions::with_control(Arc::clone(&control))
                .deadline(Instant::now() - Duration::from_secs(1)),
        )
        .expect("execute_with_sink_and_options() failed");

    assert!(report.php_success);
    assert!(report.aborted);
    assert!(!report.client_closed);
    assert!(report.timed_out);
    assert_eq!(report.post_finish_duration, None);
    assert_eq!(report.abort_reason, Some(AbortReason::DeadlineExceeded));
    assert!(control.is_deadline_exceeded());

    let events = events.lock().unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(
            event,
            SinkEvent::Abort(AbortReason::DeadlineExceeded)
        )));
    assert!(!events
        .iter()
        .any(|event| matches!(event, SinkEvent::Write(_))));
}

#[test]
fn cancel_or_deadline_does_not_skip_request_shutdown() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("control_probe.php");
    let marker_path = sidecar_path("deadline-does-not-skip-shutdown");
    let _ = std::fs::remove_file(&marker_path);
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_CONTROL_SHUTDOWN_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink_and_options(
            exec,
            RecordingSink::new(Arc::clone(&events)),
            ExecutionOptions::with_deadline(
                Instant::now() - Duration::from_secs(1),
            ),
        )
        .expect("execute_with_sink_and_options() failed");

    assert!(report.php_success);
    assert!(report.aborted);
    assert!(!report.client_closed);
    assert!(report.timed_out);
    assert_eq!(report.post_finish_duration, None);
    assert_eq!(report.abort_reason, Some(AbortReason::DeadlineExceeded));
    assert_eq!(
        std::fs::read_to_string(&marker_path)
            .expect("control_probe.php should write shutdown sidecar"),
        "shutdown"
    );

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn post_finish_host_cancel_reports_abort_reason() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish_marker.php");
    let marker_path = sidecar_path("post-finish-host-cancel");
    let _ = std::fs::remove_file(&marker_path);
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let control = Arc::new(ExecutionControl::default());

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_MARKER_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink_and_options(
            exec,
            CancellingSink::on_finish(
                Arc::clone(&events),
                Arc::clone(&control),
            ),
            ExecutionOptions::with_control(Arc::clone(&control)),
        )
        .expect("execute_with_sink_and_options() failed");

    assert!(report.php_success);
    assert!(report.finalized_early);
    assert!(report.aborted);
    assert!(!report.client_closed);
    assert!(!report.timed_out);
    assert!(report
        .post_finish_duration
        .is_some());
    assert_eq!(report.abort_reason, Some(AbortReason::HostAbort));
    assert!(control.is_cancelled());

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path)
            .expect("fastcgi_finish_marker.php should write marker sidecar"),
    )
    .expect("marker sidecar should be valid JSON");

    assert_eq!(marker["finished"], true);
    assert_eq!(marker["marker"], "after-finish");

    let events = events.lock().unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(event, SinkEvent::Finish { .. })));
    assert!(!events
        .iter()
        .any(|event| matches!(event, SinkEvent::Abort(_))));

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn post_finish_deadline_reports_deadline_reason() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish_marker.php");
    let marker_path = sidecar_path("post-finish-deadline");
    let _ = std::fs::remove_file(&marker_path);
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let control = Arc::new(ExecutionControl::default());

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_MARKER_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink_and_options(
            exec,
            DeadliningSink::new(Arc::clone(&events), Arc::clone(&control)),
            ExecutionOptions::with_control(Arc::clone(&control)),
        )
        .expect("execute_with_sink_and_options() failed");

    assert!(report.php_success);
    assert!(report.finalized_early);
    assert!(report.aborted);
    assert!(!report.client_closed);
    assert!(report.timed_out);
    assert!(report
        .post_finish_duration
        .is_some());
    assert_eq!(report.abort_reason, Some(AbortReason::DeadlineExceeded));
    assert!(control.is_deadline_exceeded());

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path)
            .expect("fastcgi_finish_marker.php should write marker sidecar"),
    )
    .expect("marker sidecar should be valid JSON");

    assert_eq!(marker["finished"], true);
    assert_eq!(marker["marker"], "after-finish");

    let events = events.lock().unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(event, SinkEvent::Finish { .. })));
    assert!(!events
        .iter()
        .any(|event| matches!(event, SinkEvent::Abort(_))));

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn client_closed_then_host_cancel_preserves_both_states() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));
    let control = Arc::new(ExecutionControl::default());

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink_and_options(
            exec,
            ClosingCancellingSink::new(
                Arc::clone(&events),
                Arc::clone(&control),
            ),
            ExecutionOptions::with_control(Arc::clone(&control)),
        )
        .expect("execute_with_sink_and_options() failed");

    assert!(report.php_success);
    assert!(report.client_closed);
    assert!(report.aborted);
    assert!(!report.timed_out);
    assert_eq!(report.post_finish_duration, None);
    assert_eq!(report.abort_reason, Some(AbortReason::HostAbort));
    assert!(control.is_cancelled());

    let events = events.lock().unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(
            event,
            SinkEvent::Abort(AbortReason::ClientClosed)
        )));
}

#[test]
fn host_sink_abort_finish_reports_sink_failure_and_aborts_sink() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");
    let events = Arc::new(std::sync::Mutex::new(Vec::<SinkEvent>::new()));

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let report = php
        .execute_with_sink(
            exec,
            RecordingSink::with_finish_result(
                Arc::clone(&events),
                SinkResult::Abort,
            ),
        )
        .expect("execute_with_sink() failed");

    assert!(report.php_success);
    assert!(report.aborted);
    assert_eq!(report.abort_reason, Some(AbortReason::SinkFailure));

    let events = events.lock().unwrap();
    assert!(matches!(events.first(), Some(SinkEvent::Headers(200, _))));
    assert!(events
        .iter()
        .any(|event| matches!(event, SinkEvent::Finish { .. })));
    assert!(matches!(
        events.last(),
        Some(SinkEvent::Abort(AbortReason::SinkFailure))
    ));
}

#[test]
fn execute_streaming_fastcgi_finish_delivers_pre_finish_output() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish.php");
    let marker_path = sidecar_path("streaming-fastcgi-finish");
    let _ = std::fs::remove_file(&marker_path);

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_FINISH_RESULT",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let (result, body) = execute_streaming_collect(&php, exec);
    let body = String::from_utf8_lossy(&body);

    assert_eq!(result.status_code(), 200);
    assert!(result.body().is_empty());
    assert!(body.contains("\"available\":true"));
    assert!(body.contains("\"pre\":true"));
    assert!(!body.contains("after"));
    assert_eq!(result.header_val("X-Ripht-Finalized"), Some("yes"));

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path)
            .expect("fastcgi_finish.php should write result sidecar"),
    )
    .expect("finish result sidecar should be valid JSON");

    assert_eq!(marker["first"], true);
    assert_eq!(marker["second"], false);

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn execute_streaming_fastcgi_finish_discards_late_output_and_headers() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish_late_output.php");
    let marker_path = sidecar_path("streaming-late-output");
    let _ = std::fs::remove_file(&marker_path);

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_LATE_OUTPUT_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let (result, body) = execute_streaming_collect(&php, exec);
    let body = String::from_utf8_lossy(&body);

    assert!(result.body().is_empty());
    assert!(body.contains("\"before\":true"));
    assert!(!body.contains("after"));
    assert!(result
        .header_val("X-Ripht-Before-Finish")
        .is_some());
    assert!(result
        .header_val("X-Ripht-After-Finish")
        .is_none());

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path)
            .expect("late-output fixture should write marker sidecar"),
    )
    .expect("late-output sidecar should be valid JSON");

    assert_eq!(marker["finished"], true);
    assert_eq!(marker["marker"], "late-output-complete");

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn execute_streaming_fastcgi_finish_finalizes_output_handlers() {
    let php = RiphtSapi::instance();
    let script_path =
        php_script_path("fastcgi_finish_final_output_handler.php");
    let marker_path = sidecar_path("streaming-final-output-handler");
    let _ = std::fs::remove_file(&marker_path);

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_FINAL_HANDLER_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let (result, body) = execute_streaming_collect(&php, exec);

    assert!(result.body().is_empty());
    assert_eq!(String::from_utf8_lossy(&body), "before|final");

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path)
            .expect("final-handler fixture should write marker sidecar"),
    )
    .expect("final-handler sidecar should be valid JSON");

    assert_eq!(marker["finished"], true);
    assert_eq!(marker["marker"], "final-handler-complete");

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn execute_streaming_sink_events_flush_does_not_finish() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let (result, body) = execute_streaming_collect(&php, exec);

    assert_eq!(result.status_code(), 200);
    assert!(result.body().is_empty());
    assert_eq!(String::from_utf8_lossy(&body), "alphaomega");
}

#[test]
fn execute_remains_buffered_compatible_after_sink_api() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("hello.php execution failed");

    assert_eq!(result.status_code(), 200);
    assert!(result
        .body_string()
        .contains("Hello"));
}

#[test]
fn post_request_works() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("post_form.php");

    let exec = WebRequest::post()
        .with_content_type("application/x-www-form-urlencoded")
        .with_body(b"name=Jane%20Doe&email=jane%40example.com".to_vec())
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("POST request execution failed");

    assert_eq!(result.status_code(), 200);

    let json: serde_json::Value = serde_json::from_str(&result.body_string())
        .expect("failed to parse response body as JSON");

    assert_eq!(json["method"], "POST");
}

#[test]
fn stress_sequential_requests() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    for i in 0..1000 {
        let exec = WebRequest::get()
            .with_uri(format!("/?i={}", i))
            .build(&script_path)
            .expect("failed to build WebRequest");

        let result = php
            .execute(exec)
            .unwrap_or_else(|_| panic!("request {} execution failed", i));

        assert_eq!(
            result.status_code(),
            200,
            "Request {} had non-200 status",
            i
        );
    }
}

#[test]
fn stress_large_output() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("large_output.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("large output request execution failed");

    assert!(
        result.body().len() >= 1024 * 1024,
        "Expected 1MB+ output, got {} bytes",
        result.body().len()
    );
}

#[test]
fn stress_mixed_methods() {
    let php = RiphtSapi::instance();

    let get_script = php_script_path("get_params.php");
    let post_script = php_script_path("post_form.php");

    for i in 0..500 {
        if i % 2 == 0 {
            let exec = WebRequest::get()
                .with_uri(format!("/?i={}", i))
                .build(&get_script)
                .expect("failed to build GET WebRequest");

            let result = php
                .execute(exec)
                .unwrap_or_else(|_| {
                    panic!("GET request {} execution failed", i)
                });

            assert_eq!(result.status_code(), 200);
        } else {
            let exec = WebRequest::post()
                .with_uri(format!("/post?i={}", i))
                .with_content_type("application/x-www-form-urlencoded")
                .with_body(b"name=test&value=123".to_vec())
                .build(&post_script)
                .expect("failed to build POST WebRequest");

            let result = php
                .execute(exec)
                .unwrap_or_else(|_| {
                    panic!("POST request {} execution failed", i)
                });

            assert_eq!(result.status_code(), 200);
        }
    }
}

#[test]
fn test_context_isolation_between_requests() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("server_vars.php");

    for i in 0..5 {
        let exec = WebRequest::get()
            .with_uri(format!("/test?request={}", i))
            .with_server_name(format!("server{}", i))
            .with_remote_addr(format!("127.0.0.{}", i))
            .build(&script_path)
            .expect("failed to build WebRequest");

        let result = php
            .execute(exec)
            .unwrap_or_else(|_| panic!("request {} execution failed", i));

        assert_eq!(result.status_code(), 200);

        let json: serde_json::Value =
            serde_json::from_str(&result.body_string())
                .expect("failed to parse response body as JSON");
        assert_eq!(
            json["server_name"],
            format!("server{}", i),
            "Request {} should have correct server_name",
            i
        );
    }
}

#[test]
fn test_cstring_pointer_validity_during_execution() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("post_form.php");

    let query_string = "foo=bar&baz=qux";
    let exec = WebRequest::post()
        .with_uri(format!("/test?{}", query_string))
        .with_content_type("application/x-www-form-urlencoded")
        .with_body(b"name=test".to_vec())
        .with_raw_cookie_header("session=abc123")
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("POST request execution failed");

    assert_eq!(result.status_code(), 200);
}

#[test]
fn test_post_data_bounds_with_real_script() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("large_input.php");

    let test_sizes = vec![0, 1, 100, 1024, 10 * 1024, 100 * 1024, 1024 * 1024];

    for size in test_sizes {
        let post_data = vec![b'x'; size];
        let exec = WebRequest::post()
            .with_content_type("application/octet-stream")
            .with_body(post_data)
            .build(&script_path)
            .expect("failed to build WebRequest");

        let result = php
            .execute(exec)
            .unwrap_or_else(|_| {
                panic!("request with {} bytes execution failed", size)
            });

        assert_eq!(result.status_code(), 200);

        let json: serde_json::Value =
            serde_json::from_str(&result.body_string())
                .expect("failed to parse response body as JSON");

        assert_eq!(
            json["input_length"]
                .as_u64()
                .unwrap() as usize,
            size,
            "Input length should match for size {}",
            size
        );
    }
}

#[test]
fn test_header_parsing_with_real_php_headers() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("headers.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("headers.php request execution failed");

    assert_eq!(result.status_code(), 200);

    let content_type = result.header_val("Content-Type");
    if content_type.is_some() {
        assert_eq!(
            content_type,
            Some("application/json"),
            "Content-Type should be application/json if present"
        );
    }

    let has_custom_header = result
        .header_val("X-Custom-Header")
        .is_some()
        || result
            .header_val("x-custom-header")
            .is_some();

    if has_custom_header {
        assert_eq!(
            result
                .header_val("X-Custom-Header")
                .or_else(|| result.header_val("x-custom-header")),
            Some("test-value"),
            "X-Custom-Header should be set if headers are captured"
        );
    }
}

#[test]
fn test_error_handling_with_errors_script() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("errors.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute(exec)
        .expect("errors.php request execution failed");

    assert_eq!(result.status_code(), 200);

    assert!(
        result
            .all_messages()
            .any(|_| true),
        "Response should contain messages from error_log() and trigger_error()"
    );

    let has_error = result
        .all_messages()
        .any(|e| {
            e.message
                .contains("Sending an error log")
        });

    assert!(
        has_error,
        "Response should contain error message from error_log()"
    );
}

#[test]
fn test_state_isolation_after_errors() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    let bad_exec = WebRequest::get().build("/nonexistent/path.php");

    assert!(bad_exec.is_err());

    let good_exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let good_result = php
        .execute(good_exec)
        .expect("request after error path should succeed");

    assert_eq!(good_result.status_code(), 200);
}

#[test]
fn test_sapi_initializes() {
    let php = RiphtSapi::instance();

    assert!(php.is_initialized());
}

#[test]
fn test_file_not_found() {
    let req = WebRequest::get().build("/nonexistent/path.php");

    assert!(req.is_err());
}

#[test]
fn test_get_ini_display_errors() {
    let php = RiphtSapi::instance();
    let _ = php.set_ini("display_errors", "0");

    assert_eq!(php.get_ini("display_errors"), Some("0".into()));
}

#[test]
fn test_get_ini_nonexistent() {
    let php = RiphtSapi::instance();

    let value = php.get_ini("this_ini_key_does_not_exist_12345");
    assert!(value.is_none(), "Non-existent INI should return None");
}

#[test]
fn test_set_ini_and_get_ini() {
    let php = RiphtSapi::instance();

    let result = php.set_ini("memory_limit", "256M");
    assert!(result.is_ok(), "set_ini should succeed for valid INI key");

    let value = php.get_ini("memory_limit");
    assert!(value.is_some(), "memory_limit should be readable after set");
    assert_eq!(
        value.unwrap(),
        "256M",
        "memory_limit should reflect the set value"
    );
}

#[test]
fn test_set_ini_invalid_key() {
    let php = RiphtSapi::instance();

    let result = php.set_ini("key\0with\0nulls", "value");
    assert!(
        result.is_err(),
        "set_ini should fail for key with null bytes"
    );
}

#[test]
fn test_set_ini_invalid_value() {
    let php = RiphtSapi::instance();

    let result = php.set_ini("memory_limit", "value\0with\0nulls");
    assert!(
        result.is_err(),
        "set_ini should fail for value with null bytes"
    );
}

#[test]
fn test_execution_error_script_not_found() {
    let php = RiphtSapi::instance();

    let ctx = ExecutionContext::script("/nonexistent/path/to/script.php");
    let result = php.execute(ctx);

    assert!(
        result.is_err(),
        "execute should fail for nonexistent script"
    );
    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("not found"),
        "Error should mention script not found"
    );
}

#[test]
fn test_multipart_upload_basic() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("file_upload.php");

    let boundary = "boundary123";
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"field\"\r\n\r\n\
test_value\r\n\
--{boundary}--\r\n",
        boundary = boundary
    );

    let exec = WebRequest::post()
        .with_content_type(format!(
            "multipart/form-data; boundary={}",
            boundary
        ))
        .with_body(body.into_bytes())
        .build(&script_path)
        .expect("failed to build multipart WebRequest");

    let result = php
        .execute(exec)
        .expect("multipart POST request execution failed");
    assert_eq!(result.status_code(), 200);

    let body_str = result.body_string();
    let json: serde_json::Value = serde_json::from_str(&body_str)
        .unwrap_or_else(|_| {
            panic!("failed to parse JSON response: {}", body_str)
        });

    assert_eq!(
        json["post_data"]["field"], "test_value",
        "POST field should be 'test_value': {}",
        body_str
    );
}

#[test]
fn test_multipart_upload_with_file() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("file_upload.php");

    let boundary = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("----multipart-form-boundary-{:x}", timestamp)
    };
    let file_content = "Hello, this is test file content!";
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"myfile\"; filename=\"test.txt\"\r\n\
Content-Type: text/plain\r\n\
\r\n\
{file_content}\r\n\
--{boundary}\r\n\
Content-Disposition: form-data; name=\"description\"\r\n\
\r\n\
A test file\r\n\
--{boundary}--\r\n",
        boundary = boundary,
        file_content = file_content
    );

    let exec = WebRequest::post()
        .with_content_type(format!(
            "multipart/form-data; boundary={}",
            boundary
        ))
        .with_body(body.into_bytes())
        .build(&script_path)
        .expect("failed to build file upload WebRequest");

    let result = php
        .execute(exec)
        .expect("file upload request execution failed");
    assert_eq!(result.status_code(), 200);

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse response body as JSON");

    assert_eq!(
        json["post_data"]["description"], "A test file",
        "POST field 'description' should be set"
    );

    assert!(
        json["files"]["myfile"].is_object(),
        "FILES should contain 'myfile' entry"
    );

    let file_entry = &json["files"]["myfile"];
    assert_eq!(file_entry["name"], "test.txt");
    assert_eq!(file_entry["error"], 0);

    assert_eq!(file_entry["tmp_exists"], true, "Temp file should exist");
    assert_eq!(
        file_entry["tmp_readable"], true,
        "Temp file should be readable"
    );

    assert_eq!(
        file_entry["tmp_content"], file_content,
        "Temp file content should match uploaded content"
    );
    assert_eq!(
        file_entry["tmp_content_length"],
        file_content.len(),
        "Temp file size should match uploaded content length"
    );
}

#[test]
fn test_multipart_upload_temp_file_creation() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("file_upload.php");

    let boundary = "boundary456";
    let file_content = "Test file content for temp file verification";
    let body = format!(
        "--{boundary}\r\n\
Content-Disposition: form-data; name=\"upload\"; filename=\"verify.txt\"\r\n\
Content-Type: text/plain\r\n\
\r\n\
{file_content}\r\n\
--{boundary}--\r\n",
        boundary = boundary,
        file_content = file_content
    );

    let exec = WebRequest::post()
        .with_content_type(format!(
            "multipart/form-data; boundary={}",
            boundary
        ))
        .with_body(body.into_bytes())
        .build(&script_path)
        .expect("failed to build temp file upload WebRequest");

    let result = php
        .execute(exec)
        .expect("temp file upload request execution failed");
    assert_eq!(result.status_code(), 200);

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse response body as JSON");

    let upload_tmp_dir = json["upload_tmp_dir"]
        .as_str()
        .unwrap_or("");

    let file_entry = &json["files"]["upload"];
    assert!(
        file_entry["tmp_name"].is_string(),
        "Temp file name should be set in $_FILES"
    );

    let tmp_name = file_entry["tmp_name"]
        .as_str()
        .expect("tmp_name field should be a string");

    if !upload_tmp_dir.is_empty() {
        let normalized_tmp_dir = upload_tmp_dir.trim_end_matches('/');
        let normalized_tmp_name = tmp_name.trim_end_matches('/');
        let normalized_tmp_dir_alt = normalized_tmp_dir.replace("/private", "");

        assert!(
            normalized_tmp_name.starts_with(normalized_tmp_dir)
                || normalized_tmp_name.starts_with(&normalized_tmp_dir_alt)
                || normalized_tmp_name
                    .replace("/private", "")
                    .starts_with(&normalized_tmp_dir_alt),
            "Temp file should be in upload_tmp_dir: {} (upload_tmp_dir: {})",
            tmp_name,
            upload_tmp_dir
        );
    } else {
        assert!(
            tmp_name.contains("/tmp/")
                || tmp_name.contains("\\tmp\\")
                || tmp_name.contains("/var/folders"),
            "Temp file should be in a temp directory: {}",
            tmp_name
        );
    }

    assert_eq!(
        file_entry["tmp_content"], file_content,
        "Temp file content should match uploaded content"
    );
}

#[test]
fn test_session_basic() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("session.php");

    let exec1 = WebRequest::get()
        .build(&script_path)
        .expect("failed to build first session WebRequest");
    let result1 = php
        .execute(exec1)
        .expect("first session request execution failed");
    assert_eq!(result1.status_code(), 200);

    let body1 = result1.body_string();
    assert!(
        body1.contains("session_id"),
        "Response should contain session_id: {}",
        body1
    );
    assert!(
        body1.contains("\"visit_count\":1")
            || body1.contains("\"visit_count\": 1"),
        "First request should have visit_count=1: {}",
        body1
    );

    let session_cookie = result1
        .all_headers()
        .find(|h| {
            h.name()
                .eq_ignore_ascii_case("Set-Cookie")
        })
        .and_then(|h| {
            if h.value()
                .starts_with("PHPSESSID=")
            {
                h.value()
                    .split(';')
                    .next()
                    .map(|s| s.to_string())
            } else {
                None
            }
        });

    if let Some(cookie_val) = session_cookie {
        let exec2 = WebRequest::get()
            .with_raw_cookie_header(&cookie_val)
            .build(&script_path)
            .expect("failed to build second session WebRequest");
        let result2 = php
            .execute(exec2)
            .expect("second session request execution failed");

        let body2 = result2.body_string();
        assert!(
            body2.contains("\"visit_count\":2")
                || body2.contains("\"visit_count\": 2"),
            "Second request should have visit_count=2: {}",
            body2
        );
    }
}

#[test]
fn test_head_request_method() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    let exec = WebRequest::head()
        .build(&script_path)
        .expect("failed to build HEAD WebRequest");
    let result = php
        .execute(exec)
        .expect("HEAD request execution failed");

    assert_eq!(result.status_code(), 200);
}

#[test]
fn test_options_request_method() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");

    let exec = WebRequest::options()
        .build(&script_path)
        .expect("failed to build OPTIONS WebRequest");
    let result = php
        .execute(exec)
        .expect("OPTIONS request execution failed");
    assert_eq!(result.status_code(), 200);
}

#[test]
fn test_streaming_sse_output() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("streaming.php");

    let chunks = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
    let chunks_clone = Arc::clone(&chunks);

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build streaming WebRequest");
    let result = php
        .execute_streaming(exec, move |chunk| {
            chunks_clone
                .lock()
                .unwrap()
                .push(chunk.to_vec());
        })
        .expect("SSE streaming request execution failed");

    assert_eq!(result.status_code(), 200);

    let received_chunks = chunks.lock().unwrap();
    assert!(
        received_chunks.len() > 1,
        "Should receive multiple chunks, got {}",
        received_chunks.len()
    );

    assert!(
        result.body().is_empty(),
        "Response body should be empty when streaming (data sent to callback)"
    );

    let combined: Vec<u8> = received_chunks
        .iter()
        .flat_map(|c| c.iter().copied())
        .collect();
    let combined_str = String::from_utf8_lossy(&combined);
    assert!(
        combined_str.contains("Chunk 1"),
        "Streamed output should contain Chunk 1"
    );
    assert!(
        combined_str.contains("Chunk 5"),
        "Streamed output should contain Chunk 5"
    );
    assert!(
        combined_str.contains("[DONE]"),
        "Streamed output should contain [DONE]"
    );
}

#[test]
fn exit_status_reports_php_exit_code() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("shutdown_behavior.php");

    let exec = WebRequest::get()
        .with_uri("/shutdown_behavior.php?action=exit_code")
        .build(&script_path)
        .expect("failed to build exit-code WebRequest");
    let result = php
        .execute(exec)
        .expect("exit-code request execution failed");

    assert_eq!(result.status_code(), 200);
    assert_eq!(result.exit_status(), 42);
}

#[test]
fn streaming_exit_status_reports_php_fatal_error() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("shutdown_behavior.php");
    let chunks = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
    let captured_chunks = Arc::clone(&chunks);

    let exec = WebRequest::get()
        .with_uri("/shutdown_behavior.php?action=fatal")
        .build(&script_path)
        .expect("failed to build fatal streaming WebRequest");
    let result = php
        .execute_streaming(exec, move |chunk| {
            captured_chunks
                .lock()
                .unwrap()
                .push(chunk.to_vec());
        })
        .expect("fatal streaming request execution failed");

    assert_eq!(result.status_code(), 200);
    assert_ne!(result.exit_status(), 0);

    let body: Vec<u8> = chunks
        .lock()
        .unwrap()
        .iter()
        .flatten()
        .copied()
        .collect();
    let body = String::from_utf8_lossy(&body);

    assert!(body.contains("will_fatal"));
    assert!(body.contains("Fatal error"));
}

#[test]
fn test_streaming_large_output() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("large_output.php");

    let chunks = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
    let chunks_clone = Arc::clone(&chunks);

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build large output streaming WebRequest");
    let result = php
        .execute_streaming(exec, move |chunk| {
            chunks_clone
                .lock()
                .unwrap()
                .push(chunk.to_vec());
        })
        .expect("large output streaming request execution failed");

    assert_eq!(result.status_code(), 200);

    assert!(
        result.body().is_empty(),
        "Response body should be empty when streaming"
    );

    let received_chunks = chunks.lock().unwrap();
    assert!(
        !received_chunks.is_empty(),
        "Should receive at least one chunk"
    );

    let callback_total: usize = received_chunks
        .iter()
        .map(|c| c.len())
        .sum();
    assert!(
        callback_total >= 1024 * 1024,
        "Should receive 1MB+ via callback, got {} bytes",
        callback_total
    );

    drop(received_chunks);

    let exec2 = WebRequest::get()
        .build(&script_path)
        .expect("failed to build non-streaming WebRequest");
    let result2 = php
        .execute(exec2)
        .expect("non-streaming large output request execution failed");
    assert!(
        result2.body().len() >= 1024 * 1024,
        "Non-streaming should buffer the full output: {} bytes",
        result2.body().len()
    );
}

// Tests for internal SAPI state (post_read flag, server_context cleanup)
// have been moved to src/sapi/callbacks.rs unit tests since they require
// access to internal FFI types.

#[test]
fn test_header_edge_cases_duplicate_set_cookie_headers() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("header_edge_cases.php");

    let exec = WebRequest::get()
        .with_uri("/header_edge_cases.php?test=duplicate")
        .build(&script_path)
        .expect("failed to build header edge cases WebRequest");

    let result = php
        .execute(exec)
        .expect("header edge cases (duplicate) request execution failed");

    let set_cookies = result.header_vals("Set-Cookie");
    assert_eq!(
        set_cookies.len(),
        3,
        "Expected 3 Set-Cookie headers, got {:?}",
        set_cookies
    );

    assert!(set_cookies
        .iter()
        .any(|v| v.contains("a=1")));
    assert!(set_cookies
        .iter()
        .any(|v| v.contains("b=2")));
    assert!(set_cookies
        .iter()
        .any(|v| v.contains("c=3")));
}

#[test]
fn test_header_edge_cases_header_remove() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("header_edge_cases.php");

    let exec = WebRequest::get()
        .with_uri("/header_edge_cases.php?test=remove")
        .build(&script_path)
        .expect("failed to build header remove WebRequest");

    let result = php
        .execute(exec)
        .expect("header edge cases (remove) request execution failed");

    assert!(
        result
            .header_val("X-To-Remove")
            .is_none(),
        "X-To-Remove should not be present after header_remove()"
    );

    let kept = result
        .header_val("X-Kept")
        .map(|v| v.contains("still here"))
        .unwrap_or(false);
    assert!(kept, "X-Kept should be present");
}

#[test]
fn test_status_codes_and_redirect_location_header() {
    let php = RiphtSapi::instance();

    let status_script = php_script_path("status_codes.php");
    let exec_201 = WebRequest::get()
        .with_uri("/status_codes.php?code=201&method=code")
        .build(&status_script)
        .expect("failed to build status 201 WebRequest");

    let result_201 = php
        .execute(exec_201)
        .expect("status_codes.php (201) request execution failed");
    assert_eq!(result_201.status_code(), 201);

    let exec_307 = WebRequest::get()
        .with_uri("/status_codes.php?code=307&method=header")
        .build(&status_script)
        .expect("failed to build status 307 WebRequest");

    let result_307 = php
        .execute(exec_307)
        .expect("status_codes.php (307) request execution failed");
    assert_eq!(result_307.status_code(), 307);

    let redirect_script = php_script_path("redirect_handling.php");
    let exec_redirect = WebRequest::get()
        .with_uri("/redirect_handling.php?type=301")
        .build(&redirect_script)
        .expect("failed to build redirect WebRequest");

    let result_redirect = php
        .execute(exec_redirect)
        .expect("redirect_handling.php request execution failed");

    assert_eq!(result_redirect.status_code(), 301);

    let location = result_redirect
        .header_val("Location")
        .expect("redirect response missing Location header");
    assert!(
        location.contains("/redirected.php"),
        "Expected Location to contain /redirected.php, got: {}",
        location
    );
}

#[test]
fn test_binary_output_byte_integrity() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("binary_output.php");

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build binary output WebRequest");

    let result = php
        .execute(exec)
        .expect("binary output request execution failed");

    assert_eq!(result.body().len(), 256, "Expected 256 bytes");
    assert_eq!(result.body()[0], 0);
    assert_eq!(result.body()[1], 1);
    assert_eq!(result.body()[255], 255);
}

#[test]
fn test_webrequest_shaping_via_superglobals() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("superglobals.php");

    let body = b"raw-body-123".to_vec();
    let path_info = "/extra/path";
    let document_root = std::env::temp_dir().join("ripht_sapi_docroot");

    let exec = WebRequest::post()
        .with_uri("/superglobals.php?alpha=1")
        .with_header("X-Foo-Bar", "baz")
        .with_https(true)
        .with_document_root(&document_root)
        .with_path_info(path_info)
        // Intentionally omit Content-Type + Content-Length to exercise defaults
        .with_body(body.clone())
        .build(&script_path)
        .expect("failed to build superglobals WebRequest");

    let result = php
        .execute(exec)
        .expect("superglobals.php request execution failed");

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse superglobals response as JSON");

    // Header mapping: X-Foo-Bar => HTTP_X_FOO_BAR => appears as X_FOO_BAR in HTTP_HEADERS
    assert_eq!(json["HTTP_HEADERS"]["X_FOO_BAR"], "baz");

    // HTTPS shaping
    assert_eq!(json["SERVER"]["REQUEST_SCHEME"], "https");
    assert_eq!(json["SERVER"]["HTTPS"], "on");

    // PATH_INFO / PATH_TRANSLATED shaping
    assert_eq!(json["SERVER"]["PATH_INFO"], path_info);
    let expected_translated =
        format!("{}{}", document_root.to_string_lossy(), path_info);
    assert_eq!(json["SERVER"]["PATH_TRANSLATED"], expected_translated);

    // Default Content-Type / Content-Length behavior when body is present
    assert_eq!(json["SERVER"]["CONTENT_TYPE"], "application/octet-stream");
    assert_eq!(json["SERVER"]["CONTENT_LENGTH"], body.len().to_string());
}

#[test]
fn execute_with_hooks_calls_output_once_with_final_body() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("hello.php");
    let outputs = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute_with_hooks(
            exec,
            OutputCaptureHooks {
                outputs: Arc::clone(&outputs),
                action: OutputAction::Continue,
            },
        )
        .expect("execute_with_hooks() failed");

    let outputs = outputs.lock().unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0], result.body());
    assert!(result
        .body_string()
        .contains("Hello"));
}

#[test]
fn execute_with_hooks_observes_php_flush_once() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("sink_events.php");
    let flushes = Arc::new(std::sync::Mutex::new(0));

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute_with_hooks(
            exec,
            FlushCaptureHooks {
                flushes: Arc::clone(&flushes),
            },
        )
        .expect("execute_with_hooks() failed");

    assert_eq!(result.body(), b"alphaomega");
    assert_eq!(*flushes.lock().unwrap(), 1);
}

#[test]
fn execute_with_hooks_fastcgi_finish_discards_late_output_and_headers() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("fastcgi_finish_late_output.php");
    let marker_path = sidecar_path("hooks-late-output");
    let _ = std::fs::remove_file(&marker_path);
    let outputs = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_LATE_OUTPUT_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute_with_hooks(
            exec,
            OutputCaptureHooks {
                outputs: Arc::clone(&outputs),
                action: OutputAction::Done,
            },
        )
        .expect("execute_with_hooks() failed");

    assert!(result.body().is_empty());
    assert!(result
        .header_val("X-Ripht-Before-Finish")
        .is_some());
    assert!(result
        .header_val("X-Ripht-After-Finish")
        .is_none());

    let outputs = outputs.lock().unwrap();
    assert_eq!(outputs.len(), 1);

    let body = String::from_utf8_lossy(&outputs[0]);
    assert!(body.contains("\"before\":true"));
    assert!(!body.contains("after"));

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path)
            .expect("late-output fixture should write marker sidecar"),
    )
    .expect("late-output sidecar should be valid JSON");

    assert_eq!(marker["finished"], true);
    assert_eq!(marker["marker"], "late-output-complete");

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn execute_with_hooks_fastcgi_finish_finalizes_output_handlers() {
    let php = RiphtSapi::instance();
    let script_path =
        php_script_path("fastcgi_finish_final_output_handler.php");
    let marker_path = sidecar_path("hooks-final-output-handler");
    let _ = std::fs::remove_file(&marker_path);
    let outputs = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));

    let exec = WebRequest::get()
        .with_env(
            "RIPHT_FASTCGI_FINAL_HANDLER_PATH",
            marker_path
                .to_string_lossy()
                .into_owned(),
        )
        .build(&script_path)
        .expect("failed to build WebRequest");

    let result = php
        .execute_with_hooks(
            exec,
            OutputCaptureHooks {
                outputs: Arc::clone(&outputs),
                action: OutputAction::Continue,
            },
        )
        .expect("execute_with_hooks() failed");

    let outputs = outputs.lock().unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0], b"before|final");
    assert_eq!(result.body(), b"before|final");

    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&marker_path)
            .expect("final-handler fixture should write marker sidecar"),
    )
    .expect("final-handler sidecar should be valid JSON");

    assert_eq!(marker["finished"], true);
    assert_eq!(marker["marker"], "final-handler-complete");

    let _ = std::fs::remove_file(marker_path);
}

#[test]
fn test_execute_with_hooks_can_filter_headers_and_handle_output() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("headers.php");

    struct FilterAndCaptureHooks {
        captured: Arc<std::sync::Mutex<Vec<u8>>>,
    }

    impl ExecutionHooks for FilterAndCaptureHooks {
        fn on_header(&mut self, name: &str, _value: &str) -> bool {
            !name.eq_ignore_ascii_case("X-Another-Header")
        }

        fn on_output(&mut self, data: &[u8]) -> OutputAction {
            self.captured
                .lock()
                .unwrap()
                .extend_from_slice(data);
            OutputAction::Done
        }
    }

    let captured = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));

    let exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build hooks test WebRequest");

    let result = php
        .execute_with_hooks(
            exec,
            FilterAndCaptureHooks {
                captured: Arc::clone(&captured),
            },
        )
        .expect("execute_with_hooks() failed");

    assert!(
        result.body().is_empty(),
        "Body should be empty when hooks handle output"
    );

    assert!(result
        .header_val("X-Custom-Header")
        .is_some());
    assert!(
        result
            .header_val("X-Another-Header")
            .is_none(),
        "X-Another-Header should be filtered out by hooks"
    );

    let captured_bytes = captured
        .lock()
        .unwrap()
        .clone();
    assert!(!captured_bytes.is_empty(), "Expected captured output");

    let captured_json: serde_json::Value =
        serde_json::from_slice(&captured_bytes)
            .expect("captured output should be valid JSON");
    assert_eq!(captured_json["method"], "GET");
}

#[test]
fn test_env_vars_visible_via_getenv() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("env_vars.php");

    let exec = WebRequest::get()
        .with_env("TEST_ENV_KEY", "hello-env")
        .build(&script_path)
        .expect("failed to build env vars WebRequest");

    let result = php
        .execute(exec)
        .expect("env_vars.php request execution failed");

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse env vars response as JSON");

    assert_eq!(json["TEST_ENV_KEY"], "hello-env");
    assert!(json["MISSING_ENV_KEY"].is_null());
}

#[test]
fn test_request_scoped_ini_overrides_apply_and_do_not_leak() {
    let php = RiphtSapi::instance();
    let script_path = php_script_path("ini_overrides.php");

    let base_exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build baseline INI WebRequest");
    let base_result = php
        .execute(base_exec)
        .expect("ini_overrides.php baseline request execution failed");

    let base_json: serde_json::Value =
        serde_json::from_slice(&base_result.body())
            .expect("failed to parse baseline INI response as JSON");

    let base_display_errors = base_json["display_errors"]
        .as_str()
        .unwrap_or("")
        .to_string();

    // Flip the value to exercise request-scoped ini overrides.
    let override_value = if base_display_errors.is_empty() {
        "1"
    } else {
        "0"
    };

    let exec = WebRequest::get()
        .with_ini("display_errors", override_value)
        .build(&script_path)
        .expect("failed to build INI override WebRequest");

    let result = php
        .execute(exec)
        .expect("ini_overrides.php override request execution failed");

    let json: serde_json::Value = serde_json::from_slice(&result.body())
        .expect("failed to parse INI override response as JSON");

    let got = json["display_errors"]
        .as_str()
        .unwrap_or("");
    if override_value == "0" {
        // For boolean directives PHP may report off as an empty string.
        assert!(
            got.is_empty() || got == "0",
            "Expected display_errors to be off, got: {:?}",
            got
        );
    } else {
        assert_eq!(got, "1");
    }

    // Verify no leak into subsequent requests.
    let after_exec = WebRequest::get()
        .build(&script_path)
        .expect("failed to build follow-up INI WebRequest");
    let after_result = php
        .execute(after_exec)
        .expect("ini_overrides.php follow-up request execution failed");

    let after_json: serde_json::Value =
        serde_json::from_slice(&after_result.body())
            .expect("failed to parse follow-up INI response as JSON");

    let after_display_errors = after_json["display_errors"]
        .as_str()
        .unwrap_or("");
    assert_eq!(
        after_display_errors, base_display_errors,
        "display_errors should not leak across requests"
    );
}
