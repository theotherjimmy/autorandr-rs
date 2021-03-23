use x11rb::{
    connect,
    connection::Connection,
    cookie::Cookie,
    protocol::randr::{
        ConnectionExt as RandrExt, GetCrtcInfoReply, GetScreenResourcesCurrentReply, NotifyMask,
        Output, SetCrtcConfigReply, SetCrtcConfigRequest,
    },
    protocol::xproto::{Atom, ConnectionExt as XprotoExt, Timestamp, Window},
    protocol::Event,
};

use edid::{parse, EDID};
use nom::IResult;

use std::{
    collections::{HashMap, HashSet},
    error::Error,
};

mod app;
mod config;
use config::{Config, Mode, MonConfig, Monitor, Position, SingleConfig};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Read an EDID from an output.
fn get_edid<C: Connection>(conn: &C, atom_edid: Atom, output: Output) -> Result<Option<EDID>> {
    let cookie = conn.randr_get_output_property(output, atom_edid, 19u32, 0, 256, false, true)?;
    let props = cookie.reply()?;
    match parse(&props.data) {
        IResult::Done(_, edid) => Ok(Some(edid)),
        _ => Ok(None),
    }
}

/// A convienience function to complete a RandR getScreenResourcesCurrent request.
fn get_outputs<C: Connection>(conn: &C, root: Window) -> Result<GetScreenResourcesCurrentReply> {
    Ok(conn.randr_get_screen_resources_current(root)?.reply()?)
}

/// Construct an iterator that represents a mapping from Xorg output ids to monitor descriptions.
/// The monitor descriptions are generated from the EDID of the display.
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

/// Find the config that matches the attached monitors. On a match, this returns a tuple of
/// (name, frame buffer size, map from output to output config).
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

/// Create a map from human mode descriptions, in width and height, to Xorg mode identifiers
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

/// Create a request to disable a CRTC or a default CRTC config request.
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

/// Make the current Xorg server match the specified configuration.
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
            None => continue, // Skip this output; it's not in the setup
        };
        let mode_ids = modes
            .get(&conf.mode)
            .ok_or_else(|| format!("desired mode, {}, not found", conf.mode))?;
        let out_info = conn.randr_get_output_info(out, timestamp)?.reply()?;
        let mode = *out_info
            .modes
            .iter()
            .find(|&m| mode_ids.contains(m))
            .ok_or_else(|| format!("out does not support the desired mode, {:?}", conf.mode))?;
        let dest_crtc = if out_info.crtc != 0 {
            out_info.crtc
        } else {
            *out_info
                .crtcs
                .iter()
                .find(|&c| free_crtcs.contains(c))
                .ok_or_else(|| format!("No Crtc available for monitor id {}", out))?
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
            let rotation = if crtc_info.rotation != 0 {
                crtc_info.rotation
            } else {
                1
            };
            crtc_enables.push(SetCrtcConfigRequest {
                x,
                y,
                rotation,
                mode,
                outputs: vec![out].into(),
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

/// Called for each screen change notificaiton. Detects connected monitors and switches
/// to the appropriate config.
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

/// You know.
fn main() {
    let args = app::args().get_matches();
    // Unwrap below is safe, because the program exits from `get_matches` above when a config
    // is not provided.
    let config_name = args.value_of("config").unwrap();
    let config = Config::from_fname_or_exit(&config_name);
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
