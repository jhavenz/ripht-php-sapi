use std::error::Error;

use ripht_sapi_gauntlet::run_gauntlet_report;

fn main() -> Result<(), Box<dyn Error>> {
    let run = run_gauntlet_report()?;

    if run.report.passed {
        println!(
            "gauntlet-report: pass cases={} artifact={}",
            run.report.cases.len(),
            run.artifact_path.display()
        );
    } else {
        println!(
            "gauntlet-report: fail cases={} artifact={}",
            run.report.cases.len(),
            run.artifact_path.display()
        );
    }

    for case in &run.report.cases {
        println!(
            "gauntlet-report: case={} expected={:?} comparison_passed={}",
            case.name, case.expected, case.comparison_passed
        );

        for difference in &case.differences {
            println!("gauntlet-report: diff {}", difference.summary);
        }
    }

    if !run.report.passed {
        std::process::exit(1);
    }

    Ok(())
}
