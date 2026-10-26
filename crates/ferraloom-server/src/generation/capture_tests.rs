use super::*;
use ferraloom_data::Block;
use ferraloom_game::level::generation::ChunkPyramid;
use ferraloom_worldgen::{
    generator::ChunkGeneratorAccess,
    region::SavedTick,
    structure::{
        TerrainAdjustment,
        references::StructureReferences,
        scattered_piece::ScatteredFeaturePiece,
        start::{StructurePiece, StructureStart},
        structures::jungle_temple::JungleTemplePiece,
    },
};

fn insert_terrain(state: &mut GenerationState, pos: ChunkPos) {
    let mut proto = ProtoChunk::empty(&state.context, pos).unwrap();
    proto.region.status = ChunkStatus::Terrain;
    state.holders.insert(
        pos.pack(),
        GenerationChunkHolder::new(proto, ChunkStatus::Terrain),
    );
}

fn assert_fresh(state: &mut GenerationState, pos: ChunkPos, registry: &RegistryAdapter) {
    let fresh = state.holders[&pos.pack()]
        .chunk
        .capture(&state.context, registry)
        .unwrap();
    let cached = state.capture_proto(pos.pack(), registry).unwrap();
    let bytes = |disk: &SerializableChunkData| {
        let mut bytes = Vec::new();
        ferraloom_nbt::NbtIo::write_compound(
            &mut bytes,
            ferraloom_nbt::Compression::None,
            &disk.write().unwrap(),
        )
        .unwrap();
        bytes
    };
    assert_eq!(bytes(&cached), bytes(&fresh));
}

#[test]
fn light_only_updates_keep_packed_terrain_and_match_fresh_bytes() {
    let (mut state, registry) = super::relight_tests::fixture();
    let pos = ChunkPos::new(0, 0);
    insert_terrain(&mut state, pos);
    let proto = &mut state.holders.get_mut(&pos.pack()).unwrap().chunk;
    proto.region.chunk.sections[0].set_block_state(0, 0, 0, Block::STONE.default_state());
    let mut padding = storage::ChunkSection {
        y: Some(-1),
        sky_light: Some(storage::ByteArray(vec![-1; 2048])),
        ..Default::default()
    };
    padding
        .extra
        .insert("future:padding", ferraloom_nbt::Tag::Long(42));
    proto.disk.sections = Some(vec![padding]);
    state.capture_proto(pos.pack(), &registry).unwrap();
    // Include both build-height sections, existing and new padding on either
    // side, explicit all-zero layers, and missing layers. None must stay
    // distinct from a stored zero array, even on an otherwise empty padding.
    for (y, layer, value) in [
        (0, LightLayer::Sky, Some(7)),
        (0, LightLayer::Block, Some(5)),
        (-1, LightLayer::Sky, Some(0)),
        (-2, LightLayer::Block, Some(9)),
        (2, LightLayer::Sky, Some(15)),
        (1, LightLayer::Block, Some(0)),
        (0, LightLayer::Sky, None),
        (0, LightLayer::Block, None),
        (-2, LightLayer::Block, None),
        (-1, LightLayer::Sky, None),
        (2, LightLayer::Sky, None),
        (3, LightLayer::Block, None),
    ] {
        state.apply_light_updates(LightUpdates {
            sections: vec![ferraloom_light::LightSectionUpdate {
                layer,
                section: SectionPos::new(pos.x, y, pos.z),
                data: value.map(|value| Arc::new(ferraloom_light::DataLayer::new(value))),
            }],
            ..Default::default()
        });
        assert!(state.captured_terrain[&pos.pack()].is_some());
        assert_fresh(&mut state, pos, &registry);
    }
}

#[test]
fn cached_terrain_keeps_outer_structure_piece_flags_and_proto_payload_fresh() {
    let (mut state, registry) = super::relight_tests::fixture();
    // FEATURES can mutate a referenced start outside its block-write radius.
    let pos = ChunkPos::new(8, 0);
    insert_terrain(&mut state, pos);
    let id = "minecraft:jungle_pyramid".parse().unwrap();
    state
        .holders
        .get_mut(&pos.pack())
        .unwrap()
        .chunk
        .region
        .structures
        .starts
        .insert(
            id,
            StructureStart::new(
                "minecraft:jungle_pyramid".parse().unwrap(),
                ferraloom_data::registries::worldgen_structure_type::Id::from_name(
                    "minecraft:jungle_temple",
                )
                .unwrap(),
                [pos.x, pos.z],
                0,
                TerrainAdjustment::None,
                vec![StructurePiece::JungleTemple(JungleTemplePiece {
                    scattered: ScatteredFeaturePiece::new(
                        [128, 0, 0],
                        [12, 10, 15],
                        ferraloom_util::Direction::North,
                    ),
                    placed_main_chest: false,
                    placed_hidden_chest: false,
                    placed_traps: [false; 2],
                })],
            ),
        );
    let before = state.capture_proto(pos.pack(), &registry).unwrap();
    state.invalidate_step_terrain(
        ChunkPos::new(0, 0),
        ChunkPyramid::generation().step_to(ChunkStatus::Features),
    );
    assert!(state.captured_terrain[&pos.pack()].is_some());
    let proto = &mut state.holders.get_mut(&pos.pack()).unwrap().chunk;
    proto.region.status = ChunkStatus::Features;
    let start = proto.region.structures.starts.values_mut().next().unwrap();
    start.references += 1;
    let StructurePiece::JungleTemple(piece) = &mut start.pieces[0] else {
        unreachable!()
    };
    piece.placed_main_chest = true;
    proto.region.structures.references.insert(
        "minecraft:jungle_pyramid".parse().unwrap(),
        StructureReferences::from_array(&[pos.pack()]),
    );
    proto.region.block_entities.insert(
        [129, 1, 1],
        ferraloom_nbt::parse_compound("{id:\"future:receiver\",x:129,y:1,z:1,opaque:[B;-1b,0b]}")
            .unwrap(),
    );
    proto.region.entities.push(
        ferraloom_nbt::parse_compound("{id:\"minecraft:pig\",CustomName:\"owned\"}").unwrap(),
    );
    proto.region.block_ticks.schedule(SavedTick {
        position: [129, 1, 1],
        kind: Block::STONE,
        priority: -2,
    });
    proto.region.chunk.post_processing.push([129, 1, 1]);
    let cached = state.capture_proto(pos.pack(), &registry).unwrap();
    assert_ne!(cached.structures, before.structures);
    assert_ne!(cached.status, before.status);
    assert_eq!(cached.block_entities.as_ref().unwrap().len(), 1);
    assert_eq!(cached.entities.as_ref().unwrap().len(), 1);
    assert_eq!(cached.block_ticks.as_ref().unwrap().len(), 1);
    assert_eq!(cached.post_processing.as_ref().unwrap()[0].len(), 1);
    assert_fresh(&mut state, pos, &registry);
}

#[test]
fn native_step_write_radius_invalidates_only_writable_terrain() {
    let (mut state, registry) = super::relight_tests::fixture();
    for x in [-2, -1, 0, 1, 2, 8] {
        let pos = ChunkPos::new(x, 0);
        insert_terrain(&mut state, pos);
        state.capture_proto(pos.pack(), &registry).unwrap();
    }
    let center = ChunkPos::new(0, 0);
    state.invalidate_step_terrain(
        center,
        ChunkPyramid::generation().step_to(ChunkStatus::Features),
    );
    for x in [-2, -1, 0, 1, 2, 8] {
        let pos = ChunkPos::new(x, 0);
        assert_eq!(
            state.captured_terrain.contains_key(&pos.pack()),
            x.abs() > 1
        );
        if x.abs() <= 1 {
            // Includes the raw section-write path, which bypasses light hooks.
            state
                .holders
                .get_mut(&pos.pack())
                .unwrap()
                .chunk
                .region
                .chunk
                .sections[1]
                .set_block_state(0, 15, 0, Block::STONE.default_state());
        }
        assert_fresh(&mut state, pos, &registry);
    }
    state.invalidate_step_terrain(
        center,
        ChunkPyramid::generation().step_to(ChunkStatus::Biomes),
    );
    assert!(!state.captured_terrain.contains_key(&center.pack()));
    assert!(
        state
            .captured_terrain
            .contains_key(&ChunkPos::new(1, 0).pack())
    );
    let biome = state.context.generator.data().biome_ids[&"minecraft:desert".parse().unwrap()];
    state
        .holders
        .get_mut(&center.pack())
        .unwrap()
        .chunk
        .region
        .chunk
        .sections[0]
        .biomes_mut()
        .set(0, 0, 0, biome);
    assert_fresh(&mut state, center, &registry);
    state.forget(center);
    assert!(!state.captured_terrain.contains_key(&center.pack()));
}
