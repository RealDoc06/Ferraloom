//! Port of AbstractFurnaceBlockEntity#serverTick and its fuel/cooking helpers.
use super::furnace::AbstractFurnaceBlockEntity;
use super::loot_runtime::LiveLootRuntime;
use crate::{
    block::{UpdateFlags, entity::BlockEntityRef},
    crafting::RecipeInput,
    entity::player::inventory::{
        component, count, is_same_item_same_components, max_stack_size, set_count,
    },
    inventory::Container,
    level::{Level, LevelError},
    loot::{ContextKeySet, Holder, LootParams, LootRandomSelection},
};
use ferraloom_data::{Item, properties as p};
use ferraloom_nbt::Tag;
use ferraloom_protocol::{
    Identifier,
    codec::Either,
    component::{ComponentTypeId, DataComponent, DataComponentPatch, ItemStack, Persistent},
};
use ferraloom_util::Vec3;

fn fuel_context(handle: BlockEntityRef, level: &Level) -> Result<LootParams, LevelError> {
    let receiver = level.block_entity_state(handle)?;
    let params = LootParams {
        origin: Some(Vec3::new(
            f64::from(handle.pos.x) + 0.5,
            f64::from(handle.pos.y) + 0.5,
            f64::from(handle.pos.z) + 0.5,
        )),
        block_state: Some(receiver.data().block_state),
        block_entity: Some(Tag::Compound(
            receiver
                .save_with_full_metadata()
                .map_err(|_| LevelError::Inventory("furnace loot NBT"))?,
        )),
        block_entity_components: Some(receiver.collect_components()),
        container: Some(
            receiver
                .container()
                .expect("furnace container")
                .items
                .clone(),
        ),
        ..Default::default()
    };
    params
        .validate(
            &ContextKeySet::vanilla(
                &Identifier::parse("container_process").expect("built-in context"),
            )
            .expect("container context"),
        )
        .map_err(|_| LevelError::Inventory("furnace loot context"))?;
    Ok(params)
}
fn fuel_properties(
    handle: BlockEntityRef,
    fuel: &ItemStack,
    level: &mut Level,
) -> Result<(i32, f32), LevelError> {
    let Some(DataComponent::CookingFuel(fuel)) =
        component(fuel, ComponentTypeId::COOKING_FUEL).cloned()
    else {
        return Ok((0, 1.0));
    };
    let resources = level.container_resources()?;
    let params = fuel_context(handle, level)?;
    let duration = match fuel.burn_time {
        Either::Left(duration) => duration,
        Either::Right(id) => resources
            .loot
            .evaluate_int_with_runtime(
                &Holder::Reference(id),
                &params,
                &mut fuel_runtime(handle, level)?,
            )
            .map_err(|_| LevelError::Inventory("furnace burn-time provider"))?,
    };
    let params = fuel_context(handle, level)?;
    let speed = match fuel.speed_multiplier {
        Either::Left(speed) => speed.0,
        Either::Right(id) => resources
            .loot
            .evaluate_float_with_runtime(
                &Holder::Reference(id),
                &params,
                &mut fuel_runtime(handle, level)?,
            )
            .map_err(|_| LevelError::Inventory("furnace speed provider"))?,
    };
    Ok((duration, speed))
}
fn fuel_runtime(
    handle: BlockEntityRef,
    level: &mut Level,
) -> Result<LiveLootRuntime<'_>, LevelError> {
    let mut runtime = LiveLootRuntime::new(level, LootRandomSelection::Level);
    runtime
        .bind_block_entity_source(handle)
        .map_err(|_| LevelError::Inventory("furnace loot receiver"))?;
    Ok(runtime)
}
fn can_burn(entity: &AbstractFurnaceBlockEntity, result: &ItemStack) -> bool {
    let output = &entity.parent.items[2];
    output.is_empty()
        || is_same_item_same_components(output, result)
            && count(output) + count(result)
                <= entity.parent.max_stack_size().min(max_stack_size(result))
}
fn consume_fuel(handle: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
    let remainder = {
        let entity = level.block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?;
        let fuel = &mut entity.parent.items[1];
        let remainder = fuel
            .0
            .as_ref()
            .expect("nonempty fuel")
            .item
            .data()
            .crafting_remainder;
        set_count(fuel, count(fuel) - 1);
        remainder
    };
    if let Some(remainder) = remainder {
        let components = DataComponentPatch::from_json(
            &serde_json::from_str(remainder.components_json)
                .expect("extracted remainder components"),
        )
        .map_err(|_| LevelError::Inventory("fuel crafting remainder components"))?;
        let mut stack = ItemStack::new(remainder.item, remainder.count, components);
        if level
            .block_entity::<AbstractFurnaceBlockEntity>(handle)?
            .parent
            .items[1]
            .is_empty()
        {
            level
                .block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?
                .parent
                .replace_item(1, stack);
        } else {
            super::drops::drop_item_stack(
                level,
                Vec3::new(
                    f64::from(handle.pos.x),
                    f64::from(handle.pos.y),
                    f64::from(handle.pos.z),
                ),
                &mut stack,
            )?;
        }
    }
    Ok(())
}
fn burn(entity: &mut AbstractFurnaceBlockEntity, result: ItemStack) {
    if entity.parent.items[2].is_empty() {
        entity.parent.replace_item(2, result);
    } else {
        let total = count(&entity.parent.items[2]) + count(&result);
        set_count(&mut entity.parent.items[2], total);
    }
    let items = &entity.parent.items;
    if items[0]
        .0
        .as_ref()
        .is_some_and(|stack| stack.item == Item::WET_SPONGE)
        && !items[1].is_empty()
        && items[1]
            .0
            .as_ref()
            .is_some_and(|stack| stack.item == Item::BUCKET)
    {
        entity
            .parent
            .replace_item(1, ItemStack::new(Item::WATER_BUCKET, 1, Default::default()));
    }
    let remaining = count(&entity.parent.items[0]) - 1;
    set_count(&mut entity.parent.items[0], remaining);
}

pub fn server_tick(handle: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
    level.with_block_entity_callback(handle, |level| {
        let (state, was_lit, mut is_lit, has_input, has_fuel) = {
            let entity = level.block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?;
            let was_lit = entity.lit_time_remaining > 0;
            if was_lit {
                entity.lit_time_remaining -= 1;
            }
            (
                entity.parent.parent.block_state,
                was_lit,
                entity.lit_time_remaining > 0,
                !entity.parent.items[0].is_empty(),
                !entity.parent.items[1].is_empty(),
            )
        };
        let mut changed = false;
        if is_lit || has_fuel && has_input {
            if has_input {
                let resources = level.container_resources()?;
                let entity = level.block_entity::<AbstractFurnaceBlockEntity>(handle)?;
                let recipe = resources
                    .recipes
                    .get_recipe_for(
                        entity.recipe_type(),
                        RecipeInput::Single(&entity.parent.items[0]),
                        &(),
                        entity.last_recipe.as_ref(),
                    )
                    .map_err(|_| LevelError::Inventory("furnace recipe lookup"))?;
                if let Some(recipe) = recipe {
                    let result = recipe
                        .value
                        .assemble(RecipeInput::Single(&entity.parent.items[0]), &())
                        .map_err(|_| LevelError::Inventory("furnace recipe assembly"))?;
                    let can_burn = !result.is_empty() && can_burn(entity, &result);
                    let recipe_time = recipe.value.cooking_time().expect("cooking recipe");
                    let entity = level.block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?;
                    entity.last_recipe = Some(recipe.id.clone());
                    if can_burn {
                        if !is_lit {
                            let fuel = entity.parent.items[1].clone();
                            let (duration, speed) = fuel_properties(handle, &fuel, level)?;
                            let entity =
                                level.block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?;
                            entity.lit_time_remaining = duration;
                            entity.lit_total_time = duration;
                            entity.speed_multiplier = speed;
                            if entity.cooking_total_time > 0
                                && entity.cooking_timer < entity.cooking_total_time
                            {
                                let completion =
                                    entity.cooking_timer as f32 / entity.cooking_total_time as f32;
                                entity.cooking_total_time = entity.total_cook_time(recipe_time);
                                entity.cooking_timer =
                                    (completion * entity.cooking_total_time as f32).ceil() as i32;
                            }
                            if duration > 0 {
                                consume_fuel(handle, level)?;
                                is_lit = true;
                                changed = true;
                            }
                        }
                        let entity =
                            level.block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?;
                        if is_lit {
                            entity.cooking_timer += 1;
                            if entity.cooking_timer >= entity.cooking_total_time {
                                entity.cooking_timer = 0;
                                entity.cooking_total_time = entity.total_cook_time(recipe_time);
                                burn(entity, result);
                                let id = Identifier::parse(&recipe.id.to_string())
                                    .expect("validated recipe key");
                                entity.recipes_used.record(id);
                                changed = true;
                            }
                        } else {
                            entity.cooking_timer = 0;
                        }
                    } else {
                        entity.cooking_timer = 0;
                    }
                }
                // A missing recipe deliberately leaves an existing timer alone.
            } else {
                level
                    .block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?
                    .cooking_timer = 0;
            }
        } else {
            let entity = level.block_entity_mut::<AbstractFurnaceBlockEntity>(handle)?;
            if entity.cooking_timer > 0 {
                entity.cooking_timer = (entity.cooking_timer - 2)
                    .max(0)
                    .min(entity.cooking_total_time);
            }
        }
        if was_lit != is_lit {
            changed = true;
            let state = state.with(p::LIT, is_lit).expect("furnace LIT");
            level.set_block(
                handle.pos,
                state,
                UpdateFlags::ALL,
                crate::block::UPDATE_LIMIT,
            )?;
        }
        if changed {
            handle.set_changed(level)?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        block::{containers::runtime::ContainerResources, entity::create},
        crafting::{Ingredient, Recipe, RecipeHolder, RecipeKind, RecipeManager, RecipeType},
        level::{Region, chunk::LevelChunk},
        loot::LootData,
    };
    use ferraloom_data::{
        Block,
        datapack::{ResourceManager, TagSet},
    };
    use ferraloom_protocol::component::{CookingFuel, ItemStackTemplate};
    use ferraloom_util::{BlockPos, ChunkPos};
    use std::sync::Arc;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    #[test]
    fn cooking_time_float_order_matches_native() {
        let mut entity = AbstractFurnaceBlockEntity::new(
            crate::block::entity::BlockEntityData::new(
                crate::block::entity::BlockEntityType::FURNACE,
                BlockPos::ZERO,
                Block::FURNACE.default_state(),
            )
            .unwrap(),
        );
        let mut checked = 0;
        for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("cook-time\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            entity.speed_multiplier = f32::from_bits(u32::from_str_radix(fields[1], 16).unwrap());
            assert_eq!(
                entity.total_cook_time(fields[2].parse().unwrap()),
                fields[3].parse::<i32>().unwrap(),
                "{line}"
            );
            checked += 1;
        }
        assert_eq!(checked, 30);
    }
    #[test]
    fn cooking_timeline_relights_and_preserves_missing_recipe_timer() {
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        level.set_block_tags(Arc::new(
            TagSet::load(&ResourceManager::new(Vec::new())).unwrap(),
        ));
        let section = ferraloom_chunk::LevelChunkSection::new(
            ferraloom_chunk::Strategy::block_states(),
            ferraloom_chunk::Strategy::biomes(Arc::new(
                ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
            )),
            ferraloom_chunk::BiomeId(0),
        );
        let mut chunk = LevelChunk::new(ChunkPos::new(0, 0), 0..1, section);
        let pos = BlockPos::new(3, 3, 3);
        let state = Block::FURNACE.default_state().with(p::LIT, true).unwrap();
        chunk.set_block_state(pos, state);
        level.add_chunk(chunk).unwrap();
        let mut entity = create(pos, state).unwrap();
        let mut fuel = DataComponentPatch::default();
        fuel.set(DataComponent::CookingFuel(CookingFuel {
            burn_time: Either::Left(3),
            speed_multiplier: Either::Left(2.0.into()),
        }));
        entity.container_mut().unwrap().items[0] =
            ItemStack::new(Item::IRON_ORE, 2, Default::default());
        entity.container_mut().unwrap().items[1] = ItemStack::new(Item::COAL, 2, fuel);
        level.set_block_entity(entity).unwrap();
        let handle = level.block_entity_ref(pos).unwrap().unwrap();
        let recipe = RecipeHolder {
            id: "test:iron".parse().unwrap(),
            value: Recipe {
                kind: RecipeKind::Cooking {
                    recipe_type: RecipeType::Smelting,
                    ingredient: Ingredient::of(vec![Item::IRON_ORE]).unwrap(),
                    result: ItemStackTemplate {
                        item: Item::IRON_INGOT,
                        count: 1,
                        components: Default::default(),
                    },
                    cooking_time: 6,
                    experience: 0.7,
                },
                group: String::new(),
                category: String::new(),
                show_notification: true,
            },
        };
        let recipes = Arc::new(RecipeManager::new(vec![recipe]).unwrap());
        let recipe_synchronization = Arc::new(crate::crafting::RecipeSynchronization::new(
            &recipes,
            |_| true,
        ));
        level.set_container_resources(
            Arc::new(ContainerResources {
                crafting: crate::inventory::RuntimeCraftingContext::fixture(),
                recipes: Arc::clone(&recipes),
                loot: Arc::new(LootData::default()),
                advancements: Arc::new(crate::advancements::ServerAdvancementManager::new(
                    Vec::new(),
                )),
                recipe_tags: Default::default(),
                recipe_synchronization,
            }),
            0,
        );
        level
            .block_entity_mut::<AbstractFurnaceBlockEntity>(handle)
            .unwrap()
            .cooking_total_time = 6;
        for (lit, cooking, output) in [
            (3, 1, 0),
            (2, 2, 0),
            (1, 0, 1),
            (3, 1, 1),
            (2, 2, 1),
            (1, 0, 2),
        ] {
            server_tick(handle, &mut level).unwrap();
            let entity = level
                .block_entity::<AbstractFurnaceBlockEntity>(handle)
                .unwrap();
            assert_eq!(
                (
                    entity.lit_time_remaining,
                    entity.cooking_timer,
                    count(&entity.parent.items[2])
                ),
                (lit, cooking, output)
            );
        }
        assert_eq!(
            level
                .block_entity::<AbstractFurnaceBlockEntity>(handle)
                .unwrap()
                .recipes_used
                .values()
                .copied()
                .sum::<i32>(),
            2
        );
        let entity = level
            .block_entity_mut::<AbstractFurnaceBlockEntity>(handle)
            .unwrap();
        entity.lit_time_remaining = 2;
        entity.cooking_timer = 9;
        entity.parent.items[0] = ItemStack::new(Item::STONE, 1, Default::default());
        server_tick(handle, &mut level).unwrap();
        assert_eq!(
            level
                .block_entity::<AbstractFurnaceBlockEntity>(handle)
                .unwrap()
                .cooking_timer,
            9
        );
    }
}
