use super::{connected_tests::connected_level, stair_slab_tests::ticks, tests::POS};
use crate::{
    block::{BlockMethod, DirectionalPlaceContext, LevelAccessRef, LevelReaderRef},
    level::Level,
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{Pack, ResourceManager, TagSet},
    registries::fluid::Id as Fluid,
};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::Direction;
use std::{
    io::Read,
    sync::{Arc, LazyLock},
};
static VECTORS: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/building/fire-powder.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
});
static FLUID_TAGS: LazyLock<Arc<TagSet<Fluid>>> = LazyLock::new(|| {
    let jar =
        std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies native fluid tags");
    let resources =
        ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
    Arc::new(TagSet::load(&resources).unwrap())
});
fn cases(prefix: &str) -> impl Iterator<Item = &str> {
    VECTORS.lines().filter(move |line| line.starts_with(prefix))
}
fn state(id: &str) -> BlockState {
    BlockState::from_id(id.parse().unwrap()).unwrap()
}
fn side(id: &str) -> Direction {
    Direction::VALUES[id.parse::<usize>().unwrap()]
}
pub(super) fn level() -> Level {
    let mut level = connected_level();
    level.set_fluid_tags(Arc::clone(&FLUID_TAGS));
    level
}
fn neighbors(level: &mut Level, mask: u8, neighbor: BlockState) {
    for (index, side) in Direction::VALUES.into_iter().enumerate() {
        level
            .set_block_state(
                POS.relative(side, 1),
                if mask & (1 << index) == 0 {
                    Block::AIR.default_state()
                } else {
                    neighbor
                },
            )
            .unwrap();
    }
}
#[test]
fn concrete_powder_landing_matches_native_replaced_fluids_and_all_water_masks() {
    use crate::{
        entity::{EntityState, item::falling_block::FallingBlockEntity},
        level::effects::LevelEffect,
    };
    let mut level = level();
    let entity = level.next_entity_ref();
    level
        .add_entity(
            entity,
            EntityState::FallingBlock(Box::new(FallingBlockEntity::new(1, 42))),
        )
        .unwrap();
    let mut checked = 0;
    for line in include_str!("../../../tests/java/blocks/building/powder-land.tsv").lines() {
        let f: Vec<_> = line.split('\t').collect();
        let original = state(f[1]);
        neighbors(
            &mut level,
            f[3].parse().unwrap(),
            Block::WATER.default_state(),
        );
        level.set_block_state(POS, original).unwrap();
        level.take_effects();
        level
            .required_block_behavior(original.block(), BlockMethod::OnLand)
            .unwrap()
            .on_land(original, POS, state(f[2]), entity, &mut level)
            .unwrap();
        assert_eq!(
            level.get_block_state(POS).unwrap().id().to_string(),
            f[4],
            "{line}"
        );
        let mutations = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                LevelEffect::BlockUpdated { pos, new, .. } if pos == POS => Some(new.id()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(format!("{mutations:?}"), f[5], "{line}");
        checked += 1;
    }
    assert_eq!(checked, 5120);
}
#[test]
fn fire_placement_distinguishes_direct_fire_soul_selection_and_all_fuel_masks() {
    let mut level = level();
    let item = ItemStack::default();
    let mut matched = 0;
    for line in cases("fire-place\t") {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        neighbors(
            &mut level,
            f[2].parse().unwrap(),
            Block::OAK_PLANKS.default_state(),
        );
        level.set_block_state(POS.below(1), state(f[3])).unwrap();
        level
            .set_block_state(POS, Block::AIR.default_state())
            .unwrap();
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            Direction::North,
            &item,
            Direction::Up,
        )
        .unwrap();
        assert_eq!(
            level
                .get_state_for_placement(block, &context)
                .unwrap()
                .unwrap()
                .id(),
            f[4].parse::<u16>().unwrap(),
            "{line}"
        );
        assert_eq!(
            level
                .required_block_behavior(block, BlockMethod::CanSurvive)
                .unwrap()
                .can_survive(block.default_state(), POS, &LevelReaderRef::Server(&level))
                .unwrap(),
            f[5].parse::<bool>().unwrap(),
            "survive {line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 896);
}
#[test]
fn fire_shape_updates_preserve_age_or_select_soul_fire_using_live_neighbors() {
    let mut level = level();
    let mut matched = 0;
    for line in cases("fire-update\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let side = side(f[4]);
        neighbors(
            &mut level,
            f[2].parse().unwrap(),
            Block::OAK_PLANKS.default_state(),
        );
        level.set_block_state(POS.below(1), state(f[3])).unwrap();
        level.set_block_state(POS, current).unwrap();
        let result = level
            .required_block_behavior(current.block(), BlockMethod::UpdateShape)
            .unwrap()
            .update_shape(
                current,
                POS,
                side,
                POS.relative(side, 1),
                Block::AIR.default_state(),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(result.id(), f[5].parse::<u16>().unwrap(), "{line}");
        matched += 1;
    }
    assert_eq!(matched, 21546);
    assert!(ticks(&mut level).is_empty());
}
#[test]
fn powder_placement_matches_every_color_liquid_mask_and_waterlogged_support_footprint() {
    let mut level = level();
    let item = ItemStack::default();
    let mut matched = 0;
    for line in cases("powder-place\t").chain(cases("powder-neighbor\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        if f[0] == "powder-place" {
            neighbors(
                &mut level,
                f[3].parse().unwrap(),
                Block::WATER.default_state(),
            );
            level.set_block_state(POS, state(f[2])).unwrap();
        } else {
            neighbors(&mut level, 0, Block::AIR.default_state());
            level
                .set_block_state(POS, Block::AIR.default_state())
                .unwrap();
            level
                .set_block_state(POS.relative(side(f[3]), 1), state(f[2]))
                .unwrap();
        }
        let context = DirectionalPlaceContext::new(
            &LevelReaderRef::Server(&level),
            POS,
            Direction::North,
            &item,
            Direction::Up,
        )
        .unwrap();
        assert_eq!(
            level
                .get_state_for_placement(block, &context)
                .unwrap()
                .unwrap()
                .id(),
            f[4].parse::<u16>().unwrap(),
            "{line}"
        );
        matched += 1;
    }
    assert_eq!(matched, 7424);
}
#[test]
fn powder_shape_updates_keep_native_below_liquid_gate_and_falling_tick_delay() {
    let mut matched = 0;
    for line in cases("powder-update\t") {
        let f: Vec<_> = line.split('\t').collect();
        let current = state(f[1]);
        let side = side(f[4]);
        let mut level = level();
        neighbors(
            &mut level,
            f[3].parse().unwrap(),
            Block::WATER.default_state(),
        );
        level.set_block_state(POS, state(f[2])).unwrap();
        let result = level
            .required_block_behavior(current.block(), BlockMethod::UpdateShape)
            .unwrap()
            .update_shape(
                current,
                POS,
                side,
                POS.relative(side, 1),
                Block::STONE.default_state(),
                &mut LevelAccessRef::Server(&mut level),
            )
            .unwrap();
        assert_eq!(result.id(), f[5].parse::<u16>().unwrap(), "{line}");
        assert_eq!(ticks(&mut level), f[6], "ticks {line}");
        matched += 1;
    }
    assert_eq!(matched, 18432);
}
