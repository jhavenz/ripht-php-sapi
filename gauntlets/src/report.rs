use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    artifact_path, artifact_report_path, write_json_artifact, HeaderValue,
    ParityComparison, RuntimeFailure, RuntimeFailureKind, RuntimeMode,
    RuntimeResult,
};

pub const RIPHT_REPORT_ARTIFACT: &str = "ripht-report.json";
pub const PROBE_HEADER_NAME: &str = "X-Ripht-Sink";
pub const PROBE_HEADER_VALUE: &str = "yes";

const IGNORED_HEADERS: &[&str] = &[
    "Connection",
    "Content-Length",
    "Date",
    "Keep-Alive",
    "Server",
    "Transfer-Encoding",
    "X-Powered-By",
];

#[derive(Debug)]
pub struct ReportRun {
    pub report: GauntletReport,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GauntletReport {
    pub generated_unix_epoch_secs: u64,
    pub passed: bool,
    pub policy: ReportPolicy,
    pub cases: Vec<ReportCase>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReportPolicy {
    pub exact_fields: Vec<String>,
    pub probe_headers: Vec<HeaderExpectation>,
    pub ignored_headers: Vec<String>,
    pub allowed_divergences: Vec<String>,
    pub timing_tolerances: Vec<String>,
    pub skip_semantics: String,
    pub raw_artifact_storage: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HeaderExpectation {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedComparison {
    Pass,
    Fail,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ReportCase {
    pub name: String,
    pub expected: ExpectedComparison,
    pub passed: bool,
    pub comparison_passed: bool,
    pub expected_runtime: String,
    pub actual_runtime: String,
    pub differences: Vec<ReportDiff>,
    pub raw_artifacts: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReportDiff {
    pub field: String,
    pub expected: String,
    pub actual: String,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeComparison {
    pub passed: bool,
    pub differences: Vec<ReportDiff>,
}

impl RuntimeComparison {
    pub fn parity_comparison(&self) -> ParityComparison {
        ParityComparison {
            passed: self.passed,
            differences: self
                .differences
                .iter()
                .map(|difference| difference.summary.clone())
                .collect(),
        }
    }
}

pub fn run_gauntlet_report() -> std::io::Result<ReportRun> {
    let artifact_path = artifact_path(RIPHT_REPORT_ARTIFACT);
    let report = build_gauntlet_report(now_unix_epoch_secs()?);

    write_json_artifact(&artifact_path, &report)?;

    Ok(ReportRun {
        report,
        artifact_path,
    })
}

pub fn compare_runtime_parity(
    expected_label: &str,
    actual_label: &str,
    expected: &RuntimeResult,
    actual: &RuntimeResult,
) -> RuntimeComparison {
    let mut differences = Vec::new();

    push_failure_diff(&mut differences, expected_label, &expected.failure);
    push_failure_diff(&mut differences, actual_label, &actual.failure);

    if expected.status_code != actual.status_code {
        differences.push(ReportDiff {
            field: "status_code".to_string(),
            expected: format!("{:?}", expected.status_code),
            actual: format!("{:?}", actual.status_code),
            summary: format!(
                "status mismatch: {expected_label}={:?} {actual_label}={:?}",
                expected.status_code, actual.status_code
            ),
        });
    }

    if expected.body != actual.body {
        differences.push(ReportDiff {
            field: "body".to_string(),
            expected: String::from_utf8_lossy(&expected.body).into_owned(),
            actual: String::from_utf8_lossy(&actual.body).into_owned(),
            summary: format!(
                "body mismatch: {expected_label}=`{}` {actual_label}=`{}`",
                String::from_utf8_lossy(&expected.body),
                String::from_utf8_lossy(&actual.body)
            ),
        });
    }

    push_probe_header_diff(&mut differences, expected_label, expected);
    push_probe_header_diff(&mut differences, actual_label, actual);

    if !actual.messages.is_empty() {
        differences.push(ReportDiff {
            field: format!("{actual_label}.messages"),
            expected: "[]".to_string(),
            actual: actual
                .messages
                .iter()
                .map(|message| message.message.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            summary: format!("{actual_label} emitted runtime messages"),
        });
    }

    RuntimeComparison {
        passed: differences.is_empty(),
        differences,
    }
}

pub fn report_policy() -> ReportPolicy {
    ReportPolicy {
        exact_fields: vec![
            "status_code".to_string(),
            "body".to_string(),
            "runtime failure state".to_string(),
            "probe headers".to_string(),
            "external runtime messages".to_string(),
        ],
        probe_headers: vec![HeaderExpectation {
            name: PROBE_HEADER_NAME.to_string(),
            value: PROBE_HEADER_VALUE.to_string(),
        }],
        ignored_headers: IGNORED_HEADERS
            .iter()
            .map(|header| (*header).to_string())
            .collect(),
        allowed_divergences: vec![
            "transport headers outside the probe set".to_string(),
            "runtime duration differences".to_string(),
        ],
        timing_tolerances: vec![
            "duration_ms is recorded for review and never exact-matched"
                .to_string(),
        ],
        skip_semantics:
            "missing optional external binaries skip; present runtimes that fail startup, request, or parsing fail"
                .to_string(),
        raw_artifact_storage:
            "raw gauntlet artifacts are written under the ignored gauntlets/artifacts directory or RIPHT_GAUNTLET_ARTIFACT_DIR"
                .to_string(),
    }
}

fn build_gauntlet_report(generated_unix_epoch_secs: u64) -> GauntletReport {
    let passing = build_report_case(
        "report_policy_passing",
        ExpectedComparison::Pass,
        runtime_result(
            "ripht",
            RuntimeMode::RiphtBuffered,
            Some(200),
            b"alphaomega",
            vec![
                HeaderValue::new(PROBE_HEADER_NAME, PROBE_HEADER_VALUE),
                HeaderValue::new("Date", "Mon, 06 Jul 2026 00:00:00 GMT"),
            ],
            None,
        ),
        runtime_result(
            "php_fpm",
            RuntimeMode::PhpFpm,
            Some(200),
            b"alphaomega",
            vec![
                HeaderValue::new(PROBE_HEADER_NAME, PROBE_HEADER_VALUE),
                HeaderValue::new("Server", "reference-runtime"),
            ],
            None,
        ),
    );
    let failing = build_report_case(
        "report_policy_intentional_diff",
        ExpectedComparison::Fail,
        runtime_result(
            "ripht",
            RuntimeMode::RiphtBuffered,
            Some(200),
            b"alphaomega",
            vec![HeaderValue::new(PROBE_HEADER_NAME, PROBE_HEADER_VALUE)],
            None,
        ),
        runtime_result(
            "frankenphp",
            RuntimeMode::FrankenPhp,
            Some(500),
            b"alphabeta",
            Vec::new(),
            Some(RuntimeFailure::new(
                RuntimeFailureKind::Execute,
                "intentional report-policy fixture failure",
            )),
        ),
    );
    let cases = vec![passing, failing];
    let passed = cases
        .iter()
        .all(|case| case.passed);

    GauntletReport {
        generated_unix_epoch_secs,
        passed,
        policy: report_policy(),
        cases,
    }
}

fn build_report_case(
    name: &str,
    expected: ExpectedComparison,
    expected_result: RuntimeResult,
    actual_result: RuntimeResult,
) -> ReportCase {
    let comparison = compare_runtime_parity(
        &expected_result.runtime,
        &actual_result.runtime,
        &expected_result,
        &actual_result,
    );
    let passed = match expected {
        ExpectedComparison::Pass => comparison.passed,
        ExpectedComparison::Fail => !comparison.passed,
    };

    ReportCase {
        name: name.to_string(),
        expected,
        passed,
        comparison_passed: comparison.passed,
        expected_runtime: expected_result.runtime,
        actual_runtime: actual_result.runtime,
        differences: comparison.differences,
        raw_artifacts: vec![artifact_report_path(RIPHT_REPORT_ARTIFACT)],
    }
}

fn push_failure_diff(
    differences: &mut Vec<ReportDiff>,
    label: &str,
    failure: &Option<RuntimeFailure>,
) {
    if let Some(failure) = failure {
        differences.push(ReportDiff {
            field: format!("{label}.failure"),
            expected: "none".to_string(),
            actual: failure.message.clone(),
            summary: format!("{label} failed: {}", failure.message),
        });
    }
}

fn push_probe_header_diff(
    differences: &mut Vec<ReportDiff>,
    label: &str,
    result: &RuntimeResult,
) {
    if contains_exact_header(
        &result.headers,
        PROBE_HEADER_NAME,
        PROBE_HEADER_VALUE,
    ) {
        return;
    }

    differences.push(ReportDiff {
        field: format!("{label}.headers.{PROBE_HEADER_NAME}"),
        expected: PROBE_HEADER_VALUE.to_string(),
        actual: header_values(&result.headers, PROBE_HEADER_NAME),
        summary: format!(
            "{label} missing {PROBE_HEADER_NAME}: {PROBE_HEADER_VALUE}"
        ),
    });
}

fn contains_exact_header(
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

fn header_values(headers: &[HeaderValue], name: &str) -> String {
    let values: Vec<_> = headers
        .iter()
        .filter(|header| {
            header
                .name
                .eq_ignore_ascii_case(name)
        })
        .map(|header| header.value.as_str())
        .collect();

    if values.is_empty() {
        return "<missing>".to_string();
    }

    values.join(", ")
}

fn runtime_result(
    runtime: &str,
    mode: RuntimeMode,
    status_code: Option<u16>,
    body: &[u8],
    headers: Vec<HeaderValue>,
    failure: Option<RuntimeFailure>,
) -> RuntimeResult {
    RuntimeResult {
        runtime: runtime.to_string(),
        mode,
        case: "report_policy_fixture".to_string(),
        status_code,
        exit_status: Some(0),
        headers,
        body: body.to_vec(),
        messages: Vec::new(),
        report: None,
        events: Vec::new(),
        duration_ms: 0,
        artifact_path: None,
        failure,
    }
}

fn now_unix_epoch_secs() -> std::io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use crate::{RuntimeMessage, RuntimeMode};

    use super::{
        compare_runtime_parity, report_policy, runtime_result,
        ExpectedComparison, HeaderValue, PROBE_HEADER_NAME, PROBE_HEADER_VALUE,
    };

    #[test]
    fn report_policy_declares_matching_rules() {
        let policy = report_policy();

        assert!(policy
            .exact_fields
            .contains(&"status_code".to_string()));
        assert!(policy
            .ignored_headers
            .contains(&"Date".to_string()));
        assert!(policy
            .skip_semantics
            .contains("missing optional external binaries skip"));
    }

    #[test]
    fn parity_policy_ignores_volatile_headers() {
        let ripht = runtime_result(
            "ripht",
            RuntimeMode::RiphtBuffered,
            Some(200),
            b"alphaomega",
            vec![
                HeaderValue::new(PROBE_HEADER_NAME, PROBE_HEADER_VALUE),
                HeaderValue::new("Date", "Mon, 06 Jul 2026 00:00:00 GMT"),
            ],
            None,
        );
        let fpm = runtime_result(
            "php_fpm",
            RuntimeMode::PhpFpm,
            Some(200),
            b"alphaomega",
            vec![
                HeaderValue::new(PROBE_HEADER_NAME, PROBE_HEADER_VALUE),
                HeaderValue::new("Server", "php-fpm"),
            ],
            None,
        );

        assert!(
            compare_runtime_parity("ripht", "php_fpm", &ripht, &fpm).passed
        );
    }

    #[test]
    fn parity_policy_reports_structured_diffs() {
        let ripht = runtime_result(
            "ripht",
            RuntimeMode::RiphtBuffered,
            Some(200),
            b"alphaomega",
            vec![HeaderValue::new(PROBE_HEADER_NAME, PROBE_HEADER_VALUE)],
            None,
        );
        let mut fpm = runtime_result(
            "php_fpm",
            RuntimeMode::PhpFpm,
            Some(500),
            b"alphabeta",
            Vec::new(),
            None,
        );

        fpm.messages
            .push(RuntimeMessage {
                level: "stderr".to_string(),
                message: "warning".to_string(),
            });

        let comparison =
            compare_runtime_parity("ripht", "php_fpm", &ripht, &fpm);
        let fields: Vec<_> = comparison
            .differences
            .iter()
            .map(|difference| difference.field.as_str())
            .collect();

        assert!(!comparison.passed);
        assert!(fields.contains(&"status_code"));
        assert!(fields.contains(&"body"));
        assert!(fields.contains(&"php_fpm.headers.X-Ripht-Sink"));
        assert!(fields.contains(&"php_fpm.messages"));
    }

    #[test]
    fn report_case_can_expect_an_intentional_diff() {
        let report = super::build_gauntlet_report(0);
        let failing = report
            .cases
            .iter()
            .find(|case| case.expected == ExpectedComparison::Fail)
            .expect("report should include an intentional diff case");

        assert!(report.passed);
        assert!(failing.passed);
        assert!(!failing.comparison_passed);
        assert!(!failing.differences.is_empty());
    }
}
