use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    artifact_path, artifact_report_path, run_fpm_parity, run_frankenphp_parity,
    run_gauntlet_report, run_ripht_lifecycle, run_ripht_modes,
    run_ripht_resiliency, run_ripht_smoke, write_json_artifact, LifecycleRun,
    ModesRun, ReportRun, ResiliencyRun, SmokeRun, RIPHT_FPM_PARITY_ARTIFACT,
    RIPHT_FRANKENPHP_PARITY_ARTIFACT, RIPHT_LIFECYCLE_ARTIFACT,
    RIPHT_MODES_ARTIFACT, RIPHT_REPORT_ARTIFACT, RIPHT_RESILIENCY_ARTIFACT,
    RIPHT_SMOKE_ARTIFACT,
};

pub const RIPHT_BATTERY_ARTIFACT: &str = "ripht-battery.json";
pub const STRICT_EXTERNAL_ENV: &str = "RIPHT_GAUNTLET_STRICT_EXTERNAL";

#[derive(Debug)]
pub struct BatteryRun {
    pub report: BatteryReport,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BatteryReport {
    pub generated_unix_epoch_secs: u64,
    pub passed: bool,
    pub strict_external: bool,
    pub summary: BatterySummary,
    pub cases: Vec<BatteryCaseReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BatterySummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub required_failed: usize,
    pub optional_failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BatteryCaseReport {
    pub name: String,
    pub group: String,
    pub required: bool,
    pub passed: bool,
    pub skipped: bool,
    pub artifact_path: PathBuf,
    pub failure_summary: Option<String>,
}

pub fn run_gauntlet_battery() -> std::io::Result<BatteryRun> {
    let artifact_path = artifact_path(RIPHT_BATTERY_ARTIFACT);
    let report = build_gauntlet_battery_report(
        now_unix_epoch_secs()?,
        strict_external_enabled(),
    );

    write_json_artifact(&artifact_path, &report)?;

    Ok(BatteryRun {
        report,
        artifact_path,
    })
}

fn build_gauntlet_battery_report(
    generated_unix_epoch_secs: u64,
    strict_external: bool,
) -> BatteryReport {
    let cases = vec![
        required_case(
            "gauntlet-ripht-smoke",
            "ripht",
            RIPHT_SMOKE_ARTIFACT,
            run_ripht_smoke,
            |run: &SmokeRun| run.report.passed,
        ),
        required_case(
            "gauntlet-ripht-modes",
            "ripht",
            RIPHT_MODES_ARTIFACT,
            run_ripht_modes,
            |run: &ModesRun| run.report.passed,
        ),
        required_case(
            "gauntlet-lifecycle",
            "ripht",
            RIPHT_LIFECYCLE_ARTIFACT,
            run_ripht_lifecycle,
            |run: &LifecycleRun| run.report.passed,
        ),
        required_case(
            "gauntlet-resiliency",
            "ripht",
            RIPHT_RESILIENCY_ARTIFACT,
            run_ripht_resiliency,
            |run: &ResiliencyRun| run.report.passed,
        ),
        required_case(
            "gauntlet-report",
            "reporting",
            RIPHT_REPORT_ARTIFACT,
            run_gauntlet_report,
            |run: &ReportRun| run.report.passed,
        ),
        fpm_parity_case(strict_external),
        frankenphp_parity_case(strict_external),
    ];
    let summary = summarize_cases(&cases);
    let passed = summary.required_failed == 0 && summary.optional_failed == 0;

    BatteryReport {
        generated_unix_epoch_secs,
        passed,
        strict_external,
        summary,
        cases,
    }
}

fn required_case<R>(
    name: &str,
    group: &str,
    artifact_name: &str,
    run: impl FnOnce() -> std::io::Result<R>,
    report_passed: impl FnOnce(&R) -> bool,
) -> BatteryCaseReport {
    match run() {
        Ok(run) => {
            let passed = report_passed(&run);

            BatteryCaseReport {
                name: name.to_string(),
                group: group.to_string(),
                required: true,
                passed,
                skipped: false,
                artifact_path: artifact_report_path(artifact_name),
                failure_summary: (!passed).then(|| {
                    format!("{name} reported failure in its artifact")
                }),
            }
        }
        Err(err) => BatteryCaseReport {
            name: name.to_string(),
            group: group.to_string(),
            required: true,
            passed: false,
            skipped: false,
            artifact_path: artifact_report_path(artifact_name),
            failure_summary: Some(format!("{name} failed to run: {err}")),
        },
    }
}

fn fpm_parity_case(strict_external: bool) -> BatteryCaseReport {
    match run_fpm_parity() {
        Ok(run) => external_case(
            "gauntlet-fpm-parity",
            RIPHT_FPM_PARITY_ARTIFACT,
            run.report.passed,
            run.report.skipped,
            run.report
                .skip_reason
                .as_deref(),
            strict_external,
            &run.report
                .comparison
                .differences,
        ),
        Err(err) => external_run_error(
            "gauntlet-fpm-parity",
            RIPHT_FPM_PARITY_ARTIFACT,
            err,
        ),
    }
}

fn frankenphp_parity_case(strict_external: bool) -> BatteryCaseReport {
    match run_frankenphp_parity() {
        Ok(run) => external_case(
            "gauntlet-frankenphp-parity",
            RIPHT_FRANKENPHP_PARITY_ARTIFACT,
            run.report.passed,
            run.report.skipped,
            run.report
                .skip_reason
                .as_deref(),
            strict_external,
            &run.report
                .comparison
                .differences,
        ),
        Err(err) => external_run_error(
            "gauntlet-frankenphp-parity",
            RIPHT_FRANKENPHP_PARITY_ARTIFACT,
            err,
        ),
    }
}

fn external_case(
    name: &str,
    artifact_name: &str,
    report_passed: bool,
    skipped: bool,
    skip_reason: Option<&str>,
    strict_external: bool,
    differences: &[String],
) -> BatteryCaseReport {
    let passed = if skipped {
        !strict_external
    } else {
        report_passed
    };
    let failure_summary = external_failure_summary(
        name,
        report_passed,
        skipped,
        skip_reason,
        strict_external,
        differences,
    );

    BatteryCaseReport {
        name: name.to_string(),
        group: "external_parity".to_string(),
        required: false,
        passed,
        skipped,
        artifact_path: artifact_report_path(artifact_name),
        failure_summary,
    }
}

fn external_failure_summary(
    name: &str,
    report_passed: bool,
    skipped: bool,
    skip_reason: Option<&str>,
    strict_external: bool,
    differences: &[String],
) -> Option<String> {
    if skipped && strict_external {
        return Some(format!(
            "{name} skipped while {STRICT_EXTERNAL_ENV} requires external parity: {}",
            skip_reason.unwrap_or("missing external runtime")
        ));
    }

    if skipped {
        return None;
    }

    if differences.is_empty() {
        return (!report_passed)
            .then(|| format!("{name} reported failure in its artifact"));
    }

    Some(differences.join("; "))
}

fn external_run_error(
    name: &str,
    artifact_name: &str,
    err: std::io::Error,
) -> BatteryCaseReport {
    BatteryCaseReport {
        name: name.to_string(),
        group: "external_parity".to_string(),
        required: false,
        passed: false,
        skipped: false,
        artifact_path: artifact_report_path(artifact_name),
        failure_summary: Some(format!("{name} failed to run: {err}")),
    }
}

fn summarize_cases(cases: &[BatteryCaseReport]) -> BatterySummary {
    let passed = cases
        .iter()
        .filter(|case| case.passed)
        .count();
    let skipped = cases
        .iter()
        .filter(|case| case.skipped)
        .count();
    let required_failed = cases
        .iter()
        .filter(|case| case.required && !case.passed)
        .count();
    let optional_failed = cases
        .iter()
        .filter(|case| !case.required && !case.passed)
        .count();

    BatterySummary {
        total: cases.len(),
        passed,
        failed: cases.len() - passed,
        skipped,
        required_failed,
        optional_failed,
    }
}

fn strict_external_enabled() -> bool {
    std::env::var(STRICT_EXTERNAL_ENV)
        .map(|value| {
            matches!(
                value
                    .to_ascii_lowercase()
                    .as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

fn now_unix_epoch_secs() -> std::io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        external_case, summarize_cases, BatteryCaseReport, STRICT_EXTERNAL_ENV,
    };

    #[test]
    fn skipped_external_passes_default_battery() {
        let case = external_case(
            "gauntlet-fpm-parity",
            "ripht-fpm-parity.json",
            false,
            true,
            Some("php-fpm binary not found"),
            false,
            &["php-fpm binary not found".to_string()],
        );

        assert!(case.passed);
        assert!(case.skipped);
        assert_eq!(case.failure_summary, None);
        assert_eq!(
            case.artifact_path,
            PathBuf::from("gauntlets/artifacts/ripht-fpm-parity.json")
        );
    }

    #[test]
    fn skipped_external_fails_strict_battery() {
        let case = external_case(
            "gauntlet-frankenphp-parity",
            "ripht-frankenphp-parity.json",
            false,
            true,
            Some("frankenphp binary not found"),
            true,
            &["frankenphp binary not found".to_string()],
        );

        assert!(!case.passed);
        assert!(case.skipped);
        assert!(case
            .failure_summary
            .as_deref()
            .unwrap_or_default()
            .contains(STRICT_EXTERNAL_ENV));
    }

    #[test]
    fn failed_external_fails_default_battery() {
        let case = external_case(
            "gauntlet-fpm-parity",
            "ripht-fpm-parity.json",
            false,
            false,
            None,
            false,
            &["RIPHT_GAUNTLET_FPM_BIN path does not exist".to_string()],
        );

        assert!(!case.passed);
        assert!(!case.skipped);
        assert_eq!(
            case.failure_summary,
            Some("RIPHT_GAUNTLET_FPM_BIN path does not exist".to_string())
        );
    }

    #[test]
    fn failed_external_without_differences_gets_failure_summary() {
        let case = external_case(
            "gauntlet-fpm-parity",
            "ripht-fpm-parity.json",
            false,
            false,
            None,
            false,
            &[],
        );

        assert!(!case.passed);
        assert_eq!(
            case.failure_summary,
            Some(
                "gauntlet-fpm-parity reported failure in its artifact"
                    .to_string()
            )
        );
    }

    #[test]
    fn summary_counts_required_and_optional_failures() {
        let cases = vec![
            battery_case("required-pass", true, true, false),
            battery_case("required-fail", true, false, false),
            battery_case("optional-skip", false, true, true),
            battery_case("optional-fail", false, false, false),
        ];

        assert_eq!(
            summarize_cases(&cases),
            super::BatterySummary {
                total: 4,
                passed: 2,
                failed: 2,
                skipped: 1,
                required_failed: 1,
                optional_failed: 1,
            }
        );
    }

    fn battery_case(
        name: &str,
        required: bool,
        passed: bool,
        skipped: bool,
    ) -> BatteryCaseReport {
        BatteryCaseReport {
            name: name.to_string(),
            group: "test".to_string(),
            required,
            passed,
            skipped,
            artifact_path: PathBuf::from("gauntlets/artifacts/test.json"),
            failure_summary: None,
        }
    }
}
