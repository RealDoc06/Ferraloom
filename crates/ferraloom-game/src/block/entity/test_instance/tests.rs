use super::*;
use crate::{
    Behaviors,
    block::{BlockBehavior, entity},
    level::{LevelContext, Region, chunk::LevelChunk, effects::LevelEffect, redstone::Orientation},
};
use ferraloom_data::BlockState;
use ferraloom_nbt::parse_compound;
use ferraloom_protocol::component::{ItemStack, Persistent};
use ferraloom_util::ChunkPos;
use std::{cell::RefCell, io::Read, sync::Arc};

const POS: BlockPos = BlockPos::new(8, 64, 8);
const CHUNK: ChunkPos = ChunkPos::new(0, 0);
fn prime(state: &mut BlockEntityState) {
    let BlockEntityState::TestInstance(entity) = state else {
        unreachable!()
    };
    entity.data = TestInstanceData {
        test: Some("minecraft:unregistered".parse().unwrap()),
        size: Vec3i::new(7, 8, 9),
        rotation: Rotation::Clockwise180,
        ignore_entities: true,
        status: TestInstanceStatus::Finished,
        error_message: Some(Component::literal("old")),
    };
    entity.error_markers.push(ErrorMarker {
        pos: BlockPos::new(4, 5, 6),
        text: Component::literal("old marker"),
    });
}
thread_local! {
    static TRACE:RefCell<Vec<String>>=const{RefCell::new(Vec::new())};
    static RECEIVER:RefCell<Option<BlockEntityRef>>=const{RefCell::new(None)};
    static REPLACE:RefCell<bool>=const{RefCell::new(false)};
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
        assert_eq!(source, Block::TEST_INSTANCE_BLOCK);
        let handle = RECEIVER.with_borrow(|handle| handle.unwrap());
        let entity = level.block_entity::<TestInstanceBlockEntity>(handle)?;
        let visible = format!("{}:{}", entity.data.size.x, entity.error_markers.len());
        assert!(level.try_mark_chunk_saved(CHUNK)?);
        assert!(
            level.take_effects().is_empty(),
            "update must follow the analog callback"
        );
        TRACE.with_borrow_mut(|trace| {
            trace.push(format!("changed:{visible}"));
            trace.push(format!("output:{visible}"));
        });
        if REPLACE.with_borrow(|replace| *replace) {
            level.set_block_entity(entity::create(
                POS,
                Block::TEST_INSTANCE_BLOCK.default_state(),
            )?)?;
            level.collect_detached_block_entities();
            assert_eq!(
                level
                    .block_entity::<TestInstanceBlockEntity>(handle)?
                    .data
                    .size
                    .x,
                1
            );
        }
        Ok(())
    }
}
static OBSERVER: Observer = Observer;
static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::COMPARATOR, &OBSERVER);
fn level(state: BlockEntityState) -> (Level, BlockEntityRef) {
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
        .set_block_state(POS, state.data().block_state)
        .unwrap();
    level
        .set_block_state(POS.east(1), Block::COMPARATOR.default_state())
        .unwrap();
    level.set_block_entity(state).unwrap();
    let handle = level.block_entity_ref(POS).unwrap().unwrap();
    level.take_effects();
    level.try_mark_chunk_saved(CHUNK).unwrap();
    TRACE.with_borrow_mut(Vec::clear);
    RECEIVER.with_borrow_mut(|current| *current = Some(handle));
    REPLACE.with_borrow_mut(|replace| *replace = false);
    (level, handle)
}

#[test]
fn native_test_instance_factory_codec_partials_and_live_update_order_match() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/test-instance-factory.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let context = LevelContext::fixture(0, 63);
    let registries = context.registry_context().unwrap();
    let state = Block::TEST_INSTANCE_BLOCK.default_state();
    let mut checked = [0; 2];
    for row in vectors.lines() {
        let f: Vec<_> = row.split('\t').collect();
        assert!(entity::supports(state.block()));
        assert!(entity::supports_creation(state.block()));
        let mut receiver = entity::new_block_entity(POS, state).unwrap().unwrap();
        match f[0] {
            "test-instance-data" => {
                if f[2] != "@new" {
                    prime(&mut receiver);
                }
                let input = (f[2] != "@new").then(|| parse_compound(f[2]).unwrap());
                let trace = if f[1] == "true" {
                    let (mut level, handle) = level(receiver);
                    if let Some(input) = &input {
                        handle.load_with_components(input, &mut level).unwrap();
                    }
                    let mut trace = TRACE.with_borrow(Clone::clone);
                    for effect in level.take_effects() {
                        let LevelEffect::BlockUpdated { pos, old, new } = effect else {
                            panic!("unexpected {effect:?}")
                        };
                        assert_eq!(pos, POS);
                        trace.push(format!("update:{}:{}:3", old.id(), new.id()));
                    }
                    receiver = level.block_entity_state(handle).unwrap().clone();
                    trace.join(";")
                } else {
                    if let Some(input) = &input {
                        receiver.load_with_components(input, registries);
                    }
                    String::new()
                };
                assert_eq!(
                    receiver.save_with_full_metadata().unwrap(),
                    parse_compound(f[3]).unwrap(),
                    "save {row}"
                );
                assert_eq!(
                    receiver.get_update_tag().unwrap(),
                    parse_compound(f[4]).unwrap(),
                    "update {row}"
                );
                assert_eq!(
                    receiver.collect_components(),
                    entity::components::load(Some(&parse_compound(f[5]).unwrap()), registries),
                    "components {row}"
                );
                assert_eq!(trace, f[6], "callback window {row}");
                let round = entity::load_static(
                    POS,
                    state,
                    &receiver.save_with_full_metadata().unwrap(),
                    registries,
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    round.save_with_full_metadata().unwrap(),
                    parse_compound(f[7]).unwrap(),
                    "round {row}"
                );
                checked[0] += 1;
            }
            "test-instance-components" => {
                prime(&mut receiver);
                receiver
                    .apply_components_from_item_stack(
                        &ItemStack::from_nbt(&Tag::Compound(parse_compound(f[1]).unwrap()))
                            .unwrap(),
                    )
                    .unwrap();
                assert_eq!(
                    receiver.save_with_full_metadata().unwrap(),
                    parse_compound(f[2]).unwrap(),
                    "{row}"
                );
                assert_eq!(
                    receiver.collect_components(),
                    entity::components::load(Some(&parse_compound(f[3]).unwrap()), registries),
                    "{row}"
                );
                checked[1] += 1;
            }
            _ => panic!("unknown native vector {row}"),
        }
        assert!(receiver.get_update_packet().unwrap().is_some());
        assert_eq!(receiver.has_server_ticker(None), Ok(false));
    }
    assert_eq!(checked, [42, 1]);
}

#[test]
fn live_instance_load_keeps_the_replaced_receiver_through_set_changed_and_error_replacement() {
    let mut receiver = entity::create(POS, Block::TEST_INSTANCE_BLOCK.default_state()).unwrap();
    prime(&mut receiver);
    let (mut level, handle) = level(receiver);
    level.retain_block_entity(handle).unwrap();
    REPLACE.with_borrow_mut(|replace| *replace = true);
    handle.load_with_components(&parse_compound("{data:{size:[I;1,2,3],rotation:'none',ignore_entities:0b,status:'running'},errors:[{pos:[I;9,8,7],text:'new'}],components:{'minecraft:custom_name':'after'}}").unwrap(),&mut level).unwrap();
    let replacement = level.block_entity_ref(POS).unwrap().unwrap();
    assert_ne!(replacement, handle);
    let old = level
        .block_entity::<TestInstanceBlockEntity>(handle)
        .unwrap();
    assert_eq!(old.error_markers[0].pos, BlockPos::new(9, 8, 7));
    assert_eq!(old.data.size, Vec3i::new(1, 2, 3));
    assert_eq!(old.parent.components.entries.len(), 1);
    let new = level
        .block_entity::<TestInstanceBlockEntity>(replacement)
        .unwrap();
    assert!(new.error_markers.is_empty());
    assert_eq!(new.data.size, Vec3i::new(0, 0, 0));
    assert_eq!(
        level.take_effects(),
        vec![LevelEffect::BlockUpdated {
            pos: POS,
            old: Block::AIR.default_state(),
            new: Block::TEST_INSTANCE_BLOCK.default_state()
        }]
    );
    level.release_block_entity(handle);
    level.collect_detached_block_entities();
    assert!(level.block_entity_state(handle).is_err());
}
