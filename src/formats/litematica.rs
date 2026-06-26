//! Litematica `.litematic` loader.
//!
//! Stores one or more *regions*, each with its own palette and packed index
//! array. A region may extend in the negative direction (its `Size` components
//! can be negative); this loader normalizes every region to a common coordinate
//! space, computes the enclosing bounding box, and merges them into one
//! canonical [`Schematic`].

use chunkedge::layer::chunk::Block;
use chunkedge::nbt::{Compound, List, Value};

use crate::block::resolve_from_compound;
use crate::error::{Result, SchematicError};
use crate::schematic::Schematic;

/// A single parsed region, normalized into schematic-space coordinates but not
/// yet placed into the merged bounding box.
struct Region {
    /// Minimum corner of the region in schematic space (inclusive).
    start: [i64; 3],
    /// Per-axis extent in blocks (always non-negative).
    extent: [u32; 3],
    /// Resolved block palette for this region.
    palette: Vec<Block>,
    /// Decoded palette indices in region-local order `y*sx*sz + z*sx + x`.
    indices: Vec<u32>,
    /// Block entities with region-local coordinates.
    tiles: Vec<([i64; 3], Compound)>,
}

/// Parse a Litematica root compound into a [`Schematic`].
pub(crate) fn parse(root: &Compound) -> Result<Schematic> {
    let regions = match root.get("Regions") {
        Some(Value::Compound(c)) => c,
        Some(_) => return Err(SchematicError::WrongType("Regions")),
        None => return Err(SchematicError::MissingTag("Regions")),
    };

    let mut parsed: Vec<Region> = Vec::new();
    for (_name, value) in regions.iter() {
        let Value::Compound(region) = value else {
            return Err(SchematicError::WrongType("Regions entry"));
        };
        parsed.push(parse_region(region)?);
    }

    if parsed.is_empty() {
        return Err(SchematicError::Malformed(
            "litematica has no regions".to_owned(),
        ));
    }

    let mut gmin = [i64::MAX; 3];
    let mut gmax = [i64::MIN; 3];
    for region in &parsed {
        for a in 0..3 {
            let end = region.start[a] + region.extent[a] as i64;
            gmin[a] = gmin[a].min(region.start[a]);
            gmax[a] = gmax[a].max(end);
        }
    }

    let size = [
        (gmax[0] - gmin[0]) as u32,
        (gmax[1] - gmin[1]) as u32,
        (gmax[2] - gmin[2]) as u32,
    ];
    let sx = size[0] as usize;
    let sy = size[1] as usize;
    let sz = size[2] as usize;
    let volume = sx * sy * sz;

    // Index 0 is air, the fill for bounding-box cells no region covers.
    let air = resolve_from_compound("minecraft:air", None)?;
    let mut palette: Vec<Block> = vec![Block::new(air, None)];
    let mut blocks: Vec<u32> = vec![0; volume];
    let mut block_entities: Vec<([u32; 3], Compound)> = Vec::new();

    for region in parsed {
        let base = palette.len() as u32;
        let region_len = region.palette.len();
        palette.extend(region.palette);

        let [ex, _ey, ez] = region.extent;
        let (ex, ez) = (ex as usize, ez as usize);
        let layer = ex * ez; // region cells per Y layer

        let off = [
            (region.start[0] - gmin[0]) as usize,
            (region.start[1] - gmin[1]) as usize,
            (region.start[2] - gmin[2]) as usize,
        ];

        for (i, &local_idx) in region.indices.iter().enumerate() {
            if (local_idx as usize) >= region_len {
                return Err(SchematicError::Malformed(format!(
                    "block state index {local_idx} out of range for palette of {region_len}"
                )));
            }
            // Region-local order is y*sx*sz + z*sx + x.
            let y = i / layer;
            let rem = i % layer;
            let z = rem / ex;
            let x = rem % ex;

            let ix = x + off[0];
            let iy = y + off[1];
            let iz = z + off[2];
            let ir = ix + iz * sx + iy * sx * sz;
            blocks[ir] = base + local_idx;
        }

        for (coord, nbt) in region.tiles {
            let bx = (coord[0] + off[0] as i64) as u32;
            let by = (coord[1] + off[1] as i64) as u32;
            let bz = (coord[2] + off[2] as i64) as u32;
            block_entities.push(([bx, by, bz], nbt));
        }
    }

    Schematic::new(size, palette, blocks, block_entities, [0, 0, 0]).ok_or_else(|| {
        SchematicError::Malformed("litematica produced inconsistent dimensions".to_owned())
    })
}

/// Parse and normalize one region compound.
fn parse_region(region: &Compound) -> Result<Region> {
    let size = get_compound(region, "Size")?;
    let (sx, sy, sz) = (
        get_i32(size, "x", "Size.x")?,
        get_i32(size, "y", "Size.y")?,
        get_i32(size, "z", "Size.z")?,
    );

    let position = get_compound(region, "Position")?;
    let (px, py, pz) = (
        get_i32(position, "x", "Position.x")?,
        get_i32(position, "y", "Position.y")?,
        get_i32(position, "z", "Position.z")?,
    );

    // Negative size grows in the negative direction, occupying `[pos+size+1 ..= pos]`
    // instead of `[pos ..= pos+size-1]`; normalize to a min corner plus non-negative extent.
    let raw = [sx as i64, sy as i64, sz as i64];
    let pos = [px as i64, py as i64, pz as i64];
    let mut start = [0i64; 3];
    let mut extent = [0u32; 3];
    for a in 0..3 {
        let s = raw[a];
        if s < 0 {
            start[a] = pos[a] + s + 1;
        } else {
            start[a] = pos[a];
        }
        extent[a] = s.unsigned_abs() as u32;
    }

    let palette_entries = match region.get("BlockStatePalette") {
        Some(Value::List(List::Compound(entries))) => entries,
        Some(Value::List(List::End)) => {
            return Err(SchematicError::Malformed(
                "empty BlockStatePalette".to_owned(),
            ));
        }
        Some(Value::List(_)) => return Err(SchematicError::WrongType("BlockStatePalette")),
        Some(_) => return Err(SchematicError::WrongType("BlockStatePalette")),
        None => return Err(SchematicError::MissingTag("BlockStatePalette")),
    };
    if palette_entries.is_empty() {
        return Err(SchematicError::Malformed(
            "empty BlockStatePalette".to_owned(),
        ));
    }

    let mut palette: Vec<Block> = Vec::with_capacity(palette_entries.len());
    for entry in palette_entries {
        let name = match entry.get("Name") {
            Some(Value::String(s)) => s.as_str(),
            Some(_) => return Err(SchematicError::WrongType("Name")),
            None => return Err(SchematicError::MissingTag("Name")),
        };
        let props = match entry.get("Properties") {
            Some(Value::Compound(c)) => Some(c),
            Some(_) => return Err(SchematicError::WrongType("Properties")),
            None => None,
        };
        let state = resolve_from_compound(name, props)?;
        palette.push(Block::new(state, None));
    }

    let longs = match region.get("BlockStates") {
        Some(Value::LongArray(v)) => v.as_slice(),
        Some(_) => return Err(SchematicError::WrongType("BlockStates")),
        None => return Err(SchematicError::MissingTag("BlockStates")),
    };
    let bits = bits_for(palette.len());
    let volume = (extent[0] as usize) * (extent[1] as usize) * (extent[2] as usize);
    let indices = unpack_states(longs, bits, volume)?;

    let mut tiles: Vec<([i64; 3], Compound)> = Vec::new();
    match region.get("TileEntities") {
        Some(Value::List(List::Compound(entries))) => {
            for entry in entries {
                let tx = get_i32(entry, "x", "TileEntity.x")? as i64;
                let ty = get_i32(entry, "y", "TileEntity.y")? as i64;
                let tz = get_i32(entry, "z", "TileEntity.z")? as i64;
                tiles.push(([tx, ty, tz], entry.clone()));
            }
        }
        Some(Value::List(List::End)) | None => {}
        Some(_) => return Err(SchematicError::WrongType("TileEntities")),
    }

    Ok(Region {
        start,
        extent,
        palette,
        indices,
        tiles,
    })
}

/// Bits per entry for a palette of `len` distinct blocks: `max(2, ceil(log2(len)))`.
fn bits_for(len: usize) -> u32 {
    let needed = if len <= 1 {
        0
    } else {
        // Bits required to represent indices `0..len-1`, i.e. ceil(log2(len)).
        u64::BITS - (len as u64 - 1).leading_zeros()
    };
    needed.max(2)
}

/// Unpack a post-1.16 packed-long array into `count` palette indices: each long
/// holds `floor(64/bits)` entries low-bits first, never straddling a boundary.
fn unpack_states(longs: &[i64], bits: u32, count: usize) -> Result<Vec<u32>> {
    if bits == 0 || bits > 32 {
        return Err(SchematicError::Malformed(format!(
            "invalid bits-per-entry {bits}"
        )));
    }
    let per_long = (64 / bits) as usize;
    let mask: u64 = (1u64 << bits) - 1;

    let needed_longs = count.div_ceil(per_long);
    if longs.len() < needed_longs {
        return Err(SchematicError::Malformed(format!(
            "BlockStates too short: have {} longs, need {needed_longs}",
            longs.len()
        )));
    }

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let long = longs[i / per_long] as u64;
        let shift = ((i % per_long) as u32) * bits;
        out.push(((long >> shift) & mask) as u32);
    }
    Ok(out)
}

/// Fetch a child compound, distinguishing missing from wrong-type.
fn get_compound<'a>(c: &'a Compound, key: &'static str) -> Result<&'a Compound> {
    match c.get(key) {
        Some(Value::Compound(inner)) => Ok(inner),
        Some(_) => Err(SchematicError::WrongType(key)),
        None => Err(SchematicError::MissingTag(key)),
    }
}

/// Fetch an integer child, accepting any NBT number type.
fn get_i32(c: &Compound, key: &str, label: &'static str) -> Result<i32> {
    match c.get(key) {
        Some(v) => v.as_i32().ok_or(SchematicError::WrongType(label)),
        None => Err(SchematicError::MissingTag(label)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chunkedge::nbt::compound;

    fn stone() -> chunkedge::block::BlockState {
        resolve_from_compound("minecraft:stone", None).unwrap()
    }

    fn dirt() -> chunkedge::block::BlockState {
        resolve_from_compound("minecraft:dirt", None).unwrap()
    }

    fn air() -> chunkedge::block::BlockState {
        resolve_from_compound("minecraft:air", None).unwrap()
    }

    #[test]
    fn bits_for_palette() {
        assert_eq!(bits_for(1), 2);
        assert_eq!(bits_for(2), 2); // max(2, 1)
        assert_eq!(bits_for(4), 2);
        assert_eq!(bits_for(5), 3);
        assert_eq!(bits_for(16), 4);
        assert_eq!(bits_for(17), 5);
    }

    #[test]
    fn unpack_non_spanning() {
        // bits=2 => 32 entries per long. Values 1,2,3 in the low bits.
        // 0b...111001 => entry0=1, entry1=2, entry2=3.
        let packed = (1u64 | (2 << 2) | (3 << 4)) as i64;
        let out = unpack_states(&[packed], 2, 3).unwrap();
        assert_eq!(out, vec![1, 2, 3]);
    }

    #[test]
    fn single_region_stone() {
        let region = compound! {
            "Size" => compound!{ "x" => 1_i32, "y" => 1_i32, "z" => 1_i32 },
            "Position" => compound!{ "x" => 0_i32, "y" => 0_i32, "z" => 0_i32 },
            "BlockStatePalette" => List::Compound(vec![
                compound!{ "Name" => "minecraft:air" },
                compound!{ "Name" => "minecraft:stone" },
            ]),
            "BlockStates" => vec![1_i64],
        };
        let root = compound! {
            "Regions" => compound!{ "Main" => region },
        };

        let schem = parse(&root).unwrap();
        assert_eq!(schem.size(), [1, 1, 1]);
        assert_eq!(schem.offset(), [0, 0, 0]);
        assert_eq!(schem.block_at(0, 0, 0).unwrap().state, stone());
    }

    #[test]
    fn negative_size_region() {
        // Size x = -1 at Position x = 0 occupies x = 0 (range [pos+size+1 ..= pos]).
        let region = compound! {
            "Size" => compound!{ "x" => -1_i32, "y" => 1_i32, "z" => 1_i32 },
            "Position" => compound!{ "x" => 0_i32, "y" => 0_i32, "z" => 0_i32 },
            "BlockStatePalette" => List::Compound(vec![
                compound!{ "Name" => "minecraft:air" },
                compound!{ "Name" => "minecraft:stone" },
            ]),
            "BlockStates" => vec![1_i64],
        };
        let root = compound! {
            "Regions" => compound!{ "Main" => region },
        };

        let schem = parse(&root).unwrap();
        assert_eq!(schem.size(), [1, 1, 1]);
        assert_eq!(schem.block_at(0, 0, 0).unwrap().state, stone());
    }

    #[test]
    fn multi_region_merge() {
        // Region A: 1x1x1 stone at x=0.
        let region_a = compound! {
            "Size" => compound!{ "x" => 1_i32, "y" => 1_i32, "z" => 1_i32 },
            "Position" => compound!{ "x" => 0_i32, "y" => 0_i32, "z" => 0_i32 },
            "BlockStatePalette" => List::Compound(vec![
                compound!{ "Name" => "minecraft:air" },
                compound!{ "Name" => "minecraft:stone" },
            ]),
            "BlockStates" => vec![1_i64],
        };
        // Region B: 1x1x1 dirt at x=2 (leaving x=1 uncovered -> air).
        let region_b = compound! {
            "Size" => compound!{ "x" => 1_i32, "y" => 1_i32, "z" => 1_i32 },
            "Position" => compound!{ "x" => 2_i32, "y" => 0_i32, "z" => 0_i32 },
            "BlockStatePalette" => List::Compound(vec![
                compound!{ "Name" => "minecraft:air" },
                compound!{ "Name" => "minecraft:dirt" },
            ]),
            "BlockStates" => vec![1_i64],
        };
        let root = compound! {
            "Regions" => compound!{ "A" => region_a, "B" => region_b },
        };

        let schem = parse(&root).unwrap();
        assert_eq!(schem.size(), [3, 1, 1]);
        assert_eq!(schem.block_at(0, 0, 0).unwrap().state, stone());
        assert_eq!(schem.block_at(1, 0, 0).unwrap().state, air());
        assert_eq!(schem.block_at(2, 0, 0).unwrap().state, dirt());
    }
}
