//! Load popular Minecraft schematic file formats into a [ChunkEdge] world.
//!
//! Supported formats (each behind a cargo feature, all on by default):
//!
//! | Format | Extension | Feature |
//! |--------|-----------|---------|
//! | Sponge (WorldEdit / FAWE) | `.schem` | `sponge` |
//! | Litematica | `.litematic` | `litematica` |
//! | Vanilla structure block | `.nbt` | `structure` |
//!
//! The API is split into **parse** and **paste**: [`load`] (or [`from_bytes`])
//! returns a format-agnostic [`Schematic`], which you can inspect and then write
//! into a `ChunkLayer` with [`paste_into`]:
//!
//! ```no_run
//! # use chunkedge_schematics::{load, paste_into};
//! # use chunkedge::layer::ChunkLayer;
//! # use chunkedge::BlockPos;
//! # fn demo(layer: &mut ChunkLayer) -> Result<(), Box<dyn std::error::Error>> {
//! let schem = load("house.schem")?;
//! println!("size = {:?}", schem.size());
//! paste_into(&schem, layer, BlockPos::new(0, 64, 0))?;
//! # Ok(())
//! # }
//! ```
//!
//! [ChunkEdge]: https://github.com/ChunkEdge/ChunkEdge

pub mod block;
mod error;
pub mod formats;
mod paste;
mod schematic;

use std::io::Read;
use std::path::Path;

use chunkedge::nbt::Compound;

pub use error::{Result, SchematicError};
pub use formats::Format;
pub use paste::paste_into;
pub use schematic::Schematic;

/// Decode a NBT byte slice into its root compound.
pub fn read_nbt(bytes: &[u8]) -> Result<Compound> {
    let decompressed = maybe_gunzip(bytes)?;
    let mut slice = decompressed.as_slice();
    let (root, _name) = chunkedge::nbt::from_binary::<String>(&mut slice)
        .map_err(|e| SchematicError::Nbt(e.to_string()))?;
    Ok(root)
}

fn maybe_gunzip(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes).read_to_end(&mut out)?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

/// Load a schematic from a file, guessing the format from its extension and
/// falling back to content sniffing.
pub fn load<P: AsRef<Path>>(path: P) -> Result<Schematic> {
    let path = path.as_ref();
    let bytes = std::fs::read(path)?;
    let hint = path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(Format::from_extension);
    from_bytes(&bytes, hint)
}

/// Load a schematic from an in-memory byte slice.
///
/// `format` is an optional hint. When `None`, or
/// when the hinted format does not match the data, the format is detected from
/// the NBT structure.
pub fn from_bytes(bytes: &[u8], format: Option<Format>) -> Result<Schematic> {
    let root = read_nbt(bytes)?;
    let format = format
        .filter(|f| format_matches(*f, &root))
        .or_else(|| Format::detect(&root))
        .ok_or(SchematicError::UnknownFormat)?;
    formats::parse(format, &root)
}

fn format_matches(hint: Format, root: &Compound) -> bool {
    Format::detect(root).is_none_or(|detected| detected == hint)
}
