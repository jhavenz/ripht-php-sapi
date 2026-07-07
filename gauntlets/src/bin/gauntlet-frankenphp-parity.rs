use std::error::Error;

use ripht_sapi_gauntlet::run_frankenphp_parity;

fn main() -> Result<(), Box<dyn Error>> {
    let run = run_frankenphp_parity()?;

    if run.report.skipped {
        println!(
            "gauntlet-frankenphp-parity: skip case={} reason={} artifact={}",
            run.report.case,
            run.report
                .skip_reason
                .as_deref()
                .unwrap_or("unknown"),
            run.artifact_path.display()
        );

        return Ok(());
    }

    if run.report.passed {
        println!(
            "gauntlet-frankenphp-parity: pass case={} binary={} artifact={}",
            run.report.case,
            run.report
                .frankenphp_binary
                .as_deref()
                .unwrap_or("unknown"),
            run.artifact_path.display()
        );

        return Ok(());
    }

    println!(
        "gauntlet-frankenphp-parity: fail case={} artifact={}",
        run.report.case,
        run.artifact_path.display()
    );
    for difference in &run
        .report
        .comparison
        .differences
    {
        println!("gauntlet-frankenphp-parity: diff {difference}");
    }

    std::process::exit(1);
}
