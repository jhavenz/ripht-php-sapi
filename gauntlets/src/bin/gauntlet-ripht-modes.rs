use std::error::Error;

use ripht_sapi_gauntlet::run_ripht_modes;

fn main() -> Result<(), Box<dyn Error>> {
    let run = run_ripht_modes()?;

    if run.report.passed {
        println!(
            "gauntlet-ripht-modes: pass case={} modes={} artifact={}",
            run.report.case,
            run.report.results.len(),
            run.artifact_path.display()
        );

        return Ok(());
    }

    println!(
        "gauntlet-ripht-modes: fail case={} modes={} artifact={}",
        run.report.case,
        run.report.results.len(),
        run.artifact_path.display()
    );

    std::process::exit(1);
}
