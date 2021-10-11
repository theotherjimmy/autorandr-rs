//! Command line argument parser for autorandr-rs(1)

pub mod autorandrd {
    use clap::{App, Arg};
    pub const NAME: &'static str = "autorandrd";

    pub fn args() -> App<'static, 'static> {
        App::new(NAME)
            .version("0.3")
            .about(
                "Watches for changes in connected monitors and switches configurations with EDIDs",
            )
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
            .arg(
                Arg::with_name("verbosity")
                    .short("v")
                    .multiple(true)
                    .help("Increase message verbosity"),
            )
    }
}

pub mod randr_edid {
    use clap::App;
    pub const NAME: &'static str = "randr-edid";

    pub fn args() -> App<'static, 'static> {
        App::new(NAME)
            .version("0.3")
            .about("Print the EDIDs of all attached monitors in an autorandrd(5) compatible format")
    }
}
