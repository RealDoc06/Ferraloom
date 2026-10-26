use super::*;
use crate::{
    Behaviors,
    block::entity,
    block::{BlockBehavior, BlockMethod},
    level::{LevelContext, Region, chunk::LevelChunk, effects::LevelEffect, redstone::Orientation},
};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::parse_compound;
use ferraloom_protocol::{
    component::{ItemStack, Persistent},
    play::clientbound::Packet,
};
use ferraloom_util::ChunkPos;
use std::{cell::RefCell, io::Read, sync::Arc};

const POS: BlockPos = BlockPos::new(8, 64, 8);
const CHUNK: ChunkPos = ChunkPos::new(0, 0);
fn vectors() -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/end-gateway-factory.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
}
fn prime(state: &mut BlockEntityState) {
    let BlockEntityState::EndGateway(entity) = state else {
        unreachable!()
    };
    entity.age = 77;
    entity.teleport_cooldown = 7;
    entity.set_exit_position(BlockPos::new(1, 2, 3), true);
}
fn scalar(entity: &TheEndGatewayBlockEntity) -> String {
    let position = entity.exit_portal.map_or_else(
        || "null".into(),
        |pos| format!("BlockPos{{x={}, y={}, z={}}}", pos.x, pos.y, pos.z),
    );
    format!(
        "{},{},{position},{},{},{}",
        entity.age,
        entity.teleport_cooldown,
        entity.exact_teleport,
        entity.is_spawning(),
        entity.is_cooling_down()
    )
}
fn state_scalar(state: &BlockEntityState) -> String {
    let BlockEntityState::EndGateway(entity) = state else {
        unreachable!()
    };
    scalar(entity)
}

#[test]
fn native_end_gateway_data_events_and_components_match() {
    let context = LevelContext::fixture(0, 63);
    let registries = context.registry_context().unwrap();
    let state = Block::END_GATEWAY.default_state();
    assert!(entity::supports(state.block()));
    assert!(entity::supports_creation(state.block()));
    let mut checked = [0; 3];
    for row in vectors().lines() {
        let fields: Vec<_> = row.split('\t').collect();
        let mut entity = entity::new_block_entity(POS, state).unwrap().unwrap();
        assert!(entity.has_server_ticker(None).unwrap());
        match fields[0] {
            "gateway-data" => {
                if fields[1] != "@new" {
                    prime(&mut entity);
                    entity.load_with_components(&parse_compound(fields[1]).unwrap(), registries);
                }
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(fields[2]).unwrap(),
                    "save {row}"
                );
                assert_eq!(
                    entity.get_update_tag().unwrap(),
                    parse_compound(fields[3]).unwrap(),
                    "update {row}"
                );
                assert_eq!(
                    entity.get_update_packet().unwrap().is_some(),
                    fields[4] == "true",
                    "packet {row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    entity::components::load(Some(&parse_compound(fields[5]).unwrap()), registries),
                    "components {row}"
                );
                assert_eq!(state_scalar(&entity), fields[6], "fields {row}");
                let round = entity::load_static(
                    POS,
                    state,
                    &entity.save_with_full_metadata().unwrap(),
                    registries,
                )
                .unwrap()
                .unwrap();
                assert_eq!(
                    round.save_with_full_metadata().unwrap(),
                    parse_compound(fields[7]).unwrap(),
                    "round trip {row}"
                );
                checked[0] += 1;
            }
            "gateway-components" => {
                prime(&mut entity);
                let stack = ItemStack::from_nbt(&Tag::Compound(parse_compound(fields[1]).unwrap()))
                    .unwrap();
                entity.apply_components_from_item_stack(&stack).unwrap();
                assert_eq!(
                    entity.save_with_full_metadata().unwrap(),
                    parse_compound(fields[2]).unwrap(),
                    "save {row}"
                );
                assert_eq!(
                    entity.collect_components(),
                    entity::components::load(Some(&parse_compound(fields[3]).unwrap()), registries),
                    "components {row}"
                );
                assert_eq!(state_scalar(&entity), fields[4], "fields {row}");
                checked[1] += 1;
            }
            "gateway-event" => {
                prime(&mut entity);
                let behavior = entity.behavior();
                assert_eq!(
                    behavior.trigger_event(&mut entity, fields[1].parse().unwrap(), -7),
                    fields[2] == "true",
                    "{row}"
                );
                assert_eq!(state_scalar(&entity), fields[3], "{row}");
                checked[2] += 1;
            }
            "gateway-tick" => {}
            _ => panic!("native gateway vector kind"),
        }
    }
    assert_eq!(checked, [22, 1, 5]);
}

thread_local! { static TRACE: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) }; }
fn deliver_pending_events(level: &mut Level) -> Result<(), LevelError> {
    level.run_block_events()?;
    for effect in level.take_effects() {
        match effect {
            LevelEffect::PlayerPacket { recipients, packet } => {
                assert!(recipients.is_empty());
                let Packet::BlockEvent(event) = *packet else {
                    panic!("unexpected gateway packet")
                };
                assert_eq!(event.pos, POS);
                assert_eq!(event.block.0, i32::from(Block::END_GATEWAY.protocol_id()));
                TRACE.with_borrow_mut(|trace| {
                    trace.push(format!("event:{}:{}", event.b0, event.b1))
                });
            }
            effect => panic!("unexpected gateway effect: {effect:?}"),
        }
    }
    Ok(())
}
struct ComparatorObserver;
impl BlockBehavior for ComparatorObserver {
    fn neighbor_changed(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        source: Block,
        orientation: Option<Orientation>,
        moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        assert_eq!(source, Block::END_GATEWAY);
        assert!(orientation.is_none());
        assert!(!moved);
        // Reset after every notification: the next callback requires another
        // real setChanged, even though chunk dirty flags ordinarily deduplicate.
        assert!(level.chunk_mut(CHUNK).unwrap().try_mark_saved());
        // Synchronous consumption proves blockEvent was already queued before
        // the comparator notification, rather than appended after it.
        deliver_pending_events(level)?;
        TRACE.with_borrow_mut(|trace| {
            trace.push("changed".into());
            trace.push("output".into());
        });
        Ok(())
    }
}
static OBSERVER: ComparatorObserver = ComparatorObserver;
static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_block(Block::COMPARATOR, &OBSERVER);
fn tick_level() -> Level {
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
        .set_chunk_loading_status(
            CHUNK,
            crate::level::distance_manager::FullChunkStatus::BlockTicking,
            true,
        )
        .unwrap();
    level.set_chunk_block_ticking(CHUNK, true).unwrap();
    level
        .update_entity_chunk_status(CHUNK, crate::level::entity::Visibility::Ticking)
        .unwrap();
    for request in level.take_entity_storage_requests() {
        if let crate::level::entity::EntityStorageRequest::Load { id, .. } = request {
            level.complete_entity_chunk_load(id, Ok(None)).unwrap();
        }
    }
    level.process_pending_entity_loads().unwrap();
    level
        .set_block_state(POS, Block::END_GATEWAY.default_state())
        .unwrap();
    level
        .set_block_state(POS.east(1), Block::COMPARATOR.default_state())
        .unwrap();
    level
}

#[test]
fn native_bound_end_gateway_ticker_and_pending_event_order_match() {
    let mut checked = 0;
    for row in vectors()
        .lines()
        .filter(|row| row.starts_with("gateway-tick\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let mut level = tick_level();
        let state = Block::END_GATEWAY.default_state();
        let mut entity = entity::create(POS, state).unwrap();
        let BlockEntityState::EndGateway(gateway) = &mut entity else {
            unreachable!()
        };
        gateway.age = fields[1].parse().unwrap();
        gateway.teleport_cooldown = fields[2].parse().unwrap();
        level.set_block_entity(entity).unwrap();
        let receiver = level.block_entity_ref(POS).unwrap().unwrap();
        assert!(
            level
                .required_block_behavior(Block::END_GATEWAY, BlockMethod::TriggerEvent)
                .is_ok()
        );
        level.take_effects();
        level.chunk_mut(CHUNK).unwrap().try_mark_saved();
        TRACE.with_borrow_mut(Vec::clear);
        level.tick_block_entities(true).unwrap();
        deliver_pending_events(&mut level).unwrap();
        assert_eq!(
            scalar(
                level
                    .block_entity::<TheEndGatewayBlockEntity>(receiver)
                    .unwrap()
            ),
            fields[3],
            "fields {row}"
        );
        TRACE.with_borrow(|trace| assert_eq!(trace.join(";"), fields[4], "notifications {row}"));
        assert!(
            !level.chunk(CHUNK).unwrap().is_unsaved(),
            "each change was observed: {row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 50);
}
