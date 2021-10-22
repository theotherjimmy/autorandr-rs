//! Command line argument parser for monitor-layout(1)

use clap::{App, Arg, SubCommand};

pub const NAME: &'static str = "monitor-layout";

pub fn args() -> App<'static, 'static> {
    App::new(NAME)
        .about("Utilities for laying out monitors in Xorg sessions")
        .version("0.3")
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .help("Increase message verbosity"),
        )
        .subcommand(
            SubCommand::with_name("daemon")
                .about("Watch for changes in connected monitors and apply matching layouts")
                .arg(
                    Arg::with_name("config")
                        .value_name("CONFIG")
                        .help("The configuration file")
                        .required(true)
                        .index(1),
                ),
        )
        .subcommand(
            SubCommand::with_name("check")
                .about("Check the configuration for errors")
                .arg(
                    Arg::with_name("config")
                        .value_name("CONFIG")
                        .help("The configuration file")
                        .required(true)
                        .index(1),
                ),
        )
        .subcommand(
            SubCommand::with_name("print-edids").about(
                "Read the edids and print them as they would appear in a configuration file",
            ),
        )
}
