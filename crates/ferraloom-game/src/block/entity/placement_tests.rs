use super::*;
use crate::{
    Behaviors,
    block::{containers::command::CommandBlockEntity, entity},
    level::{LevelContext, Region, chunk::LevelChunk},
};
use ferraloom_protocol::{
    Identifier,
    component::{DataComponent, DataComponentPatch},
};
use std::{io::Read, sync::Arc};

const POS: BlockPos = BlockPos::new(8, 5, 8);
static BEHAVIORS: Behaviors = Behaviors::EMPTY;
fn vectors() -> String {
    let mut data = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/entities/post-placement.tsv.gz").as_slice(),
    )
    .read_to_string(&mut data)
    .unwrap();
    data
}
fn state(id: &str) -> BlockState {
    BlockState::from_id(id.parse().unwrap()).unwrap()
}
pub(super) fn bare_level() -> Level {
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
    let resources = ferraloom_data::datapack::ResourceManager::new(vec![
        ferraloom_data::datapack::Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap(),
    ]);
    level.set_block_tags(Arc::new(
        ferraloom_data::datapack::TagSet::<Block>::load(&resources).unwrap(),
    ));
    level
}
fn install(level: &mut Level, state: BlockState) -> entity::BlockEntityRef {
    level.set_block_state(POS, state).unwrap();
    level
        .set_block_entity(entity::create(POS, state).unwrap())
        .unwrap();
    level.block_entity_ref(POS).unwrap().unwrap()
}
fn ticks(level: &Level) -> String {
    let mut data = ferraloom_storage::SerializableChunkData::default();
    level
        .save_chunk_ticks(ChunkPos::new(0, 0), &mut data)
        .unwrap();
    data.block_ticks
        .unwrap()
        .into_iter()
        .map(|tick| {
            let block = Block::from_name(&tick.id.unwrap().to_string_checked().unwrap()).unwrap();
            format!("{}:{}", tick.delay.unwrap(), block.protocol_id())
        })
        .collect::<Vec<_>>()
        .join(",")
}
/// Real native save/load and update tags cover command defaults, nullable output,
/// structure clamping/legacy enums, and skull profiles/components.
#[test]
fn native_placement_entity_data_round_trips() {
    let mut count = 0;
    let mut component_cases = 0;
    for line in vectors().lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f[0] == "components" {
            let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
            let mut receiver = entity::create(POS, block.default_state()).unwrap();
            let input = ferraloom_nbt::parse_compound(f[2]).unwrap();
            let mut patch = DataComponentPatch::default();
            for (key, value) in input.iter() {
                patch.set(
                    DataComponent::from_nbt(
                        &Identifier::parse(&key.to_string_lossy()).unwrap(),
                        value,
                    )
                    .unwrap(),
                );
            }
            receiver.apply_components(&Default::default(), &patch);
            assert_eq!(
                receiver.save_without_metadata().unwrap(),
                ferraloom_nbt::parse_compound(f[3]).unwrap(),
                "{line}"
            );
            let mut collected = CompoundTag::new();
            for component in receiver.collect_components().entries {
                collected.insert(component.key().as_str(), component.to_nbt().unwrap());
            }
            assert_eq!(
                collected,
                ferraloom_nbt::parse_compound(f[4]).unwrap(),
                "{line}"
            );
            component_cases += 1;
            continue;
        }
        if f[0] != "data" {
            continue;
        }
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let mut receiver = entity::create(POS, block.default_state()).unwrap();
        receiver.load_with_components(
            &ferraloom_nbt::parse_compound(f[3]).unwrap(),
            crate::level::LevelContext::fixture(0, 63)
                .registry_context()
                .unwrap(),
        );
        let expected = ferraloom_nbt::parse_compound(f[4]).unwrap();
        assert_eq!(
            receiver.save_without_metadata().unwrap(),
            expected,
            "{line}"
        );
        assert_eq!(
            receiver.get_update_tag().unwrap(),
            ferraloom_nbt::parse_compound(f[5]).unwrap(),
            "{line}"
        );
        assert_eq!(
            receiver.get_update_packet().unwrap().is_some(),
            f[6] == "true",
            "{line}"
        );
        let mut loaded = entity::create(POS, block.default_state()).unwrap();
        loaded.load_with_components(
            &expected,
            crate::level::LevelContext::fixture(0, 63)
                .registry_context()
                .unwrap(),
        );
        assert_eq!(loaded.save_without_metadata().unwrap(), expected, "{line}");
        // The fields represented by item components leave custom NBT only through
        // the native removeComponentsFromTag path.
        let mut custom = receiver.save_custom_only().unwrap();
        receiver.remove_components_from_tag(&mut custom);
        let components = receiver.collect_components();
        if matches!(receiver, BlockEntityState::Skull(_)) {
            for key in ["custom_name", "note_block_sound", "profile"] {
                assert!(custom.get(key).is_none());
            }
            if f[2] == "1" {
                assert!(
                    components
                        .get(ferraloom_protocol::component::ComponentTypeId::CUSTOM_NAME)
                        .is_some()
                );
            }
        }
        count += 1;
    }
    assert_eq!((count, component_cases), (18, 12));
}

/// loadWithComponents runs Command's scheduling and Structure's state mutation
/// against the canonical receiver before replacing the extra component map.
#[test]
fn native_live_load_runs_callbacks_on_the_original_generation() {
    let mut cases = 0;
    for line in vectors().lines() {
        let f: Vec<_> = line.split('\t').collect();
        if !matches!(f[0], "live-load" | "custom-live-load") {
            continue;
        }
        let mut level = bare_level();
        let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
        let mask: u8 = f[2].parse().unwrap();
        let handle = install(&mut level, block.default_state());
        if block != Block::STRUCTURE_BLOCK {
            level
                .block_entity_mut::<CommandBlockEntity>(handle)
                .unwrap()
                .automatic = mask & 1 != 0;
        }
        let tag = ferraloom_nbt::parse_compound(f[3]).unwrap();
        if f[0] == "custom-live-load" {
            let mut patch = DataComponentPatch::default();
            patch.set(DataComponent::MaxStackSize(
                ferraloom_protocol::codec::VarInt(7),
            ));
            level
                .block_entity_state_mut(handle)
                .unwrap()
                .apply_components(&Default::default(), &patch);
            handle.load_custom_only(&tag, &mut level).unwrap();
        } else {
            handle.load_with_components(&tag, &mut level).unwrap();
        }
        assert_eq!(level.block_entity_ref(POS).unwrap(), Some(handle));
        assert_eq!(
            level
                .block_entity_state(handle)
                .unwrap()
                .save_without_metadata()
                .unwrap(),
            ferraloom_nbt::parse_compound(f[4]).unwrap(),
            "{line}"
        );
        assert_eq!(level.get_block_state(POS).unwrap(), state(f[5]), "{line}");
        assert_eq!(ticks(&level), f[6], "{line}");
        cases += 1;
    }
    assert_eq!(cases, 64);
}
