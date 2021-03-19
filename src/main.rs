use x11rb::{
    connect,
    connection::Connection,
    protocol::randr::{ConnectionExt as RandrExt, NotifyMask, Output},
    protocol::xproto::{Atom, ConnectionExt as XprotoExt, Window, Timestamp},
    protocol::Event,
};

use ansi_term::{Colour::{Red, Blue}, Style};
use edid::{parse, Descriptor, EDID};
use nom::IResult;
use serde::{Deserialize, Deserializer, Serialize};
use toml::from_slice;

use std::{
    convert::TryInto,
    collections::{HashMap, HashSet},
    error::Error,
    io::Read,
    path::Path,
    hash::Hash,
};

mod app;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn get_edid<C: Connection>(
    conn: &C,
    atom_edid: Atom,
    output: Output,
) -> Result<Option<EDID>> {
    let cookie = conn.randr_get_output_property(output, atom_edid, 19u32, 0, 256, false, true)?;
    let props = cookie.reply()?;
    match parse(&props.data) {
        IResult::Done(_, edid) => Ok(Some(edid)),
        _ => Ok(None),
    }
}

fn get_outputs<C: Connection>(conn: &C, root: Window) -> Result<Vec<Output>> {
    let cookie = conn.randr_get_screen_resources_current(root)?;
    Ok(cookie.reply()?.outputs)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Monitor {
    product: Option<String>,
    serial: Option<String>,
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

fn get_monitors<'o, C: Connection>(
    conn: &'o C,
    outputs: &'o Vec<Output>,
    atom_edid: Atom,
) -> impl Iterator<Item = (Output, Monitor)> + 'o {
    outputs.iter().filter_map(move |out| {
        match get_edid(conn, atom_edid, *out) {
            Ok(Some(m)) => Some((*out, Monitor::from(m))),
            Ok(None) => None,
            Err(e) => {
                eprintln!("Error reading EDID for Output {}: {}", out, e);
                None
            }
        }

    })
}

fn get_config<'a, C: Connection>(
    config: &'a Config,
    conn: &'a C,
    outputs: &'a Vec<Output>,
    atom_edid: Atom,
) -> Option<(&'a String, HashMap<Output, &'a MonConfig>)> {
    let out_to_mon: HashMap<_, _> = get_monitors(conn, outputs, atom_edid).collect();
    let mut monitors: Vec<_> = out_to_mon.values().cloned().collect();
    monitors.sort();
    let (name, setup) = config.0.get(&monitors)?;
    let mut out = HashMap::with_capacity(setup.len());
    for (output, mon) in out_to_mon.into_iter() {
        // Unwrap is checked by Config type on creating
        out.insert(output, setup.get(&mon).unwrap());
    }
    Some((name, out))
}

fn mode_map<C: Connection>(conn: &C, root: Window) -> Result<(HashMap<Mode, HashSet<u32>>, Timestamp)>{
    let resources = conn.randr_get_screen_resources(root)?.reply()?;
    let mut modes: HashMap<_, HashSet<u32>> = HashMap::with_capacity(resources.modes.len());
    for mi in resources.modes.iter() {
        modes.entry(Mode{w: mi.width, h: mi.height}).or_default().insert(mi.id);
    }
    Ok((
        modes,
        resources.timestamp
    ))
}

fn apply_config<C: Connection>(
    conn: &C,
    outputs: &Vec<Output>,
    setup: HashMap<Output, &MonConfig>,
    root: Window,
) -> Result<bool> {
    let (modes, timestamp) = mode_map(conn, root)?;
    let mut used_crtcs = HashSet::with_capacity(outputs.len());
    let mut config_applied = false;
    let primary = conn.randr_get_output_primary(root)?.reply()?.output;
    for &out in outputs {
        if let Some(conf) = setup.get(&out) {
            if let Some(mode_ids) = modes.get(&conf.mode) {
                let out_info = conn.randr_get_output_info(out, timestamp)?.reply()?;
                let mode_id = match out_info.modes.iter().find(|&m| mode_ids.contains(m)) {
                    Some(&mi) => mi,
                    None => Err(format!("out does not support the desired mode {:?}", out_info.modes))?
                };
                let dest_crtc = if out_info.crtc != 0 { out_info.crtc } else {
                    match out_info.crtcs.iter().find(|&c| !used_crtcs.contains(c)) {
                        Some(&c) => c,
                        None => Err(format!("No Crtc available for monitor id {}", out))?
                    }
                };
                let crtc_info = conn.randr_get_crtc_info(dest_crtc, timestamp)?.reply()?;
                let outs = vec![out];
                let rotation  = if crtc_info.rotation != 0 { crtc_info.rotation } else { 1 };
                let Position{x, y} = conf.position;
                if x != crtc_info.x || y != crtc_info.y || mode_id != crtc_info.mode {
                    conn.randr_set_crtc_config(
                        dest_crtc,
                        crtc_info.timestamp,
                        crtc_info.timestamp,
                        x,
                        y,
                        mode_id,
                        rotation,
                        &outs
                    )?.reply()?;
                    config_applied = true;
                }
                if conf.primary && primary != out {
                    conn.randr_set_output_primary(root, out)?;
                    config_applied = true;
                }
                used_crtcs.insert(out_info.crtc);
            }
        }
    }
    Ok(config_applied)
}

fn str_err(e: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}

#[derive(Deserialize, Debug)]
struct Position{ x: i16, y: i16 }
impl Position {
    fn new_from_string(s: &str) -> std::result::Result<Self, Box<dyn Error>> {
        let mut iter = s.split('x');
        let x = iter.next().ok_or_else(|| str_err("Position is missing X component"))?;
        let y = iter.next().ok_or_else(|| str_err("Position is missing Y component"))?;
        Ok(Self{x: x.parse()?, y: y.parse()?})
    }

    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new_from_string(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize, Debug, Hash, PartialEq, Eq)]
struct Mode{ w: u16, h: u16 }
impl Mode {
    fn new_from_string(s: &str) -> std::result::Result<Self, Box<dyn Error>> {
        let mut iter = s.split('x');
        let w = iter.next().ok_or_else(|| str_err("Position is missing X component"))?;
        let h = iter.next().ok_or_else(|| str_err("Position is missing Y component"))?;
        Ok(Self{w: w.parse()?, h: h.parse()?})
    }

    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new_from_string(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize, Debug)]
struct MonConfig {
    #[serde(deserialize_with = "Mode::deserialize")]
    mode: Mode,
    #[serde(deserialize_with = "Position::deserialize")]
    position: Position,
    primary: bool,
}


#[derive(Deserialize, Debug)]
struct SingleConfig {
    monitors: Vec<String>,
    #[serde(flatten)]
    setup: HashMap<String, MonConfig>,
}

#[derive(Deserialize, Debug)]
struct ConfigIn {
    monitors: HashMap<String, Monitor>,
    configurations: HashMap<String, SingleConfig>,
}

struct Config(HashMap<Vec<Monitor>, (String, HashMap<Monitor, MonConfig>)>);

impl TryInto<Config> for ConfigIn {
    type Error = String;
    fn try_into(self) -> std::result::Result<Config, Self::Error>{
        let Self { monitors: mon_names, configurations } = self;
        let mut out = HashMap::with_capacity(configurations.len());
        for (conf_name, SingleConfig{ monitors, setup }) in configurations.into_iter() {
            let mut mon_set = Vec::with_capacity(monitors.len());
            for mon_name in monitors.into_iter() {
                let mon_desc = mon_names.get(&mon_name).ok_or_else(|| format!(
                    "In configurations.{}: Monitor in maching statement, {}, not found",
                    conf_name, mon_name
                ))?;
                mon_set.push(mon_desc.clone())
            }
            mon_set.sort();
            let mut conf_out = HashMap::with_capacity(setup.len());
            for (mon_name, mon_cfg) in setup.into_iter() {
                let mon_desc = mon_names.get(&mon_name).ok_or_else(|| format!(
                    "In configurations.{}: Monitor named in configuration, {}, not found",
                    conf_name, mon_name
                ))?;
                conf_out.insert(mon_desc.clone(), mon_cfg);
            }
            out.insert(mon_set, (conf_name, conf_out));
        }
        Ok(Config(out))
    }
}

fn read_to_bytes<P: AsRef<Path>>(fname: P) -> Result<Vec<u8>> {
    let mut file = std::fs::File::open(&fname)?;
    let mut bytes = Vec::with_capacity(4096);
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}


fn switch_setup<C: Connection>(
    config: &Config,
    conn: &C,
    outputs: &Vec<Output>,
    edid: Atom,
    root: Window,
    force_print: bool,
) -> () {
    match get_config(&config, conn, &outputs, edid) {
        Some((name, setup)) => {
            match apply_config(conn, &outputs, setup, root) {
                Ok(changed) => if changed || force_print {
                    println!("Monitor configuration: {}", name)
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        None => eprintln!("Error: Monitor change indicated, and the connected monitors did not match a config"),
    }
}

fn ok_or_exit<T, E>(r: std::result::Result<T, E>, f: impl Fn(E) -> i32) -> T {
    match r { Ok(t) => t, Err(e) => std::process::exit(f(e)) }
}

fn main() {
    let args = app::args().get_matches();
    // Unwrap below is safe, because the program exits from `get_matches` above when a config
    // is not provided.
    let config_name = args.value_of("config").unwrap();
    let config = ok_or_exit(read_to_bytes(config_name), |e| {
        eprintln!("Error opening configuration file {}: {}", config_name, e);
        1
    });
    let config: ConfigIn = ok_or_exit(from_slice(&config), |e| {
        match e.line_col() {
            Some((line, col)) => {
                let mut lines = config.split(|&c| c == b'\n').skip(line);
                match lines.next() {
                    Some(l) => {
                        let line_len = line.to_string().len();
                        eprintln!(
                            "{}: {}",
                            Red.bold().paint("error"),
                            Style::new().bold().paint(e.to_string())
                        );
                        eprintln!("{:>line_len$}{} {}:{}:{}", "", Blue.bold().paint("-->"), config_name, line+1, col+1, line_len=line_len);
                        eprintln!("{:>line_len$} {}", "", Blue.bold().paint("|"), line_len=line_len);
                        eprintln!("{:>line_len$} {}  {}", line+1, Blue.bold().paint("|"), String::from_utf8_lossy(l), line_len=line_len);
                        eprintln!("{:>line_len$} {}  {:>col$}{}", "", Blue.bold().paint("|"), "", Red.bold().paint("^"),  col=col, line_len=line_len);
                    }
                    None => eprintln!("error: {}", e),
                }
            }
            None => eprintln!("error: {}", e),
        }
        2
    });
    let config: Config = ok_or_exit(config.try_into(), |s| {
        // TODO: Try to get line information for this stuff
        eprintln!(
            "{}: {}",
            Red.bold().paint("error"),
            Style::new().bold().paint(s)
        );
        2
    });
    if !args.is_present("check") {
        let (conn, screen_num) = connect(None).unwrap();
        let setup = conn.setup();
        let atom_edid = conn
            .intern_atom(false, b"EDID")
            .unwrap()
            .reply()
            .unwrap()
            .atom;
        let outputs = get_outputs(&conn, setup.roots[screen_num].root).unwrap();
        let root = setup.roots[screen_num].root;
        conn.randr_select_input(root, NotifyMask::SCREEN_CHANGE)
            .unwrap()
            .check()
            .unwrap();
        switch_setup(&config, &conn, &outputs, atom_edid, root, true);
        loop {
            match conn.wait_for_event() {
                Ok(Event::RandrScreenChangeNotify(_)) => {
                    switch_setup(&config, &conn, &outputs, atom_edid, root, false)
                }
                _ => (),
            }
        }
    }
}
