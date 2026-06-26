//! Load a schematic file and print what's in it.
//!
//! ```sh
//! cargo run --example inspect -- path/to/build.schem
//! ```

use std::collections::BTreeMap;

use chunkedge_schematics::load;

fn main() {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: cargo run --example inspect -- <file>");
            std::process::exit(2);
        }
    };

    let schem = match load(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to load {path}: {e}");
            std::process::exit(1);
        }
    };

    let [x, y, z] = schem.size();
    println!("file:    {path}");
    println!("size:    {x} x {y} x {z}  ({} cells)", schem.volume());
    println!("offset:  {:?}", schem.offset());
    println!("palette: {} entries", schem.palette().len());
    println!("block entities: {}", schem.block_entities().len());

    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut non_air = 0u64;
    for (_pos, block) in schem.blocks() {
        let name = format!("{:?}", block.state);
        if name.contains("air") {
            continue;
        }
        non_air += 1;
        *counts.entry(name).or_default() += 1;
    }

    println!("non-air blocks: {non_air}");

    let mut top: Vec<(String, u64)> = counts.into_iter().collect();
    top.sort_by_key(|b| std::cmp::Reverse(b.1));
    println!("most common:");
    for (name, count) in top.into_iter().take(10) {
        println!("  {count:>8}  {name}");
    }
}
