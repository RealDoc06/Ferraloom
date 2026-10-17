use ferraloom_chunk::{BiomeId, ContainerKind, IdMap, LevelChunkSection, Strategy};
use ferraloom_data::{BLOCK_STATES, BlockState};
use ferraloom_light::{
    DataLayer, LevelLightEngine, LightLayer, LightWorld, SectionSnapshots, SectionType,
    light_dampening_into, shape_occludes,
};
use ferraloom_util::core::{BlockPos, ChunkPos, Direction, SectionPos};
use std::{
    borrow::Cow,
    io::{Cursor, Read},
    sync::{Arc, OnceLock},
};

struct Input<'a>(Cursor<&'a [u8]>);
impl Input<'_> {
    fn byte(&mut self) -> u8 {
        let mut b = [0];
        self.0.read_exact(&mut b).unwrap();
        b[0]
    }
    fn bool(&mut self) -> bool {
        self.byte() != 0
    }
    fn short(&mut self) -> u16 {
        let mut b = [0; 2];
        self.0.read_exact(&mut b).unwrap();
        u16::from_be_bytes(b)
    }
    fn int(&mut self) -> i32 {
        let mut b = [0; 4];
        self.0.read_exact(&mut b).unwrap();
        i32::from_be_bytes(b)
    }
    fn block(&mut self) -> BlockPos {
        BlockPos::new(self.int(), self.int(), self.int())
    }
    fn chunk(&mut self) -> ChunkPos {
        ChunkPos::new(self.int(), self.int())
    }
    fn section(&mut self) -> SectionPos {
        SectionPos::new(self.int(), self.int(), self.int())
    }
    fn rle(&mut self) -> Box<[u8; 2048]> {
        let mut bytes = Box::new([0; 2048]);
        let mut offset = 0;
        for _ in 0..self.int() {
            let count = usize::from(self.short());
            let value = self.byte();
            bytes[offset..offset + count].fill(value);
            offset += count;
        }
        assert_eq!(offset, 2048);
        bytes
    }
}
fn chunks() -> impl Iterator<Item = ChunkPos> {
    (-1..=1).flat_map(|z| (-1..=1).map(move |x| ChunkPos::new(x, z)))
}
fn empty_section() -> LevelChunkSection {
    static STRATEGIES: OnceLock<(Strategy<BlockState>, Strategy<BiomeId>)> = OnceLock::new();
    let (block, biome) = STRATEGIES.get_or_init(|| {
        (
            Strategy::new(
                ContainerKind::BlockStates,
                Arc::new(
                    IdMap::new(
                        (0..BLOCK_STATES.len())
                            .map(|id| BlockState::from_id(id as u16).unwrap())
                            .collect(),
                    )
                    .unwrap(),
                ),
            ),
            Strategy::new(
                ContainerKind::Biomes,
                Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap()),
            ),
        )
    });
    LevelChunkSection::new(block.clone(), biome.clone(), BiomeId(0))
}
fn world(min: i32, max: i32) -> SectionSnapshots<'static> {
    let mut world = SectionSnapshots::new(min, max);
    for chunk in chunks() {
        world.insert_chunk(
            chunk,
            (min..max)
                .map(|_| Some(Cow::Owned(empty_section())))
                .collect(),
        );
    }
    world
}
fn run(
    engine: &mut LevelLightEngine,
    world: &impl LightWorld,
    notifications: &mut Vec<(LightLayer, SectionPos)>,
) {
    notifications.extend(engine.run_light_updates(world).affected_sections);
}
fn initialize(
    engine: &mut LevelLightEngine,
    world: &impl LightWorld,
    lighted: bool,
    notifications: &mut Vec<(LightLayer, SectionPos)>,
) {
    for chunk in chunks() {
        for y in world.min_section()..world.max_section() {
            let pos = SectionPos::new(chunk.x, y, chunk.z);
            if world.section(pos).is_some_and(|s| !s.has_only_air()) {
                engine.update_section_status(pos, false);
            }
        }
    }
    run(engine, world, notifications);
    for chunk in chunks() {
        engine.set_light_enabled(world, chunk, lighted);
        engine.retain_data(chunk, false);
    }
}
fn disk_reload(
    engine: &mut LevelLightEngine,
    world: &impl LightWorld,
    sky: bool,
    notifications: &mut Vec<(LightLayer, SectionPos)>,
) {
    let mut new = LevelLightEngine::new(true, sky);
    for chunk in chunks() {
        new.retain_data(chunk, true);
        for y in world.min_section() - 1..=world.max_section() {
            for layer in [LightLayer::Block, LightLayer::Sky] {
                let pos = SectionPos::new(chunk.x, y, chunk.z);
                if let Some(data) = engine.data_layer(layer, pos) {
                    new.queue_section_data(layer, pos, Some(data.as_ref().clone()));
                }
            }
        }
    }
    initialize(&mut new, world, true, notifications);
    run(&mut new, world, notifications);
    *engine = new;
}
fn compare(
    input: &mut Input<'_>,
    engine: &LevelLightEngine,
    world: &impl LightWorld,
    name: &str,
    checkpoint: usize,
) {
    for z in -2..=2 {
        for x in -2..=2 {
            for y in world.min_section() - 1..=world.max_section() {
                let section = SectionPos::new(x, y, z);
                for layer in [LightLayer::Block, LightLayer::Sky] {
                    let expected_type = input.byte();
                    let actual_type = match engine.section_type(layer, section) {
                        SectionType::Empty => 0,
                        SectionType::LightOnly => 1,
                        SectionType::LightAndData => 2,
                    };
                    assert_eq!(
                        actual_type, expected_type,
                        "{name} checkpoint {checkpoint} {section:?} {layer:?} section state"
                    );
                    let exists = input.bool();
                    let actual = engine.data_layer(layer, section);
                    assert_eq!(
                        actual.is_some(),
                        exists,
                        "{name} checkpoint {checkpoint} {section:?} {layer:?} layer existence"
                    );
                    if exists {
                        let homogeneous = input.byte();
                        let expected = input.rle();
                        let actual = actual.unwrap();
                        let actual_homogeneous = (0..=15)
                            .find(|&v| actual.is_definitely_filled_with(v))
                            .unwrap_or(255);
                        assert_eq!(
                            actual_homogeneous, homogeneous,
                            "{name} checkpoint {checkpoint} {section:?} {layer:?} lazy allocation"
                        );
                        compare_bytes(
                            &actual.to_bytes(),
                            &expected,
                            name,
                            checkpoint,
                            section,
                            layer,
                            "stored",
                        );
                    }
                    let expected = input.rle();
                    let mut actual = Box::new([0; 2048]);
                    for i in 0..4096 {
                        let pos = section.origin().offset(
                            (i & 15) as i32,
                            (i >> 8) as i32,
                            ((i >> 4) & 15) as i32,
                        );
                        actual[i >> 1] |= engine.light_value(layer, pos) << ((i & 1) * 4);
                    }
                    compare_bytes(
                        &actual, &expected, name, checkpoint, section, layer, "queried",
                    );
                }
            }
        }
    }
}
fn compare_bytes(
    actual: &[u8; 2048],
    expected: &[u8; 2048],
    name: &str,
    checkpoint: usize,
    section: SectionPos,
    layer: LightLayer,
    kind: &str,
) {
    if let Some(index) = actual.iter().zip(expected).position(|(a, b)| a != b) {
        let nibble = if actual[index] & 15 != expected[index] & 15 {
            index * 2
        } else {
            index * 2 + 1
        };
        let pos = section.origin().offset(
            (nibble & 15) as i32,
            (nibble >> 8) as i32,
            ((nibble >> 4) & 15) as i32,
        );
        panic!(
            "{name} checkpoint {checkpoint} {kind} {layer:?} at {pos:?}: actual={}, vanilla={}",
            (actual[index] >> ((nibble & 1) * 4)) & 15,
            (expected[index] >> ((nibble & 1) * 4)) & 15
        );
    }
}
fn scenario(name: &str, bytes: &[u8]) {
    let mut input = Input(Cursor::new(bytes));
    assert_eq!(input.int(), 0x464c3236);
    let min = input.int();
    let max = input.int();
    let sky = input.bool();
    let mut world = world(min, max);
    let mut engine = LevelLightEngine::new(true, sky);
    let mut checkpoints = 0;
    let mut notifications = Vec::new();
    loop {
        match input.byte() {
            0 => break,
            command @ (1 | 4) => {
                let pos = input.block();
                let state = BlockState::from_id(input.short()).unwrap();
                let section_pos = SectionPos::from_block(pos);
                let section = world.section_mut(section_pos).unwrap();
                let was_empty = section.has_only_air();
                section.set_block_state(
                    (pos.x & 15) as u8,
                    (pos.y & 15) as u8,
                    (pos.z & 15) as u8,
                    state,
                );
                if command == 4 {
                    if was_empty != section.has_only_air() {
                        engine.update_section_status(section_pos, section.has_only_air());
                    }
                    engine.check_block(pos);
                }
            }
            2 => initialize(&mut engine, &world, input.bool(), &mut notifications),
            3 => {
                for chunk in chunks() {
                    engine.propagate_light_sources(&world, chunk);
                }
                run(&mut engine, &world, &mut notifications);
            }
            5 => {
                run(&mut engine, &world, &mut notifications);
                compare(&mut input, &engine, &world, name, checkpoints);
                let expected: Vec<_> = (0..input.int())
                    .map(|_| {
                        let layer = if input.bool() {
                            LightLayer::Sky
                        } else {
                            LightLayer::Block
                        };
                        (layer, input.section())
                    })
                    .collect();
                assert_eq!(
                    notifications, expected,
                    "{name} checkpoint {checkpoints}: section notifications and traversal order"
                );
                notifications.clear();
                checkpoints += 1;
                assert!(!engine.has_light_work());
            }
            6 => {
                let chunk = input.chunk();
                let enabled = input.bool();
                engine.set_light_enabled(&world, chunk, enabled);
            }
            7 => {
                let pos = input.section();
                let empty = input.bool();
                engine.update_section_status(pos, empty);
            }
            8 => {
                let chunk = input.chunk();
                let retain = input.bool();
                engine.retain_data(chunk, retain);
            }
            9 => {
                let pos = input.section();
                let layer = if input.bool() {
                    LightLayer::Sky
                } else {
                    LightLayer::Block
                };
                let value = input.byte();
                engine.queue_section_data(
                    layer,
                    pos,
                    (value != 255).then(|| DataLayer::new(value)),
                );
            }
            10 => disk_reload(&mut engine, &world, sky, &mut notifications),
            command => panic!("unknown oracle command {command}"),
        }
    }
    assert!(checkpoints > 0);
    assert_eq!(input.0.position(), bytes.len() as u64);
}
macro_rules! scenario_tests {
    ($($name:ident),* $(,)?) => { $(#[test] fn $name() { scenario(stringify!($name),include_bytes!(concat!("fixtures/",stringify!($name),".bin"))); })* };
}
scenario_tests!(
    open,
    caves,
    room,
    directional,
    dampening,
    overhang,
    boundaries,
    gaps,
    batches,
    lifecycle
);

#[test]
fn all_occlusion_face_pairs_match_vanilla() {
    let bytes = include_bytes!("fixtures/faces.bin");
    let mut input = Input(Cursor::new(bytes));
    let count = input.int();
    let states: Vec<_> = (0..count)
        .map(|_| BlockState::from_id(input.short()).unwrap())
        .collect();
    for &a in &states {
        for &b in &states {
            for direction in Direction::VALUES {
                assert_eq!(
                    shape_occludes(a, b, direction),
                    input.bool(),
                    "face {}, {} {direction:?}",
                    a.id(),
                    b.id()
                );
                assert_eq!(
                    light_dampening_into(a, b, direction, 3),
                    input.byte(),
                    "dampening {}, {} {direction:?}",
                    a.id(),
                    b.id()
                );
            }
        }
    }
    assert_eq!(input.0.position(), bytes.len() as u64);
}

#[test]
fn shape_flags_match_vanilla_states() {
    let mut input = Input(Cursor::new(include_bytes!("fixtures/shape_flags.bin")));
    let count = input.int();
    assert_eq!(count as usize, BLOCK_STATES.len());
    for id in 0..count {
        let state = BlockState::from_id(id as u16).unwrap();
        assert_eq!(
            ferraloom_light::is_empty_shape(state),
            input.bool(),
            "empty-shape flag for state {id}"
        );
    }
}
