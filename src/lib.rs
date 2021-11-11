use std::error::Error;
use x11rb::{
    connection::Connection,
    protocol::randr::{ConnectionExt as RandrExt, GetScreenResourcesCurrentReply, Output},
    protocol::xproto::{Atom, ConnectionExt as XprotoExt, Window},
};

use edid::{parse, EDID};
use nom::IResult;
use miette::{IntoDiagnostic, Result};

pub mod app;
pub mod commands;
pub mod config;

use config::Monitor;

/// Read an EDID from an output.
pub fn get_edid<C: Connection>(
    conn: &C,
    atom_edid: Atom,
    output: Output,
) -> Result<Option<EDID>, Box<dyn Error>> {
    let cookie = conn.randr_get_output_property(output, atom_edid, 19u32, 0, 256, false, true)?;
    let props = cookie.reply()?;
    match parse(&props.data) {
        IResult::Done(_, edid) => Ok(Some(edid)),
        _ => Ok(None),
    }
}

/// A convienience function to complete a RandR getScreenResourcesCurrent request.
pub fn get_outputs<C: Connection>(
    conn: &C,
    root: Window,
) -> Result<GetScreenResourcesCurrentReply> {
    Ok(
        conn.randr_get_screen_resources_current(root)
            .into_diagnostic()?
            .reply()
            .into_diagnostic()?
    )
}

/// Construct an iterator that represents a mapping from Xorg output ids to monitor descriptions.
/// The monitor descriptions are generated from the EDID of the display.
pub fn get_monitors<'o, C: Connection>(
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

/// Get the atom that allows reading an EDID from an output
pub fn edid_atom<C: Connection>(conn: &C) -> Result<Atom> {
    Ok(
        conn.intern_atom(false, b"EDID")
            .into_diagnostic()?
            .reply()
            .into_diagnostic()?
            .atom
    )
}
