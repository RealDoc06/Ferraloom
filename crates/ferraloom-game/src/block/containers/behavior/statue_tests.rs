use crate::{
    Behaviors,
    block::{BlockBehavior, BlockMethod},
    level::{Level, LevelContext, LevelError, Region, chunk::LevelChunk, redstone::Orientation},
};
use ferraloom_data::{Block, BlockClass, BlockState};
use ferraloom_util::{BlockPos, ChunkPos, core::Plane};
use std::{cell::RefCell, collections::BTreeSet, sync::Arc};

const POS: BlockPos = BlockPos::new(8, 5, 8);
thread_local! {
    static NEIGHBORS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
struct ComparatorObserver;
impl BlockBehavior for ComparatorObserver {
    fn neighbor_changed(
        &self,
        _state: BlockState,
        pos: BlockPos,
        source: Block,
        orientation: Option<Orientation>,
        moved: bool,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        assert!(orientation.is_none());
        NEIGHBORS.with_borrow_mut(|trace| {
            trace.push(format!(
                "n:{},{},{}:{}:null:{moved}",
                pos.x,
                pos.y,
                pos.z,
                source.id()
            ));
        });
        Ok(())
    }
}
static OBSERVER: ComparatorObserver = ComparatorObserver;
static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::COMPARATOR, &OBSERVER);

fn level() -> Level {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::new(0, 0),
            0..1,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            ),
        ))
        .unwrap();
    for direction in Plane::Horizontal {
        level
            .set_block_state(
                POS.relative(direction, 1),
                Block::COMPARATOR.default_state(),
            )
            .unwrap();
    }
    level.take_effects();
    level
}

/// Port of CopperGolemStatueBlock#affectNeighborsAfterRemoval: comparator only.
#[test]
fn native_statue_removal_notifies_only_comparators_in_order() {
    let mut blocks = BTreeSet::new();
    let mut rows = 0;
    for line in include_str!("../../../../tests/java/blocks/entities/statue-removal.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields[0], "statue-remove");
        let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        assert!(matches!(
            state.block().class(),
            BlockClass::CopperGolemStatueBlock | BlockClass::WeatheringCopperGolemStatueBlock
        ));
        let moved = fields[2].parse().unwrap();
        let mut level = level();
        NEIGHBORS.with_borrow_mut(Vec::clear);
        level
            .required_block_behavior(state.block(), BlockMethod::AffectNeighborsAfterRemoval)
            .unwrap()
            .affect_neighbors_after_removal(state, POS, moved, &mut level)
            .unwrap();
        let mut expected = fields[3].split(';');
        assert_eq!(
            expected.next().unwrap(),
            format!("u:{}", state.block().id())
        );
        NEIGHBORS.with_borrow(|trace| assert_eq!(trace, &expected.collect::<Vec<_>>(), "{line}"));
        assert!(level.take_effects().is_empty(), "{line}");
        blocks.insert(state.block());
        rows += 1;
    }
    assert_eq!(rows, 16);
    assert_eq!(blocks.len(), 8);
}
