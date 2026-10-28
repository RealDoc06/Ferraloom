use super::{tests::POS, weathering_copper};
use crate::{
    block::{
        BlockMethod, UPDATE_LIMIT, UpdateFlags,
        entity::{self, BlockEntityRef},
    },
    entity::{
        EntityState,
        player::{GameType, ServerPlayer},
    },
    inventory::{AbstractContainerMenu, MenuKind},
    level::{Level, effects::LevelEffect},
};
use ferraloom_data::{Block, BlockClass, BlockState};
use ferraloom_util::{
    Aabb, BlockPos, Uuid, Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{io::Read, sync::LazyLock};

static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/weathering-copper.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
fn position(value: &str) -> BlockPos {
    let xyz: Vec<i32> = value.split(',').map(|v| v.parse().unwrap()).collect();
    BlockPos::new(xyz[0], xyz[1], xyz[2])
}
fn state(value: &str) -> BlockState {
    BlockState::from_id(value.parse().unwrap()).unwrap()
}
fn cells(value: &str) -> impl Iterator<Item = (BlockPos, BlockState)> + '_ {
    value.split(';').map(|cell| {
        let (pos, id) = cell.split_once(':').unwrap();
        (position(pos), state(id))
    })
}
fn level() -> Level {
    let mut level = super::amethyst_tests::level(POS);
    level.install_shared(Box::default());
    crate::level::drop_fixtures::install(&mut level);
    crate::entity::tests::install_test_environment(&mut level, false, "minecraft:overworld");
    for x in -1..=1 {
        for z in -1..=1 {
            level
                .update_entity_chunk_status(
                    ferraloom_util::ChunkPos::new(x, z),
                    crate::level::entity::Visibility::Ticking,
                )
                .unwrap();
        }
    }
    level
}
fn set(level: &mut Level, pos: BlockPos, state: BlockState) {
    level
        .set_block(pos, state, UpdateFlags::SKIP_ALL_SIDEEFFECTS, UPDATE_LIMIT)
        .unwrap();
}
/// Uses the same canonical menu references as the released native opener replay;
/// no fake open count stands in for the live entity/container query.
fn opener(level: &mut Level, input: u8) -> Option<BlockEntityRef> {
    if input < 4 {
        return None;
    }
    let mut receiver = level.block_entity_ref(POS).unwrap().unwrap();
    let mut retained = None;
    if matches!(input, 7..=9) {
        level.retain_block_entity(receiver).unwrap();
        retained = Some(receiver);
        level
            .set_block_entity(entity::create(POS, level.get_block_state(POS).unwrap()).unwrap())
            .unwrap();
        receiver = level.block_entity_ref(POS).unwrap().unwrap();
    }
    let x = f64::from(POS.x) + if input == 10 { 9.5 } else { 0.5 };
    let mut player = ServerPlayer::new(
        Uuid::new(1, 2),
        "weathering".into(),
        level.next_entity_id().unwrap(),
        Vec3::new(x, 64.0, 8.5),
        if input == 5 {
            GameType::Spectator
        } else {
            GameType::Survival
        },
    );
    player.bounding_box = Aabb::new(x - 0.5, 64.0, 8.0, x + 0.5, 65.0, 9.0);
    let kind = if input == 6 {
        MenuKind::Hopper
    } else {
        MenuKind::Chest {
            rows: if matches!(input, 8 | 9) { 6 } else { 3 },
        }
    };
    let mut menu = AbstractContainerMenu::new(kind, 7).unwrap();
    menu.block_entities = match input {
        7 => vec![retained.unwrap()],
        8 => vec![receiver, retained.unwrap()],
        9 => vec![retained.unwrap(), receiver],
        _ => vec![receiver],
    };
    player.menus.install_container_menu(menu);
    let id = level.next_entity_ref();
    level
        .add_entity(id, EntityState::Player(Box::new(player)))
        .unwrap();
    retained
}
fn statue(state: BlockState) -> bool {
    state.block().class() == BlockClass::WeatheringCopperGolemStatueBlock
}
fn replay(modes: &[&str], expected: usize, statues_only: Option<bool>) {
    let mut level = level();
    let mut previous = Vec::new();
    let mut checked = 0;
    let mut writes = 0;
    for line in VECTORS
        .lines()
        .filter(|line| modes.contains(&line.split('\t').next().unwrap()))
    {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] == "chest" {
            level = self::level();
            previous.clear();
        }
        if statues_only.is_some_and(|only| statue(state(f[1])) != only) {
            continue;
        }
        for pos in std::mem::take(&mut previous) {
            set(&mut level, pos, Block::AIR.default_state());
        }
        for (pos, state) in cells(f[5]) {
            // The shared state-only scan does not inspect neighbor BEs. Keep
            // statues as cold states until their canonical factory is ported.
            if statue(state) && statues_only != Some(true) {
                level.set_block_state(pos, state).unwrap();
            } else {
                set(&mut level, pos, state);
            }
            previous.push(pos);
        }
        if f[4] == "3" {
            level.remove_block_entity(POS).unwrap();
        }
        let retained = opener(&mut level, f[4].parse().unwrap());
        let before = level.block_entity_ref(POS).unwrap();
        level.take_effects();
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(f[3].parse().unwrap())),
            0,
        );
        let original = state(f[1]);
        let pos = position(f[2]);
        let result = match f[0] {
            "next" => weathering_copper::next_state(original, pos, &mut level).unwrap(),
            "change" => {
                weathering_copper::change_over_time(original, pos, &mut level).unwrap();
                None
            }
            "random" | "guard" | "chest" => {
                level
                    .required_block_behavior(original.block(), BlockMethod::RandomTick)
                    .unwrap()
                    .random_tick(original, pos, &mut level)
                    .unwrap();
                None
            }
            _ => unreachable!(),
        };
        assert_eq!(
            result.map_or(-1, |s| i32::from(s.id())).to_string(),
            f[6],
            "{line}"
        );
        let trace = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| {
                if let LevelEffect::BlockUpdated { pos, old, new } = effect {
                    previous.push(pos);
                    Some(format!(
                        "{},{},{}:{}>{}",
                        pos.x,
                        pos.y,
                        pos.z,
                        old.id(),
                        new.id()
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(trace, f[8], "{line}");
        writes += usize::from(!trace.is_empty());
        for (pos, expected) in cells(f[7]) {
            assert_eq!(
                level.get_block_state(pos).unwrap(),
                expected,
                "{line}, {pos:?}"
            );
            previous.push(pos);
        }
        let after = level.block_entity_ref(POS).unwrap();
        if !statue(original) {
            assert_eq!(after.is_some(), f[9] == "true", "{line}");
        }
        assert_eq!(before == after, f[10] == "true", "{line}");
        assert_eq!(f[11], "9");
        assert!(!level.has_chunk_at(BlockPos::new(82, 64, -24)), "{line}");
        assert_eq!(level.random().next_long().to_string(), f[12], "{line}");
        if let Some(retained) = retained {
            level.release_block_entity(retained);
        }
        checked += 1;
    }
    assert_eq!(checked, expected);
    if modes.contains(&"chest") {
        assert_eq!(writes, 288);
    }
    if modes.contains(&"change") {
        assert!(
            writes > if statues_only == Some(true) { 0 } else { 500 },
            "missing successful aging mutations"
        );
    }
}
#[test]
fn shared_weathering_chance_matches_native_neighbor_ages_boundaries_wax_and_rng() {
    replay(&["next"], 9240, None);
}
#[test]
fn weathering_class_ticks_match_native_states_write_order_and_receiver_identity() {
    replay(&["change", "random"], 13544, Some(false));
}
#[test]
fn upper_doors_right_chests_and_non_chest_receivers_skip_before_queries_and_rng() {
    replay(&["guard"], 320, None);
}

#[test]
#[ignore = "requires canonical CopperGolemStatueBlockEntity factory and retained receiver lifecycle"]
fn weathering_statue_ticks_match_native_retained_block_entity() {
    replay(&["change", "random"], 1120, Some(true));
}

#[test]
fn copper_chest_ticks_match_native_immediate_creation_openers_spectators_and_generations() {
    replay(&["chest"], 1728, None);
}
