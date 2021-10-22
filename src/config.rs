//! Parser for the monitor-layout(5) configuration file
use edid::{Descriptor, EDID};
use kdl::{parse_document, KdlError, KdlNode as Node, KdlValue};
use thiserror::Error;

use std::{
    cmp::max,
    collections::HashMap,
    convert::TryFrom,
    fmt::{Display, Formatter},
    io::{Error as IoError, Read},
    num::ParseIntError,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0} is missing its {1} field")]
    MissingField(&'static str, &'static str),
    #[error("unknown monitor {1} in layout {0}")]
    UnknownMonitor(String, String),
    #[error("Value {0} type mismatch; expected {1}")]
    FieldTypeMisMatch(&'static str, &'static str),
    #[error("Field parse error")]
    ParseInt(#[from] ParseIntError),
    #[error("Node type mismatch, expected {0} found {1}")]
    NodeTypeMismatch(&'static str, String),
    #[error("Parse Error")]
    ParseError(#[from] KdlError),
    #[error("Duplicate singleton node {0}")]
    DuplicateSingleton(&'static str),
    #[error("Unexpected node {0}")]
    Unexpected(String),
    #[error("Io Error")]
    Io(#[from] IoError),
}

pub type Result<T> = std::result::Result<T, Error>;

trait FromNode: Sized {
    fn from_node(f: &Node) -> Result<Self>;
}

/// A position, expressed an <x>x<y>
#[derive(Debug)]
pub struct Position {
    pub x: i16,
    pub y: i16,
}

/// A monitor mode, expressed an <w>x<h>
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub struct Mode {
    pub w: u16,
    pub h: u16,
}

impl Mode {
    /// Create a mode that may contain both modes self and other
    pub fn union(&self, other: &Self) -> Self {
        Self {
            w: std::cmp::max(self.w, other.w),
            h: std::cmp::max(self.h, other.h),
        }
    }
}

impl Display for Mode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "{}x{}", self.w, self.h)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

#[derive(Debug)]
pub struct MonConfig {
    pub name: String,
    pub mode: Mode,
    pub position: Position,
    pub primary: bool,
}

fn extract_int_value(n: &Node, field: &'static str, name: &'static str) -> Result<i64> {
    match n.properties.get(field) {
        None => Err(Error::MissingField(name, field)),
        Some(KdlValue::Int(i)) => Ok(*i),
        Some(_) => Err(Error::FieldTypeMisMatch(name, "int")),
    }
}

fn extract_bool_value(n: &Node, field: &'static str, name: &'static str) -> Result<bool> {
    match n.properties.get(field) {
        None => Ok(false),
        Some(KdlValue::Boolean(v)) => Ok(*v),
        Some(_) => Err(Error::FieldTypeMisMatch(name, "boolean")),
    }
}
fn get_name(n: &Node, name: &'static str) -> Result<String> {
    match n.values.get(0) {
        None => Err(Error::MissingField(name, "name")),
        Some(KdlValue::String(out)) => Ok(out.clone()),
        Some(_) => Err(Error::FieldTypeMisMatch(name, "String")),
    }
}

impl FromNode for MonConfig {
    fn from_node(n: &Node) -> Result<Self> {
        if n.name != "monitor" {
            return Err(Error::NodeTypeMismatch("monitor", n.name.clone()));
        }
        let name = get_name(n, "layout.monitor")?;
        let x = extract_int_value(n, "x", "layout.monitor")? as i16;
        let y = extract_int_value(n, "y", "layout.monitor")? as i16;
        let w = extract_int_value(n, "w", "layout.monitor")? as u16;
        let h = extract_int_value(n, "h", "layout.monitor")? as u16;
        let primary = extract_bool_value(n, "primary", "layout.monitor")?;
        let mode = Mode { w, h };
        let position = Position { x, y };
        Ok(Self {
            name,
            mode,
            position,
            primary,
        })
    }
}

#[derive(Debug)]
struct LayoutIn {
    name: String,
    matches: Vec<String>,
    layout: Vec<MonConfig>,
}

impl FromNode for LayoutIn {
    fn from_node(n: &Node) -> Result<Self> {
        if n.name != "layout" {
            return Err(Error::NodeTypeMismatch("layout", n.name.clone()));
        }
        let name = get_name(n, "layout")?;
        let mut layout = Vec::new();
        let mut matches = None;
        for node in &n.children {
            match node.name.as_str() {
                "monitor" => layout.push(MonConfig::from_node(node)?),
                "matches" => {
                    if matches.is_none() {
                        let m: Result<Vec<_>> = node
                            .values
                            .iter()
                            .map(|v| match v {
                                KdlValue::String(mon_name) => Ok(mon_name.clone()),
                                _ => Err(Error::FieldTypeMisMatch("matches", "String")),
                            })
                            .collect();
                        matches = Some(m?);
                    } else {
                        return Err(Error::DuplicateSingleton("layout.matches"));
                    }
                }
                _ => return Err(Error::Unexpected(node.name.clone())),
            }
        }
        if let Some(matches) = matches {
            Ok(Self {
                name,
                matches,
                layout,
            })
        } else {
            Err(Error::MissingField("layout", "matches"))
        }
    }
}

pub struct SingleConfig {
    pub name: String,
    pub fb_size: Mode,
    pub setup: HashMap<Monitor, MonConfig>,
}

fn extract_optional_str(
    n: &Node,
    field: &'static str,
    name: &'static str,
) -> Result<Option<String>> {
    match n.properties.get(field) {
        None => Ok(None),
        Some(KdlValue::String(v)) => Ok(Some(v.clone())),
        Some(_) => Err(Error::FieldTypeMisMatch(name, "String")),
    }
}

pub struct Config(pub HashMap<Vec<Monitor>, SingleConfig>);

impl TryFrom<Vec<Node>> for Config {
    type Error = Error;
    fn try_from(document: Vec<Node>) -> Result<Self> {
        let mut layouts = Vec::new();
        let mut mon_names = HashMap::new();
        for cld in &document {
            match cld.name.as_str() {
                "layout" => layouts.push(LayoutIn::from_node(cld)?),
                "monitor" => {
                    let name = get_name(cld, "monitor")?;
                    if !cld.children.is_empty() {
                        Err(Error::Unexpected(format!("in monitor {}", name)))?
                    }
                    let product = extract_optional_str(cld, "product", "monitor")?;
                    let serial = extract_optional_str(cld, "serial", "monitor")?;
                    mon_names.insert(name, Monitor { product, serial });
                }
                _ => Err(Error::Unexpected(cld.name.clone()))?,
            }
        }
        let mut out = HashMap::new();
        for LayoutIn {
            name: conf_name,
            matches,
            layout: setup,
        } in layouts
        {
            let mut mon_set = Vec::with_capacity(matches.len());
            for m in matches.into_iter() {
                let mon_desc = mon_names
                    .get(&m)
                    .ok_or_else(|| Error::UnknownMonitor(conf_name.clone(), m))?;
                mon_set.push(mon_desc.clone())
            }
            mon_set.sort();
            let mut fb_size = Mode { w: 0, h: 0 };
            let mut next_setup = HashMap::with_capacity(setup.len());
            for mon in setup.into_iter() {
                let mon_desc = mon_names
                    .get(&mon.name)
                    .ok_or_else(|| Error::UnknownMonitor(conf_name.clone(), mon.name.clone()))?;
                fb_size.w = max(fb_size.w, mon.position.x as u16 + mon.mode.w);
                fb_size.h = max(fb_size.h, mon.position.y as u16 + mon.mode.h);
                next_setup.insert(mon_desc.clone(), mon);
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

impl Config {
    pub fn from_fname(config_name: &str) -> Result<Self> {
        let mut file = std::fs::File::open(&config_name)?;
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        let document = parse_document(&text)?;
        Config::try_from(document)
    }
}
