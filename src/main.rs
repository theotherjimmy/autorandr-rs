use miette::{IntoDiagnostic, Result};
use monitor_layout::app;

fn main() -> Result<()> {
    let args = crate::app::args().get_matches();
    stderrlog::new()
        .verbosity(args.occurrences_of("verbosity") as usize)
        .timestamp(stderrlog::Timestamp::Off)
        .init()
        .unwrap();
    match args.subcommand() {
        ("daemon", Some(args)) => monitor_layout::commands::daemon(args),
        ("check", Some(args)) => monitor_layout::commands::check(args).map(|_| ()),
        ("print-edids", Some(args)) => monitor_layout::commands::print_edids(args),
        _ => {
            app::args().print_help().into_diagnostic()?;
            println!("");
            Ok(())
        }
    }
}
