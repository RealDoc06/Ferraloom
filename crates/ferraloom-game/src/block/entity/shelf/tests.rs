use super::*;
use crate::{
    Behaviors,
    block::{BlockBehavior, entity},
    level::{
        LevelContext, Region,
        chunk::LevelChunk,
        effects::LevelEffect,
        game_event::test_support::{self, Observation},
        redstone::Orientation,
    },
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, ChunkPos};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

const POS: BlockPos = BlockPos::new(8, 64, 8);
const CHUNK: ChunkPos = ChunkPos::new(0, 0);
thread_local! {
    static RECEIVER: Cell<Option<BlockEntityRef>> = const { Cell::new(None) };
    static MUTATE: Cell<bool> = const { Cell::new(false) };
    static TRACE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
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
        assert!(level.try_mark_chunk_saved(CHUNK)?);
        assert!(
            level.take_effects().is_empty(),
            "event/update follow the base callback"
        );
        let handle = RECEIVER.get().unwrap();
        let state = level.block_entity::<BlockEntityData>(handle)?.block_state;
        assert_eq!(source, state.block());
        TRACE.with_borrow_mut(|trace| {
            trace.extend(["changed".into(), format!("output:{}", state.id())])
        });
        if MUTATE.get() {
            level
                .block_entity_mut::<BlockEntityData>(handle)?
                .block_state = Block::SPRUCE_SHELF.default_state();
        }
        Ok(())
    }
}
static OBSERVER: Observer = Observer;
static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::COMPARATOR, &OBSERVER);

#[test]
fn native_shelf_set_changed_preserves_base_event_update_order_and_fresh_state() {
    let mut checked = 0;
    for row in include_str!("../../../../tests/java/blocks/entities/shelf-changed.tsv").lines() {
        let f: Vec<_> = row.split('\t').collect();
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
        let state = Block::OAK_SHELF.default_state();
        level.set_block_state(POS, state).unwrap();
        level
            .set_block_state(POS.east(1), Block::COMPARATOR.default_state())
            .unwrap();
        level
            .set_block_entity(entity::create(POS, state).unwrap())
            .unwrap();
        let handle = level.block_entity_ref(POS).unwrap().unwrap();
        let data = level.block_entity_mut::<BlockEntityData>(handle).unwrap();
        data.has_level = f[1] == "true";
        data.block_state = BlockState::from_id(f[2].parse().unwrap()).unwrap();
        RECEIVER.set(Some(handle));
        MUTATE.set(f[4] == "true");
        TRACE.with_borrow_mut(Vec::clear);
        test_support::observe(&mut level);
        level.take_effects();
        level.try_mark_chunk_saved(CHUNK).unwrap();
        match f[3] {
            "0" => handle.set_changed(&mut level).unwrap(),
            "1" => set_changed_with_event(handle, None, &mut level).unwrap(),
            "2" => set_changed_with_event(handle, Some(GameEvent::BLOCK_DEACTIVATE), &mut level)
                .unwrap(),
            _ => panic!("native shelf event mode"),
        }
        let mut trace = TRACE.with_borrow(Clone::clone);
        for observation in test_support::take(&mut level) {
            match observation {
                Observation::GameEvent {
                    event,
                    position,
                    source,
                    affected_state,
                } => {
                    assert_eq!(position, ferraloom_util::Vec3::new(8.5, 64.5, 8.5), "{row}");
                    assert!(source.is_none(), "{row}");
                    trace.push(format!(
                        "event:{}:{}",
                        event.name(),
                        affected_state.unwrap().id()
                    ));
                }
                Observation::Effect(LevelEffect::BlockUpdated { pos, old, new }) => {
                    assert_eq!(pos, POS, "{row}");
                    trace.push(format!("update:{}:{}", old.id(), new.id()));
                }
                other => panic!("unexpected shelf notification {other:?}: {row}"),
            }
        }
        let expected = f[6]
            .split(';')
            .map(|entry| {
                if entry.starts_with("update:") {
                    entry.strip_suffix(":3").expect("native update flags")
                } else {
                    entry
                }
            })
            .collect::<Vec<_>>()
            .join(";");
        assert_eq!(trace.join(";"), expected, "{row}");
        assert_eq!(
            level
                .block_entity::<BlockEntityData>(handle)
                .unwrap()
                .block_state
                .id(),
            f[5].parse::<u16>().unwrap(),
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 24);
}
