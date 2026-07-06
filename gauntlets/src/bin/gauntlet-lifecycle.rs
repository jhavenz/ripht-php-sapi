use std::error::Error;

use ripht_sapi_gauntlet::run_ripht_lifecycle;

fn main() -> Result<(), Box<dyn Error>> {
    let run = run_ripht_lifecycle()?;

    if run.report.passed {
        println!(
            "gauntlet-lifecycle: pass cases={} artifact={}",
            run.report.cases.len(),
            run.artifact_path.display()
        );

        return Ok(());
    }

    println!(
        "gauntlet-lifecycle: fail cases={} artifact={}",
        run.report.cases.len(),
        run.artifact_path.display()
    );

    std::process::exit(1);
}
