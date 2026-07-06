use std::error::Error;

use ripht_sapi_gauntlet::run_ripht_smoke;

fn main() -> Result<(), Box<dyn Error>> {
    let run = run_ripht_smoke()?;

    if run.report.passed {
        println!(
            "gauntlet-ripht-smoke: pass case={} artifact={}",
            run.report.result.case,
            run.artifact_path.display()
        );

        return Ok(());
    }

    println!(
        "gauntlet-ripht-smoke: fail case={} artifact={}",
        run.report.result.case,
        run.artifact_path.display()
    );

    std::process::exit(1);
}
