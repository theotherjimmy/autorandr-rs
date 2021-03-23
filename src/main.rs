use x11rb::{
    connect,
    connection::Connection,
    cookie::Cookie,
    protocol::randr::{
        ConnectionExt as RandrExt, GetScreenResourcesCurrentReply, NotifyMask, Output,
        GetCrtcInfoReply, SetCrtcConfigReply, SetCrtcConfigRequest,
    },
    protocol::xproto::{Atom, ConnectionExt as XprotoExt, Timestamp, Window},
    protocol::Event,
};

use ansi_term::{
    Colour::{Blue, Red},
    Style,
};
use edid::{parse, Descriptor, EDID};
use nom::IResult;
use serde::{Deserialize, Deserializer, Serialize};
use toml::from_slice;

use std::{
    cmp::max,
    collections::{HashMap, HashSet},
    convert::TryInto,
    error::Error,
    fmt::{Display, Formatter},
    hash::Hash,
    io::Read,
    path::Path,
};

mod app;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn get_edid<C: Connection>(conn: &C, atom_edid: Atom, output: Output) -> Result<Option<EDID>> {
    let cookie = conn.randr_get_output_property(output, atom_edid, 19u32, 0, 256, false, true)?;
    let props = cookie.reply()?;
    match parse(&props.data) {
        IResult::Done(_, edid) => Ok(Some(edid)),
        _ => Ok(None),
    }
}

fn get_outputs<C: Connection>(conn: &C, root: Window) -> Result<GetScreenResourcesCurrentReply> {
    Ok(conn.randr_get_screen_resources_current(root)?.reply()?)
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
    outputs
        .iter()
        .filter_map(move |out| match get_edid(conn, atom_edid, *out) {
            Ok(Some(m)) => Some((*out, Monitor::from(m))),
            Ok(None) => None,
            Err(e) => {
                eprintln!("Error reading EDID for Output {}: {}", out, e);
                None
            }
        })
}

fn get_config<'a, C: Connection>(
    config: &'a Config,
    conn: &'a C,
    outputs: &'a Vec<Output>,
    atom_edid: Atom,
) -> Option<(&'a String, &'a Mode, HashMap<Output, &'a MonConfig>)> {
    let out_to_mon: HashMap<_, _> = get_monitors(conn, outputs, atom_edid).collect();
    let mut monitors: Vec<_> = out_to_mon.values().cloned().collect();
    monitors.sort();
    let SingleConfig {
        name,
        setup,
        fb_size,
    } = config.0.get(&monitors)?;
    let mut out = HashMap::with_capacity(setup.len());
    for (output, mon) in out_to_mon.into_iter() {
        // Unwrap is checked by Config type on creating
        out.insert(output, setup.get(&mon).unwrap());
    }
    Some((name, fb_size, out))
}

fn mode_map<C: Connection>(
    conn: &C,
    root: Window,
) -> Result<(HashMap<Mode, HashSet<u32>>, Timestamp)> {
    let resources = conn.randr_get_screen_resources(root)?.reply()?;
    let mut modes: HashMap<_, HashSet<u32>> = HashMap::with_capacity(resources.modes.len());
    for mi in resources.modes.iter() {
        modes
            .entry(Mode {
                w: mi.width,
                h: mi.height,
            })
            .or_default()
            .insert(mi.id);
    }
    Ok((modes, resources.timestamp))
}

/// Create a request to disable a CRTC or a default CRTC config request
fn disable_crtc<'a, 'b>(crtc: u32, from: &'a GetCrtcInfoReply) -> SetCrtcConfigRequest<'b> {
    SetCrtcConfigRequest {
        crtc,
        timestamp: from.timestamp,
        config_timestamp: from.timestamp,
        x: from.x,
        y: from.y,
        mode: 0,
        rotation: from.rotation,
        outputs: Vec::new().into(),
    }
}

fn apply_config<C: Connection>(
    conn: &C,
    res: &GetScreenResourcesCurrentReply,
    fb_size: &Mode,
    setup: HashMap<Output, &MonConfig>,
    root: Window,
) -> Result<bool> {
    let (modes, timestamp) = mode_map(conn, root)?;
    let mut free_crtcs: HashSet<_> = res.crtcs.iter().collect();
    let _primary = conn.randr_get_output_primary(root)?.reply()?.output;
    let mut crtc_disables = Vec::with_capacity(res.crtcs.len());
    let mut crtc_enables = Vec::with_capacity(res.crtcs.len());
    let mut mm_w = 0;
    let mut mm_h = 0;
    // This loop can't easily be a filter_map, as it needs to be able to use '?'
    for &out in &res.outputs {
        let conf = match setup.get(&out) {
            Some(c) => c,
            None => continue // Skip this output; it's not in the setup
        };
        let mode_ids = modes.get(&conf.mode).ok_or_else(|| format!(
            "desired mode, {}, not found", conf.mode
        ))?;
        let out_info = conn.randr_get_output_info(out, timestamp)?.reply()?;
        let mode = *out_info.modes.iter().find(|&m| mode_ids.contains(m)).ok_or_else(||
            format!("out does not support the desired mode, {:?}", conf.mode)
        )?;
        let dest_crtc = if out_info.crtc != 0 {
            out_info.crtc
        } else {
            *out_info.crtcs.iter().find(|&c| free_crtcs.contains(c)).ok_or_else(||
                format!("No Crtc available for monitor id {}", out)
            )?
        };
        let crtc_info = conn.randr_get_crtc_info(dest_crtc, timestamp)?.reply()?;
        //TODO: This is not a correct computation of the screen size
        mm_w += out_info.mm_width;
        mm_h += out_info.mm_height;
        let Position { x, y } = conf.position;
        if x != crtc_info.x || y != crtc_info.y || mode != crtc_info.mode {
            // We're being conservative with screen changes in that we're disabling
            // any active CTRCs before they move or resize.
            if crtc_info.mode != 0 {
                crtc_disables.push(disable_crtc(dest_crtc, &crtc_info));
            }
            let rotation = if crtc_info.rotation != 0 { crtc_info.rotation } else { 1 };
            crtc_enables.push(SetCrtcConfigRequest {
                x, y, rotation, mode, outputs: vec![out].into(),
                ..disable_crtc(dest_crtc, &crtc_info)
            });
        }
        free_crtcs.remove(&dest_crtc);
    }
    // If there were CRTCs left over after allocating the next setup, ensure that they are
    // disabled
    for &crtc in free_crtcs.into_iter() {
        let info = conn.randr_get_crtc_info(crtc, timestamp)?.reply()?;
        if !info.outputs.is_empty() || info.mode != 0 {
            crtc_disables.push(disable_crtc(crtc, &info));
        }
    }

    if crtc_disables.is_empty() && crtc_enables.is_empty() {
        Ok(false)
    } else {
        // First, we disable any CTRCs that must be disabled
        let cookies: Vec<Cookie<C, SetCrtcConfigReply>> = crtc_disables
            .into_iter()
            .map(|req| req.send(conn))
            .collect::<std::result::Result<_, _>>()?;
        let _responses: Vec<SetCrtcConfigReply> = cookies
            .into_iter()
            .map(|cookie| cookie.reply())
            .collect::<std::result::Result<_, _>>()?;
        // Then we change the screen size
        conn.randr_set_screen_size(root, fb_size.w, fb_size.h, mm_w, mm_h)?
            .check()?;
        // Finally we enable and change modes of CRTCs
        let cookies: Vec<Cookie<C, SetCrtcConfigReply>> = crtc_enables
            .into_iter()
            .map(|req| req.send(conn))
            .collect::<std::result::Result<_, _>>()?;
        let _responses: Vec<SetCrtcConfigReply> = cookies
            .into_iter()
            .map(|cookie| cookie.reply())
            .collect::<std::result::Result<_, _>>()?;
        Ok(true)
    }
}

fn str_err(e: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}

#[derive(Deserialize, Debug)]
struct Position {
    x: i16,
    y: i16,
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
struct Mode {
    w: u16,
    h: u16,
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

#[derive(Deserialize, Debug)]
struct MonConfig {
    #[serde(deserialize_with = "Mode::deserialize")]
    mode: Mode,
    #[serde(deserialize_with = "Position::deserialize")]
    position: Position,
    primary: bool,
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

struct SingleConfig {
    name: String,
    fb_size: Mode,
    setup: HashMap<Monitor, MonConfig>,
}

struct Config(HashMap<Vec<Monitor>, SingleConfig>);

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

fn read_to_bytes<P: AsRef<Path>>(fname: P) -> Result<Vec<u8>> {
    let mut file = std::fs::File::open(&fname)?;
    let mut bytes = Vec::with_capacity(4096);
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn switch_setup<C: Connection>(
    config: &Config,
    conn: &C,
    edid: Atom,
    root: Window,
    force_print: bool,
) -> () {
    let res = match get_outputs(conn, root) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: Could not get outputs because {}", e);
            return;
        }
    };
    match get_config(&config, conn, &res.outputs, edid) {
        Some((name, fb_size, setup)) => match apply_config(conn, &res, fb_size, setup, root) {
            Ok(changed) => {
                if changed || force_print {
                    println!("Monitor configuration: {}", name)
                }
            }
            Err(e) => eprintln!("Error: {}", e),
        },
        None => eprintln!(
            "Error: Monitor change indicated, and the connected monitors did not match a config"
        ),
    }
}

fn ok_or_exit<T, E>(r: std::result::Result<T, E>, f: impl Fn(E) -> i32) -> T {
    match r {
        Ok(t) => t,
        Err(e) => std::process::exit(f(e)),
    }
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
                        eprintln!(
                            "{:>line_len$}{} {}:{}:{}",
                            "",
                            Blue.bold().paint("-->"),
                            config_name,
                            line + 1,
                            col + 1,
                            line_len = line_len
                        );
                        eprintln!(
                            "{:>line_len$} {}",
                            "",
                            Blue.bold().paint("|"),
                            line_len = line_len
                        );
                        eprintln!(
                            "{:>line_len$} {}  {}",
                            line + 1,
                            Blue.bold().paint("|"),
                            String::from_utf8_lossy(l),
                            line_len = line_len
                        );
                        eprintln!(
                            "{:>line_len$} {}  {:>col$}{}",
                            "",
                            Blue.bold().paint("|"),
                            "",
                            Red.bold().paint("^"),
                            col = col,
                            line_len = line_len
                        );
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
        let root = setup.roots[screen_num].root;
        conn.randr_select_input(root, NotifyMask::SCREEN_CHANGE)
            .unwrap()
            .check()
            .unwrap();
        switch_setup(&config, &conn, atom_edid, root, true);
        loop {
            match conn.wait_for_event() {
                Ok(Event::RandrScreenChangeNotify(_)) => {
                    switch_setup(&config, &conn, atom_edid, root, false)
                }
                _ => (),
            }
        }
    }
}
