use std::error::Error;

use ripht_sapi_gauntlet::run_cli_parity;

fn main() -> Result<(), Box<dyn Error>> {
    let run = run_cli_parity()?;

    println!(
        "gauntlet-cli-parity: {} skipped={} artifact={}",
        if run.report.passed { "pass" } else { "fail" },
        run.report.skipped,
        run.artifact_path.display()
    );

    for difference in &run
        .report
        .comparison
        .differences
    {
        println!("gauntlet-cli-parity: difference {difference}");
    }

    if !run.report.passed {
        std::process::exit(1);
    }

    Ok(())
}
