use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ripht_php_sapi::{
    AbortReason, ExecutionControl, ExecutionOptions, ExecutionReport,
    ResponseHeader, ResponseSink, RiphtSapi, SinkResult, WebRequest,
};
use serde_json::Value;

use crate::{
    artifact_path, artifact_report_path, case::scripts_dir,
    write_json_artifact, HeaderValue, LifecycleEvent, ReportMetadata,
    ResiliencyCaseReport, ResiliencyReport, RuntimeFailure, RuntimeFailureKind,
    RuntimeMessage, RuntimeMode, RuntimeResult,
};

pub const RIPHT_RESILIENCY_ARTIFACT: &str = "ripht-resiliency.json";

#[derive(Debug)]
pub struct ResiliencyRun {
    pub report: ResiliencyReport,
    pub artifact_path: PathBuf,
}

pub fn run_ripht_resiliency() -> std::io::Result<ResiliencyRun> {
    let artifact_path = artifact_path(RIPHT_RESILIENCY_ARTIFACT);
    let mut report = build_ripht_resiliency_report(now_unix_epoch_secs()?);

    for case in &mut report.cases {
        case.result.artifact_path =
            Some(artifact_report_path(RIPHT_RESILIENCY_ARTIFACT));
    }

    write_json_artifact(&artifact_path, &report)?;

    Ok(ResiliencyRun {
        report,
        artifact_path,
    })
}

fn build_ripht_resiliency_report(
    generated_unix_epoch_secs: u64,
) -> ResiliencyReport {
    let cases: Vec<_> = resiliency_scenarios()
        .iter()
        .map(run_resiliency_scenario)
        .collect();
    let passed = cases
        .iter()
        .all(|case| case.passed);

    ResiliencyReport {
        generated_unix_epoch_secs,
        passed,
        cases,
    }
}

fn run_resiliency_scenario(
    scenario: &ResiliencyScenario,
) -> ResiliencyCaseReport {
    let sidecars = scenario_sidecars(scenario);
    let preparation_failures = prepare_sidecars(&sidecars);

    let mut result = scenario.execute(&sidecars);

    if !preparation_failures.is_empty() {
        result.failure = Some(RuntimeFailure::new(
            RuntimeFailureKind::Assertion,
            preparation_failures.join("; "),
        ));
    } else if result.failure.is_none() {
        let failures = (scenario.assertion)(&result, &sidecars);
        if !failures.is_empty() {
            result.failure = Some(RuntimeFailure::new(
                RuntimeFailureKind::Assertion,
                failures.join("; "),
            ));
        }
    }

    cleanup_sidecars(&sidecars);

    let passed = result.failure.is_none();

    ResiliencyCaseReport {
        case: scenario.name.to_string(),
        passed,
        result,
    }
}

struct ResiliencyScenario {
    name: &'static str,
    script: &'static str,
    sidecars: &'static [SidecarSpec],
    execution: ResiliencyExecution,
    assertion: fn(&RuntimeResult, &[Sidecar]) -> Vec<String>,
}

impl ResiliencyScenario {
    const fn new(
        name: &'static str,
        script: &'static str,
        execution: ResiliencyExecution,
        assertion: fn(&RuntimeResult, &[Sidecar]) -> Vec<String>,
    ) -> Self {
        Self {
            name,
            script,
            sidecars: &[],
            execution,
            assertion,
        }
    }

    const fn with_sidecars(mut self, sidecars: &'static [SidecarSpec]) -> Self {
        self.sidecars = sidecars;
        self
    }

    fn execute(&self, sidecars: &[Sidecar]) -> RuntimeResult {
        execute_with_resiliency_sink(self, sidecars)
    }
}

#[derive(Debug, Clone, Copy)]
enum ResiliencyExecution {
    ClientClosedWrite,
    SinkWriteAbort,
    SinkFinishAbort,
    HostCancelOnWrite,
    DeadlinePreDelivery,
    DeadlineOnWrite,
    DeadlineShutdownCleanup,
    PostFinishHostCancel,
    PostFinishDeadline,
    ClientClosedThenHostCancel,
}

struct SidecarSpec {
    env_name: &'static str,
    suffix: &'static str,
}

impl SidecarSpec {
    const fn new(env_name: &'static str, suffix: &'static str) -> Self {
        Self { env_name, suffix }
    }
}

struct Sidecar {
    env_name: &'static str,
    path: PathBuf,
}

impl Sidecar {
    fn path_string(&self) -> String {
        self.path
            .to_string_lossy()
            .into_owned()
    }
}

const CONTROL_SHUTDOWN: &[SidecarSpec] = &[SidecarSpec::new(
    "RIPHT_CONTROL_SHUTDOWN_PATH",
    "control-shutdown",
)];
const FINISH_MARKER: &[SidecarSpec] = &[SidecarSpec::new(
    "RIPHT_FASTCGI_MARKER_PATH",
    "finish-marker",
)];

fn resiliency_scenarios() -> Vec<ResiliencyScenario> {
    vec![
        ResiliencyScenario::new(
            "resiliency_client_closed_write",
            "sink_events.php",
            ResiliencyExecution::ClientClosedWrite,
            assert_client_closed_write,
        ),
        ResiliencyScenario::new(
            "resiliency_sink_write_abort",
            "sink_events.php",
            ResiliencyExecution::SinkWriteAbort,
            assert_sink_write_abort,
        ),
        ResiliencyScenario::new(
            "resiliency_sink_finish_abort",
            "sink_events.php",
            ResiliencyExecution::SinkFinishAbort,
            assert_sink_finish_abort,
        ),
        ResiliencyScenario::new(
            "resiliency_host_cancel_on_write",
            "control_probe.php",
            ResiliencyExecution::HostCancelOnWrite,
            assert_host_cancel_on_write,
        )
        .with_sidecars(CONTROL_SHUTDOWN),
        ResiliencyScenario::new(
            "resiliency_deadline_pre_delivery",
            "sink_events.php",
            ResiliencyExecution::DeadlinePreDelivery,
            assert_deadline_pre_delivery,
        ),
        ResiliencyScenario::new(
            "resiliency_deadline_on_write",
            "sink_events.php",
            ResiliencyExecution::DeadlineOnWrite,
            assert_deadline_on_write,
        ),
        ResiliencyScenario::new(
            "resiliency_deadline_shutdown_cleanup",
            "control_probe.php",
            ResiliencyExecution::DeadlineShutdownCleanup,
            assert_deadline_shutdown_cleanup,
        )
        .with_sidecars(CONTROL_SHUTDOWN),
        ResiliencyScenario::new(
            "resiliency_post_finish_host_cancel",
            "fastcgi_finish_marker.php",
            ResiliencyExecution::PostFinishHostCancel,
            assert_post_finish_host_cancel,
        )
        .with_sidecars(FINISH_MARKER),
        ResiliencyScenario::new(
            "resiliency_post_finish_deadline",
            "fastcgi_finish_marker.php",
            ResiliencyExecution::PostFinishDeadline,
            assert_post_finish_deadline,
        )
        .with_sidecars(FINISH_MARKER),
        ResiliencyScenario::new(
            "resiliency_client_closed_then_host_cancel",
            "sink_events.php",
            ResiliencyExecution::ClientClosedThenHostCancel,
            assert_client_closed_then_host_cancel,
        ),
    ]
}

fn execute_with_resiliency_sink(
    scenario: &ResiliencyScenario,
    sidecars: &[Sidecar],
) -> RuntimeResult {
    let started_at = Instant::now();
    let control = Arc::new(ExecutionControl::default());
    let events = Arc::new(Mutex::new(Vec::new()));
    let sapi = RiphtSapi::instance();

    let ctx = match build_request(scenario, sidecars) {
        Ok(ctx) => ctx,
        Err(err) => {
            return RuntimeResult::failure(
                "ripht",
                RuntimeMode::RiphtSinkWithOptions,
                scenario.name,
                started_at.elapsed(),
                RuntimeFailure::new(RuntimeFailureKind::BuildRequest, err),
            );
        }
    };

    let options =
        options_for_execution(scenario.execution, Arc::clone(&control));
    let sink = ResiliencySink::new(
        Arc::clone(&events),
        Arc::clone(&control),
        scenario.execution,
    );
    let execution = sapi.execute_with_sink_and_options(ctx, sink, options);

    match execution {
        Ok(report) => report_runtime(
            scenario.name,
            started_at,
            report,
            events
                .lock()
                .map(|events| events.clone())
                .unwrap_or_default(),
        ),
        Err(err) => RuntimeResult::failure(
            "ripht",
            RuntimeMode::RiphtSinkWithOptions,
            scenario.name,
            started_at.elapsed(),
            RuntimeFailure::new(RuntimeFailureKind::Execute, err.to_string()),
        ),
    }
}

fn build_request(
    scenario: &ResiliencyScenario,
    sidecars: &[Sidecar],
) -> Result<ripht_php_sapi::ExecutionContext, String> {
    let mut request = WebRequest::get();

    for sidecar in sidecars {
        request = request.with_env(sidecar.env_name, sidecar.path_string());
    }

    request
        .build(scripts_dir().join(scenario.script))
        .map_err(|err| err.to_string())
}

fn options_for_execution(
    execution: ResiliencyExecution,
    control: Arc<ExecutionControl>,
) -> ExecutionOptions {
    match execution {
        ResiliencyExecution::DeadlinePreDelivery
        | ResiliencyExecution::DeadlineShutdownCleanup => {
            ExecutionOptions::with_control(control)
                .deadline(Instant::now() - Duration::from_secs(1))
        }
        _ => ExecutionOptions::with_control(control),
    }
}

#[derive(Debug)]
struct ResiliencySink {
    events: Arc<Mutex<Vec<LifecycleEvent>>>,
    control: Arc<ExecutionControl>,
    execution: ResiliencyExecution,
    finished: bool,
}

impl ResiliencySink {
    fn new(
        events: Arc<Mutex<Vec<LifecycleEvent>>>,
        control: Arc<ExecutionControl>,
        execution: ResiliencyExecution,
    ) -> Self {
        Self {
            events,
            control,
            execution,
            finished: false,
        }
    }

    fn push_event(&self, event: LifecycleEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

impl ResponseSink for ResiliencySink {
    fn send_headers(
        &mut self,
        status: u16,
        headers: &[ResponseHeader],
    ) -> SinkResult {
        self.push_event(LifecycleEvent::Headers {
            status_code: status,
            headers: headers
                .iter()
                .map(|header| HeaderValue::new(header.name(), header.value()))
                .collect(),
        });

        SinkResult::Continue
    }

    fn write(&mut self, bytes: &[u8]) -> SinkResult {
        self.push_event(LifecycleEvent::Write {
            bytes: bytes.to_vec(),
        });

        match self.execution {
            ResiliencyExecution::ClientClosedWrite => SinkResult::Closed,
            ResiliencyExecution::SinkWriteAbort => SinkResult::Abort,
            ResiliencyExecution::HostCancelOnWrite => {
                self.control.cancel();
                SinkResult::Continue
            }
            ResiliencyExecution::DeadlineOnWrite => {
                self.control
                    .set_deadline(Instant::now() - Duration::from_secs(1));
                SinkResult::Continue
            }
            ResiliencyExecution::ClientClosedThenHostCancel => {
                self.control.cancel();
                SinkResult::Closed
            }
            _ => SinkResult::Continue,
        }
    }

    fn flush(&mut self) -> SinkResult {
        self.push_event(LifecycleEvent::Flush);
        SinkResult::Continue
    }

    fn finish(&mut self) -> SinkResult {
        self.finished = true;
        self.push_event(LifecycleEvent::Finish);

        match self.execution {
            ResiliencyExecution::SinkFinishAbort => SinkResult::Abort,
            ResiliencyExecution::PostFinishHostCancel => {
                self.control.cancel();
                SinkResult::Continue
            }
            ResiliencyExecution::PostFinishDeadline => {
                self.control
                    .set_deadline(Instant::now() - Duration::from_secs(1));
                SinkResult::Continue
            }
            _ => SinkResult::Continue,
        }
    }

    fn abort(&mut self, reason: AbortReason) {
        self.push_event(LifecycleEvent::Abort {
            reason: format!("{reason:?}"),
        });
    }

    fn is_finished(&self) -> bool {
        self.finished
    }
}

fn report_runtime(
    case: &str,
    started_at: Instant,
    report: ExecutionReport,
    events: Vec<LifecycleEvent>,
) -> RuntimeResult {
    RuntimeResult {
        runtime: "ripht".to_string(),
        mode: RuntimeMode::RiphtSinkWithOptions,
        case: case.to_string(),
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

fn scenario_sidecars(scenario: &ResiliencyScenario) -> Vec<Sidecar> {
    scenario
        .sidecars
        .iter()
        .map(|spec| Sidecar {
            env_name: spec.env_name,
            path: std::env::temp_dir().join(format!(
                "ripht-gauntlet-resiliency-{}-{}-{}.json",
                std::process::id(),
                scenario.name,
                spec.suffix
            )),
        })
        .collect()
}

fn prepare_sidecars(sidecars: &[Sidecar]) -> Vec<String> {
    let mut failures = Vec::new();

    for sidecar in sidecars {
        match fs::remove_file(&sidecar.path) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => failures.push(format!(
                "failed to remove stale {} sidecar `{}`: {err}",
                sidecar.env_name,
                sidecar.path.display()
            )),
        }
    }

    failures
}

fn cleanup_sidecars(sidecars: &[Sidecar]) {
    for sidecar in sidecars {
        let _ = fs::remove_file(&sidecar.path);
    }
}

fn assert_client_closed_write(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_report_failures(result);

    require_report_field(
        result,
        |report| !report.aborted,
        "expected not aborted",
        &mut failures,
    );
    require_report_field(
        result,
        |report| report.client_closed,
        "expected client_closed",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.timed_out,
        "expected not timed out",
        &mut failures,
    );
    require_report_field(
        result,
        |report| report.abort_reason.is_none(),
        "expected no abort reason",
        &mut failures,
    );
    require_event_reason(result, "ClientClosed", &mut failures);
    require_no_finish_event(result, &mut failures);
    require_body_excludes(result, b"omega", &mut failures);

    failures
}

fn assert_sink_write_abort(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_report_failures(result);

    require_report_field(
        result,
        |report| report.aborted,
        "expected aborted",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.client_closed,
        "expected not client_closed",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.timed_out,
        "expected not timed out",
        &mut failures,
    );
    require_abort_reason(result, "SinkFailure", &mut failures);
    require_event_reason(result, "SinkFailure", &mut failures);
    require_no_finish_event(result, &mut failures);

    failures
}

fn assert_sink_finish_abort(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_report_failures(result);

    require_report_field(
        result,
        |report| report.aborted,
        "expected aborted",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.client_closed,
        "expected not client_closed",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.timed_out,
        "expected not timed out",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.finalized_early,
        "expected not finalized early",
        &mut failures,
    );
    require_report_field(
        result,
        |report| {
            report
                .post_finish_duration_ms
                .is_none()
        },
        "expected no post-finish duration",
        &mut failures,
    );
    require_abort_reason(result, "SinkFailure", &mut failures);
    require_finish_event(result, &mut failures);
    require_last_event_reason(result, "SinkFailure", &mut failures);

    failures
}

fn assert_host_cancel_on_write(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_report_failures(result);

    require_report_field(
        result,
        |report| report.aborted,
        "expected aborted",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.client_closed,
        "expected not client_closed",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.timed_out,
        "expected not timed out",
        &mut failures,
    );
    require_abort_reason(result, "HostAbort", &mut failures);
    require_event_reason(result, "HostAbort", &mut failures);
    require_body_contains(result, b"alpha", &mut failures);
    require_body_excludes(result, b"omega", &mut failures);
    require_sidecar_string(
        sidecars,
        "RIPHT_CONTROL_SHUTDOWN_PATH",
        "shutdown",
        &mut failures,
    );

    failures
}

fn assert_deadline_pre_delivery(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_report_failures(result);

    require_report_field(
        result,
        |report| report.aborted,
        "expected aborted",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.client_closed,
        "expected not client_closed",
        &mut failures,
    );
    require_report_field(
        result,
        |report| report.timed_out,
        "expected timed out",
        &mut failures,
    );
    require_abort_reason(result, "DeadlineExceeded", &mut failures);
    require_event_reason(result, "DeadlineExceeded", &mut failures);
    require_no_delivery_event(result, &mut failures);

    failures
}

fn assert_deadline_on_write(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_report_failures(result);

    require_report_field(
        result,
        |report| report.aborted,
        "expected aborted",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.client_closed,
        "expected not client_closed",
        &mut failures,
    );
    require_report_field(
        result,
        |report| report.timed_out,
        "expected timed out",
        &mut failures,
    );
    require_abort_reason(result, "DeadlineExceeded", &mut failures);
    require_event_reason(result, "DeadlineExceeded", &mut failures);
    require_body_contains(result, b"alpha", &mut failures);
    require_body_excludes(result, b"omega", &mut failures);

    failures
}

fn assert_deadline_shutdown_cleanup(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = assert_deadline_pre_delivery(result, &[]);

    require_sidecar_string(
        sidecars,
        "RIPHT_CONTROL_SHUTDOWN_PATH",
        "shutdown",
        &mut failures,
    );

    failures
}

fn assert_post_finish_host_cancel(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_report_failures(result);

    require_report_field(
        result,
        |report| report.finalized_early,
        "expected finalized early",
        &mut failures,
    );
    require_report_field(
        result,
        |report| report.aborted,
        "expected aborted",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.client_closed,
        "expected not client_closed",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.timed_out,
        "expected not timed out",
        &mut failures,
    );
    require_report_field(
        result,
        |report| {
            report
                .post_finish_duration_ms
                .is_some()
        },
        "expected post-finish duration",
        &mut failures,
    );
    require_abort_reason(result, "HostAbort", &mut failures);
    require_finish_event(result, &mut failures);
    require_no_abort_event(result, &mut failures);
    require_finish_marker(sidecars, &mut failures);

    failures
}

fn assert_post_finish_deadline(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_report_failures(result);

    require_report_field(
        result,
        |report| report.finalized_early,
        "expected finalized early",
        &mut failures,
    );
    require_report_field(
        result,
        |report| report.aborted,
        "expected aborted",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.client_closed,
        "expected not client_closed",
        &mut failures,
    );
    require_report_field(
        result,
        |report| report.timed_out,
        "expected timed out",
        &mut failures,
    );
    require_report_field(
        result,
        |report| {
            report
                .post_finish_duration_ms
                .is_some()
        },
        "expected post-finish duration",
        &mut failures,
    );
    require_abort_reason(result, "DeadlineExceeded", &mut failures);
    require_finish_event(result, &mut failures);
    require_no_abort_event(result, &mut failures);
    require_finish_marker(sidecars, &mut failures);

    failures
}

fn assert_client_closed_then_host_cancel(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_report_failures(result);

    require_report_field(
        result,
        |report| report.client_closed,
        "expected client_closed",
        &mut failures,
    );
    require_report_field(
        result,
        |report| report.aborted,
        "expected aborted",
        &mut failures,
    );
    require_report_field(
        result,
        |report| !report.timed_out,
        "expected not timed out",
        &mut failures,
    );
    require_abort_reason(result, "HostAbort", &mut failures);
    require_event_reason(result, "ClientClosed", &mut failures);

    failures
}

fn standard_report_failures(result: &RuntimeResult) -> Vec<String> {
    let mut failures = Vec::new();

    if result.runtime != "ripht" {
        failures.push("expected ripht runtime".to_string());
    }
    if result.mode != RuntimeMode::RiphtSinkWithOptions {
        failures.push("expected sink-with-options mode".to_string());
    }
    if result.status_code != Some(200) {
        failures.push("expected status 200".to_string());
    }
    if result.exit_status != Some(0) {
        failures.push("expected exit status 0".to_string());
    }
    if result.report.is_none() {
        failures.push("expected execution report".to_string());
    }

    failures
}

fn require_report_field(
    result: &RuntimeResult,
    predicate: impl Fn(&ReportMetadata) -> bool,
    message: &str,
    failures: &mut Vec<String>,
) {
    let Some(report) = &result.report else {
        return;
    };

    if !predicate(report) {
        failures.push(message.to_string());
    }
}

fn require_abort_reason(
    result: &RuntimeResult,
    expected: &str,
    failures: &mut Vec<String>,
) {
    require_report_field(
        result,
        |report| report.abort_reason.as_deref() == Some(expected),
        &format!("expected abort reason {expected}"),
        failures,
    );
}

fn require_event_reason(
    result: &RuntimeResult,
    expected: &str,
    failures: &mut Vec<String>,
) {
    if !result.events.iter().any(|event| {
        matches!(event, LifecycleEvent::Abort { reason } if reason == expected)
    }) {
        failures.push(format!("expected abort event {expected}"));
    }
}

fn require_last_event_reason(
    result: &RuntimeResult,
    expected: &str,
    failures: &mut Vec<String>,
) {
    if !matches!(
        result.events.last(),
        Some(LifecycleEvent::Abort { reason }) if reason == expected
    ) {
        failures.push(format!("expected final abort event {expected}"));
    }
}

fn require_no_abort_event(result: &RuntimeResult, failures: &mut Vec<String>) {
    if result
        .events
        .iter()
        .any(|event| matches!(event, LifecycleEvent::Abort { .. }))
    {
        failures.push("expected no abort event".to_string());
    }
}

fn require_finish_event(result: &RuntimeResult, failures: &mut Vec<String>) {
    if !result
        .events
        .iter()
        .any(|event| matches!(event, LifecycleEvent::Finish))
    {
        failures.push("expected finish event".to_string());
    }
}

fn require_no_finish_event(result: &RuntimeResult, failures: &mut Vec<String>) {
    if result
        .events
        .iter()
        .any(|event| matches!(event, LifecycleEvent::Finish))
    {
        failures.push("expected no finish event".to_string());
    }
}

fn require_no_delivery_event(
    result: &RuntimeResult,
    failures: &mut Vec<String>,
) {
    if result
        .events
        .iter()
        .any(|event| {
            matches!(
                event,
                LifecycleEvent::Headers { .. }
                    | LifecycleEvent::Write { .. }
                    | LifecycleEvent::Flush
                    | LifecycleEvent::Finish
            )
        })
    {
        failures.push("expected no response delivery event".to_string());
    }
}

fn require_body_contains(
    result: &RuntimeResult,
    needle: &[u8],
    failures: &mut Vec<String>,
) {
    if !contains_bytes(&result.body, needle) {
        failures.push(format!(
            "expected body to contain `{}`",
            String::from_utf8_lossy(needle)
        ));
    }
}

fn require_body_excludes(
    result: &RuntimeResult,
    needle: &[u8],
    failures: &mut Vec<String>,
) {
    if contains_bytes(&result.body, needle) {
        failures.push(format!(
            "expected body to exclude `{}`",
            String::from_utf8_lossy(needle)
        ));
    }
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn require_sidecar_string(
    sidecars: &[Sidecar],
    env_name: &str,
    expected: &str,
    failures: &mut Vec<String>,
) {
    match sidecar_string(sidecars, env_name) {
        Ok(value) if value == expected => {}
        Ok(value) => failures.push(format!(
            "expected {env_name} sidecar `{expected}`, got `{value}`"
        )),
        Err(message) => failures.push(message),
    }
}

fn require_finish_marker(sidecars: &[Sidecar], failures: &mut Vec<String>) {
    match sidecar_json(sidecars, "RIPHT_FASTCGI_MARKER_PATH") {
        Ok(value) => {
            if value
                .get("finished")
                .and_then(Value::as_bool)
                != Some(true)
            {
                failures
                    .push("expected finish marker `finished` true".to_string());
            }
            if value
                .get("marker")
                .and_then(Value::as_str)
                != Some("after-finish")
            {
                failures
                    .push("expected finish marker after-finish".to_string());
            }
        }
        Err(message) => failures.push(message),
    }
}

fn sidecar_json(sidecars: &[Sidecar], env_name: &str) -> Result<Value, String> {
    let content = sidecar_string(sidecars, env_name)?;

    serde_json::from_str(&content)
        .map_err(|err| format!("{env_name} sidecar should contain JSON: {err}"))
}

fn sidecar_string(
    sidecars: &[Sidecar],
    env_name: &str,
) -> Result<String, String> {
    let sidecar = sidecars
        .iter()
        .find(|sidecar| sidecar.env_name == env_name)
        .ok_or_else(|| format!("{env_name} sidecar was not configured"))?;

    fs::read_to_string(&sidecar.path)
        .map_err(|err| format!("{env_name} sidecar should be written: {err}"))
}

fn now_unix_epoch_secs() -> std::io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::{
        assert_client_closed_write, assert_deadline_pre_delivery,
        assert_sink_finish_abort, build_ripht_resiliency_report,
        require_body_contains, require_body_excludes,
    };
    use crate::{LifecycleEvent, ReportMetadata, RuntimeMode, RuntimeResult};

    #[test]
    fn resiliency_report_requires_all_cases_to_pass() {
        let report = build_ripht_resiliency_report(0);

        assert!(report.passed);
        assert_eq!(report.cases.len(), 10);
        assert!(report
            .cases
            .iter()
            .all(|case| case.result.failure.is_none()));
    }

    #[test]
    fn body_match_helpers_check_byte_substrings() {
        let result = RuntimeResult {
            runtime: "ripht".to_string(),
            mode: RuntimeMode::RiphtSinkWithOptions,
            case: "helper".to_string(),
            status_code: Some(200),
            exit_status: Some(0),
            headers: Vec::new(),
            body: b"alpha".to_vec(),
            messages: Vec::new(),
            report: None,
            events: Vec::new(),
            duration_ms: 0,
            artifact_path: None,
            failure: None,
        };
        let mut failures = Vec::new();

        require_body_contains(&result, b"alpha", &mut failures);
        require_body_excludes(&result, b"omega", &mut failures);

        assert!(failures.is_empty());
    }

    #[test]
    fn sink_finish_abort_rejects_extra_classification_state() {
        let result = result_with_report(
            ReportMetadata {
                status_code: 200,
                exit_status: 0,
                php_success: true,
                finalized_early: true,
                aborted: true,
                client_closed: true,
                timed_out: true,
                post_finish_duration_ms: Some(1),
                abort_reason: Some("SinkFailure".to_string()),
            },
            vec![
                LifecycleEvent::Finish,
                LifecycleEvent::Abort {
                    reason: "SinkFailure".to_string(),
                },
            ],
            b"alphaomega".to_vec(),
        );

        let failures = assert_sink_finish_abort(&result, &[]);

        assert!(failures
            .iter()
            .any(|failure| failure == "expected not client_closed"));
        assert!(failures
            .iter()
            .any(|failure| failure == "expected not timed out"));
        assert!(failures
            .iter()
            .any(|failure| failure == "expected not finalized early"));
        assert!(failures
            .iter()
            .any(|failure| failure == "expected no post-finish duration"));
    }

    #[test]
    fn deadline_pre_delivery_rejects_response_delivery_events() {
        let result = result_with_report(
            ReportMetadata {
                status_code: 200,
                exit_status: 0,
                php_success: true,
                finalized_early: false,
                aborted: true,
                client_closed: false,
                timed_out: true,
                post_finish_duration_ms: None,
                abort_reason: Some("DeadlineExceeded".to_string()),
            },
            vec![
                LifecycleEvent::Headers {
                    status_code: 200,
                    headers: Vec::new(),
                },
                LifecycleEvent::Abort {
                    reason: "DeadlineExceeded".to_string(),
                },
            ],
            Vec::new(),
        );

        let failures = assert_deadline_pre_delivery(&result, &[]);

        assert!(failures
            .iter()
            .any(|failure| failure == "expected no response delivery event"));
    }

    #[test]
    fn client_closed_write_rejects_later_fixture_output() {
        let result = result_with_report(
            ReportMetadata {
                status_code: 200,
                exit_status: 0,
                php_success: true,
                finalized_early: false,
                aborted: false,
                client_closed: true,
                timed_out: false,
                post_finish_duration_ms: None,
                abort_reason: None,
            },
            vec![LifecycleEvent::Abort {
                reason: "ClientClosed".to_string(),
            }],
            b"alphaomega".to_vec(),
        );

        let failures = assert_client_closed_write(&result, &[]);

        assert!(failures
            .iter()
            .any(|failure| failure == "expected body to exclude `omega`"));
    }

    fn result_with_report(
        report: ReportMetadata,
        events: Vec<LifecycleEvent>,
        body: Vec<u8>,
    ) -> RuntimeResult {
        RuntimeResult {
            runtime: "ripht".to_string(),
            mode: RuntimeMode::RiphtSinkWithOptions,
            case: "helper".to_string(),
            status_code: Some(200),
            exit_status: Some(0),
            headers: Vec::new(),
            body,
            messages: Vec::new(),
            report: Some(report),
            events,
            duration_ms: 0,
            artifact_path: None,
            failure: None,
        }
    }
}
