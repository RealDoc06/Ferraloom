use super::*;
use serde_json::Value;
use std::fmt::Debug;

fn check_property<T: PropertyValue + Debug>(
    state: BlockState,
    property: Property<T>,
    name: &str,
    report: &Value,
    display: impl Fn(T) -> String,
) {
    let rows = report["states"].as_array().unwrap();
    let first = rows[0]["id"].as_u64().unwrap() as usize;
    let source = &rows[state.id() as usize - first]["properties"];
    let current = state.get(property).unwrap();
    assert_eq!(display(current), source[name].as_str().unwrap());
    assert_eq!(state.with(property, current), Some(state));
    for allowed in report["properties"][name].as_array().unwrap() {
        let value = (0..=u8::MAX)
            .filter_map(T::from_index)
            .find(|&value| display(value) == allowed.as_str().unwrap())
            .unwrap();
        let changed = state.with(property, value).unwrap();
        assert_eq!(changed.block(), state.block());
        let target = &rows[changed.id() as usize - first]["properties"];
        for (key, expected) in source.as_object().unwrap() {
            assert_eq!(&target[key], if key == name { allowed } else { expected });
        }
    }
}

#[test]
fn tricky_state_properties_match_every_reported_state() {
    let blocks: Value =
        serde_json::from_str(include_str!("../../../data/generated/blocks.json")).unwrap();
    for block in [
        Block::REDSTONE_WIRE,
        Block::OAK_STAIRS,
        Block::NOTE_BLOCK,
        Block::CHISELED_BOOKSHELF,
    ] {
        let report = &blocks[block.name()];
        for row in report["states"].as_array().unwrap() {
            let state = BlockState::from_id(row["id"].as_u64().unwrap() as u16).unwrap();
            assert_eq!(state.block(), block);
            if row["default"] == true {
                assert_eq!(state, block.default_state());
            }
            macro_rules! check {
                ($property:ident, $name:literal) => {
                    check_property(state, properties::$property, $name, report, |v| {
                        v.to_string()
                    });
                };
                ($property:ident, $name:literal, enum) => {
                    check_property(state, properties::$property, $name, report, |v| {
                        v.as_str().to_owned()
                    });
                };
            }
            match block {
                Block::REDSTONE_WIRE => {
                    check!(EAST, "east", enum);
                    check!(NORTH, "north", enum);
                    check!(SOUTH, "south", enum);
                    check!(WEST, "west", enum);
                    check!(POWER, "power");
                }
                Block::OAK_STAIRS => {
                    check!(FACING, "facing", enum);
                    check!(HALF, "half", enum);
                    check!(SHAPE, "shape", enum);
                    check!(WATERLOGGED, "waterlogged");
                }
                Block::NOTE_BLOCK => {
                    check!(INSTRUMENT, "instrument", enum);
                    check!(NOTE, "note");
                    check!(POWERED, "powered");
                }
                Block::CHISELED_BOOKSHELF => {
                    check!(FACING, "facing", enum);
                    check!(SLOT_0_OCCUPIED, "slot_0_occupied");
                    check!(SLOT_1_OCCUPIED, "slot_1_occupied");
                    check!(SLOT_2_OCCUPIED, "slot_2_occupied");
                    check!(SLOT_3_OCCUPIED, "slot_3_occupied");
                    check!(SLOT_4_OCCUPIED, "slot_4_occupied");
                    check!(SLOT_5_OCCUPIED, "slot_5_occupied");
                }
                _ => unreachable!(),
            }
        }
    }
    assert_eq!(Block::STONE.default_state().get(properties::POWERED), None);
    assert_eq!(
        Block::OAK_STAIRS
            .default_state()
            .with(properties::FACING, properties::Facing::Up),
        None
    );
    assert_eq!(
        Block::REDSTONE_WIRE
            .default_state()
            .with(properties::POWER, 16),
        None
    );
}

#[test]
fn registry_counts_and_protocol_ids_match_reports() {
    let report: Value =
        serde_json::from_str(include_str!("../../../data/generated/registries.json")).unwrap();
    assert_eq!(REGISTRIES.len(), 95);
    for registry in REGISTRIES {
        let expected = &report[registry.name];
        assert_eq!(
            registry.count,
            expected["entries"].as_object().unwrap().len()
        );
        assert_eq!(
            u64::from(registry.protocol_id),
            expected["protocol_id"].as_u64().unwrap()
        );
    }
    assert_eq!(BLOCKS.len(), registries::block::ALL.len());
    assert_eq!(ITEMS.len(), registries::item::ALL.len());
    assert_eq!(ENTITY_TYPES.len(), registries::entity_type::ALL.len());
    for id in registries::block::ALL {
        assert_eq!(Block::from_protocol_id(id.protocol_id()), Some(*id));
        assert_eq!(Block::from_name(id.name()), Some(*id));
        assert_eq!(
            report["minecraft:block"]["entries"][id.name()]["protocol_id"].as_u64(),
            Some(u64::from(id.protocol_id()))
        );
    }
    assert!(BlockState::from_id(u16::MAX).is_none());
}

#[test]
fn extracted_facts_retain_vanilla_state_variation() {
    assert!(Block::WATER.default_state().data().fluid.source);
    assert!(!Block::WATER.default_state().data().fluid.falling);
    let falling_water = Block::WATER
        .default_state()
        .with(properties::LEVEL, 8)
        .unwrap();
    assert!(falling_water.data().fluid.falling);
    assert_eq!(Block::AIR.default_state().data().collision.boxes(), &[]);
    assert_eq!(
        Block::SAND.default_state().data().collision.boxes().len(),
        1
    );
    assert!(
        Block::OAK_STAIRS
            .default_state()
            .with(properties::WATERLOGGED, true)
            .unwrap()
            .data()
            .fluid
            .source
    );
    assert!(Block::BAMBOO.default_state().data().context_dependent_shape);
    assert!(
        Block::MOVING_PISTON
            .default_state()
            .data()
            .context_dependent_shape
    );
    for block in [
        Block::SCAFFOLDING,
        Block::POWDER_SNOW,
        Block::LIGHT,
        Block::WATER,
    ] {
        assert!(block.default_state().data().context_dependent_shape);
    }
    assert_eq!(Item::AIR.data().max_stack_size, 64);
    assert_eq!(Item::DIAMOND_PICKAXE.data().max_damage, 1561);
    assert_eq!(Item::STONE.data().block, Some(Block::STONE));
    assert!(
        EntityType::ZOMBIE
            .data()
            .attributes
            .iter()
            .any(|(id, value)| *id == registries::attribute::Id::MAX_HEALTH && *value == 20.0)
    );
}

#[test]
fn packet_ids_match_protocol_state_and_direction() {
    let report: Value =
        serde_json::from_str(include_str!("../../../data/generated/packets.json")).unwrap();
    use packets::{handshake, login, play};
    let keep_alive = play::clientbound::Packet::KeepAlive;
    assert_eq!(
        u64::from(keep_alive.protocol_id()),
        report["play"]["clientbound"][keep_alive.name()]["protocol_id"]
            .as_u64()
            .unwrap()
    );
    assert_eq!(handshake::serverbound::Packet::Intention.protocol_id(), 0);
    assert_eq!(login::clientbound::Packet::LoginDisconnect.protocol_id(), 0);
    for &packet in play::clientbound::ALL {
        assert_eq!(
            play::clientbound::Packet::from_protocol_id(packet.protocol_id()),
            Some(packet)
        );
    }
}

#[test]
fn generic_state_properties_match_every_report_state() {
    let report: Value =
        serde_json::from_str(include_str!("../../../data/generated/blocks.json")).unwrap();
    for (name, block) in report.as_object().unwrap() {
        let block_id = Block::from_name(name).unwrap();
        assert_eq!(block_id.name(), name);
        for row in block["states"].as_array().unwrap() {
            let state = BlockState::from_id(row["id"].as_u64().unwrap() as u16).unwrap();
            let actual: Vec<_> = state.properties().collect();
            let mut expected: Vec<_> = row["properties"]
                .as_object()
                .into_iter()
                .flat_map(|p| p.iter())
                .map(|(k, v)| (k.as_str(), v.as_str().unwrap()))
                .collect();
            expected.sort_unstable_by_key(|(name, _)| *name);
            assert_eq!(actual, expected, "{name}");
            assert_eq!(BlockState::from_properties(block_id, actual), Some(state));
        }
        assert_eq!(
            BlockState::from_properties(block_id, [("unknown", "bad")]),
            Some(block_id.default_state())
        );
    }
    assert_eq!(
        BlockState::from_properties(
            Block::OAK_STAIRS,
            [("facing", "invalid"), ("waterlogged", "true")]
        ),
        Some(
            Block::OAK_STAIRS
                .default_state()
                .with(properties::WATERLOGGED, true)
                .unwrap()
        )
    );
}

#[test]
fn generic_static_registry_lookup_matches_numeric_reports() {
    let report: Value =
        serde_json::from_str(include_str!("../../../data/generated/registries.json")).unwrap();
    for (name, registry) in report.as_object().unwrap() {
        let entries = registry_entries(name).unwrap();
        assert_eq!(
            entries.len(),
            registry["entries"].as_object().unwrap().len()
        );
        for (entry, value) in registry["entries"].as_object().unwrap() {
            assert_eq!(
                entries[value["protocol_id"].as_u64().unwrap() as usize],
                entry
            );
        }
    }
    assert_eq!(registries::block::name(Block::STONE), "minecraft:stone");
    assert_eq!(
        registries::block::from_name("minecraft:stone"),
        Some(Block::STONE)
    );
}

#[test]
fn every_state_rotation_and_mirror_matches_vanilla_registry() {
    use ferraloom_util::{Mirror, Rotation};
    let captured: Value = serde_json::from_str(include_str!(
        "../../../data/generated/state_transforms.json"
    ))
    .unwrap();
    for id in 0..BLOCK_STATES.len() {
        let state = BlockState::from_id(id as u16).unwrap();
        for (name, actual) in [
            ("clockwise_90", state.rotate(Rotation::Clockwise90)),
            ("clockwise_180", state.rotate(Rotation::Clockwise180)),
            (
                "counterclockwise_90",
                state.rotate(Rotation::CounterClockwise90),
            ),
            ("left_right", state.mirror(Mirror::LeftRight)),
            ("front_back", state.mirror(Mirror::FrontBack)),
        ] {
            assert_eq!(
                actual.id(),
                captured[name][id].as_u64().unwrap() as u16,
                "state {id} {name}"
            );
            assert_eq!(
                actual.block(),
                state.block(),
                "state {id} {name} changed block"
            );
        }
        assert_eq!(state.rotate(Rotation::None), state);
        assert_eq!(state.mirror(Mirror::None), state);
    }
}

#[test]
fn cached_face_occlusion_distinguishes_faces_and_non_occluding_blocks() {
    use ferraloom_util::Direction;
    for face in Direction::VALUES {
        assert!(Block::STONE.default_state().is_face_occlusion_full(face));
        assert!(!Block::AIR.default_state().is_face_occlusion_full(face));
        assert!(!Block::GLASS.default_state().is_face_occlusion_full(face));
    }
    let bottom = Block::STONE_SLAB.default_state();
    assert!(bottom.is_face_occlusion_full(Direction::Down));
    assert!(!bottom.is_face_occlusion_full(Direction::Up));
}

#[test]
fn cached_block_facts_preserve_dynamic_shape_fallback_and_post_processing() {
    use ferraloom_util::Direction;
    assert!(Block::STONE.default_state().is_solid());
    assert!(!Block::WATER.default_state().is_solid());
    assert!(Block::CHEST.default_state().has_block_entity());
    assert!(!Block::STONE.default_state().has_block_entity());
    assert_eq!(
        Block::STONE
            .default_state()
            .cached_sturdy_face(Direction::Up),
        Some(true)
    );
    assert_eq!(
        Block::STONE_SLAB
            .default_state()
            .cached_sturdy_face(Direction::Up),
        Some(false)
    );
    assert_eq!(
        Block::SCAFFOLDING
            .default_state()
            .cached_sturdy_face(Direction::Up),
        None
    );
    assert_eq!(
        Block::STONE.default_state().cached_collision_full_block(),
        Some(true)
    );
    assert_eq!(Block::STONE.default_state().post_process_y_offset(), None);
    assert_eq!(
        Block::BROWN_MUSHROOM
            .default_state()
            .post_process_y_offset(),
        Some(0)
    );
    assert_eq!(
        Block::MAGMA_BLOCK.default_state().post_process_y_offset(),
        Some(1)
    );
}

#[test]
fn block_class_hierarchy_matches_extracted_vanilla_classes() {
    use crate::{Block, BlockClass};
    let data: serde_json::Value =
        serde_json::from_str(include_str!("../../../data/generated/block_classes.json")).unwrap();
    for (&block, concrete) in crate::registries::block::ALL
        .iter()
        .zip(data["blocks"].as_array().unwrap())
    {
        let index = concrete.as_u64().unwrap() as usize;
        assert_eq!(
            block.class().java_name(),
            data["classes"][index]["name"].as_str().unwrap()
        );
        for (class_index, &class) in BlockClass::ALL.iter().enumerate() {
            assert_eq!(
                block.is_instance_of(class),
                data["classes"][index]["instances"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|id| id.as_u64().unwrap() as usize == class_index),
                "{} {class:?}",
                block.name()
            );
        }
    }
    assert!(Block::TALL_SEAGRASS.is_instance_of(BlockClass::DoublePlantBlock));
    assert!(Block::PALE_MOSS_CARPET.is_instance_of(BlockClass::MossyCarpetBlock));
    assert!(Block::OAK_STAIRS.is_instance_of(BlockClass::SimpleWaterloggedBlock));
    assert!(!Block::STONE.is_instance_of(BlockClass::DoublePlantBlock));
    const CONCRETE: BlockClass = Block::STONE.class();
    const IS_BLOCK: bool = Block::STONE.is_instance_of(BlockClass::Block);
    assert_eq!(CONCRETE, BlockClass::Block);
    assert!(std::hint::black_box(IS_BLOCK));
}

#[test]
fn cached_air_fact_matches_vanilla_state_registry() {
    let ids: Vec<u16> =
        serde_json::from_str(include_str!("../../../data/generated/air_states.json")).unwrap();
    for id in 0..crate::BLOCK_STATES.len() {
        let state = crate::BlockState::from_id(id as u16).unwrap();
        assert_eq!(state.is_air(), ids.contains(&(id as u16)));
    }
    const AIR: bool = crate::Block::AIR.default_state().is_air();
    assert!(std::hint::black_box(AIR));
}

#[test]
fn dynamic_integer_properties_preserve_absence_and_invalid_values() {
    use crate::{BlockStatePropertyError, properties};
    let state = Block::WHEAT.default_state();
    assert!(state.has_integer_property("age"));
    assert_eq!(
        state.with_integer_property("age", 7).unwrap(),
        state.with(properties::AGE, 7).unwrap()
    );
    assert_eq!(
        state.with_integer_property("age", 8),
        Err(BlockStatePropertyError::InvalidValue(8))
    );
    assert_eq!(
        state.with_integer_property("waterlogged", 0),
        Err(BlockStatePropertyError::MissingOrNonInteger)
    );
    assert!(!Block::OAK_LOG.default_state().has_integer_property("axis"));
    let wet = Block::OAK_STAIRS
        .default_state()
        .with(properties::WATERLOGGED, true)
        .unwrap();
    assert_eq!(
        Block::STONE_STAIRS
            .default_state()
            .with_properties_of(wet)
            .get(properties::WATERLOGGED),
        Some(true)
    );
    // IntegerProperty equality includes its allowed range.
    let wheat = state.with(properties::AGE, 7).unwrap();
    let cactus = Block::CACTUS.default_state();
    assert_eq!(cactus.with_properties_of(wheat), cactus);
}
