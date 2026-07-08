use std::env;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ripht_php_sapi::{CliRequest, RiphtSapi};

use crate::{
    artifact_path, artifact_report_path, case::scripts_dir,
    write_json_artifact, CliParityReport, GauntletCase, ParityComparison,
    RuntimeFailure, RuntimeFailureKind, RuntimeMessage, RuntimeMode,
    RuntimeResult,
};

pub const RIPHT_CLI_PARITY_ARTIFACT: &str = "ripht-cli-parity.json";
pub const PHP_CLI_BIN_ENV: &str = "RIPHT_GAUNTLET_PHP_BIN";

const CLI_STDIN: &[u8] = b"";
const CLI_ARGS: &[&str] = &["alpha", "two words", "--flag=value"];

#[derive(Debug)]
pub struct CliParityRun {
    pub report: CliParityReport,
    pub artifact_path: PathBuf,
}

pub fn run_cli_parity() -> std::io::Result<CliParityRun> {
    let artifact_path = artifact_path(RIPHT_CLI_PARITY_ARTIFACT);
    let mut report = build_cli_parity_report(now_unix_epoch_secs()?);

    report.ripht.artifact_path =
        Some(artifact_report_path(RIPHT_CLI_PARITY_ARTIFACT));

    if let Some(result) = &mut report.php_cli {
        result.artifact_path =
            Some(artifact_report_path(RIPHT_CLI_PARITY_ARTIFACT));
    }

    write_json_artifact(&artifact_path, &report)?;

    Ok(CliParityRun {
        report,
        artifact_path,
    })
}

fn build_cli_parity_report(generated_unix_epoch_secs: u64) -> CliParityReport {
    let case = GauntletCase::get("cli_argv_parity", "cli_argv.php");
    let ripht = execute_ripht_cli(&case);

    let php = match discover_php_cli() {
        Ok(binary) => binary,
        Err(err) => {
            let reason = err.message();
            let skipped = err.is_skip();
            let php_cli = (!skipped).then(|| {
                RuntimeResult::failure(
                    "php_cli",
                    RuntimeMode::PhpCli,
                    case.name,
                    Duration::ZERO,
                    RuntimeFailure::new(
                        RuntimeFailureKind::Execute,
                        reason.clone(),
                    ),
                )
            });

            return CliParityReport {
                generated_unix_epoch_secs,
                passed: false,
                skipped,
                skip_reason: skipped.then_some(reason.clone()),
                case: case.name.to_string(),
                php_binary: None,
                ripht,
                php_cli,
                comparison: ParityComparison {
                    passed: false,
                    differences: vec![reason],
                },
            };
        }
    };

    let php_binary = Some(php.label.clone());
    let php_cli = execute_php_cli(&php, &case);
    let comparison = compare_cli_parity(&ripht, &php_cli);
    let passed = comparison.passed;

    CliParityReport {
        generated_unix_epoch_secs,
        passed,
        skipped: false,
        skip_reason: None,
        case: case.name.to_string(),
        php_binary,
        ripht,
        php_cli: Some(php_cli),
        comparison,
    }
}

fn execute_ripht_cli(case: &GauntletCase) -> RuntimeResult {
    let started_at = Instant::now();
    let mut request = CliRequest::new().with_stdin(CLI_STDIN.to_vec());

    for arg in CLI_ARGS {
        request = request.with_arg(*arg);
    }

    let ctx = match request.build(case.script_path()) {
        Ok(ctx) => ctx,
        Err(err) => {
            return RuntimeResult::failure(
                "ripht",
                RuntimeMode::RiphtBuffered,
                case.name,
                started_at.elapsed(),
                RuntimeFailure::new(
                    RuntimeFailureKind::BuildRequest,
                    err.to_string(),
                ),
            )
        }
    };

    match RiphtSapi::instance().execute(ctx) {
        Ok(result) => RuntimeResult {
            runtime: "ripht".to_string(),
            mode: RuntimeMode::RiphtBuffered,
            case: case.name.to_string(),
            status_code: Some(result.status_code()),
            exit_status: Some(result.exit_status()),
            headers: Vec::new(),
            body: result.body(),
            messages: result
                .all_messages()
                .map(|message| RuntimeMessage {
                    level: message.level.to_string(),
                    message: message.message.clone(),
                })
                .collect(),
            report: None,
            events: Vec::new(),
            duration_ms: started_at
                .elapsed()
                .as_millis(),
            artifact_path: None,
            failure: None,
        },
        Err(err) => RuntimeResult::failure(
            "ripht",
            RuntimeMode::RiphtBuffered,
            case.name,
            started_at.elapsed(),
            RuntimeFailure::new(RuntimeFailureKind::Execute, err.to_string()),
        ),
    }
}

fn execute_php_cli(
    binary: &PhpCliBinary,
    case: &GauntletCase,
) -> RuntimeResult {
    let started_at = Instant::now();
    let mut child = match Command::new(&binary.command)
        .current_dir(scripts_dir())
        .arg(case.script)
        .args(CLI_ARGS)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return RuntimeResult::failure(
                "php_cli",
                RuntimeMode::PhpCli,
                case.name,
                started_at.elapsed(),
                RuntimeFailure::new(
                    RuntimeFailureKind::Execute,
                    err.to_string(),
                ),
            )
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(err) = stdin.write_all(CLI_STDIN) {
            return RuntimeResult::failure(
                "php_cli",
                RuntimeMode::PhpCli,
                case.name,
                started_at.elapsed(),
                RuntimeFailure::new(
                    RuntimeFailureKind::Execute,
                    err.to_string(),
                ),
            );
        }
    }

    match child.wait_with_output() {
        Ok(output) => RuntimeResult {
            runtime: "php_cli".to_string(),
            mode: RuntimeMode::PhpCli,
            case: case.name.to_string(),
            status_code: None,
            exit_status: output.status.code(),
            headers: Vec::new(),
            body: output.stdout,
            messages: stderr_messages(&output.stderr),
            report: None,
            events: Vec::new(),
            duration_ms: started_at
                .elapsed()
                .as_millis(),
            artifact_path: None,
            failure: None,
        },
        Err(err) => RuntimeResult::failure(
            "php_cli",
            RuntimeMode::PhpCli,
            case.name,
            started_at.elapsed(),
            RuntimeFailure::new(RuntimeFailureKind::Execute, err.to_string()),
        ),
    }
}

fn compare_cli_parity(
    ripht: &RuntimeResult,
    php_cli: &RuntimeResult,
) -> ParityComparison {
    let mut differences = Vec::new();

    push_failure_diff(&mut differences, "ripht", &ripht.failure);
    push_failure_diff(&mut differences, "php_cli", &php_cli.failure);

    if ripht.exit_status != php_cli.exit_status {
        differences.push(format!(
            "exit status mismatch: ripht={:?} php_cli={:?}",
            ripht.exit_status, php_cli.exit_status
        ));
    }

    match (
        serde_json::from_slice::<serde_json::Value>(&ripht.body),
        serde_json::from_slice::<serde_json::Value>(&php_cli.body),
    ) {
        (Ok(ripht_json), Ok(php_json)) if ripht_json != php_json => {
            differences.push(format!(
                "json body mismatch: ripht={} php_cli={}",
                ripht_json, php_json
            ));
        }
        (Ok(_), Ok(_)) => {}
        (Err(err), _) => differences.push(format!(
            "ripht stdout was not JSON: {err}; body=`{}`",
            String::from_utf8_lossy(&ripht.body)
        )),
        (_, Err(err)) => differences.push(format!(
            "php_cli stdout was not JSON: {err}; body=`{}`",
            String::from_utf8_lossy(&php_cli.body)
        )),
    }

    if !php_cli.messages.is_empty() {
        differences.push(format!(
            "php_cli emitted stderr: {}",
            php_cli
                .messages
                .iter()
                .map(|message| message.message.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    ParityComparison {
        passed: differences.is_empty(),
        differences,
    }
}

fn push_failure_diff(
    differences: &mut Vec<String>,
    label: &str,
    failure: &Option<RuntimeFailure>,
) {
    if let Some(failure) = failure {
        differences.push(format!(
            "{label} failure: {:?}: {}",
            failure.kind, failure.message
        ));
    }
}

fn stderr_messages(stderr: &[u8]) -> Vec<RuntimeMessage> {
    let stderr = String::from_utf8_lossy(stderr);

    if stderr.trim().is_empty() {
        Vec::new()
    } else {
        vec![RuntimeMessage {
            level: "stderr".to_string(),
            message: stderr.into_owned(),
        }]
    }
}

fn discover_php_cli() -> Result<PhpCliBinary, PhpCliDiscoveryError> {
    if let Ok(binary) = env::var(PHP_CLI_BIN_ENV) {
        let binary = binary.trim();

        if binary.is_empty() {
            return Err(PhpCliDiscoveryError::InvalidExplicit(
                "empty binary path".to_string(),
            ));
        }

        return validate_php_cli(binary, false);
    }

    validate_php_cli("php", true)
}

fn validate_php_cli(
    command: &str,
    missing_is_skip: bool,
) -> Result<PhpCliBinary, PhpCliDiscoveryError> {
    match Command::new(command)
        .arg("-v")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) if status.success() => Ok(PhpCliBinary {
            command: command.to_string(),
            label: command.to_string(),
        }),
        Ok(status) => Err(PhpCliDiscoveryError::InvalidExplicit(format!(
            "`{command} -v` exited with {status}"
        ))),
        Err(err) if missing_is_skip => {
            Err(PhpCliDiscoveryError::Missing(err.to_string()))
        }
        Err(err) => Err(PhpCliDiscoveryError::InvalidExplicit(format!(
            "`{command}` is not executable: {err}"
        ))),
    }
}

fn now_unix_epoch_secs() -> std::io::Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(std::io::Error::other)
}

struct PhpCliBinary {
    command: String,
    label: String,
}

enum PhpCliDiscoveryError {
    Missing(String),
    InvalidExplicit(String),
}

impl PhpCliDiscoveryError {
    fn is_skip(&self) -> bool {
        matches!(self, Self::Missing(_))
    }

    fn message(&self) -> String {
        match self {
            Self::Missing(message) => {
                format!("php CLI binary not found on PATH: {message}")
            }
            Self::InvalidExplicit(message) => {
                format!("{PHP_CLI_BIN_ENV} is invalid: {message}")
            }
        }
    }
}
