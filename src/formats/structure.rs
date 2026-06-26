//! Vanilla structure block `.nbt` loader.
//!
//! Handles the format written by structure blocks and the `/structure` command.
//! The root holds a `size` triple, a `palette` (or multi-variant `palettes`) of
//! block states, and a `blocks` list pairing each cell's palette index with its
//! relative position and optional block-entity NBT.

use chunkedge::layer::chunk::Block;
use chunkedge::nbt::{Compound, List, Value};

use crate::block::resolve_from_compound;
use crate::error::{Result, SchematicError};
use crate::schematic::Schematic;

/// Read an `[x, y, z]` triple stored as either an `Int` list or an `IntArray`.
fn read_xyz(value: &Value, ctx: &'static str) -> Result<[i32; 3]> {
    let slice: &[i32] = match value {
        Value::List(List::Int(v)) => v.as_slice(),
        Value::IntArray(v) => v.as_slice(),
        _ => return Err(SchematicError::WrongType(ctx)),
    };
    if slice.len() != 3 {
        return Err(SchematicError::Malformed(format!(
            "`{ctx}` must have length 3, got {}",
            slice.len()
        )));
    }
    Ok([slice[0], slice[1], slice[2]])
}

/// Resolve the active palette list, preferring the singular `palette` and
/// falling back to the first variant of a plural `palettes`.
fn palette_entries(root: &Compound) -> Result<&[Compound]> {
    if let Some(value) = root.get("palette") {
        let Value::List(list) = value else {
            return Err(SchematicError::WrongType("palette"));
        };
        return match list {
            List::Compound(v) => Ok(v.as_slice()),
            List::End => Ok(&[]),
            _ => Err(SchematicError::WrongType("palette")),
        };
    }

    if let Some(value) = root.get("palettes") {
        let Value::List(List::List(variants)) = value else {
            return Err(SchematicError::WrongType("palettes"));
        };
        let first = variants
            .first()
            .ok_or_else(|| SchematicError::Malformed("`palettes` is empty".to_owned()))?;
        return match first {
            List::Compound(v) => Ok(v.as_slice()),
            List::End => Ok(&[]),
            _ => Err(SchematicError::WrongType("palettes")),
        };
    }

    Err(SchematicError::MissingTag("palette"))
}

/// Resolve one palette entry (`{ Name: String, Properties?: Compound }`) into a
/// [`Block`].
fn resolve_entry(entry: &Compound) -> Result<Block> {
    let name = match entry.get("Name") {
        Some(Value::String(name)) => name.as_str(),
        Some(_) => return Err(SchematicError::WrongType("Name")),
        None => return Err(SchematicError::MissingTag("Name")),
    };
    let properties = match entry.get("Properties") {
        Some(Value::Compound(props)) => Some(props),
        Some(_) => return Err(SchematicError::WrongType("Properties")),
        None => None,
    };
    let state = resolve_from_compound(name, properties)?;
    Ok(Block::new(state, None))
}

/// Ensure `minecraft:air` is in `palette` (reusing an existing entry), returning
/// its index for use as the default fill of omitted cells.
fn ensure_air(palette: &mut Vec<Block>) -> Result<u32> {
    let air = resolve_from_compound("minecraft:air", None)?;
    if let Some(idx) = palette.iter().position(|block| block.state == air) {
        return Ok(idx as u32);
    }
    let idx = palette.len() as u32;
    palette.push(Block::new(air, None));
    Ok(idx)
}

pub(crate) fn parse(root: &Compound) -> Result<Schematic> {
    let size_value = root.get("size").ok_or(SchematicError::MissingTag("size"))?;
    let size_raw = read_xyz(size_value, "size")?;
    let mut size = [0u32; 3];
    for (axis, &component) in size_raw.iter().enumerate() {
        if component < 0 {
            return Err(SchematicError::Malformed(format!(
                "negative size component `{component}`"
            )));
        }
        size[axis] = component as u32;
    }
    let [sx, sy, sz] = size;

    let mut palette: Vec<Block> = palette_entries(root)?
        .iter()
        .map(resolve_entry)
        .collect::<Result<_>>()?;
    let air_index = ensure_air(&mut palette)?;

    let volume = (sx as usize) * (sy as usize) * (sz as usize);
    let mut blocks = vec![air_index; volume];
    let mut block_entities: Vec<([u32; 3], Compound)> = Vec::new();

    let blocks_value = root
        .get("blocks")
        .ok_or(SchematicError::MissingTag("blocks"))?;
    let Value::List(blocks_list) = blocks_value else {
        return Err(SchematicError::WrongType("blocks"));
    };
    let entries: &[Compound] = match blocks_list {
        List::Compound(v) => v.as_slice(),
        List::End => &[],
        _ => return Err(SchematicError::WrongType("blocks")),
    };

    for entry in entries {
        let state_index = match entry.get("state") {
            Some(Value::Int(index)) => *index,
            Some(_) => return Err(SchematicError::WrongType("state")),
            None => return Err(SchematicError::MissingTag("state")),
        };
        if state_index < 0 || state_index as usize >= palette.len() {
            return Err(SchematicError::Malformed(format!(
                "palette index `{state_index}` out of range"
            )));
        }

        let pos_value = entry.get("pos").ok_or(SchematicError::MissingTag("pos"))?;
        let pos = read_xyz(pos_value, "pos")?;
        if pos.iter().any(|&c| c < 0) {
            return Err(SchematicError::Malformed(format!(
                "negative block position `{pos:?}`"
            )));
        }
        let [x, y, z] = [pos[0] as u32, pos[1] as u32, pos[2] as u32];
        if x >= sx || y >= sy || z >= sz {
            return Err(SchematicError::Malformed(format!(
                "block position `{pos:?}` outside size `{size:?}`"
            )));
        }

        let index = (x + z * sx + y * sx * sz) as usize;
        blocks[index] = state_index as u32;

        if let Some(nbt) = entry.get("nbt") {
            let Value::Compound(nbt) = nbt else {
                return Err(SchematicError::WrongType("nbt"));
            };
            block_entities.push(([x, y, z], nbt.clone()));
        }
    }

    Schematic::new(size, palette, blocks, block_entities, [0, 0, 0])
        .ok_or_else(|| SchematicError::Malformed("inconsistent structure dimensions".to_owned()))
}

#[cfg(test)]
mod tests {
    use chunkedge::block::BlockKind;
    use chunkedge::nbt::{List, Value, compound};

    use super::parse;

    #[test]
    fn parses_single_stone() {
        let root = compound! {
            "size" => List::Int(vec![1, 1, 1]),
            "palette" => List::Compound(vec![compound! { "Name" => "minecraft:stone" }]),
            "blocks" => List::Compound(vec![compound! {
                "state" => 0_i32,
                "pos" => List::Int(vec![0, 0, 0]),
            }]),
        };

        let schematic = parse(&root).expect("structure should parse");
        assert_eq!(schematic.size(), [1, 1, 1]);

        let stone = BlockKind::from_str("stone").unwrap().to_state();
        assert_eq!(schematic.block_at(0, 0, 0).unwrap().state, stone);
    }

    #[test]
    fn accepts_int_array_dimensions() {
        // `size` and `pos` may be stored as IntArray rather than an Int list.
        let root = compound! {
            "size" => Value::IntArray(vec![1, 1, 1]),
            "palette" => List::Compound(vec![compound! { "Name" => "minecraft:stone" }]),
            "blocks" => List::Compound(vec![compound! {
                "state" => 0_i32,
                "pos" => Value::IntArray(vec![0, 0, 0]),
            }]),
        };

        let schematic = parse(&root).expect("structure should parse");
        assert_eq!(schematic.size(), [1, 1, 1]);
    }

    #[test]
    fn captures_block_entity() {
        let block_entity = compound! { "id" => "minecraft:chest" };
        let root = compound! {
            "size" => List::Int(vec![1, 1, 1]),
            "palette" => List::Compound(vec![compound! { "Name" => "minecraft:chest" }]),
            "blocks" => List::Compound(vec![compound! {
                "state" => 0_i32,
                "pos" => List::Int(vec![0, 0, 0]),
                "nbt" => block_entity.clone(),
            }]),
        };

        let schematic = parse(&root).expect("structure should parse");
        let entities = schematic.block_entities();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].0, [0, 0, 0]);
        assert!(matches!(
            entities[0].1.get("id"),
            Some(Value::String(id)) if id == "minecraft:chest"
        ));
    }

    #[test]
    fn missing_size_is_an_error() {
        let root = compound! {
            "palette" => List::Compound(vec![compound! { "Name" => "minecraft:stone" }]),
            "blocks" => List::Compound(Vec::new()),
        };
        assert!(parse(&root).is_err());
    }
}
