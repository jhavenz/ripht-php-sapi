use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use ripht_sapi_gauntlet::{
    artifact_path, write_json_artifact, GauntletCase, RiphtBufferedAdapter,
    RuntimeAdapter, RuntimeFailure, RuntimeFailureKind, SmokeReport,
};

fn main() -> Result<(), Box<dyn Error>> {
    let case = GauntletCase::get("ripht_smoke_hello", "hello.php");
    let mut adapter = RiphtBufferedAdapter::new();
    let mut result = adapter.execute(&case);
    let mut passed = result.failure.is_none()
        && result.status_code == Some(200)
        && !result.body.is_empty();

    if !passed && result.failure.is_none() {
        result.failure = Some(RuntimeFailure::new(
            RuntimeFailureKind::Assertion,
            "expected status 200 and a non-empty body",
        ));
    }

    passed = result.failure.is_none();

    let artifact = artifact_path("ripht-smoke.json");
    result.artifact_path = Some(artifact.clone());

    let report = SmokeReport {
        generated_unix_epoch_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs(),
        passed,
        result,
    };

    write_json_artifact(&artifact, &report)?;

    if report.passed {
        println!(
            "gauntlet-ripht-smoke: pass case={} artifact={}",
            report.result.case,
            artifact.display()
        );

        return Ok(());
    }

    println!(
        "gauntlet-ripht-smoke: fail case={} artifact={}",
        report.result.case,
        artifact.display()
    );

    std::process::exit(1);
}
