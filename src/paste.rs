//! Writing a [`Schematic`] into a live ChunkEdge world.

use chunkedge::block::BlockState;
use chunkedge::layer::ChunkLayer;
use chunkedge::layer::chunk::UnloadedChunk;
use chunkedge::{BlockPos, ChunkPos};

use crate::error::Result;
use crate::schematic::Schematic;

/// Paste `schem` into `layer` with its minimum corner at `origin`.
///
/// Chunks that do not yet exist are created on demand. The schematic's own
/// metadata offset is added to `origin`. `minecraft:air` cells are skipped so an
/// existing world is not erased where the schematic is empty. Block-entity NBT
/// is written after the block states are placed.
///
/// Returns the number of (non-air) block states placed. Cells that fall outside
/// the layer's vertical bounds are silently skipped, as ChunkEdge's
/// [`ChunkLayer::set_block`] does.
pub fn paste_into(schem: &Schematic, layer: &mut ChunkLayer, origin: BlockPos) -> Result<usize> {
    let [ox, oy, oz] = schem.offset();
    let base = BlockPos::new(origin.x + ox, origin.y + oy, origin.z + oz);

    let mut placed = 0usize;

    for ([x, y, z], block) in schem.blocks() {
        if block.state == BlockState::AIR {
            continue;
        }

        let pos = BlockPos::new(base.x + x as i32, base.y + y as i32, base.z + z as i32);
        ensure_chunk(layer, pos);

        if layer.set_block(pos, block.state).is_some() {
            placed += 1;
        }
        if let Some(nbt) = &block.nbt
            && let Some(slot) = layer.block_entity_mut(pos) {
                *slot = nbt.clone();
            }
    }

    for ([x, y, z], nbt) in schem.block_entities() {
        let pos = BlockPos::new(base.x + *x as i32, base.y + *y as i32, base.z + *z as i32);
        ensure_chunk(layer, pos);
        if let Some(slot) = layer.block_entity_mut(pos) {
            *slot = nbt.clone();
        }
    }

    Ok(placed)
}

fn ensure_chunk(layer: &mut ChunkLayer, pos: BlockPos) {
    let cpos = ChunkPos::from(pos);
    if layer.chunk(cpos).is_none() {
        layer.insert_chunk(cpos, UnloadedChunk::new());
    }
}
