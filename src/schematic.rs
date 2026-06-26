//! Format-agnostic in-memory representation of a schematic.
//!
//! Every format loader converts its native, on-disk layout into this single
//! canonical structure. Blocks live in a dense, palette-indexed array using one
//! fixed coordinate ordering (`x + z*size_x + y*size_x*size_z`), so consumers
//! (most importantly [`crate::paste::paste_into`]) never need to know which
//! format the data came from.

use chunkedge::layer::chunk::Block;
use chunkedge::nbt::Compound;

/// A loaded schematic: a fixed-size cuboid of blocks plus block-entity NBT.
#[derive(Clone, Debug)]
pub struct Schematic {
    /// Dimensions in blocks, `[x, y, z]`.
    pub(crate) size: [u32; 3],
    /// Distinct blocks. `blocks` holds indices into this list.
    pub(crate) palette: Vec<Block>,
    /// Palette index for every cell, ordered `x + z*size_x + y*size_x*size_z`.
    pub(crate) blocks: Vec<u32>,
    /// Block-entity NBT keyed by relative position `[x, y, z]`.
    pub(crate) block_entities: Vec<([u32; 3], Compound)>,
    /// Suggested placement offset relative to the paste origin.
    pub(crate) offset: [i32; 3],
}

impl Schematic {
    /// Build a schematic from its component parts.
    ///
    /// `blocks.len()` must equal `size_x * size_y * size_z` and every entry must
    /// be a valid index into `palette`; otherwise `None` is returned.
    pub(crate) fn new(
        size: [u32; 3],
        palette: Vec<Block>,
        blocks: Vec<u32>,
        block_entities: Vec<([u32; 3], Compound)>,
        offset: [i32; 3],
    ) -> Option<Self> {
        let volume = (size[0] as usize) * (size[1] as usize) * (size[2] as usize);
        if blocks.len() != volume {
            return None;
        }
        if blocks.iter().any(|&i| i as usize >= palette.len()) {
            return None;
        }
        Some(Self {
            size,
            palette,
            blocks,
            block_entities,
            offset,
        })
    }

    /// Dimensions in blocks, `[x, y, z]`.
    pub fn size(&self) -> [u32; 3] {
        self.size
    }

    /// Total number of cells (`size_x * size_y * size_z`).
    pub fn volume(&self) -> usize {
        self.blocks.len()
    }

    /// Placement offset relative to the paste origin.
    pub fn offset(&self) -> [i32; 3] {
        self.offset
    }

    /// The distinct block palette.
    pub fn palette(&self) -> &[Block] {
        &self.palette
    }

    /// Block-entity NBT entries, keyed by relative position.
    pub fn block_entities(&self) -> &[([u32; 3], Compound)] {
        &self.block_entities
    }

    /// Flat index for a coordinate, or `None` if out of bounds.
    fn index(&self, x: u32, y: u32, z: u32) -> Option<usize> {
        if x >= self.size[0] || y >= self.size[1] || z >= self.size[2] {
            return None;
        }
        let [sx, _sy, sz] = self.size;
        Some((x + z * sx + y * sx * sz) as usize)
    }

    /// The block at a relative coordinate, or `None` if out of bounds.
    pub fn block_at(&self, x: u32, y: u32, z: u32) -> Option<&Block> {
        let idx = self.index(x, y, z)?;
        let pal = self.blocks[idx] as usize;
        Some(&self.palette[pal])
    }

    /// Iterate over every cell as `([x, y, z], &Block)`.
    pub fn blocks(&self) -> impl Iterator<Item = ([u32; 3], &Block)> + '_ {
        let [sx, _sy, sz] = self.size;
        self.blocks.iter().enumerate().map(move |(i, &pal)| {
            let i = i as u32;
            let layer = sx * sz;
            let y = i / layer;
            let rem = i % layer;
            let z = rem / sx;
            let x = rem % sx;
            ([x, y, z], &self.palette[pal as usize])
        })
    }
}
