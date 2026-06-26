//! Load a schematic from disk and paste it into a live ChunkEdge server world.
//!
//! ```sh
//! cargo run --example paste -- path/to/build.schem
//! ```

use chunkedge::prelude::*;
use chunkedge_schematics::{Schematic, load, paste_into};

const ORIGIN: BlockPos = BlockPos::new(0, 64, 0);

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: cargo run --example paste -- <file.schem|.litematic|.nbt>");
        std::process::exit(2);
    });

    let schem = match load(&path) {
        Ok(schem) => {
            let [x, y, z] = schem.size();
            println!(
                "loaded {path}: {x}x{y}x{z}, {} palette entries",
                schem.palette().len()
            );
            schem
        }
        Err(e) => {
            eprintln!("failed to load {path}: {e}");
            std::process::exit(1);
        }
    };

    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(LoadedSchematic(schem))
        .add_systems(Startup, setup)
        .add_systems(Update, (init_clients, despawn_disconnected_clients))
        .run();
}

#[derive(Resource)]
struct LoadedSchematic(Schematic);

fn setup(
    mut commands: Commands,
    server: Res<Server>,
    dimensions: Res<DimensionTypeRegistry>,
    biomes: Res<BiomeRegistry>,
    schem: Res<LoadedSchematic>,
) {
    let mut layer = LayerBundle::new(ident!("overworld"), &dimensions, &biomes, &server);

    for z in -2..2 {
        for x in -2..2 {
            layer.chunk.insert_chunk([x, z], UnloadedChunk::new());
        }
    }

    match paste_into(&schem.0, &mut layer.chunk, ORIGIN) {
        Ok(placed) => println!("pasted {placed} blocks at {ORIGIN:?}"),
        Err(e) => eprintln!("paste failed: {e}"),
    }

    commands.spawn(layer);
}

fn init_clients(
    mut clients: Query<
        (
            &mut EntityLayerId,
            &mut VisibleChunkLayer,
            &mut VisibleEntityLayers,
            &mut Position,
            &mut GameMode,
        ),
        Added<Client>,
    >,
    layers: Query<Entity, (With<ChunkLayer>, With<EntityLayer>)>,
) {
    for (mut layer_id, mut visible_chunk, mut visible_entities, mut pos, mut mode) in &mut clients {
        let Ok(layer) = layers.single() else {
            continue;
        };
        layer_id.0 = layer;
        visible_chunk.0 = layer;
        visible_entities.0.insert(layer);
        pos.set([0.5, f64::from(ORIGIN.y) + 1.0, 0.5]);
        *mode = GameMode::Creative;
    }
}
