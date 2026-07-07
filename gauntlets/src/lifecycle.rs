use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::{
    artifact_path, artifact_report_path, ripht_mode_adapters,
    write_json_artifact, GauntletCase, HeaderValue, LifecycleCaseReport,
    LifecycleEvent, LifecycleReport, ReportMetadata, RuntimeAdapter,
    RuntimeFailure, RuntimeFailureKind, RuntimeMode, RuntimeResult,
};

pub const RIPHT_LIFECYCLE_ARTIFACT: &str = "ripht-lifecycle.json";

#[derive(Debug)]
pub struct LifecycleRun {
    pub report: LifecycleReport,
    pub artifact_path: PathBuf,
}

pub fn run_ripht_lifecycle() -> std::io::Result<LifecycleRun> {
    let artifact_path = artifact_path(RIPHT_LIFECYCLE_ARTIFACT);
    let mut report = build_ripht_lifecycle_report(now_unix_epoch_secs()?);

    for case in &mut report.cases {
        for result in &mut case.results {
            result.artifact_path =
                Some(artifact_report_path(RIPHT_LIFECYCLE_ARTIFACT));
        }
    }

    write_json_artifact(&artifact_path, &report)?;

    Ok(LifecycleRun {
        report,
        artifact_path,
    })
}

fn build_ripht_lifecycle_report(
    generated_unix_epoch_secs: u64,
) -> LifecycleReport {
    let cases: Vec<_> = lifecycle_scenarios()
        .iter()
        .map(run_lifecycle_scenario)
        .collect();
    let passed = cases
        .iter()
        .all(|case| case.passed);

    LifecycleReport {
        generated_unix_epoch_secs,
        passed,
        cases,
    }
}

fn run_lifecycle_scenario(scenario: &LifecycleScenario) -> LifecycleCaseReport {
    let mut results: Vec<_> = ripht_mode_adapters()
        .into_iter()
        .map(|mut adapter| execute_scenario_mode(scenario, adapter.as_mut()))
        .collect();
    let passed = evaluate_lifecycle_results(&mut results);

    LifecycleCaseReport {
        case: scenario.name.to_string(),
        passed,
        results,
    }
}

fn execute_scenario_mode(
    scenario: &LifecycleScenario,
    adapter: &mut dyn RuntimeAdapter,
) -> RuntimeResult {
    let mode = adapter.mode();
    let sidecars = scenario_sidecars(scenario, mode);
    prepare_sidecars(&sidecars);

    let mut case = scenario.case();
    for sidecar in &sidecars {
        case = case.with_env(sidecar.env_name, sidecar.path_string());
    }

    let mut result = adapter.execute(&case);

    if result.failure.is_none() {
        let failures = (scenario.assertion)(&result, &sidecars);
        if !failures.is_empty() {
            result.failure = Some(RuntimeFailure::new(
                RuntimeFailureKind::Assertion,
                failures.join("; "),
            ));
        }
    }

    cleanup_sidecars(&sidecars);

    result
}

fn evaluate_lifecycle_results(results: &mut [RuntimeResult]) -> bool {
    let mut passed = results
        .iter()
        .all(|result| result.failure.is_none());
    let modes: Vec<_> = results
        .iter()
        .map(|result| result.mode)
        .collect();

    if modes == expected_modes() {
        return passed;
    }

    passed = false;

    let failure_index = results
        .iter()
        .position(|result| result.failure.is_none())
        .or_else(|| (!results.is_empty()).then_some(0));

    if let Some(index) = failure_index {
        results[index].failure = Some(RuntimeFailure::new(
            RuntimeFailureKind::Assertion,
            "expected exactly one result for each Ripht lifecycle mode",
        ));
    }

    passed
}

struct LifecycleScenario {
    name: &'static str,
    script: &'static str,
    uri: Option<&'static str>,
    sidecars: &'static [SidecarSpec],
    assertion: fn(&RuntimeResult, &[Sidecar]) -> Vec<String>,
}

impl LifecycleScenario {
    const fn get(
        name: &'static str,
        script: &'static str,
        assertion: fn(&RuntimeResult, &[Sidecar]) -> Vec<String>,
    ) -> Self {
        Self {
            name,
            script,
            uri: None,
            sidecars: &[],
            assertion,
        }
    }

    const fn with_uri(mut self, uri: &'static str) -> Self {
        self.uri = Some(uri);
        self
    }

    const fn with_sidecars(mut self, sidecars: &'static [SidecarSpec]) -> Self {
        self.sidecars = sidecars;
        self
    }

    fn case(&self) -> GauntletCase {
        let mut case = GauntletCase::get(self.name, self.script);

        if let Some(uri) = self.uri {
            case = case.with_uri(uri);
        }

        case
    }
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

const FINISH_RESULT: &[SidecarSpec] = &[SidecarSpec::new(
    "RIPHT_FASTCGI_FINISH_RESULT",
    "finish-result",
)];
const FINISH_MARKER: &[SidecarSpec] = &[SidecarSpec::new(
    "RIPHT_FASTCGI_MARKER_PATH",
    "finish-marker",
)];
const LATE_OUTPUT: &[SidecarSpec] = &[SidecarSpec::new(
    "RIPHT_FASTCGI_LATE_OUTPUT_PATH",
    "late-output",
)];
const FINAL_HANDLER: &[SidecarSpec] = &[SidecarSpec::new(
    "RIPHT_FASTCGI_FINAL_HANDLER_PATH",
    "final-handler",
)];
const CONTROL_SHUTDOWN: &[SidecarSpec] = &[SidecarSpec::new(
    "RIPHT_CONTROL_SHUTDOWN_PATH",
    "control-shutdown",
)];

fn lifecycle_scenarios() -> Vec<LifecycleScenario> {
    vec![
        LifecycleScenario::get(
            "lifecycle_sink_events",
            "sink_events.php",
            assert_sink_events,
        ),
        LifecycleScenario::get(
            "lifecycle_status_204",
            "status_codes.php",
            assert_status_204,
        )
        .with_uri("/status_codes.php?code=204"),
        LifecycleScenario::get(
            "lifecycle_duplicate_headers",
            "header_edge_cases.php",
            assert_duplicate_headers,
        )
        .with_uri("/header_edge_cases.php?test=duplicate"),
        LifecycleScenario::get(
            "lifecycle_output_buffering",
            "output_buffering.php",
            assert_output_buffering,
        ),
        LifecycleScenario::get(
            "lifecycle_finish_once",
            "fastcgi_finish.php",
            assert_finish_once,
        )
        .with_sidecars(FINISH_RESULT),
        LifecycleScenario::get(
            "lifecycle_finish_marker",
            "fastcgi_finish_marker.php",
            assert_finish_marker,
        )
        .with_sidecars(FINISH_MARKER),
        LifecycleScenario::get(
            "lifecycle_late_output",
            "fastcgi_finish_late_output.php",
            assert_late_output,
        )
        .with_sidecars(LATE_OUTPUT),
        LifecycleScenario::get(
            "lifecycle_duplicate_finish_buffers",
            "fastcgi_finish_duplicate_buffers.php",
            assert_duplicate_finish_buffers,
        )
        .with_sidecars(FINISH_RESULT),
        LifecycleScenario::get(
            "lifecycle_final_output_handler",
            "fastcgi_finish_final_output_handler.php",
            assert_final_output_handler,
        )
        .with_sidecars(FINAL_HANDLER),
        LifecycleScenario::get(
            "lifecycle_php_messages",
            "errors.php",
            assert_php_messages,
        ),
        LifecycleScenario::get(
            "lifecycle_shutdown_function",
            "control_probe.php",
            assert_shutdown_function,
        )
        .with_sidecars(CONTROL_SHUTDOWN),
        LifecycleScenario::get(
            "lifecycle_fatal_error",
            "shutdown_behavior.php",
            assert_fatal_error,
        )
        .with_uri("/shutdown_behavior.php?action=fatal"),
    ]
}

fn scenario_sidecars(
    scenario: &LifecycleScenario,
    mode: RuntimeMode,
) -> Vec<Sidecar> {
    scenario
        .sidecars
        .iter()
        .map(|spec| Sidecar {
            env_name: spec.env_name,
            path: std::env::temp_dir().join(format!(
                "ripht-gauntlet-lifecycle-{}-{}-{:?}-{}.json",
                std::process::id(),
                scenario.name,
                mode,
                spec.suffix
            )),
        })
        .collect()
}

fn prepare_sidecars(sidecars: &[Sidecar]) {
    for sidecar in sidecars {
        let _ = fs::remove_file(&sidecar.path);
    }
}

fn cleanup_sidecars(sidecars: &[Sidecar]) {
    for sidecar in sidecars {
        let _ = fs::remove_file(&sidecar.path);
    }
}

fn assert_sink_events(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_success_failures(result);

    require_body_eq(result, b"alphaomega", &mut failures);
    require_header(result, "X-Ripht-Sink", "yes", &mut failures);
    require_clean_sink_report(result, false, &mut failures);
    require_sink_event_order(result, &mut failures);

    failures
}

fn assert_status_204(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = success_failures(result, false);

    if result.status_code != Some(204) {
        failures.push("expected status 204".to_string());
    }
    require_body_eq(result, b"", &mut failures);

    failures
}

fn assert_duplicate_headers(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_success_failures(result);
    let cookie_count = header_count(result, "Set-Cookie");

    if cookie_count != 3 {
        failures
            .push(format!("expected 3 Set-Cookie headers, got {cookie_count}"));
    }

    require_body_contains(result, b"duplicate", &mut failures);

    failures
}

fn assert_output_buffering(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_success_failures(result);

    require_body_contains(result, b"First buffered content", &mut failures);
    require_body_contains(result, b"Nested buffer content", &mut failures);
    require_body_contains(result, b"Second buffered content", &mut failures);
    require_body_contains(result, b"transformed content", &mut failures);
    require_body_contains(result, br#""final_level": 0"#, &mut failures);

    failures
}

fn assert_finish_once(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_success_failures(result);

    require_body_contains(result, b"available", &mut failures);
    require_body_contains(result, b"pre", &mut failures);
    require_body_excludes(result, b"after", &mut failures);
    require_header(result, "X-Ripht-Finalized", "yes", &mut failures);
    require_clean_sink_report(result, true, &mut failures);

    match sidecar_json(sidecars, "RIPHT_FASTCGI_FINISH_RESULT") {
        Ok(value) => {
            require_json_bool(&value, "first", true, &mut failures);
            require_json_bool(&value, "second", false, &mut failures);
        }
        Err(message) => failures.push(message),
    }

    failures
}

fn assert_finish_marker(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_success_failures(result);

    require_body_contains(result, b"pre", &mut failures);
    require_clean_sink_report(result, true, &mut failures);

    match sidecar_json(sidecars, "RIPHT_FASTCGI_MARKER_PATH") {
        Ok(value) => {
            require_json_bool(&value, "finished", true, &mut failures);
            require_json_string(
                &value,
                "marker",
                "after-finish",
                &mut failures,
            );
        }
        Err(message) => failures.push(message),
    }

    failures
}

fn assert_late_output(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_success_failures(result);

    require_body_contains(result, b"before", &mut failures);
    require_body_excludes(result, b"after", &mut failures);
    require_header(result, "X-Ripht-Before-Finish", "yes", &mut failures);
    require_absent_header(result, "X-Ripht-After-Finish", &mut failures);
    require_clean_sink_report(result, true, &mut failures);

    match sidecar_json(sidecars, "RIPHT_FASTCGI_LATE_OUTPUT_PATH") {
        Ok(value) => {
            require_json_bool(&value, "finished", true, &mut failures);
            require_json_string(
                &value,
                "marker",
                "late-output-complete",
                &mut failures,
            );
        }
        Err(message) => failures.push(message),
    }

    failures
}

fn assert_duplicate_finish_buffers(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_success_failures(result);

    require_body_eq(result, b"before", &mut failures);
    require_clean_sink_report(result, true, &mut failures);

    match sidecar_json(sidecars, "RIPHT_FASTCGI_FINISH_RESULT") {
        Ok(value) => {
            require_json_bool(&value, "first", true, &mut failures);
            require_json_bool(&value, "second", false, &mut failures);
            require_json_string(
                &value,
                "post_finish_buffer",
                "after",
                &mut failures,
            );
        }
        Err(message) => failures.push(message),
    }

    failures
}

fn assert_final_output_handler(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_success_failures(result);

    require_body_eq(result, b"before|final", &mut failures);
    require_clean_sink_report(result, true, &mut failures);

    match sidecar_json(sidecars, "RIPHT_FASTCGI_FINAL_HANDLER_PATH") {
        Ok(value) => {
            require_json_bool(&value, "finished", true, &mut failures);
            require_json_string(
                &value,
                "marker",
                "final-handler-complete",
                &mut failures,
            );
        }
        Err(message) => failures.push(message),
    }

    failures
}

fn assert_php_messages(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = success_failures(result, true);

    if result.messages.is_empty() {
        failures.push("expected at least one PHP message".to_string());
    }

    require_body_contains(result, b"status", &mut failures);
    require_body_contains(result, b"ok", &mut failures);

    failures
}

fn assert_fatal_error(
    result: &RuntimeResult,
    _sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = Vec::new();

    if result.runtime != "ripht" {
        failures.push("expected ripht runtime".to_string());
    }
    if result.status_code != Some(200) {
        failures.push("expected status 200".to_string());
    }
    if result.exit_status == Some(0) || result.exit_status.is_none() {
        failures.push("expected nonzero fatal-error exit status".to_string());
    }
    if result.messages.is_empty() {
        failures.push(
            "expected fatal error to be captured as a PHP message".to_string(),
        );
    }
    require_fatal_sink_report(result, &mut failures);

    require_body_contains(result, b"will_fatal", &mut failures);
    require_body_contains(result, b"Fatal error", &mut failures);

    failures
}

fn assert_shutdown_function(
    result: &RuntimeResult,
    sidecars: &[Sidecar],
) -> Vec<String> {
    let mut failures = standard_success_failures(result);

    require_body_eq(result, b"alphaomega", &mut failures);
    require_clean_sink_report(result, false, &mut failures);

    match sidecar_string(sidecars, "RIPHT_CONTROL_SHUTDOWN_PATH") {
        Ok(value) if value == "shutdown" => {}
        Ok(value) => failures.push(format!(
            "expected shutdown sidecar value `shutdown`, got `{value}`"
        )),
        Err(message) => failures.push(message),
    }

    failures
}

fn success_failures(
    result: &RuntimeResult,
    allow_messages: bool,
) -> Vec<String> {
    let mut failures = Vec::new();

    if result.runtime != "ripht" {
        failures.push("expected ripht runtime".to_string());
    }
    if result.exit_status != Some(0) {
        failures.push("expected exit status 0".to_string());
    }
    if !allow_messages && !result.messages.is_empty() {
        failures.push("expected no PHP messages".to_string());
    }

    failures
}

fn standard_success_failures(result: &RuntimeResult) -> Vec<String> {
    let mut failures = success_failures(result, false);

    if result.status_code != Some(200) {
        failures.push("expected status 200".to_string());
    }

    failures
}

fn require_body_eq(
    result: &RuntimeResult,
    expected: &[u8],
    failures: &mut Vec<String>,
) {
    if result.body != expected {
        failures.push(format!(
            "expected exact body `{}`",
            String::from_utf8_lossy(expected)
        ));
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

fn require_header(
    result: &RuntimeResult,
    expected_name: &str,
    expected_value: &str,
    failures: &mut Vec<String>,
) {
    if !contains_header(&result.headers, expected_name, expected_value) {
        failures
            .push(format!("expected header {expected_name}: {expected_value}"));
    }
}

fn require_absent_header(
    result: &RuntimeResult,
    expected_name: &str,
    failures: &mut Vec<String>,
) {
    if result
        .headers
        .iter()
        .any(|header| {
            header
                .name
                .eq_ignore_ascii_case(expected_name)
        })
    {
        failures.push(format!("expected no {expected_name} header"));
    }
}

fn contains_header(
    headers: &[HeaderValue],
    expected_name: &str,
    expected_value: &str,
) -> bool {
    headers.iter().any(|header| {
        header
            .name
            .eq_ignore_ascii_case(expected_name)
            && header.value == expected_value
    })
}

fn header_count(result: &RuntimeResult, expected_name: &str) -> usize {
    result
        .headers
        .iter()
        .filter(|header| {
            header
                .name
                .eq_ignore_ascii_case(expected_name)
        })
        .count()
}

fn require_clean_sink_report(
    result: &RuntimeResult,
    finalized_early: bool,
    failures: &mut Vec<String>,
) {
    if !is_sink_mode(result.mode) {
        return;
    }

    let Some(report) = &result.report else {
        failures.push("expected sink execution report".to_string());
        return;
    };

    if !clean_sink_report(report, finalized_early) {
        failures.push(
            "expected sink report to match lifecycle finalization".to_string(),
        );
    }
}

fn clean_sink_report(report: &ReportMetadata, finalized_early: bool) -> bool {
    report.status_code == 200
        && report.exit_status == 0
        && report.php_success
        && report.finalized_early == finalized_early
        && !report.aborted
        && !report.client_closed
        && !report.timed_out
        && report.abort_reason.is_none()
        && (!finalized_early
            || report
                .post_finish_duration_ms
                .is_some())
}

fn require_fatal_sink_report(
    result: &RuntimeResult,
    failures: &mut Vec<String>,
) {
    if !is_sink_mode(result.mode) {
        return;
    }

    let Some(report) = &result.report else {
        failures.push("expected fatal sink execution report".to_string());
        return;
    };

    if report.php_success {
        failures
            .push("expected fatal sink report to mark PHP failure".to_string());
    }
    if report.exit_status == 0 {
        failures.push(
            "expected fatal sink report exit status to be nonzero".to_string(),
        );
    }
}

fn require_sink_event_order(
    result: &RuntimeResult,
    failures: &mut Vec<String>,
) {
    if !is_sink_mode(result.mode) {
        return;
    }

    let matches_order = matches!(
        result.events.as_slice(),
        [
            LifecycleEvent::Headers { .. },
            LifecycleEvent::Write { bytes: alpha },
            LifecycleEvent::Flush,
            LifecycleEvent::Write { bytes: omega },
            LifecycleEvent::Finish,
        ] if alpha == b"alpha" && omega == b"omega"
    );

    if !matches_order {
        failures.push(
            "expected sink headers/write/flush/write/finish order".to_string(),
        );
    }
}

fn is_sink_mode(mode: RuntimeMode) -> bool {
    matches!(
        mode,
        RuntimeMode::RiphtSink | RuntimeMode::RiphtSinkWithOptions
    )
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

fn require_json_bool(
    value: &Value,
    key: &str,
    expected: bool,
    failures: &mut Vec<String>,
) {
    if value
        .get(key)
        .and_then(Value::as_bool)
        != Some(expected)
    {
        failures.push(format!("expected JSON field {key} to be {expected}"));
    }
}

fn require_json_string(
    value: &Value,
    key: &str,
    expected: &str,
    failures: &mut Vec<String>,
) {
    if value
        .get(key)
        .and_then(Value::as_str)
        != Some(expected)
    {
        failures.push(format!("expected JSON field {key} to be `{expected}`"));
    }
}

fn expected_modes() -> Vec<RuntimeMode> {
    vec![
        RuntimeMode::RiphtBuffered,
        RuntimeMode::RiphtStreaming,
        RuntimeMode::RiphtHooks,
        RuntimeMode::RiphtSink,
        RuntimeMode::RiphtSinkWithOptions,
    ]
}

fn now_unix_epoch_secs() -> std::io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::{assert_php_messages, evaluate_lifecycle_results};
    use crate::{RuntimeMode, RuntimeResult};

    #[test]
    fn lifecycle_evaluation_rejects_missing_modes() {
        let mut results = vec![runtime_result(RuntimeMode::RiphtBuffered)];

        assert!(!evaluate_lifecycle_results(&mut results));
        assert!(results[0].failure.is_some());
    }

    #[test]
    fn php_message_assertion_requires_captured_messages() {
        let result = runtime_result(RuntimeMode::RiphtBuffered);

        let failures = assert_php_messages(&result, &[]);

        assert!(failures
            .iter()
            .any(|failure| failure == "expected at least one PHP message"));
    }

    fn runtime_result(mode: RuntimeMode) -> RuntimeResult {
        RuntimeResult {
            runtime: "ripht".to_string(),
            mode,
            case: "lifecycle_php_messages".to_string(),
            status_code: Some(200),
            exit_status: Some(0),
            headers: Vec::new(),
            body: br#"{"status":"ok"}"#.to_vec(),
            messages: Vec::new(),
            report: None,
            events: Vec::new(),
            duration_ms: 0,
            artifact_path: None,
            failure: None,
        }
    }
}
