# chunkedge-schematics

Load popular Minecraft schematic file formats into a [ChunkEdge] world.

## Supported formats

| Format | Extension | Cargo feature |
|--------|-----------|---------------|
| Sponge (WorldEdit / FAWE), v1–3 | `.schem` | `sponge` |
| Litematica | `.litematic` | `litematica` |
| Vanilla structure block | `.nbt` | `structure` |

All features are on by default; disable the ones you don't need:

```toml
[dependencies]
chunkedge-schematics = { git = "https://github.com/jaapieaapie1/chunkedge-schematics", branch = "main", default-features = false, features = ["sponge"] }
```

## Usage

The API is split into **parse** and **paste**. `load` (or `from_bytes`) returns a
format-agnostic [`Schematic`] you can inspect; `paste_into` writes it into a
`ChunkLayer`, creating chunks as needed:

```rust
use chunkedge_schematics::{load, paste_into};
use chunkedge_server::BlockPos;

let schem = load("house.schem")?;          // format guessed from extension, then content
println!("size = {:?}", schem.size());     // [x, y, z]

paste_into(&schem, layer, BlockPos::new(0, 64, 0))?;  // min corner at the origin
```

- The format is detected from the file extension, falling back to NBT content
  sniffing (so a mislabeled file still loads).
- gzip-compressed *and* raw NBT are both accepted.
- `minecraft:air` cells are skipped on paste, so existing terrain is preserved
  where the schematic is empty.
- Block-entity NBT (chests, signs, …) is carried through and written after the
  block states are placed.

## Examples

Run with `cargo run --example <name>`:

- **`inspect`** - load any schematic file and print its dimensions, palette
  size, block-entity count, and the most common blocks:
  `cargo run --example inspect -- path/to/build.schem`
- **`roundtrip`** - build a small structure in memory, encode it to NBT, and
  load it back through the public API. No file required:
  `cargo run --example roundtrip`
- **`paste`** - start a ChunkEdge server, load a file, and paste it into the
  world at `(0, 64, 0)`; connect a client to `localhost:25565` to walk it:
  `cargo run --example paste -- path/to/build.schem`

The full paste path is also covered end-to-end by
`tests/paste_into_world.rs`, which pastes into a live `ChunkLayer` (via
`chunkedge::testing::ScenarioSingleClient`) and reads the blocks back.

## How it works

Every format loader converts its native on-disk layout into one canonical
[`Schematic`]. A dense, palette-indexed block array using a single coordinate
ordering (`x + z*size_x + y*size_x*size_z`), plus block-entity NBT. Block-state
strings from schematic palettes (e.g.
`minecraft:oak_stairs[facing=east,half=bottom]`) are resolved against
ChunkEdge's generated `BlockKind` / `PropName` / `PropValue` tables.

[ChunkEdge]: https://github.com/ChunkEdge/ChunkEdge
[`Schematic`]: src/schematic.rs
