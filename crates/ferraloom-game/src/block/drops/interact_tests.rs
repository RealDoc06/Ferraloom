use super::*;
use crate::{
    Behaviors,
    block::{containers::runtime::ContainerResources, entity::create},
    crafting::{RecipeManager, RecipeSynchronization},
    level::{LevelContext, Region, chunk::LevelChunk},
    loot::{LootData, LootTable},
};
use ferraloom_data::Block;
use ferraloom_protocol::{chat::Component, component::Persistent};
use ferraloom_util::{
    ChunkPos,
    random::{LegacyRandomSource, Random, RandomSource},
};
use std::{io::Read, sync::Arc};

static BEHAVIORS: Behaviors = Behaviors::EMPTY;
const POS: BlockPos = BlockPos::new(8, 5, 8);

fn level(table: &Identifier, definition: &serde_json::Value) -> (Level, BlockEntityRef) {
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let section = ferraloom_chunk::LevelChunkSection::new(
        ferraloom_chunk::Strategy::block_states(),
        ferraloom_chunk::Strategy::biomes(Arc::new(
            ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
        )),
        ferraloom_chunk::BiomeId(0),
    );
    level
        .add_chunk(LevelChunk::new(ChunkPos::new(0, 0), 0..1, section))
        .unwrap();
    let state = Block::CHEST.default_state();
    level.set_block_state(POS, state).unwrap();
    level.set_block_entity(create(POS, state).unwrap()).unwrap();
    let original = level.block_entity_ref(POS).unwrap().unwrap();
    level
        .block_entity_state_mut(original)
        .unwrap()
        .container_mut()
        .unwrap()
        .custom_name = Some(Component::literal("before"));
    let mut loot = LootData::default();
    loot.tables
        .insert(table.clone(), LootTable::decode(definition).unwrap());
    let recipes = Arc::new(RecipeManager::new(Vec::new()).unwrap());
    let recipe_synchronization = Arc::new(RecipeSynchronization::new(&recipes, |_| true));
    level.set_container_resources(
        Arc::new(ContainerResources {
            recipes,
            loot: Arc::new(loot),
            crafting: crate::inventory::RuntimeCraftingContext::fixture(),
            advancements: Arc::new(crate::advancements::ServerAdvancementManager::new(
                Vec::new(),
            )),
            recipe_tags: Default::default(),
            recipe_synchronization,
        }),
        0,
    );
    level.rules_mut().block_drops = Some(false);
    (level, original)
}

#[test]
fn native_block_interact_poppers_preserve_evaluation_order_rng_and_receiver() {
    let mut vectors = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../tests/java/blocks/entities/interaction-helpers.jsonl.gz")
            .as_slice(),
    )
    .read_to_string(&mut vectors)
    .unwrap();
    let table = Identifier::parse("helper_delivery").unwrap();
    let state = Block::CHEST.default_state();
    let mut checked = 0;
    for row in vectors.lines() {
        let case: serde_json::Value = serde_json::from_str(row).unwrap();
        if case["kind"] != "loot" {
            continue;
        }
        let definition: serde_json::Value =
            serde_json::from_str(case["definition"].as_str().unwrap()).unwrap();
        let (mut level, original) = level(&table, &definition);
        level.set_random_streams(
            Random::Legacy(LegacyRandomSource::new(case["seed"].as_i64().unwrap())),
            0,
        );
        let with_entity = case["with_entity"].as_bool().unwrap();
        let receiver = with_entity.then_some(original);
        let mode = case["mode"].as_str().unwrap();
        let expected = case["deliveries"].as_array().unwrap();
        let mut delivered = 0;
        let result = if mode == "standard" {
            // Exercise the public adapter too, including empty-table false and
            // eager popResource draws despite BLOCK_DROPS being false.
            drop_from_block_interact_loot_table(
                &mut level, &table, POS, state, receiver, None, None,
            )
        } else {
            drop_from_block_interact_loot_table_with_popper(
                &mut level,
                &table,
                POS,
                state,
                receiver,
                None,
                None,
                |level, stack| {
                    let event = &expected[delivered];
                    let expected_stack = ItemStack::from_nbt(&Tag::Compound(
                        ferraloom_nbt::parse_compound(event["stack"].as_str().unwrap()).unwrap(),
                    ))
                    .unwrap();
                    assert_eq!(stack, expected_stack, "{row} delivery {delivered}");
                    if mode == "record" {
                        if with_entity {
                            if delivered == 0 {
                                let mut replacement = create(POS, state).unwrap();
                                replacement.container_mut().unwrap().custom_name =
                                    Some(Component::literal("replacement"));
                                level.set_block_entity(replacement).unwrap();
                            }
                            level.collect_detached_block_entities();
                            let name = format!("delivery-{delivered}");
                            level
                                .block_entity_state_mut(original)?
                                .container_mut()
                                .unwrap()
                                .custom_name = Some(Component::literal(&name));
                            assert_eq!(name, event["name"].as_str().unwrap());
                        }
                        assert_eq!(
                            level.random().next_long(),
                            event["next"].as_i64().unwrap(),
                            "{row}"
                        );
                    } else {
                        let face = Direction::VALUES
                            .into_iter()
                            .find(|direction| direction.name() == mode)
                            .unwrap();
                        assert_eq!(pop_resource_from_face(level, POS, face, stack)?, None);
                    }
                    delivered += 1;
                    Ok(())
                },
            )
        }
        .unwrap();
        assert_eq!(result, case["result"].as_bool().unwrap(), "{row}");
        if mode != "standard" {
            assert_eq!(delivered, expected.len());
        }
        assert_eq!(
            level.random().next_long(),
            case["next"].as_i64().unwrap(),
            "{row}"
        );
        let current = level.block_entity_ref(POS).unwrap().unwrap();
        assert_eq!(
            level
                .block_entity_state(current)
                .unwrap()
                .container()
                .unwrap()
                .custom_name,
            Some(Component::literal(case["current_name"].as_str().unwrap()))
        );
        if current != original {
            level.collect_detached_block_entities();
            assert_eq!(
                level.block_entity_state(original).unwrap_err(),
                LevelError::MissingBlockEntity(original)
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 192);
}
