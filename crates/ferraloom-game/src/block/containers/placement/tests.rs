use crate::{
    Behaviors,
    block::{
        BlockMethod, BlockPlaceContext, DirectionalPlaceContext, LevelReaderRef, PlacementHit,
        UseOnContext,
    },
    entity::{
        EntityData, EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
    level::{Level, LevelContext, Region, border::WorldBorder, chunk::LevelChunk},
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet},
};
use ferraloom_protocol::{component::ItemStack, play::InteractionHand};
use ferraloom_util::{
    BlockPos, ChunkPos, Direction, Uuid, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{io::Read, sync::Arc};

static BEHAVIORS: Behaviors = Behaviors::EMPTY;
const POS: BlockPos = BlockPos::new(8, 5, 8);
const SIDES: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];
pub(super) fn level() -> (Level, EntityRef) {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..1,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                )),
                ferraloom_chunk::BiomeId(0),
            ),
        ))
        .unwrap();
    let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies vanilla jar");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    level.set_block_tags(Arc::new(TagSet::<Block>::load(&resources).unwrap()));
    let player = level.next_entity_ref();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "placement".into(),
                level.next_entity_id(),
                Vec3::ZERO,
                GameType::Survival,
            ))),
        )
        .unwrap();
    (level, player)
}
fn set_border(level: &mut Level, mode: u8, head: BlockPos) {
    let mut border = WorldBorder::default();
    let (x, z, size) = match mode {
        0 => (0.0, 0.0, 59_999_968.0),
        1 => (f64::from(POS.x) + 0.5, f64::from(POS.z) + 0.5, 1.0),
        2 => (f64::from(head.x) + 0.5, f64::from(head.z) + 0.5, 1.0),
        3 => (f64::from(head.x) + 0.5, f64::from(head.z) + 0.5, 2.0),
        4 => (f64::from(head.x) - 0.5, f64::from(head.z) - 0.5, 1.0),
        _ => unreachable!(),
    };
    border.set_center(x, z);
    border.set_size(size);
    level.set_entity_world_border(border);
}
/// Real placement contexts cover each variant, fluid/click mask, partner class,
/// already-joined chest, bed border/head cell and nearest-direction tie.
#[test]
fn native_container_and_bed_placement_masks_match_vanilla() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/placement.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let (mut level, player) = level();
    let mut blocks = std::collections::BTreeSet::new();
    let mut groups = std::collections::BTreeMap::new();
    for line in vectors.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[2].parse().unwrap()).unwrap();
        let yaw: f32 = f[3].parse().unwrap();
        let pitch = f[4].parse().unwrap();
        let face = Direction::VALUES[f[6].parse::<usize>().unwrap()];
        let directional: i32 = f[7].parse().unwrap();
        let replaced = BlockState::from_id(f[8].parse().unwrap()).unwrap();
        {
            let data = level.entity_mut::<EntityData>(player).unwrap();
            data.yaw = yaw;
            data.pitch = pitch;
            data.set_shared_flag(1, f[5] == "true");
        }
        level
            .entity_mut::<crate::entity::living::LivingEntity>(player)
            .unwrap()
            .y_head_rot = f32::from_bits(u32::from_str_radix(f[13], 16).unwrap());
        level.set_block_state(POS, replaced).unwrap();
        for (side, state) in SIDES.into_iter().zip(f[10].split(',')) {
            level
                .set_block_state(
                    POS.relative(side, 1),
                    BlockState::from_id(state.parse().unwrap()).unwrap(),
                )
                .unwrap();
        }
        let forward = if directional < 0 {
            Direction::from_y_rot(f64::from(yaw))
        } else {
            let direction = Direction::VALUES[directional as usize];
            if direction.axis().is_horizontal() {
                direction
            } else {
                Direction::North
            }
        };
        set_border(&mut level, f[9].parse().unwrap(), POS.relative(forward, 1));
        let held = ItemStack::new(block.as_item(), 1, Default::default());
        let reader = LevelReaderRef::Server(&level);
        let actual = if directional < 0 {
            let context = BlockPlaceContext::new(
                &reader,
                UseOnContext::new(
                    Some(player),
                    InteractionHand::MainHand,
                    &held,
                    PlacementHit {
                        pos: POS,
                        face,
                        location: Vec3::new(8.5, 5.5, 8.5),
                        inside: false,
                    },
                ),
            )
            .unwrap();
            level.set_random_streams(Random::Legacy(LegacyRandomSource::new(263777)), 0);
            level.get_state_for_placement(block, &context)
        } else {
            let context = DirectionalPlaceContext::new(
                &reader,
                POS,
                Direction::VALUES[directional as usize],
                &held,
                face,
            )
            .unwrap();
            level.set_random_streams(Random::Legacy(LegacyRandomSource::new(263777)), 0);
            level.get_state_for_placement(block, &context)
        }
        .unwrap_or_else(|error| panic!("{line}: {error:?}"));
        assert_eq!(
            actual.map_or(-1, |state| i32::from(state.id())),
            f[11].parse::<i32>().unwrap(),
            "{line}"
        );
        assert_eq!(
            level.random().next_int(),
            f[12].parse::<i32>().unwrap(),
            "RNG {line}"
        );
        blocks.insert(block);
        *groups.entry(f[1]).or_insert(0) += 1;
    }
    assert_eq!(blocks.len(), 49);
    assert_eq!(groups.len(), 8);
    for block in blocks {
        assert!(super::supports(block, BlockMethod::GetStateForPlacement));
    }
}
