//! Port of Block's loot contexts and synchronous resource spawning.
use super::containers::loot_runtime::LiveLootRuntime;
use super::{BlockMethod, entity::BlockEntityRef};
use crate::{
    entity::{EntityRef, EntityState, item::ItemEntity},
    level::{Level, LevelError},
    loot::{ContextKeySet, LootParams},
};
use ferraloom_data::{BlockState, EntityType};
use ferraloom_nbt::Tag;
use ferraloom_protocol::{Identifier, component::ItemStack};
use ferraloom_util::{BlockPos, Direction, Vec3, mth};

/// The native LootParams.Builder carries the receiver object, rather than only
/// its initially serialized data. Keep that generation through block dispatch.
pub struct BlockLootContext {
    pub params: LootParams,
    pub block_entity: Option<BlockEntityRef>,
}

/// Port of Block#popResource. The offsets are eager; the item constructor is
/// deferred until the empty-stack and BLOCK_DROPS guards have passed.
pub fn pop_resource(
    level: &mut Level,
    pos: BlockPos,
    stack: ItemStack,
) -> Result<Option<EntityRef>, LevelError> {
    let half_height = f64::from(EntityType::ITEM.data().height) / 2.0;
    let random = level.random();
    let position = Vec3::new(
        f64::from(pos.x) + 0.5 + mth::next_double(random, -0.25, 0.25),
        f64::from(pos.y) + 0.5 + mth::next_double(random, -0.25, 0.25) - half_height,
        f64::from(pos.z) + 0.5 + mth::next_double(random, -0.25, 0.25),
    );
    spawn_resource(level, &stack, |level| {
        ItemEntity::new_at(level, position, stack.clone())
    })
}
/// Port of Block#popResourceFromFace, including conditional offset and launch
/// draws before the guard and the explicit-velocity ItemEntity constructor.
pub fn pop_resource_from_face(
    level: &mut Level,
    pos: BlockPos,
    face: Direction,
    stack: ItemStack,
) -> Result<Option<EntityRef>, LevelError> {
    let step = face.step();
    let half_width = f64::from(EntityType::ITEM.data().width) / 2.0;
    let half_height = f64::from(EntityType::ITEM.data().height) / 2.0;
    let random = level.random();
    let position = Vec3::new(
        f64::from(pos.x)
            + 0.5
            + if step.x == 0 {
                mth::next_double(random, -0.25, 0.25)
            } else {
                f64::from(step.x) * (0.5 + half_width)
            },
        f64::from(pos.y)
            + 0.5
            + if step.y == 0 {
                mth::next_double(random, -0.25, 0.25)
            } else {
                f64::from(step.y) * (0.5 + half_height)
            }
            - half_height,
        f64::from(pos.z)
            + 0.5
            + if step.z == 0 {
                mth::next_double(random, -0.25, 0.25)
            } else {
                f64::from(step.z) * (0.5 + half_width)
            },
    );
    let velocity = Vec3::new(
        if step.x == 0 {
            mth::next_double(random, -0.1, 0.1)
        } else {
            f64::from(step.x) * 0.1
        },
        if step.y == 0 {
            mth::next_double(random, 0.0, 0.1)
        } else {
            f64::from(step.y) * 0.1 + 0.1
        },
        if step.z == 0 {
            mth::next_double(random, -0.1, 0.1)
        } else {
            f64::from(step.z) * 0.1
        },
    );
    spawn_resource(level, &stack, |level| {
        ItemEntity::new_at_with_velocity(level, position, stack.clone(), velocity)
    })
}
fn spawn_resource(
    level: &mut Level,
    stack: &ItemStack,
    constructor: impl FnOnce(&mut Level) -> Result<ItemEntity, LevelError>,
) -> Result<Option<EntityRef>, LevelError> {
    if stack.is_empty() || !level.block_drops_enabled()? {
        return Ok(None);
    }
    let mut item = constructor(level)?;
    // ItemEntity#setDefaultPickUpDelay.
    item.pickup_delay = 10;
    let handle = level.next_entity_ref();
    level.add_entity(handle, EntityState::Item(Box::new(item)))?;
    Ok(Some(handle))
}

/// Port of Block#dropResources's overload without a block entity.
pub fn drop_resources_simple(
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    drop_resources(state, pos, None, level)
}
pub fn drop_resources(
    state: BlockState,
    pos: BlockPos,
    block_entity: Option<BlockEntityRef>,
    level: &mut Level,
) -> Result<(), LevelError> {
    drop_resources_with_tool(state, pos, block_entity, None, &ItemStack::default(), level)
}
pub fn drop_resources_with_tool(
    state: BlockState,
    pos: BlockPos,
    block_entity: Option<BlockEntityRef>,
    breaker: Option<EntityRef>,
    tool: &ItemStack,
    level: &mut Level,
) -> Result<(), LevelError> {
    let operation = |level: &mut Level| {
        let drops = get_drops_with_tool(state, pos, block_entity, breaker, tool, level)?;
        for stack in drops {
            pop_resource(level, pos, stack)?;
        }
        level.optional_block_call(
            state.block(),
            BlockMethod::SpawnAfterBreak,
            |behavior, level| behavior.spawn_after_break(state, pos, tool, true, level),
        )?;
        Ok(())
    };
    match block_entity {
        Some(handle) => level.with_block_entity_callback(handle, operation),
        None => operation(level),
    }
}

/// Port of Block#getDrops's overload with an empty tool and no breaker.
pub fn get_drops(
    state: BlockState,
    pos: BlockPos,
    block_entity: Option<BlockEntityRef>,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    get_drops_with_tool(state, pos, block_entity, None, &ItemStack::default(), level)
}
pub fn get_drops_with_tool(
    state: BlockState,
    pos: BlockPos,
    block_entity: Option<BlockEntityRef>,
    breaker: Option<EntityRef>,
    tool: &ItemStack,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    let mut params = LootParams {
        origin: Some(Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        )),
        tool: Some(tool.clone()),
        this_entity: breaker,
        ..Default::default()
    };
    if let Some(handle) = block_entity {
        let receiver = level.block_entity_state(handle)?;
        params.block_entity =
            Some(Tag::Compound(receiver.save_with_full_metadata().map_err(
                |_| LevelError::Inventory("block drop entity NBT"),
            )?));
        params.block_entity_components = Some(receiver.collect_components());
    }
    get_drops_from_context(
        state,
        BlockLootContext {
            params,
            block_entity,
        },
        level,
    )
}
/// Callers such as explosions can supply the native optional BLOCK parameters.
pub fn get_drops_from_params(
    state: BlockState,
    params: LootParams,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    get_drops_from_context(
        state,
        BlockLootContext {
            params,
            block_entity: None,
        },
        level,
    )
}
/// Live callers with BLOCK_ENTITY supply its original generation explicitly;
/// an NBT position never substitutes for a receiver identity.
pub fn get_drops_from_context(
    state: BlockState,
    context: BlockLootContext,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    level
        .required_block_behavior(state.block(), BlockMethod::GetDrops)?
        .get_drops(state, context, level)
}
/// Port of BlockBehaviour#getDrops; absence returns before context validation.
pub(super) fn get_base_drops(
    state: BlockState,
    context: BlockLootContext,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    get_base_drops_with_dynamic(state, context, None, level)
}
/// Port of ShulkerBoxBlock#getDrops. Register the actual receiver's lazy contents
/// before evaluating the ordinary block table; each ordinal is a fresh getItem.
pub(super) fn get_shulker_drops(
    state: BlockState,
    context: BlockLootContext,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    let contents = match context.block_entity {
        Some(handle)
            if matches!(
                level.block_entity_state(handle)?,
                crate::block::entity::BlockEntityState::ShulkerBox(_)
            ) =>
        {
            Some(handle)
        }
        _ => None,
    };
    get_base_drops_with_dynamic(state, context, contents, level)
}
fn get_base_drops_with_dynamic(
    state: BlockState,
    context: BlockLootContext,
    contents: Option<BlockEntityRef>,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    let Some(name) = state.block().loot_table_name() else {
        return Ok(Vec::new());
    };
    let BlockLootContext {
        mut params,
        block_entity,
    } = context;
    params.block_state = Some(state);
    params
        .validate(
            &ContextKeySet::vanilla(&Identifier::parse("block").expect("built-in context"))
                .expect("block context"),
        )
        .map_err(|_| LevelError::Inventory("block loot context"))?;
    let resources = level.container_resources()?;
    let table = Identifier::parse(name).expect("extracted loot-table identifier");
    let mut runtime = LiveLootRuntime::for_table(level, &resources.loot, &table, 0)
        .map_err(|_| LevelError::Inventory("block loot stream"))?;
    if let Some(handle) = block_entity {
        runtime
            .bind_block_entity_source(handle)
            .map_err(|_| LevelError::Inventory("block loot receiver"))?;
    }
    if let Some(handle) = contents {
        runtime
            .register_dynamic_drop(
                Identifier::parse("minecraft:contents").expect("built-in dynamic drop"),
                crate::block::containers::loot_runtime::DynamicDrop::ContainerContents(handle),
            )
            .map_err(|_| LevelError::Inventory("shulker contents dynamic drop"))?;
    }
    resources
        .loot
        .get_random_items_with_runtime(&table, &params, &mut runtime)
        .map_err(|_| LevelError::Inventory("block loot evaluation"))
}

/// Port of Block#dropFromBlockInteractLootTable with Block#popResource as the
/// consumer. The returned boolean reflects generated drops, even if BLOCK_DROPS
/// suppresses entity creation. All loot is evaluated before the first pop.
pub fn drop_from_block_interact_loot_table(
    level: &mut Level,
    table: &Identifier,
    pos: BlockPos,
    state: BlockState,
    block_entity: Option<BlockEntityRef>,
    tool: Option<&ItemStack>,
    interacting_entity: Option<EntityRef>,
) -> Result<bool, LevelError> {
    let operation = |level: &mut Level| {
        let mut params = LootParams {
            origin: Some(Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            )),
            block_state: Some(state),
            tool: tool.cloned(),
            interacting_entity,
            ..Default::default()
        };
        if let Some(handle) = block_entity {
            let receiver = level.block_entity_state(handle)?;
            params.block_entity =
                Some(Tag::Compound(receiver.save_with_full_metadata().map_err(
                    |_| LevelError::Inventory("block interact loot block entity codec"),
                )?));
            params.block_entity_components = Some(receiver.collect_components());
        }
        params
            .validate(
                &ContextKeySet::vanilla(
                    &Identifier::parse("block_interact").expect("built-in context"),
                )
                .expect("block interact context"),
            )
            .map_err(|_| LevelError::Inventory("block interact loot context"))?;
        let resources = level.container_resources()?;
        let drops = {
            let mut runtime = LiveLootRuntime::for_table(level, &resources.loot, table, 0)
                .map_err(|_| LevelError::Inventory("block interact loot stream"))?;
            if let Some(handle) = block_entity {
                runtime
                    .bind_block_entity_source(handle)
                    .map_err(|_| LevelError::Inventory("block interact loot receiver"))?;
            }
            resources
                .loot
                .get_random_items_with_runtime(table, &params, &mut runtime)
                .map_err(|_| LevelError::Inventory("block interact loot evaluation"))?
        };
        let generated = !drops.is_empty();
        for stack in drops {
            pop_resource(level, pos, stack)?;
        }
        Ok(generated)
    };
    match block_entity {
        Some(handle) => level.with_block_entity_callback(handle, operation),
        None => operation(level),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Behaviors, level::Region};
    use ferraloom_data::Item;
    use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    #[test]
    fn eager_pop_random_draws_match_native_when_construction_is_suppressed() {
        let mut checked = 0;
        for line in include_str!("../../tests/java/blocks/entities/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("pop-resource-rng\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let mut level = {
                let mut fixture_level = Level::new(
                    Region::new(&BEHAVIORS),
                    crate::level::LevelContext::fixture(0, 63),
                );
                fixture_level.install_shared(Box::default());
                fixture_level
            };
            level.rules_mut().block_drops = Some(false);
            level.set_random_streams(
                Random::Legacy(LegacyRandomSource::new(fields[1].parse().unwrap())),
                0,
            );
            let stack = ItemStack::new(Item::STONE, fields[3].parse().unwrap(), Default::default());
            let pos = BlockPos::new(-2, 64, 3);
            let result = match fields[2] {
                "NONE" => pop_resource(&mut level, pos, stack),
                "up" => pop_resource_from_face(&mut level, pos, Direction::Up, stack),
                "east" => pop_resource_from_face(&mut level, pos, Direction::East, stack),
                _ => unreachable!(),
            };
            assert_eq!(result.unwrap(), None);
            assert_eq!(
                level.random().next_int(),
                fields[4].parse::<i32>().unwrap(),
                "{line}"
            );
            checked += 1;
        }
        assert_eq!(checked, 18);
    }
    #[test]
    fn shulker_drops_read_retained_components_names_and_lazy_contents() {
        use crate::{
            block::{containers::runtime::ContainerResources, entity::create},
            crafting::RecipeManager,
            level::chunk::LevelChunk,
            loot::{LootData, LootTable},
        };
        use ferraloom_protocol::{
            chat::Component,
            component::{ComponentTypeId, DataComponent},
        };
        use std::sync::Arc;
        let mut level = {
            let mut fixture_level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        let pos = BlockPos::new(3, 3, 3);
        let section = ferraloom_chunk::LevelChunkSection::new(
            ferraloom_chunk::Strategy::block_states(),
            ferraloom_chunk::Strategy::biomes(Arc::new(
                ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
            )),
            ferraloom_chunk::BiomeId(0),
        );
        let state = ferraloom_data::Block::SHULKER_BOX.default_state();
        let mut chunk = LevelChunk::new(ferraloom_util::ChunkPos::new(0, 0), 0..1, section);
        chunk.set_block_state(pos, state);
        level.add_chunk(chunk).unwrap();
        level.set_block_entity(create(pos, state).unwrap()).unwrap();
        let original = level.block_entity_ref(pos).unwrap().unwrap();
        level.retain_block_entity(original).unwrap();
        let params = LootParams {
            origin: Some(Vec3::new(3.5, 3.5, 3.5)),
            block_state: Some(state),
            tool: Some(ItemStack::default()),
            block_entity: Some(ferraloom_nbt::Tag::Compound(
                ferraloom_nbt::CompoundTag::new(),
            )),
            ..Default::default()
        };
        level.set_block_entity(create(pos, state).unwrap()).unwrap();
        {
            let receiver = level
                .block_entity_state_mut(original)
                .unwrap()
                .container_mut()
                .unwrap();
            receiver.custom_name = Some(Component::literal("retained"));
            receiver.items[0] = ItemStack::new(Item::STONE, 3, Default::default());
        }
        let table = Identifier::parse(state.block().loot_table_name().unwrap()).unwrap();
        for contents in [false, true] {
            let entry = if contents {
                serde_json::json!({"type":"dynamic","name":"contents"})
            } else {
                serde_json::json!({"type":"item","name":"shulker_box","modifier":[
                    {"type":"copy_components","source":"block_entity","include":["minecraft:container"]},
                    {"type":"copy_name","source":"block_entity"}
                ]})
            };
            let mut loot = LootData::default();
            loot.tables.insert(
                table.clone(),
                LootTable::decode(
                    &serde_json::json!({"type":"block","pools":[{"rolls":1,"entries":[entry]}]}),
                )
                .unwrap(),
            );
            let recipes = Arc::new(RecipeManager::new(Vec::new()).unwrap());
            let recipe_synchronization = Arc::new(crate::crafting::RecipeSynchronization::new(
                &recipes,
                |_| true,
            ));
            level.set_container_resources(
                Arc::new(ContainerResources {
                    crafting: crate::inventory::RuntimeCraftingContext::fixture(),
                    recipes: Arc::clone(&recipes),
                    loot: Arc::new(loot),
                    advancements: Arc::new(crate::advancements::ServerAdvancementManager::new(
                        Vec::new(),
                    )),
                    recipe_tags: Default::default(),
                    recipe_synchronization,
                }),
                0,
            );
            let drops = get_drops_from_context(
                state,
                BlockLootContext {
                    params: params.clone(),
                    block_entity: Some(original),
                },
                &mut level,
            )
            .unwrap();
            // ShulkerBoxBlock#getDrops emits every slot. LootTable's splitter
            // forwards empty stacks too; popResource consumes their eager RNG.
            assert_eq!(drops.len(), if contents { 27 } else { 1 });
            if contents {
                assert!(drops[1..].iter().all(ItemStack::is_empty));
                assert_eq!(drops[0].0.as_ref().unwrap().item, Item::STONE);
                assert_eq!(crate::entity::player::inventory::count(&drops[0]), 3);
            } else {
                let components = drops[0].components().unwrap();
                assert_eq!(
                    components.get(ComponentTypeId::CUSTOM_NAME),
                    Some(&DataComponent::CustomName(Box::new(Component::literal(
                        "retained"
                    ))))
                );
                let Some(DataComponent::Container(items)) =
                    components.get(ComponentTypeId::CONTAINER)
                else {
                    panic!("missing shulker container");
                };
                assert_eq!(items.0[0].as_ref().unwrap().item, Item::STONE);
                assert_eq!(items.0[0].as_ref().unwrap().count, 3);
            }
        }
        let current = level.block_entity_ref(pos).unwrap().unwrap();
        assert!(
            level
                .block_entity_state(current)
                .unwrap()
                .container()
                .unwrap()
                .items
                .iter()
                .all(ItemStack::is_empty)
        );
        level.release_block_entity(original);
    }
}
