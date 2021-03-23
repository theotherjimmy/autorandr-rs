use ansi_term::{
    Colour::{Blue, Red},
    Style,
};
use edid::{Descriptor, EDID};
use serde::{Deserialize, Deserializer};
use toml::from_slice;

use std::{
    convert::TryInto,
    collections::HashMap,
    cmp::max,
    error::Error,
    fmt::{Display, Formatter},
    io::{Read, Result as IOResult},
    path::Path,
};

fn str_err(e: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}

#[derive(Deserialize, Debug)]
pub struct Position {
    pub x: i16,
    pub y: i16,
}

impl Position {
    fn new_from_string(s: &str) -> std::result::Result<Self, Box<dyn Error>> {
        let mut iter = s.split('x');
        let x = iter
            .next()
            .ok_or_else(|| str_err("Position is missing X component"))?;
        let y = iter
            .next()
            .ok_or_else(|| str_err("Position is missing Y component"))?;
        Ok(Self {
            x: x.parse()?,
            y: y.parse()?,
        })
    }

    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new_from_string(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize, Debug, Hash, PartialEq, Eq)]
pub struct Mode {
    pub w: u16,
    pub h: u16,
}

impl Mode {
    fn new_from_string(s: &str) -> std::result::Result<Self, Box<dyn Error>> {
        let mut iter = s.split('x');
        let w = iter
            .next()
            .ok_or_else(|| str_err("Position is missing X component"))?;
        let h = iter
            .next()
            .ok_or_else(|| str_err("Position is missing Y component"))?;
        Ok(Self {
            w: w.parse()?,
            h: h.parse()?,
        })
    }

    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new_from_string(&s).map_err(serde::de::Error::custom)
    }
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_> ) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{}x{}", self.w, self.h)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Monitor {
    pub product: Option<String>,
    pub serial: Option<String>,
}

impl From<EDID> for Monitor {
    fn from(edid: EDID) -> Self {
        let mut product = None;
        let mut serial = None;
        for desc in edid.descriptors {
            match desc {
                Descriptor::ProductName(pn) => product = Some(pn),
                Descriptor::SerialNumber(sn) => serial = Some(sn),
                _ => (),
            }
        }
        Self { product, serial }
    }
}


#[derive(Deserialize, Debug)]
pub struct MonConfig {
    #[serde(deserialize_with = "Mode::deserialize")]
    pub mode: Mode,
    #[serde(deserialize_with = "Position::deserialize")]
    pub position: Position,
    pub primary: bool,
}

#[derive(Deserialize, Debug)]
struct SingleConfigIn {
    monitors: Vec<String>,
    #[serde(flatten)]
    setup: HashMap<String, MonConfig>,
}

#[derive(Deserialize, Debug)]
struct ConfigIn {
    monitors: HashMap<String, Monitor>,
    configurations: HashMap<String, SingleConfigIn>,
}

pub struct SingleConfig {
    pub name: String,
    pub fb_size: Mode,
    pub setup: HashMap<Monitor, MonConfig>,
}

pub struct Config(pub HashMap<Vec<Monitor>, SingleConfig>);

impl TryInto<Config> for ConfigIn {
    type Error = String;
    fn try_into(self) -> std::result::Result<Config, Self::Error> {
        let Self {
            monitors: mon_names,
            configurations,
        } = self;
        let mut out = HashMap::with_capacity(configurations.len());
        for (conf_name, SingleConfigIn { monitors, setup }) in configurations.into_iter() {
            let mut mon_set = Vec::with_capacity(monitors.len());
            for mon_name in monitors.into_iter() {
                let mon_desc = mon_names.get(&mon_name).ok_or_else(|| {
                    format!(
                        "In configurations.{}: Monitor in maching statement, {}, not found",
                        conf_name, mon_name
                    )
                })?;
                mon_set.push(mon_desc.clone())
            }
            mon_set.sort();
            let mut fb_size = Mode { w: 0, h: 0 };
            let mut next_setup = HashMap::with_capacity(setup.len());
            for (mon_name, mon_cfg) in setup.into_iter() {
                let mon_desc = mon_names.get(&mon_name).ok_or_else(|| {
                    format!(
                        "In configurations.{}: Monitor named in configuration, {}, not found",
                        conf_name, mon_name
                    )
                })?;
                fb_size.w = max(fb_size.w, mon_cfg.position.x as u16 + mon_cfg.mode.w);
                fb_size.h = max(fb_size.h, mon_cfg.position.y as u16 + mon_cfg.mode.h);
                next_setup.insert(mon_desc.clone(), mon_cfg);
            }
            out.insert(
                mon_set,
                SingleConfig {
                    name: conf_name,
                    setup: next_setup,
                    fb_size,
                },
            );
        }
        Ok(Config(out))
    }
}

fn ok_or_exit<T, E>(r: Result<T, E>, f: impl Fn(E) -> i32) -> T {
    match r {
        Ok(t) => t,
        Err(e) => std::process::exit(f(e)),
    }
}

fn read_to_bytes<P: AsRef<Path>>(fname: P) -> IOResult<Vec<u8>> {
    let mut file = std::fs::File::open(&fname)?;
    let mut bytes = Vec::with_capacity(4096);
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

impl Config {
    pub fn from_fname_or_exit(config_name: &str) -> Self {
        let bytes = ok_or_exit(read_to_bytes(config_name), |e| {
            eprintln!("Error opening configuration file {}: {}", config_name, e);
            1
        });
        let config: ConfigIn = ok_or_exit(from_slice(&bytes), |e| {
            match e.line_col() {
                Some((line, col)) => {
                    let mut lines = bytes.split(|&c| c == b'\n').skip(line);
                    match lines.next() {
                        Some(l) => {
                            let pad_len = line.to_string().len();
                            let pad = "";
                            eprintln!(
                                "{err}: {err_str}\n\
                                 {pad:>pad_len$}{arrow} {fname}:{l_n}:{c_n}\n\
                                 {pad:>pad_len$} {pipe}\n\
                                 {l_n:>pad_len$} {pipe}  {line_text}\n\
                                 {pad:>pad_len$} {pipe}  {pad:>c_n$}{under}",
                                err = Red.bold().paint("error"),
                                err_str = Style::new().bold().paint(e.to_string()),
                                arrow = Blue.bold().paint("-->"),
                                fname = config_name,
                                pad = pad,
                                l_n = line + 1,
                                c_n = col + 1,
                                pipe = Blue.bold().paint("|"),
                                pad_len = pad_len,
                                line_text = String::from_utf8_lossy(l),
                                under = Red.bold().paint("^"),
                            );
                            eprintln!(
                            );
                        }
                        None => eprintln!("error: {}", e),
                    }
                }
                None => eprintln!("error: {}", e),
            }
            2
        });
        ok_or_exit(config.try_into(), |s| {
            // TODO: Try to get line information for this stuff
            eprintln!(
                "{}: {}",
                Red.bold().paint("error"),
                Style::new().bold().paint(s)
            );
            2
        })
    }
}
