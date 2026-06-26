//! End-to-end test: parse a schematic, paste it into a real `ChunkLayer`, and
//! read the blocks back out. Uses `ScenarioSingleClient`, which spins up an app
//! with the network plugin disabled and a ready-to-use chunk layer.

use chunkedge::BlockPos;
use chunkedge::block::BlockKind;
use chunkedge::layer::ChunkLayer;
use chunkedge::nbt::{Compound, List, compound};
use chunkedge::testing::ScenarioSingleClient;
use chunkedge_schematics::{Format, from_bytes, paste_into};

/// Build a 2x1x1 vanilla-structure schematic (stone, then east-facing oak
/// stairs) and return it as raw NBT bytes.
fn structure_bytes() -> Vec<u8> {
    let root: Compound = compound! {
        "size" => List::Int(vec![2, 1, 1]),
        "palette" => List::Compound(vec![
            compound! { "Name" => "minecraft:stone" },
            compound! {
                "Name" => "minecraft:oak_stairs",
                "Properties" => compound! {
                    "facing" => "east",
                    "half" => "bottom",
                    "waterlogged" => "false",
                },
            },
        ]),
        "blocks" => List::Compound(vec![
            compound! { "state" => 0_i32, "pos" => List::Int(vec![0, 0, 0]) },
            compound! { "state" => 1_i32, "pos" => List::Int(vec![1, 0, 0]) },
        ]),
    };
    let mut bytes = Vec::new();
    chunkedge::nbt::to_binary(&root, &mut bytes, None::<&str>).expect("encode nbt");
    bytes
}

#[test]
fn pastes_blocks_into_a_live_layer() {
    let schem = from_bytes(&structure_bytes(), Some(Format::Structure)).expect("load");

    let ScenarioSingleClient { mut app, layer, .. } = ScenarioSingleClient::new();

    let origin = BlockPos::new(0, 64, 0);
    let placed = {
        let mut chunk_layer = app
            .world_mut()
            .get_mut::<ChunkLayer>(layer)
            .expect("layer entity has a ChunkLayer");
        paste_into(&schem, &mut chunk_layer, origin).expect("paste")
    };
    assert_eq!(placed, 2, "both non-air blocks should be placed");

    let chunk_layer = app
        .world()
        .get::<ChunkLayer>(layer)
        .expect("layer entity has a ChunkLayer");

    let stone = BlockKind::from_str("stone").unwrap().to_state();
    let got = chunk_layer
        .block(BlockPos::new(0, 64, 0))
        .expect("block at origin")
        .state;
    assert_eq!(got, stone);

    let stairs_default = BlockKind::from_str("oak_stairs").unwrap().to_state();
    let stairs = chunk_layer
        .block(BlockPos::new(1, 64, 0))
        .expect("block at x=1")
        .state;
    assert_ne!(
        stairs, stairs_default,
        "stairs should keep their properties"
    );
}
