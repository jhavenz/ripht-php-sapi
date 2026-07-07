use std::error::Error;

use ripht_sapi_gauntlet::run_ripht_resiliency;

fn main() -> Result<(), Box<dyn Error>> {
    let run = run_ripht_resiliency()?;

    for case in &run.report.cases {
        let report = case.result.report.as_ref();
        let classification = report
            .map(|report| {
                format!(
                    "aborted={} client_closed={} timed_out={} abort_reason={} post_finish_duration_ms={}",
                    report.aborted,
                    report.client_closed,
                    report.timed_out,
                    report.abort_reason.as_deref().unwrap_or("none"),
                    report
                        .post_finish_duration_ms
                        .map(|duration| duration.to_string())
                        .unwrap_or_else(|| "none".to_string())
                )
            })
            .unwrap_or_else(|| "report=none".to_string());

        println!(
            "gauntlet-resiliency: case={} passed={} {}",
            case.case, case.passed, classification
        );
    }

    if run.report.passed {
        println!(
            "gauntlet-resiliency: pass cases={} artifact={}",
            run.report.cases.len(),
            run.artifact_path.display()
        );

        return Ok(());
    }

    println!(
        "gauntlet-resiliency: fail cases={} artifact={}",
        run.report.cases.len(),
        run.artifact_path.display()
    );

    std::process::exit(1);
}
