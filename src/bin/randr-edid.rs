use serde::Serialize;
use x11rb::{
    connect,
    connection::Connection,
    protocol::randr::{ConnectionExt as RandrExt, Output},
    protocol::xproto::Timestamp,
};

use autorandr_rs::{
    app::randr_edid, config::Monitor, edid_atom, get_monitors, get_outputs, ok_or_exit,
};

use std::collections::HashMap;
use std::error::Error;

#[derive(Serialize)]
struct ConfigOut {
    monitors: HashMap<String, Monitor>,
}

fn mon_name<C: Connection>(conn: &C, out: Output, ts: Timestamp) -> Result<String, Box<dyn Error>> {
    Ok(String::from_utf8(
        conn.randr_get_output_info(out, ts)?.reply()?.name,
    )?)
}

/// You know.
fn main() {
    // It may seem odd to thow away the arguments, but this bin does not accept
    // any command line arguments. This allows clap to handle --help and erroring
    // when a user passes anything to us
    let _ = randr_edid::args().get_matches();
    let (conn, screen_num) = ok_or_exit(connect(None), |e| {
        eprintln!("Could not connect to X server: {}", e);
        1
    });
    let setup = conn.setup();
    let atom_edid = ok_or_exit(edid_atom(&conn), |e| {
        eprintln!("Unable to intern the EDID atom: {}", e);
        1
    });
    let root = setup.roots[screen_num].root;
    let outs = ok_or_exit(get_outputs(&conn, root), |e| {
        eprintln!("Could not get outputs: {}", e);
        1
    });
    let monitors = get_monitors(&conn, &outs.outputs, atom_edid)
        .map(|(k, v)| {
            let new_k = ok_or_exit(mon_name(&conn, k, outs.timestamp), |e| {
                eprintln!("Could not read display name: {}", e);
                1
            });
            (new_k, v)
        })
        .collect::<HashMap<String, Monitor>>();
    let out = ConfigOut { monitors };
    // NOTE: The following unwrap is safe, ase there's no chance of error when
    // serializing a ConfigOut struct
    println!("{}", toml::to_string_pretty(&out).unwrap());
}
