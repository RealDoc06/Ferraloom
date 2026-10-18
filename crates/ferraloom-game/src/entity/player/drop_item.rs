//! LivingEntity#drop and #createItemStackToDrop, using canonical entity storage.
use crate::{
    entity::{
        EntityData, EntityRef, EntityReference, EntityState, item::ItemEntity, player::inventory,
    },
    inventory::{ContainerHandle, ContainerSlot, MenuError, StackValue},
    level::{Level, LevelError},
};
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{Vec3, mth, random::RandomSource};

/// Port of LivingEntity#drop's server data mutation. Swing, dropped-item
/// animation delivery remains an ordered hook; statistics run synchronously.
pub fn drop_item(
    player: EntityRef,
    stack: ItemStack,
    thrown_from_hand: bool,
    level: &mut Level,
) -> Result<Option<EntityRef>, LevelError> {
    drop_item_stack(player, StackValue::New(stack), thrown_from_hand, level)
}

/// Port of LivingEntity#createItemStackToDrop. Construction consumes the native
/// entity and player RNG draws but does not register the item or award stats.
pub fn create_item_stack_to_drop(
    player: EntityRef,
    stack: ItemStack,
    randomly: bool,
    thrown_from_hand: bool,
    level: &mut Level,
) -> Result<Option<ItemEntity>, LevelError> {
    if stack.is_empty() {
        return Ok(None);
    }
    let owner = level.entity::<EntityData>(player)?;
    let position = Vec3::new(
        owner.position.x,
        owner.position.y + f64::from(owner.eye_height) - 0.30000001192092896,
        owner.position.z,
    );
    let yaw = owner.yaw;
    let pitch = owner.pitch;
    let mut item = ItemEntity::new_at(level, position, stack);
    item.pickup_delay = 40;
    if thrown_from_hand {
        item.thrower = Some(EntityReference::of(player, level)?);
    }
    let owner = level.entity_mut::<EntityData>(player)?;
    item.parent.velocity = if randomly {
        let power = owner.random.next_float() * 0.5;
        let dir = owner.random.next_float() * mth::TWO_PI;
        Vec3::new(
            f64::from(-mth::sin(f64::from(dir)) * power),
            0.20000000298023224,
            f64::from(mth::cos(f64::from(dir)) * power),
        )
    } else {
        let sin_x = mth::sin(f64::from(pitch * 0.017453292_f32));
        let cos_x = mth::cos(f64::from(pitch * 0.017453292_f32));
        let sin_y = mth::sin(f64::from(yaw * 0.017453292_f32));
        let cos_y = mth::cos(f64::from(yaw * 0.017453292_f32));
        let dir = owner.random.next_float() * mth::TWO_PI;
        let power = 0.02 * owner.random.next_float();
        Vec3::new(
            f64::from(-sin_y * cos_x * 0.3) + f64::from(dir).cos() * f64::from(power),
            f64::from(
                -sin_x * 0.3 + 0.1 + (owner.random.next_float() - owner.random.next_float()) * 0.1,
            ),
            f64::from(cos_y * cos_x * 0.3) + f64::from(dir).sin() * f64::from(power),
        )
    };
    Ok(Some(item))
}

/// Port of LivingEntity#drop and ServerPlayer#drop for a held stack receiver.
/// An Existing receiver must stay rooted through the caller's following setter.
pub fn drop_item_stack(
    player: EntityRef,
    stack: StackValue,
    thrown_from_hand: bool,
    level: &mut Level,
) -> Result<Option<EntityRef>, LevelError> {
    if stack.as_stack(level).map_err(stack_error)?.is_empty() {
        return Ok(None);
    }
    level.entity::<EntityData>(player)?;
    let original = match &stack {
        StackValue::Existing(reference) => Some(*reference),
        StackValue::New(_) => None,
    };
    let stack = match stack {
        StackValue::New(stack) => stack,
        StackValue::Existing(reference) => {
            level.take_stack_for_move(reference).map_err(stack_error)?
        }
    };
    let Some(item) = create_item_stack_to_drop(player, stack, false, thrown_from_hand, level)?
    else {
        return Ok(None);
    };
    let entity = level.next_entity_ref();
    level.add_entity(entity, EntityState::Item(Box::new(item)))?;
    if let Some(original) = original {
        let destination = level
            .stack_ref(ContainerSlot {
                container: ContainerHandle::Entity(entity),
                slot: 0,
            })
            .map_err(stack_error)?;
        // Native drop -> addFreshEntity -> awardStat reads only the stack object; setCarried(EMPTY) follows.
        level
            .forward_moved_stack(original, destination)
            .map_err(stack_error)?;
    }
    let dropped = level.entity::<ItemEntity>(entity)?.item();
    let kind = dropped.item();
    let count = inventory::count(match original {
        Some(original) => level.stack(original).map_err(stack_error)?,
        None => dropped,
    });
    if thrown_from_hand && !level.entity::<ItemEntity>(entity)?.item().is_empty() {
        super::award_stat(player, crate::stats::Stat::Dropped(kind), count, level)?;
        super::award_stat(
            player,
            crate::stats::Stat::Custom(crate::stats::custom::Id::DROP),
            1,
            level,
        )?;
    }
    Ok(Some(entity))
}

fn stack_error(error: MenuError) -> LevelError {
    match error {
        MenuError::Level(error) => error,
        MenuError::Unsupported(operation) => LevelError::Inventory(operation),
        _ => LevelError::Inventory("dropped stack receiver"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::player::{GameType, ServerPlayer},
        inventory::StackOperation,
        level::entity::EntityManagementEvent,
    };
    use ferraloom_data::Item;
    use ferraloom_util::{Uuid, random::LegacyRandomSource};

    #[test]
    fn native_unregistered_drop_constructor_rng_and_give_variants() {
        fn vector(value: Vec3) -> String {
            format!(
                "{:x},{:x},{:x}",
                value.x.to_bits(),
                value.y.to_bits(),
                value.z.to_bits()
            )
        }
        let mut cases = 0;
        for row in include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("drop-create\t"))
        {
            let fields: Vec<_> = row.split('\t').collect();
            let scenario: usize = fields[1].parse().unwrap();
            let count: i32 = fields[3].parse().unwrap();
            let player_seed = fields[4].parse().unwrap();
            let entity_seed = fields[5].parse().unwrap();
            let rotation: Vec<_> = fields[6]
                .split(',')
                .map(|value| f32::from_bits(u32::from_str_radix(value, 16).unwrap()))
                .collect();
            let mut level = crate::entity::tests::level(false);
            let player = level.next_entity_ref();
            let mut state = ServerPlayer::new(
                Uuid::new(1, 2),
                "drop".into(),
                level.next_entity_id(),
                Vec3::new(8.5, 64.0, 8.5),
                GameType::Survival,
            );
            state.set_y_rot(rotation[0]);
            state.set_x_rot(rotation[1]);
            state.random.set_seed(player_seed);
            level
                .add_entity(player, EntityState::Player(Box::new(state)))
                .unwrap();
            level.random().set_seed(555);
            level.take_entity_management_events();
            level.take_entity_effects();
            level.take_effects();
            // No ServerShared is installed: constructing a drop has no stat dependency.
            let mut item = crate::level::entity::with_entity_random_seed(entity_seed, || {
                create_item_stack_to_drop(
                    player,
                    ItemStack::new(Item::STONE, count, Default::default()),
                    scenario >= 2,
                    scenario % 2 != 0,
                    &mut level,
                )
            })
            .unwrap();
            let (actual, entity_next) = if let Some(item) = &mut item {
                match fields[2] {
                    "normal" => {}
                    "fake" => item.make_fake_item(),
                    "leftover" => {
                        item.pickup_delay = 0;
                        item.target = Some(level.entity::<ServerPlayer>(player).unwrap().uuid);
                    }
                    mode => panic!("unexpected native mode {mode}"),
                }
                (
                    format!(
                        "{}\t{}\t{}\t{}\t{:x},{:x}\t{}\t{}\t{}\t{}\t{}",
                        vector(item.parent.position),
                        vector(item.parent.velocity),
                        item.parent.network_id,
                        item.parent.uuid,
                        item.bob_offs.to_bits(),
                        item.parent.yaw.to_bits(),
                        item.age,
                        item.pickup_delay,
                        item.thrower
                            .map(|owner| owner.uuid(&level).unwrap().to_string())
                            .unwrap_or_else(|| "-".into()),
                        item.target
                            .map(|uuid| uuid.to_string())
                            .unwrap_or_else(|| "-".into()),
                        inventory::count(item.item()),
                    ),
                    item.parent.random.next_long(),
                )
            } else {
                assert_eq!(count, 0, "{row}");
                (
                    "-\t-\t-\t-\t-\t-\t-\t-\t-\t0".into(),
                    LegacyRandomSource::new(entity_seed).next_long(),
                )
            };
            assert_eq!(actual, fields[7..17].join("\t"), "{row}");
            assert_eq!(i32::from(item.is_some()).to_string(), fields[17], "{row}");
            assert_eq!(
                format!(
                    "{:x}\t{:x}\t{:x}\t{}",
                    level
                        .entity_mut::<EntityData>(player)
                        .unwrap()
                        .random
                        .next_long(),
                    entity_next,
                    level.random().next_long(),
                    level.next_entity_id(),
                ),
                fields[18..].join("\t"),
                "{row}",
            );
            assert_eq!(level.all_entities(), [player], "{row}");
            assert!(level.take_entity_management_events().is_empty(), "{row}");
            assert!(level.take_entity_effects().is_empty(), "{row}");
            assert!(level.take_effects().is_empty(), "{row}");
            let stats = &level.entity::<ServerPlayer>(player).unwrap().stats;
            assert_eq!(stats.get_value(crate::stats::Stat::Dropped(Item::STONE)), 0);
            assert_eq!(
                stats.get_value(crate::stats::Stat::Custom(crate::stats::custom::Id::DROP)),
                0,
            );
            // Give configures the owned item first, then uses the ordinary addFreshEntity path.
            if let Some(item) = item {
                let entity = level.next_entity_ref();
                level
                    .add_entity(entity, EntityState::Item(Box::new(item)))
                    .unwrap();
                assert!(level.all_entities().contains(&entity));
                assert_eq!(
                    level.take_entity_management_events(),
                    [
                        EntityManagementEvent::Created(entity),
                        EntityManagementEvent::TrackingStart(entity),
                        EntityManagementEvent::TickingStart(entity),
                    ],
                );
            }
            cases += 1;
        }
        assert_eq!(cases, 24);
    }

    #[test]
    fn carried_drop_preserves_native_receiver_stat_count_and_registration_order() {
        let mut actual = Vec::new();
        for scenario in ["carried", "empty", "not-thrown"] {
            let mut level = crate::entity::tests::level(false);
            level.install_shared(Box::default());
            let player = level.next_entity_ref();
            let state = ServerPlayer::new(
                Uuid::new(1, 2),
                "drop".into(),
                level.next_entity_id(),
                Vec3::new(8.5, 64.0, 8.5),
                GameType::Survival,
            );
            level
                .add_entity(player, EntityState::Player(Box::new(state)))
                .unwrap();
            let menu = level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .menus
                .current_ref();
            let carried_slot = ContainerSlot {
                container: ContainerHandle::MenuCarried { player, menu },
                slot: 0,
            };
            level
                .replace_stack_object(
                    carried_slot,
                    StackValue::New(ItemStack::new(
                        Item::STONE,
                        if scenario == "empty" { 0 } else { 7 },
                        Default::default(),
                    )),
                )
                .unwrap();
            level.take_entity_management_events();
            let mut operation = StackOperation::new(&mut level);
            let carried = operation.pin(carried_slot).unwrap();
            let thrown = scenario != "not-thrown";
            let dropped = drop_item_stack(
                player,
                StackValue::Existing(carried),
                thrown,
                operation.level,
            )
            .unwrap();
            let mut calls = Vec::new();
            let mut identity = false;
            let mut dropped_count = 0;
            if let Some(entity) = dropped {
                let item = operation.level.entity::<ItemEntity>(entity).unwrap();
                identity = std::ptr::eq(operation.level.stack(carried).unwrap(), item.item());
                dropped_count = inventory::count(item.item());
                assert_eq!(
                    operation.level.take_entity_management_events(),
                    [
                        EntityManagementEvent::Created(entity),
                        EntityManagementEvent::TrackingStart(entity),
                        EntityManagementEvent::TickingStart(entity),
                    ]
                );
                calls.push(format!("add:{dropped_count}"));
                let stats = &operation
                    .level
                    .entity::<ServerPlayer>(player)
                    .unwrap()
                    .stats;
                let item_dropped = stats.get_value(crate::stats::Stat::Dropped(Item::STONE));
                let drops =
                    stats.get_value(crate::stats::Stat::Custom(crate::stats::custom::Id::DROP));
                assert_eq!(item_dropped, if thrown { 7 } else { 0 });
                assert_eq!(drops, i32::from(thrown));
                assert_eq!(inventory::count(operation.level.stack(carried).unwrap()), 7);
                assert!(operation.level.take_entity_effects().is_empty());
                if thrown {
                    calls.push(format!("item_dropped:{item_dropped}"));
                    calls.push(format!("drop:{drops}"));
                }
            } else {
                assert!(operation.level.take_entity_management_events().is_empty());
                assert!(operation.level.take_entity_effects().is_empty());
            }
            operation
                .level
                .replace_stack_object(carried_slot, StackValue::New(ItemStack::default()))
                .unwrap();
            let empty = operation
                .level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .menus
                .menu(menu)
                .unwrap()
                .carried
                .is_empty();
            actual.push(format!(
                "drop-receiver\t{scenario}\t{}\t{identity}\t{dropped_count}\t{empty}",
                if calls.is_empty() {
                    "-".into()
                } else {
                    calls.join(",")
                }
            ));
            if let Some(entity) = dropped {
                inventory::set_count(operation.level.stack_mut(carried).unwrap(), 3);
                assert_eq!(
                    inventory::count(operation.level.entity::<ItemEntity>(entity).unwrap().item()),
                    3
                );
                assert!(std::ptr::eq(
                    operation.level.stack(carried).unwrap(),
                    operation.level.entity::<ItemEntity>(entity).unwrap().item()
                ));
            }
        }
        let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("drop-receiver\t"))
            .collect();
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), 3);
    }
}
