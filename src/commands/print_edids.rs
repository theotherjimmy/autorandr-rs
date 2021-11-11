use clap::ArgMatches;
use miette::{IntoDiagnostic, Result};
use tracing::debug;
use x11rb::{
    connect,
    connection::Connection,
    protocol::randr::{ConnectionExt as RandrExt, Output},
    protocol::xproto::Timestamp,
};

use crate::{config::Monitor, edid_atom, get_monitors, get_outputs};

fn mon_name<C: Connection>(conn: &C, out: Output, ts: Timestamp) -> Result<String> {
    Ok(String::from_utf8(
        conn.randr_get_output_info(out, ts)
            .into_diagnostic()?
            .reply()
            .into_diagnostic()?
            .name,
    ).into_diagnostic()?)
}

/// You know.
pub fn main(_: &ArgMatches<'_>) -> Result<()> {
    let (conn, screen_num) = connect(None).into_diagnostic()?;
    let setup = conn.setup();
    let atom_edid = edid_atom(&conn)?;
    let root = setup.roots[screen_num].root;
    let outs = get_outputs(&conn, root)?;
    let monitors = get_monitors(&conn, &outs.outputs, atom_edid)
        .map(|(k, v)| {
            let new_k = mon_name(&conn, k, outs.timestamp)?;
            Ok((new_k, v))
        })
        .collect::<Result<Vec<(String, Monitor)>>>()?;
    for (name, m) in monitors.into_iter() {
        debug!("{:?}", m);
        let product = m
            .product
            .map(|p| format!(r#"product="{}""#, p))
            .unwrap_or_default();
        let serial = m
            .serial
            .map(|s| format!(r#"serial="{}""#, s))
            .unwrap_or_default();
        println!(
            r#"monitor "{name}" {product} {serial}"#,
            name = name,
            serial = serial,
            product = product
        );
    }
    Ok(())
}
