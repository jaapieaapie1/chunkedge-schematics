//! Build a tiny vanilla-structure schematic in memory, encode it to NBT, and
//! load it back through the public API.
//!
//! ```sh
//! cargo run --example roundtrip
//! ```

use chunkedge::nbt::{Compound, List, compound};
use chunkedge_schematics::{Format, from_bytes};

fn main() {
    // A 2x1x1 structure: stone at (0,0,0), east-facing oak stairs at (1,0,0).
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

    let schem = from_bytes(&bytes, Some(Format::Structure)).expect("load structure");

    let [x, y, z] = schem.size();
    println!(
        "loaded {x}x{y}x{z} structure, {} palette entries",
        schem.palette().len()
    );
    for ([bx, by, bz], block) in schem.blocks() {
        println!("  ({bx},{by},{bz}) -> {:?}", block.state);
    }
}
