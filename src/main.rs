use x11rb::{
    connect,
    connection::Connection,
    protocol::randr::{ConnectionExt as RandrExt, NotifyMask, Output},
    protocol::xproto::{Atom, ConnectionExt as XprotoExt, Window, Timestamp},
    protocol::Event,
};

use clap::{App, Arg};
use edid::{parse, Descriptor, EDID};
use nom::IResult;
use serde::{Deserialize, Deserializer, Serialize};
use toml::from_slice;

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    io::Read,
    path::Path,
};

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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
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

fn match_monitors<'c>(
    config: &'c ConfigIn,
    monitors: impl Iterator<Item = (Output, Monitor)> + 'c,
) -> impl Iterator<Item = (Output, String)> + 'c {
    monitors.filter_map(move |(out, mon)| {
        config
            .monitors
            .iter()
            .find(|(_k, v)| *v == &mon)
            .map(|(k, _v)| (out, k.clone()))
    })
}

fn match_config<'c>(
    config: &'c ConfigIn,
    monitors: &HashSet<String>,
) -> Option<(&'c String, &'c SingleConfig)> {
    config
        .configurations
        .iter()
        .find(|(_k, v)| &v.monitors == monitors)
}

fn get_config<'a, C: Connection>(
    config: &'a ConfigIn,
    conn: &'a C,
    outputs: &'a Vec<Output>,
    atom_edid: Atom,
) -> std::result::Result<(&'a String, HashMap<&'a Output, &'a MonConfig>), Vec<String>> {
    let monitors = get_monitors(conn, outputs, atom_edid);
    let mon_names: HashMap<Output, String> = match_monitors(config, monitors).collect();
    let (conf_name, config) = match_config(
        config,
        &mon_names.iter().map(|(_out, name)| name).cloned().collect(),
    ).ok_or_else(|| mon_names.values().cloned().collect::<Vec<_>>())?;
    Ok((
        conf_name,
        outputs
            .iter()
            .filter_map(|out| {
                mon_names
                    .get(out)
                    .and_then(|name| config.setup.get(name))
                    .map(|conf| (out, conf))
            })
            .collect(),
    ))
}

fn mode_map<C: Connection>(conn: &C, root: Window) -> Result<(HashMap<String, HashSet<u32>>, Timestamp)>{
    let resources = conn.randr_get_screen_resources(root)?.reply()?;
    let mut modes: HashMap<_, HashSet<u32>> = HashMap::with_capacity(resources.modes.len());
    for mi in resources.modes.iter() {
        let modestring = format!("{}x{}", mi.width, mi.height);
        modes.entry(modestring).or_default().insert(mi.id);
    }
    Ok((
        modes,
        resources.timestamp
    ))
}

fn apply_config<C: Connection>(
    conn: &C,
    outputs: &Vec<Output>,
    setup: HashMap<&Output, &MonConfig>,
    root: Window,
) -> Result<bool> {
    let (modes, timestamp) = mode_map(conn, root)?;
    let mut used_crtcs = HashSet::new();
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
        let x = iter.next().ok_or_else(|| str_err("Position is missing X component"))?.parse()?;
        let y = iter.next().ok_or_else(|| str_err("Position is missing Y component"))?.parse()?;
        Ok(Self{x, y})
    }

    fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::new_from_string(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Deserialize, Debug)]
struct MonConfig {
    mode: String,
    #[serde(deserialize_with = "Position::deserialize")]
    position: Position,
    primary: bool,
}

#[derive(Deserialize, Debug)]
struct SingleConfig {
    monitors: HashSet<String>,
    #[serde(flatten)]
    setup: HashMap<String, MonConfig>,
}

#[derive(Deserialize, Debug)]
struct ConfigIn {
    monitors: HashMap<String, Monitor>,
    configurations: HashMap<String, SingleConfig>,
}

fn read_to_bytes<P: AsRef<Path>>(fname: P) -> Result<Vec<u8>> {
    let mut file = std::fs::File::open(&fname)?;
    let mut bytes = Vec::with_capacity(4096);
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn switch_setup<C: Connection>(
    config: &ConfigIn,
    conn: &C,
    outputs: &Vec<Output>,
    edid: Atom,
    root: Window,
    force_print: bool,
) -> () {
    match get_config(&config, conn, &outputs, edid) {
        Ok((name, setup)) => {
            match apply_config(conn, &outputs, setup, root) {
                Ok(changed) => if changed || force_print {
                    println!("Monitor configuration: {}", name)
                }
                Err(e) => eprintln!("Error: {}", e),
            }
        }
        Err(matched_mons) => eprintln!("Error: Monitor change indicated, and no match was found for the monitors {:?}", matched_mons),
    }
}

fn main() {
    let args = App::new("An automatic X monitor configuration switcher")
        .version("0.1")
        .about("Watches for changes in connected monitors and switches configurations with EDIDs")
        .arg(
            Arg::with_name("config")
                .value_name("CONFIG")
                .help("The configuration file in TOML")
                .required(true)
                .index(1),
        )
        .get_matches();
    let config = args.value_of("config").unwrap();
    let config = read_to_bytes(config).unwrap();
    let config: ConfigIn = from_slice(&config).unwrap();
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
