pub mod container;
pub mod collector;
pub mod engine;
pub mod events;
pub mod host;
pub mod output;
pub mod report;

use crate::utils::Result;
use report::CheckReport;

pub fn run_check(container: Option<String>, output_format: &str, verbose: bool) -> Result<()> {
    eprintln!("Collecting host information...");
    let host = host::collect()?;

    eprintln!("Collecting Docker engine information...");
    let engine = engine::collect(verbose)?;

    eprintln!("Collecting container information...");
    let containers = match container {
        Some(ref id) => vec![collector::collect_one(id, verbose)?],
        None         => collector::collect_all(verbose)?,
    };

    eprintln!("Collecting recent events...");
    let ev = events::collect(events::default_since());

    let report = CheckReport {
        collected_at: chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S %z")
            .to_string(),
        host,
        engine,
        containers,
        events: ev,
    };

    output::display(&report, output_format, verbose)
}
