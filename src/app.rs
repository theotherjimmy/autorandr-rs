//! Command line argument parser for autorandr-rs(1)
use clap::{App, Arg};

pub const NAME: &'static str = "autorandr-rs";

pub fn args() -> App<'static, 'static> {
    App::new(NAME)
        .version("0.1")
        .about("Watches for changes in connected monitors and switches configurations with EDIDs")
        .arg(
            Arg::with_name("config")
                .value_name("CONFIG")
                .help("The configuration file in TOML")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("check")
                .short("c")
                .long("check")
                .help("The configuration file in TOML"),
        )
}
