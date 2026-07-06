use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    artifact_path, artifact_report_path, write_json_artifact, GauntletCase,
    RiphtBufferedAdapter, RuntimeAdapter, RuntimeFailure, RuntimeFailureKind,
    RuntimeResult, SmokeReport,
};

pub const RIPHT_SMOKE_ARTIFACT: &str = "ripht-smoke.json";

const EXPECTED_BODY: &[u8] = b"Hello from PHP!";

#[derive(Debug)]
pub struct SmokeRun {
    pub report: SmokeReport,
    pub artifact_path: PathBuf,
}

pub fn run_ripht_smoke() -> std::io::Result<SmokeRun> {
    let artifact_path = artifact_path(RIPHT_SMOKE_ARTIFACT);
    let mut report = build_ripht_smoke_report(now_unix_epoch_secs()?);

    report.result.artifact_path =
        Some(artifact_report_path(RIPHT_SMOKE_ARTIFACT));

    write_json_artifact(&artifact_path, &report)?;

    Ok(SmokeRun {
        report,
        artifact_path,
    })
}

fn build_ripht_smoke_report(generated_unix_epoch_secs: u64) -> SmokeReport {
    let case = GauntletCase::get("ripht_smoke_hello", "hello.php");
    let mut adapter = RiphtBufferedAdapter::new();
    let mut result = adapter.execute(&case);
    let passed = evaluate_ripht_smoke_result(&mut result);

    SmokeReport {
        generated_unix_epoch_secs,
        passed,
        result,
    }
}

fn evaluate_ripht_smoke_result(result: &mut RuntimeResult) -> bool {
    if result.failure.is_some() {
        return false;
    }

    let failure = match () {
        _ if result.status_code != Some(200) => Some("expected status 200"),
        _ if result.exit_status != Some(0) => Some("expected exit status 0"),
        _ if result.body != EXPECTED_BODY => {
            Some("expected exact hello.php body bytes")
        }
        _ if !result.messages.is_empty() => Some("expected no PHP messages"),
        _ => None,
    };

    if let Some(message) = failure {
        result.failure =
            Some(RuntimeFailure::new(RuntimeFailureKind::Assertion, message));

        return false;
    }

    true
}

fn now_unix_epoch_secs() -> std::io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use crate::{RuntimeMode, RuntimeResult};

    use super::{
        build_ripht_smoke_report, evaluate_ripht_smoke_result, EXPECTED_BODY,
    };

    #[test]
    fn ripht_smoke_requires_exact_success_fields() {
        let report = build_ripht_smoke_report(0);
        let result = &report.result;

        assert!(report.passed);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.exit_status, Some(0));
        assert_eq!(result.body, EXPECTED_BODY);
        assert!(result.messages.is_empty());
        assert!(result.failure.is_none());
    }

    #[test]
    fn smoke_evaluation_rejects_wrong_body() {
        let mut result = RuntimeResult {
            runtime: "ripht".to_string(),
            mode: RuntimeMode::RiphtBuffered,
            case: "ripht_smoke_hello".to_string(),
            status_code: Some(200),
            exit_status: Some(0),
            headers: Vec::new(),
            body: b"wrong".to_vec(),
            messages: Vec::new(),
            report: None,
            events: Vec::new(),
            duration_ms: 0,
            artifact_path: None,
            failure: None,
        };

        assert!(!evaluate_ripht_smoke_result(&mut result));
        assert!(result.failure.is_some());
    }
}
