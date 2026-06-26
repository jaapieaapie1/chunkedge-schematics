//! Sponge `.schem` loader (WorldEdit / FAWE), versions 1–3.
//!
//! A fixed-size cuboid stored as a palette (state-string -> index) plus a
//! varint-packed index stream. v1/v2 keep `Palette`/`BlockData` on the
//! schematic compound (v2 wraps it under `"Schematic"`); v3 nests `Palette` and
//! `Data` (note: `Data`, not `BlockData`) inside a `Blocks` compound, always
//! wrapped under `"Schematic"`.

use std::collections::HashMap;

use chunkedge::nbt::{Compound, List, Value};

use crate::block::parse_state_string;
use crate::error::{Result, SchematicError};
use crate::schematic::Schematic;
use chunkedge::layer::chunk::Block;

pub(crate) fn parse(root: &Compound) -> Result<Schematic> {
    let schem = if let Some(Value::Compound(c)) = root.get("Schematic") {
        c
    } else {
        root
    };

    let width = get_short(schem, "Width")?;
    let height = get_short(schem, "Height")?;
    let length = get_short(schem, "Length")?;
    let size = [width, height, length];
    let volume = (width as usize) * (height as usize) * (length as usize);

    let blocks_comp = match schem.get("Blocks") {
        Some(Value::Compound(c)) => Some(c),
        _ => None,
    };

    let palette_val = schem
        .get("Palette")
        .or_else(|| blocks_comp.and_then(|b| b.get("Palette")))
        .ok_or(SchematicError::MissingTag("Palette"))?;
    let Value::Compound(palette_comp) = palette_val else {
        return Err(SchematicError::WrongType("Palette"));
    };

    // v3 stores block data as "Data" inside Blocks, not "BlockData".
    let data_val = schem
        .get("BlockData")
        .or_else(|| blocks_comp.and_then(|b| b.get("Data")))
        .ok_or(SchematicError::MissingTag("BlockData"))?;
    let Value::ByteArray(byte_data) = data_val else {
        return Err(SchematicError::WrongType("BlockData"));
    };

    // Remap the file's (possibly sparse) palette indices to contiguous IR
    // positions, sorted by file index for deterministic ordering.
    let mut entries: Vec<(i32, &String)> = Vec::with_capacity(palette_comp.len());
    for (name, value) in palette_comp.iter() {
        let Value::Int(file_index) = value else {
            return Err(SchematicError::WrongType("Palette entry"));
        };
        entries.push((*file_index, name));
    }
    entries.sort_by_key(|(file_index, _)| *file_index);

    let mut ir_palette: Vec<Block> = Vec::with_capacity(entries.len());
    let mut remap: HashMap<i32, u32> = HashMap::with_capacity(entries.len());
    for (file_index, name) in entries {
        let state = parse_state_string(name)?;
        let pos = ir_palette.len() as u32;
        ir_palette.push(Block::new(state, None));
        remap.insert(file_index, pos);
    }

    // Each varint is one cell's palette index, in canonical order `x + z*W + y*W*L`.
    let bytes: &[u8] = chunkedge::nbt::conv::i8_slice_as_u8_slice(byte_data);
    let file_indices = read_varints(bytes, volume)?;

    let mut blocks: Vec<u32> = Vec::with_capacity(file_indices.len());
    for fi in file_indices {
        let fi = fi as i32;
        let &ir = remap.get(&fi).ok_or_else(|| {
            SchematicError::Malformed(format!("block data references unknown palette index {fi}"))
        })?;
        blocks.push(ir);
    }

    let block_entities = parse_block_entities(schem)?;
    let offset = parse_offset(schem)?;

    Schematic::new(size, ir_palette, blocks, block_entities, offset).ok_or_else(|| {
        SchematicError::Malformed("block data length / palette index mismatch".into())
    })
}

/// Read a `TAG_Short` dimension as an unsigned 16-bit value widened to `u32`.
fn get_short(c: &Compound, key: &'static str) -> Result<u32> {
    match c.get(key) {
        Some(Value::Short(v)) => Ok(*v as u16 as u32),
        Some(_) => Err(SchematicError::WrongType(key)),
        None => Err(SchematicError::MissingTag(key)),
    }
}

/// Decode a stream of LEB128 unsigned varints, expecting exactly `expected`
/// values.
fn read_varints(bytes: &[u8], expected: usize) -> Result<Vec<u32>> {
    let mut out = Vec::with_capacity(expected);
    let mut i = 0;
    while i < bytes.len() {
        let mut value: u32 = 0;
        let mut shift = 0u32;
        loop {
            if i >= bytes.len() {
                return Err(SchematicError::Malformed(
                    "truncated varint in BlockData".into(),
                ));
            }
            if shift >= 32 {
                return Err(SchematicError::Malformed(
                    "varint in BlockData exceeds 32 bits".into(),
                ));
            }
            let byte = bytes[i];
            i += 1;
            value |= ((byte & 0x7F) as u32) << shift;
            if (byte & 0x80) == 0 {
                break;
            }
            shift += 7;
        }
        out.push(value);
    }

    if out.len() != expected {
        return Err(SchematicError::Malformed(format!(
            "BlockData decoded to {} cells, expected {expected}",
            out.len()
        )));
    }

    Ok(out)
}

/// Read the relative `[x, y, z]` of a block entity, accepting either an
/// `IntArray` or a `List<Int>` of length 3.
fn read_pos(comp: &Compound) -> Result<[u32; 3]> {
    let pos = comp.get("Pos").ok_or(SchematicError::MissingTag("Pos"))?;
    let coords: [i32; 3] = match pos {
        Value::IntArray(a) => {
            if a.len() != 3 {
                return Err(SchematicError::Malformed("block entity Pos length".into()));
            }
            [a[0], a[1], a[2]]
        }
        Value::List(List::Int(a)) => {
            if a.len() != 3 {
                return Err(SchematicError::Malformed("block entity Pos length".into()));
            }
            [a[0], a[1], a[2]]
        }
        _ => return Err(SchematicError::WrongType("Pos")),
    };
    Ok([coords[0] as u32, coords[1] as u32, coords[2] as u32])
}

/// Collect block entities from `BlockEntities` (v2/v3) or `TileEntities` (v1).
fn parse_block_entities(schem: &Compound) -> Result<Vec<([u32; 3], Compound)>> {
    let entry = schem
        .get("BlockEntities")
        .or_else(|| schem.get("TileEntities"));
    let Some(value) = entry else {
        return Ok(Vec::new());
    };
    let Value::List(list) = value else {
        return Err(SchematicError::WrongType("BlockEntities"));
    };
    let entities = match list {
        List::Compound(entities) => entities,
        // An empty block-entity list may be typed differently (e.g. End).
        _ if list.is_empty() => return Ok(Vec::new()),
        _ => return Err(SchematicError::WrongType("BlockEntities")),
    };

    let mut out = Vec::with_capacity(entities.len());
    for comp in entities {
        let pos = read_pos(comp)?;
        out.push((pos, comp.clone()));
    }
    Ok(out)
}

/// Read the optional `Offset` (`IntArray` of length 3), defaulting to origin.
fn parse_offset(schem: &Compound) -> Result<[i32; 3]> {
    match schem.get("Offset") {
        None => Ok([0, 0, 0]),
        Some(Value::IntArray(a)) => {
            if a.len() != 3 {
                return Err(SchematicError::Malformed("Offset length".into()));
            }
            Ok([a[0], a[1], a[2]])
        }
        Some(_) => Err(SchematicError::WrongType("Offset")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chunkedge::block::BlockKind;
    use chunkedge::nbt::compound;

    fn stone_state() -> chunkedge::block::BlockState {
        BlockKind::from_str("stone").unwrap().to_state()
    }

    #[test]
    fn single_stone_v2() {
        let root = compound! {
            "Width" => 1_i16,
            "Height" => 1_i16,
            "Length" => 1_i16,
            "Palette" => compound! { "minecraft:stone" => 0_i32 },
            "BlockData" => vec![0_i8],
        };

        let schem = parse(&root).unwrap();
        assert_eq!(schem.size(), [1, 1, 1]);
        assert_eq!(schem.block_at(0, 0, 0).unwrap().state, stone_state());
        assert_eq!(schem.offset(), [0, 0, 0]);
        assert!(schem.block_entities().is_empty());
    }

    #[test]
    fn wrapped_under_schematic() {
        let inner = compound! {
            "Width" => 1_i16,
            "Height" => 1_i16,
            "Length" => 1_i16,
            "Palette" => compound! { "minecraft:stone" => 0_i32 },
            "BlockData" => vec![0_i8],
        };
        let root = compound! { "Schematic" => inner };

        let schem = parse(&root).unwrap();
        assert_eq!(schem.size(), [1, 1, 1]);
        assert_eq!(schem.block_at(0, 0, 0).unwrap().state, stone_state());
    }

    #[test]
    fn multibyte_varint_index() {
        // Index 200 encodes as a two-byte LEB128 varint (0xC8, 0x01).
        let mut palette = compound! { "minecraft:stone" => 0_i32 };
        for n in 1..=200_i32 {
            let name = if n == 200 {
                "minecraft:dirt"
            } else {
                match n % 3 {
                    0 => "minecraft:white_wool",
                    1 => "minecraft:glass",
                    _ => "minecraft:cobblestone",
                }
            }
            .to_string();
            palette.insert(name, Value::Int(n));
        }

        let block_data: Vec<i8> = vec![0x00, 0xC8u8 as i8, 0x01];

        let root = compound! {
            "Width" => 2_i16,
            "Height" => 1_i16,
            "Length" => 1_i16,
            "Palette" => palette,
            "BlockData" => block_data,
        };

        let schem = parse(&root).unwrap();
        assert_eq!(schem.size(), [2, 1, 1]);
        assert_eq!(schem.block_at(0, 0, 0).unwrap().state, stone_state());
        assert_eq!(
            schem.block_at(1, 0, 0).unwrap().state,
            BlockKind::from_str("dirt").unwrap().to_state()
        );
    }

    #[test]
    fn v3_blocks_compound() {
        let inner = compound! {
            "Width" => 1_i16,
            "Height" => 1_i16,
            "Length" => 1_i16,
            "Blocks" => compound! {
                "Palette" => compound! { "minecraft:stone" => 0_i32 },
                "Data" => vec![0_i8],
            },
        };
        let root = compound! { "Schematic" => inner };

        let schem = parse(&root).unwrap();
        assert_eq!(schem.block_at(0, 0, 0).unwrap().state, stone_state());
    }

    #[test]
    fn block_entity_with_offset() {
        let root = compound! {
            "Width" => 1_i16,
            "Height" => 1_i16,
            "Length" => 1_i16,
            "Palette" => compound! { "minecraft:chest" => 0_i32 },
            "BlockData" => vec![0_i8],
            "Offset" => Value::IntArray(vec![1, 2, 3]),
            "BlockEntities" => Value::List(List::Compound(vec![compound! {
                "Id" => "minecraft:chest",
                "Pos" => Value::IntArray(vec![0, 0, 0]),
            }])),
        };

        let schem = parse(&root).unwrap();
        assert_eq!(schem.offset(), [1, 2, 3]);
        assert_eq!(schem.block_entities().len(), 1);
        assert_eq!(schem.block_entities()[0].0, [0, 0, 0]);
    }

    #[test]
    fn varint_decoder_truncation() {
        // A lone continuation byte (high bit set) is a truncated varint.
        assert!(read_varints(&[0x80], 1).is_err());
    }

    #[test]
    fn varint_decoder_basic() {
        assert_eq!(read_varints(&[0x00, 0xC8, 0x01], 2).unwrap(), vec![0, 200]);
    }
}
