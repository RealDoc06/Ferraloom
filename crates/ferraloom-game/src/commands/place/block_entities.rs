//! The real template algorithm and live BE callbacks, with controlled setBlock
//! returns matching the native oracle. This is call-order coverage, not a live world.
use crate::{
    Behaviors,
    block::entity,
    level::{Level, LevelContext, Region, chunk::LevelChunk, distance_manager::FullChunkStatus},
};
use ferraloom_data::{Block, BlockState, FluidState};
use ferraloom_nbt::CompoundTag;
use ferraloom_util::{
    BlockPos, ChunkPos,
    random::{LegacyRandomSource, PositionalRandomFactory, Random, RandomSource},
};
use ferraloom_worldgen::{
    Error,
    feature::{FeatureRandom, FeatureRandomSelector, LiveWorldGenLevel},
    structure::{
        rule::BlockTags,
        template::{StructurePlaceSettings, StructureTemplate},
    },
};
use serde_json::{Value, json};
use std::{cell::RefCell, sync::Arc};

fn point(p: [i32; 3]) -> BlockPos {
    BlockPos::new(p[0], p[1], p[2])
}
fn required<T>(value: Result<T, crate::level::LevelError>) -> Result<T, Error> {
    value.map_err(|error| Error(format!("{error:?}")))
}
struct Probe<'a> {
    level: Level,
    trace: &'a RefCell<Vec<Value>>,
}
impl LiveWorldGenLevel for Probe<'_> {
    fn min_y(&self) -> i32 {
        0
    }
    fn height(&self) -> i32 {
        16
    }
    fn block(&self, p: [i32; 3]) -> Result<BlockState, Error> {
        required(self.level.get_block_state(point(p)))
    }
    fn set_block(&mut self, p: [i32; 3], state: BlockState, flags: u32) -> Result<bool, Error> {
        self.trace
            .borrow_mut()
            .push(json!({"call":"set","pos":p,"state":state.id(),"flags":flags}));
        // The native oracle controls setBlock/getBlockEntity too. Both fixtures
        // provide the receiver that a successful setBlock just constructed.
        required(self.level.remove_block_entity(point(p)))?;
        required(self.level.set_block_state(point(p), state))?;
        if state.has_block_entity() {
            let receiver = required(entity::create(point(p), state))?;
            required(self.level.set_block_entity(receiver))?;
        }
        Ok(true)
    }
    fn has_block_entity_at(&self, p: [i32; 3]) -> Result<bool, Error> {
        required(self.level.block_entity_ref(point(p))).map(|receiver| receiver.is_some())
    }
    fn load_block_entity(
        &mut self,
        p: [i32; 3],
        tag: CompoundTag,
        random: &mut FeatureRandomSelector<'_>,
    ) -> Result<(), Error> {
        let before = required(self.level.block_entity_ref(point(p)))?.unwrap();
        LiveWorldGenLevel::load_block_entity(&mut self.level, p, tag, random)?;
        assert_eq!(
            required(self.level.block_entity_ref(point(p)))?,
            Some(before)
        );
        let state = required(self.level.block_entity_state(before))?;
        let seed = state.randomizable().unwrap().loot_table_seed;
        self.trace
            .borrow_mut()
            .push(json!({"call":"load","pos":p,"seed":seed}));
        Ok(())
    }
    fn place_liquid(&mut self, _: [i32; 3], fluid: FluidState) -> Result<bool, Error> {
        assert_eq!(fluid, Block::AIR.default_state().data().fluid);
        Ok(false)
    }
    fn mark_template_block_entity_changed(&mut self, p: [i32; 3]) -> Result<(), Error> {
        LiveWorldGenLevel::mark_template_block_entity_changed(&mut self.level, p)?;
        self.trace
            .borrow_mut()
            .push(json!({"call":"changed","pos":p}));
        Ok(())
    }
}
struct TracedRandom<'a> {
    random: LegacyRandomSource,
    trace: &'a RefCell<Vec<Value>>,
}
impl RandomSource for TracedRandom<'_> {
    fn set_seed(&mut self, seed: i64) {
        self.random.set_seed(seed);
    }
    fn next_bits(&mut self, bits: u32) -> i32 {
        self.random.next_bits(bits)
    }
    fn next_gaussian(&mut self) -> f64 {
        self.random.next_gaussian()
    }
    fn fork(&mut self) -> Random {
        self.random.fork()
    }
    fn fork_positional(&mut self) -> PositionalRandomFactory {
        self.random.fork_positional()
    }
    fn next_long(&mut self) -> i64 {
        let value = self.random.next_long();
        self.trace
            .borrow_mut()
            .push(json!({"call":"random","value":value}));
        value
    }
}

#[test]
fn native_template_loads_existing_receivers_with_loot_seed_before_late_set_changed() {
    let vector: Value = serde_json::from_str(include_str!(
        "../../../tests/java/commands/place/block-entity-callbacks.json"
    ))
    .unwrap();
    let input = ferraloom_nbt::parse_compound(vector["input"].as_str().unwrap()).unwrap();
    let template = StructureTemplate::from_nbt(&input);
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    level
        .add_chunk(LevelChunk::new(
            ChunkPos::ZERO,
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
        .set_chunk_loading_status(ChunkPos::ZERO, FullChunkStatus::BlockTicking, true)
        .unwrap();
    let trace = RefCell::default();
    let mut probe = Probe {
        level,
        trace: &trace,
    };
    let mut random = TracedRandom {
        random: LegacyRandomSource::new(42),
        trace: &trace,
    };
    let placed = template
        .place_in_world(
            &mut probe,
            [8, 5, 8],
            [8, 5, 8],
            &mut StructurePlaceSettings {
                known_shape: true,
                ..Default::default()
            },
            &[],
            &BlockTags::default(),
            &mut FeatureRandom::Live(&mut random),
            818,
        )
        .unwrap();
    assert_eq!(json!(placed), vector["placed"]);
    assert_eq!(json!(*trace.borrow()), vector["trace"]);
    assert_eq!(json!(random.random.next_long()), vector["next_long"]);
}
