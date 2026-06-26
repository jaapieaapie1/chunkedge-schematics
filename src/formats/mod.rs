//! Per-format schematic parsers.
//!
//! Each submodule converts one on-disk format into the canonical
//! [`Schematic`](crate::schematic::Schematic) via a private
//! `parse(&Compound) -> Result<Schematic>` function. The set of formats that is
//! actually compiled in is controlled by cargo features.

use chunkedge::nbt::{Compound, Value};

use crate::error::{Result, SchematicError};
use crate::schematic::Schematic;

#[cfg(feature = "litematica")]
pub mod litematica;
#[cfg(feature = "sponge")]
pub mod sponge;
#[cfg(feature = "structure")]
pub mod structure;

/// The supported schematic file formats.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Format {
    /// Sponge `.schem` (WorldEdit / FAWE), versions 1–3.
    Sponge,
    /// Litematica `.litematic`.
    Litematica,
    /// Vanilla structure block `.nbt`.
    Structure,
}

impl Format {
    /// Guess the format from a file extension (case-insensitive).
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "schem" => Some(Format::Sponge),
            "litematic" => Some(Format::Litematica),
            "nbt" => Some(Format::Structure),
            _ => None,
        }
    }

    /// Guess the format by inspecting the root NBT compound's tag names.
    pub fn detect(root: &Compound) -> Option<Self> {
        let schem = match root.get("Schematic") {
            Some(Value::Compound(c)) => c,
            _ => root,
        };
        if schem.get("Regions").is_some() {
            return Some(Format::Litematica);
        }
        if schem.get("BlockData").is_some() || schem.get("Blocks").is_some_and(is_compound) {
            return Some(Format::Sponge);
        }
        if schem.get("blocks").is_some() && schem.get("palette").is_some() {
            return Some(Format::Structure);
        }
        None
    }

    fn feature_name(self) -> &'static str {
        match self {
            Format::Sponge => "sponge",
            Format::Litematica => "litematica",
            Format::Structure => "structure",
        }
    }
}

fn is_compound(v: &Value) -> bool {
    matches!(v, Value::Compound(_))
}

/// Parse an already-decoded root compound using the given format.
pub fn parse(format: Format, root: &Compound) -> Result<Schematic> {
    match format {
        #[cfg(feature = "sponge")]
        Format::Sponge => sponge::parse(root),
        #[cfg(feature = "litematica")]
        Format::Litematica => litematica::parse(root),
        #[cfg(feature = "structure")]
        Format::Structure => structure::parse(root),
        #[allow(unreachable_patterns)]
        other => Err(SchematicError::Unsupported(other.feature_name())),
    }
}
