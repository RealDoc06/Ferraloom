use super::*;
use crate::{
    Behaviors,
    block::{BlockBehavior, entity},
    level::{LevelContext, Region, chunk::LevelChunk, redstone::Orientation},
};
use ferraloom_data::BlockState;
use ferraloom_util::ChunkPos;
use std::{cell::RefCell, io::Read, sync::Arc};

const POS: BlockPos = BlockPos::new(8, 64, 8);
const CHUNK: ChunkPos = ChunkPos::new(0, 0);
thread_local! {static TRACE:RefCell<Vec<&'static str>>=const {RefCell::new(Vec::new())};}
struct Observer;
impl BlockBehavior for Observer {
    fn neighbor_changed(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        assert_eq!(source, Block::BEEHIVE);
        assert!(level.try_mark_chunk_saved(CHUNK)?);
        TRACE.with_borrow_mut(|trace| trace.extend(["changed", "output"]));
        Ok(())
    }
}
static OBSERVER: Observer = Observer;
static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::COMPARATOR, &OBSERVER);
fn level() -> (Level, BlockEntityRef) {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let biome = ferraloom_chunk::BiomeId(0);
    level
        .add_chunk(LevelChunk::new(
            CHUNK,
            0..6,
            ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            ),
        ))
        .unwrap();
    level
        .set_block_state(POS, Block::BEEHIVE.default_state())
        .unwrap();
    level
        .set_block_state(POS.west(1), Block::COMPARATOR.default_state())
        .unwrap();
    level
        .set_block_entity(entity::create(POS, Block::BEEHIVE.default_state()).unwrap())
        .unwrap();
    let handle = level.block_entity_ref(POS).unwrap().unwrap();
    level.try_mark_chunk_saved(CHUNK).unwrap();
    level.take_effects();
    TRACE.with_borrow_mut(Vec::clear);
    (level, handle)
}
#[test]
fn native_beehive_set_changed_fire_class_bounds_and_empty_release_match() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/beehive-changed.tsv.gz").as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let mut checked = 0;
    for row in vectors.lines() {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, handle) = level();
        let target = POS.offset(
            f[1].parse().unwrap(),
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
        );
        level
            .set_block_state(target, BlockState::from_id(f[4].parse().unwrap()).unwrap())
            .unwrap();
        // Select the other side when the native case places fire at the probe.
        let probe = if target == POS.west(1) {
            POS.east(1)
        } else {
            POS.west(1)
        };
        level
            .set_block_state(probe, Block::COMPARATOR.default_state())
            .unwrap();
        level
            .block_entity_mut::<BlockEntityData>(handle)
            .unwrap()
            .has_level = f[5] == "true";
        level.try_mark_chunk_saved(CHUNK).unwrap();
        let fire = if f[5] == "true" {
            use crate::level::blockscan::BlockMatcher;
            level
                .find_blocks_between(POS.offset(-1, -1, -1), POS.offset(1, 1, 1))
                .filter_state(|state| state.block() == Block::FIRE)
                .any_matched()
                .unwrap()
        } else {
            false
        };
        assert_eq!(fire, f[6] == "true", "{row}");
        handle.set_changed(&mut level).unwrap();
        assert_eq!(TRACE.with_borrow(|trace| trace.join(";")), f[7], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 36);
}

#[test]
fn occupied_fire_emergency_remains_required_before_the_base_notification() {
    let (mut level, handle) = level();
    let mut random = ferraloom_util::random::LegacyRandomSource::new(0);
    level
        .block_entity_mut::<BeehiveBlockEntity>(handle)
        .unwrap()
        .populate_worldgen(&mut random);
    // No fire: the native override falls through to the base, even occupied.
    handle.set_changed(&mut level).unwrap();
    TRACE.with_borrow_mut(Vec::clear);
    level
        .set_block_state(POS.north(1), Block::FIRE.default_state())
        .unwrap();
    assert_eq!(
        handle.set_changed(&mut level),
        Err(LevelError::UnsupportedEntity(EntityType::BEE))
    );
    assert!(TRACE.with_borrow(Vec::is_empty));
}
