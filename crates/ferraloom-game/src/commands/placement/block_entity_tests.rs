use super::*;
use crate::{
    Behaviors,
    block::entity,
    level::{LevelContext, Region, chunk::LevelChunk, distance_manager::FullChunkStatus},
};
use ferraloom_brigadier::StringReader;
use ferraloom_util::ChunkPos;
use std::sync::Arc;

const POS: BlockPos = BlockPos::new(8, 5, 8);
const CHUNK: ChunkPos = ChunkPos::new(0, 0);
static BEHAVIORS: Behaviors = Behaviors::EMPTY;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vector {
    block: String,
    initial: String,
    input: String,
    set_result: bool,
    entity: bool,
    affected: bool,
    trace: Vec<String>,
    before: Option<String>,
    after: Option<String>,
    full: Option<String>,
    packet: Option<String>,
    pending: Option<String>,
}
struct World {
    level: Level,
    set_result: bool,
}
impl PlacementAccess for World {
    fn is_debug(&self) -> bool {
        false
    }
    fn max_block_modifications(&self) -> i32 {
        32768
    }
    fn has_chunk_at(&self, _: BlockPos) -> bool {
        true
    }
    fn in_world_bounds(&self, _: BlockPos) -> bool {
        true
    }
    fn block_state(&mut self, pos: BlockPos) -> Result<BlockState, LevelError> {
        self.level.get_block_state(pos)
    }
    fn block_entity_data(&mut self, pos: BlockPos) -> Result<Option<Tag>, PlacementError> {
        block_entity_data(&mut self.level, pos)
    }
    fn set_block(
        &mut self,
        pos: BlockPos,
        state: BlockState,
        flags: u32,
    ) -> Result<bool, LevelError> {
        // Native vectors use a controlled setBlock return with a real receiver.
        assert_eq!((pos, flags), (POS, 818));
        assert_eq!(self.level.get_block_state(pos)?, state);
        Ok(self.set_result)
    }
    fn destroy_block(&mut self, _: BlockPos, _: bool) -> Result<bool, LevelError> {
        unreachable!()
    }
    fn update_from_neighbor_shapes(
        &mut self,
        _: BlockState,
        _: BlockPos,
    ) -> Result<BlockState, LevelError> {
        unreachable!()
    }
    fn update_neighbors_on_block_set(
        &mut self,
        _: BlockPos,
        _: BlockState,
    ) -> Result<(), LevelError> {
        unreachable!()
    }
    fn load_block_entity_data(&mut self, pos: BlockPos, tag: &Tag) -> Result<bool, PlacementError> {
        load_block_entity_data(&mut self.level, pos, tag.as_compound().unwrap())
    }
}
fn level() -> Level {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    level
        .add_chunk(LevelChunk::new(
            CHUNK,
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
    level
        .set_chunk_loading_status(CHUNK, FullChunkStatus::BlockTicking, true)
        .unwrap();
    level
}

fn vectors() -> Vec<Vector> {
    serde_json::from_str(include_str!(
        "../../../tests/java/commands/block-input/vectors.json"
    ))
    .unwrap()
}

#[test]
fn native_block_input_loads_components_compares_saved_data_and_notifies_only_changes() {
    let vectors: Vec<_> = vectors()
        .into_iter()
        .filter(|vector| vector.pending.is_none())
        .collect();
    assert_eq!(vectors.len(), 15);
    for vector in vectors {
        replay(vector);
    }
}

#[test]
#[ignore = "spawner loadWithComponents SpawnData callback; BE owner repair pending release"]
fn native_spawner_spawn_data_load_callback_notifies_at_the_load_call_site() {
    let vectors: Vec<_> = vectors()
        .into_iter()
        .filter(|vector| vector.pending.is_some())
        .collect();
    assert_eq!(vectors.len(), 1);
    for vector in vectors {
        replay(vector);
    }
}

fn replay(vector: Vector) {
    let mut world = World {
        level: level(),
        set_result: vector.set_result,
    };
    let input = super::super::arguments::blocks::parse(&mut StringReader::new(format!(
        "{}{}",
        vector.block, vector.input
    )))
    .unwrap();
    world.level.set_block_state(POS, input.state).unwrap();
    if vector.entity {
        let mut receiver = entity::create(POS, input.state).unwrap();
        receiver.load_with_components(&ferraloom_nbt::parse_compound(&vector.initial).unwrap());
        assert_eq!(
            receiver.save_without_metadata().unwrap(),
            ferraloom_nbt::parse_compound(vector.before.as_ref().unwrap()).unwrap()
        );
        world.level.set_block_entity(receiver).unwrap();
    }
    let retained = world.level.block_entity_ref(POS).unwrap();
    world.level.try_mark_chunk_saved(CHUNK).unwrap();
    world.level.take_effects();
    assert_eq!(
        input.place(&mut world, POS, 818).unwrap(),
        vector.affected,
        "{}{}",
        vector.block,
        vector.input
    );
    assert_eq!(
        world.level.block_entity_ref(POS).unwrap(),
        retained,
        "load must retain native identity and position"
    );
    let changed = vector.trace.iter().any(|call| call == "changed");
    assert_eq!(world.level.chunk(CHUNK).unwrap().is_unsaved(), changed);
    assert_eq!(
        world.level.take_block_update_chunks(),
        if changed { vec![CHUNK] } else { vec![] }
    );
    // BlockInput uses chunkSource.blockChanged, not sendBlockUpdated's
    // navigation/path-cache side effects. SpawnData callbacks remain pending.
    let effects = world.level.take_effects();
    assert_eq!(
        effects.len(),
        vector
            .trace
            .iter()
            .filter(|call| call.as_str() == "send")
            .count()
    );
    assert!(effects.iter().all(|effect| matches!(effect, crate::level::effects::LevelEffect::BlockUpdated { pos, .. } if *pos == POS)));
    let actual = block_entity_data(&mut world.level, POS).unwrap();
    assert_eq!(
        actual,
        vector
            .full
            .as_ref()
            .map(|tag| Tag::Compound(ferraloom_nbt::parse_compound(tag).unwrap()))
    );
    if let Some(handle) = retained {
        let receiver = world.level.block_entity_state(handle).unwrap();
        assert_eq!(
            receiver.save_without_metadata().unwrap(),
            ferraloom_nbt::parse_compound(vector.after.as_ref().unwrap()).unwrap()
        );
        assert_eq!(
            receiver
                .get_update_packet()
                .unwrap()
                .map(|packet| packet.tag.0),
            vector
                .packet
                .as_ref()
                .map(|tag| ferraloom_nbt::parse_compound(tag).unwrap())
        );
    }
    let updates = world.level.take_block_updates(CHUNK).unwrap();
    assert_eq!(
        updates.len(),
        if changed {
            1 + usize::from(vector.packet.is_some())
        } else {
            0
        }
    );
}
