//! Port of Block's loot contexts and synchronous resource spawning.
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
    constructor: impl FnOnce(&mut Level) -> ItemEntity,
) -> Result<Option<EntityRef>, LevelError> {
    if stack.is_empty() || !level.block_drops_enabled()? {
        return Ok(None);
    }
    let mut item = constructor(level);
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
    get_drops_from_params(state, params, level)
}
/// Callers such as explosions can supply the native optional BLOCK parameters.
pub fn get_drops_from_params(
    state: BlockState,
    params: LootParams,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    level
        .required_block_behavior(state.block(), BlockMethod::GetDrops)?
        .get_drops(state, params, level)
}
/// Port of BlockBehaviour#getDrops; absence returns before context validation.
pub(super) fn get_base_drops(
    state: BlockState,
    mut params: LootParams,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    let Some(name) = state.block().loot_table_name() else {
        return Ok(Vec::new());
    };
    params.block_state = Some(state);
    params
        .validate(
            &ContextKeySet::vanilla(&Identifier::parse("block").expect("built-in context"))
                .expect("block context"),
        )
        .map_err(|_| LevelError::Inventory("block loot context"))?;
    let resources = level.container_resources()?;
    let table = Identifier::parse(name).expect("extracted loot-table identifier");
    level
        .with_loot_sources(|sources| {
            resources.loot.get_random_items_seeded(
                &table,
                &params,
                0,
                sources,
                resources.facts.as_ref(),
            )
        })
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
        let drops = level
            .with_loot_sources(|sources| {
                resources.loot.get_random_items_seeded(
                    table,
                    &params,
                    0,
                    sources,
                    resources.facts.as_ref(),
                )
            })
            .map_err(|_| LevelError::Inventory("block interact loot evaluation"))?;
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
            let mut level = Level::new(Region::new(&BEHAVIORS));
            level.rules_mut().block_drops = Some(false);
            level.set_random_streams(
                Random::Legacy(LegacyRandomSource::new(fields[1].parse().unwrap())),
                0,
            );
            let stack = ItemStack::new(Item::STONE, fields[3].parse().unwrap(), Default::default());
            let pos = BlockPos::new(-2, 64, 3);
            let result = match fields[2] {
                "NONE" => pop_resource(&mut level, pos, stack),
                "UP" => pop_resource_from_face(&mut level, pos, Direction::Up, stack),
                "EAST" => pop_resource_from_face(&mut level, pos, Direction::East, stack),
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
}
