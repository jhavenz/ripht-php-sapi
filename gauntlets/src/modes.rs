use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    artifact_path, artifact_report_path, ripht_mode_adapters,
    write_json_artifact, GauntletCase, HeaderValue, LifecycleEvent,
    ModesReport, ReportMetadata, RuntimeFailure, RuntimeFailureKind,
    RuntimeMode, RuntimeResult,
};

pub const RIPHT_MODES_ARTIFACT: &str = "ripht-modes.json";

const EXPECTED_BODY: &[u8] = b"alphaomega";
const EXPECTED_CASE: &str = "ripht_modes_sink_events";

#[derive(Debug)]
pub struct ModesRun {
    pub report: ModesReport,
    pub artifact_path: PathBuf,
}

pub fn run_ripht_modes() -> std::io::Result<ModesRun> {
    let artifact_path = artifact_path(RIPHT_MODES_ARTIFACT);
    let mut report = build_ripht_modes_report(now_unix_epoch_secs()?);

    for result in &mut report.results {
        result.artifact_path = Some(artifact_report_path(RIPHT_MODES_ARTIFACT));
    }

    write_json_artifact(&artifact_path, &report)?;

    Ok(ModesRun {
        report,
        artifact_path,
    })
}

fn build_ripht_modes_report(generated_unix_epoch_secs: u64) -> ModesReport {
    let case = GauntletCase::get(EXPECTED_CASE, "sink_events.php");
    let mut results: Vec<_> = ripht_mode_adapters()
        .into_iter()
        .map(|mut adapter| adapter.execute(&case))
        .collect();

    let passed = evaluate_ripht_modes_results(&mut results);

    ModesReport {
        generated_unix_epoch_secs,
        passed,
        case: case.name.to_string(),
        results,
    }
}

fn evaluate_ripht_modes_results(results: &mut [RuntimeResult]) -> bool {
    let mut passed = true;

    for result in results.iter_mut() {
        if result.failure.is_none() {
            if let Some(message) = result_failure_message(result) {
                result.failure = Some(RuntimeFailure::new(
                    RuntimeFailureKind::Assertion,
                    message,
                ));
            }
        }

        passed &= result.failure.is_none();
    }

    let modes: Vec<_> = results
        .iter()
        .map(|result| result.mode)
        .collect();

    if modes != expected_modes() {
        passed = false;

        let failure_index = results
            .iter()
            .position(|result| result.failure.is_none())
            .or_else(|| (!results.is_empty()).then_some(0));

        if let Some(index) = failure_index {
            let result = &mut results[index];

            result.failure = Some(RuntimeFailure::new(
                RuntimeFailureKind::Assertion,
                "expected exactly one result for each Ripht execution mode",
            ));
        }
    }

    passed
}

fn result_failure_message(result: &RuntimeResult) -> Option<&'static str> {
    match () {
        _ if result.runtime != "ripht" => Some("expected ripht runtime"),
        _ if result.case != EXPECTED_CASE => Some("expected sink events case"),
        _ if result.status_code != Some(200) => Some("expected status 200"),
        _ if result.exit_status != Some(0) => Some("expected exit status 0"),
        _ if result.body != EXPECTED_BODY => {
            Some("expected exact sink_events.php body bytes")
        }
        _ if !contains_header(&result.headers, "X-Ripht-Sink", "yes") => {
            Some("expected X-Ripht-Sink header")
        }
        _ if !result.messages.is_empty() => Some("expected no PHP messages"),
        _ if !sink_mode_report_is_valid(result) => {
            Some("expected valid sink execution report")
        }
        _ if !sink_mode_events_are_valid(result) => {
            Some("expected complete sink lifecycle events")
        }
        _ => None,
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

fn sink_mode_report_is_valid(result: &RuntimeResult) -> bool {
    if !matches!(
        result.mode,
        RuntimeMode::RiphtSink | RuntimeMode::RiphtSinkWithOptions
    ) {
        return true;
    }

    let Some(report) = &result.report else {
        return false;
    };

    clean_sink_report(report)
}

fn clean_sink_report(report: &ReportMetadata) -> bool {
    report.status_code == 200
        && report.exit_status == 0
        && report.php_success
        && !report.finalized_early
        && !report.aborted
        && !report.client_closed
        && !report.timed_out
        && report.abort_reason.is_none()
}

fn sink_mode_events_are_valid(result: &RuntimeResult) -> bool {
    if !matches!(
        result.mode,
        RuntimeMode::RiphtSink | RuntimeMode::RiphtSinkWithOptions
    ) {
        return true;
    }

    matches!(
        result.events.as_slice(),
        [
            LifecycleEvent::Headers { .. },
            LifecycleEvent::Write { bytes: alpha },
            LifecycleEvent::Flush,
            LifecycleEvent::Write { bytes: omega },
            LifecycleEvent::Finish,
        ] if alpha == b"alpha" && omega == b"omega"
    )
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
    use super::{build_ripht_modes_report, evaluate_ripht_modes_results};
    use crate::{
        HeaderValue, LifecycleEvent, ReportMetadata, RuntimeMode, RuntimeResult,
    };

    #[test]
    fn ripht_modes_report_requires_all_modes_to_pass() {
        let report = build_ripht_modes_report(0);

        assert!(report.passed);
        assert_eq!(report.results.len(), 5);
        assert!(report
            .results
            .iter()
            .all(|result| result.failure.is_none()));
    }

    #[test]
    fn modes_evaluation_rejects_wrong_body() {
        let mut results = vec![RuntimeResult {
            runtime: "ripht".to_string(),
            mode: RuntimeMode::RiphtBuffered,
            case: "ripht_modes_sink_events".to_string(),
            status_code: Some(200),
            exit_status: Some(0),
            headers: vec![HeaderValue::new("X-Ripht-Sink", "yes")],
            body: b"wrong".to_vec(),
            messages: Vec::new(),
            report: None,
            events: Vec::new(),
            duration_ms: 0,
            artifact_path: None,
            failure: None,
        }];

        assert!(!evaluate_ripht_modes_results(&mut results));
        assert!(results[0].failure.is_some());
    }

    #[test]
    fn modes_evaluation_rejects_reordered_sink_events() {
        let mut results = expected_modes_result_set();

        results[3].events = vec![
            LifecycleEvent::Headers {
                status_code: 200,
                headers: vec![HeaderValue::new("X-Ripht-Sink", "yes")],
            },
            LifecycleEvent::Write {
                bytes: b"omega".to_vec(),
            },
            LifecycleEvent::Flush,
            LifecycleEvent::Write {
                bytes: b"alpha".to_vec(),
            },
            LifecycleEvent::Finish,
        ];

        assert!(!evaluate_ripht_modes_results(&mut results));
        assert!(results[3].failure.is_some());
    }

    fn expected_modes_result_set() -> Vec<RuntimeResult> {
        vec![
            result_for_mode(RuntimeMode::RiphtBuffered),
            result_for_mode(RuntimeMode::RiphtStreaming),
            result_for_mode(RuntimeMode::RiphtHooks),
            result_for_mode(RuntimeMode::RiphtSink),
            result_for_mode(RuntimeMode::RiphtSinkWithOptions),
        ]
    }

    fn result_for_mode(mode: RuntimeMode) -> RuntimeResult {
        let is_sink_mode = matches!(
            mode,
            RuntimeMode::RiphtSink | RuntimeMode::RiphtSinkWithOptions
        );

        RuntimeResult {
            runtime: "ripht".to_string(),
            mode,
            case: "ripht_modes_sink_events".to_string(),
            status_code: Some(200),
            exit_status: Some(0),
            headers: vec![HeaderValue::new("X-Ripht-Sink", "yes")],
            body: b"alphaomega".to_vec(),
            messages: Vec::new(),
            report: is_sink_mode.then(clean_report),
            events: if is_sink_mode {
                clean_sink_events()
            } else {
                Vec::new()
            },
            duration_ms: 0,
            artifact_path: None,
            failure: None,
        }
    }

    fn clean_report() -> ReportMetadata {
        ReportMetadata {
            status_code: 200,
            exit_status: 0,
            php_success: true,
            finalized_early: false,
            aborted: false,
            client_closed: false,
            timed_out: false,
            post_finish_duration_ms: None,
            abort_reason: None,
        }
    }

    fn clean_sink_events() -> Vec<LifecycleEvent> {
        vec![
            LifecycleEvent::Headers {
                status_code: 200,
                headers: vec![HeaderValue::new("X-Ripht-Sink", "yes")],
            },
            LifecycleEvent::Write {
                bytes: b"alpha".to_vec(),
            },
            LifecycleEvent::Flush,
            LifecycleEvent::Write {
                bytes: b"omega".to_vec(),
            },
            LifecycleEvent::Finish,
        ]
    }
}
