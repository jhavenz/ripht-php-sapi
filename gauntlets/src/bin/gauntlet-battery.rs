use std::error::Error;

use ripht_sapi_gauntlet::run_gauntlet_battery;

fn main() -> Result<(), Box<dyn Error>> {
    let run = run_gauntlet_battery()?;

    println!(
        "gauntlet-battery: {} total={} passed={} failed={} skipped={} required_failed={} optional_failed={} blocking_failed={} strict_external={} artifact={}",
        if run.report.passed { "pass" } else { "fail" },
        run.report.summary.total,
        run.report.summary.passed,
        run.report.summary.failed,
        run.report.summary.skipped,
        run.report.summary.required_failed,
        run.report.summary.optional_failed,
        run.report.summary.blocking_failed,
        run.report.strict_external,
        run.artifact_path.display()
    );

    for case in &run.report.cases {
        println!(
            "gauntlet-battery: case={} group={} required={} passed={} skipped={} blocking_failure={} artifact={}",
            case.name,
            case.group,
            case.required,
            case.passed,
            case.skipped,
            case.blocking_failure,
            case.artifact_path.display()
        );

        if let Some(summary) = &case.failure_summary {
            println!("gauntlet-battery: failure case={} {summary}", case.name);
        }
    }

    if !run.report.passed {
        std::process::exit(1);
    }

    Ok(())
}
