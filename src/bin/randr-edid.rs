use x11rb::{
    connect,
    connection::Connection,
    protocol::xproto::ConnectionExt as XprotoExt,
    protocol::randr::ConnectionExt as RandrExt,
};
use serde::Serialize;

use autorandr_rs::{get_monitors, get_outputs, app::randr_edid, config::Monitor};

use std::collections::HashMap;

#[derive(Serialize)]
struct ConfigOut {
    monitors: HashMap<String, Monitor>,
}

/// You know.
fn main() {
    let _ = randr_edid::args().get_matches();
    let (conn, screen_num) = connect(None).unwrap();
    let setup = conn.setup();
    let atom_edid = conn
        .intern_atom(false, b"EDID")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    let root = setup.roots[screen_num].root;
    let outs = get_outputs(&conn, root).unwrap().outputs;
    let monitors = get_monitors(&conn, &outs, atom_edid).map(
        |(k, v)| (String::from_utf8(conn.randr_get_output_info(k, 0).unwrap().reply().unwrap().name).unwrap(), v)
    ).collect::<HashMap<_, _>>();
    let out = ConfigOut { monitors };
    println!("{}", toml::to_string_pretty(&out).unwrap());
}
