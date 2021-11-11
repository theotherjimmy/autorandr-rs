use miette::{IntoDiagnostic, Result};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use monitor_layout::app;

fn main() -> Result<()> {
    let args = crate::app::args().get_matches();
    let level = match args.occurrences_of("verbosity") {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };
    FmtSubscriber::builder()
        .with_max_level(level)
        .without_time()
        .with_writer(std::io::stderr)
        .try_init()
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
