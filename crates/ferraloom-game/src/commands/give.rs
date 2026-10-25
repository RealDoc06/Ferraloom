//! GiveCommand over canonical inventory receivers and unregistered drop construction.
use super::{
    arguments::items::ItemInput,
    builtins::{CommandResult, component, integer},
    exceptions::error_with_arguments,
};
use crate::{
    entity::{
        EntityData, EntityRef, EntityState,
        item::ItemEntity,
        player::{self, inventory},
    },
    inventory::{ContainerHandle, ContainerSlot, MenuError, StackOperation},
    item::name::ItemStackName,
    level::{Level, LevelError},
};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_data::registries::sound_event::Id as SoundEvent;
use ferraloom_protocol::{chat::Component, play::SoundSource};
use ferraloom_util::random::RandomSource;
use std::fmt;

#[derive(Debug)]
pub enum GiveError {
    Syntax(CommandSyntaxException),
    Level(LevelError),
    Inventory(MenuError),
    Serialization(ferraloom_protocol::Error),
}
impl fmt::Display for GiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(e) => e.fmt(f),
            Self::Level(e) => e.fmt(f),
            Self::Inventory(e) => e.fmt(f),
            Self::Serialization(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for GiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Syntax(e) => Some(e),
            Self::Level(e) => Some(e),
            Self::Inventory(e) => Some(e),
            Self::Serialization(e) => Some(e),
        }
    }
}
impl From<CommandSyntaxException> for GiveError {
    fn from(e: CommandSyntaxException) -> Self {
        Self::Syntax(e)
    }
}
impl From<LevelError> for GiveError {
    fn from(e: LevelError) -> Self {
        Self::Level(e)
    }
}
impl From<MenuError> for GiveError {
    fn from(e: MenuError) -> Self {
        Self::Inventory(e)
    }
}
impl From<ferraloom_protocol::Error> for GiveError {
    fn from(e: ferraloom_protocol::Error) -> Self {
        Self::Serialization(e)
    }
}

fn add_item(item: ItemEntity, level: &mut Level) -> Result<EntityRef, LevelError> {
    let entity = level.next_entity_ref();
    level.add_entity(entity, EntityState::Item(Box::new(item)))?;
    Ok(entity)
}

/// Port of GiveCommand#giveItem. The caller captures player targets at the
/// native argument-evaluation point before lending the live Level exclusively.
pub fn run(
    input: &ItemInput,
    count: i32,
    players: &[EntityRef],
    level: &mut Level,
) -> Result<CommandResult, GiveError> {
    let prototype = input.create_item_stack(1)?;
    let maximum = inventory::max_stack_size(&prototype);
    let allowed = maximum * 100;
    if count > allowed {
        return Err(error_with_arguments(
            "commands.give.failed.toomanyitems",
            vec![integer(allowed), component(prototype.get_display_name()?)],
        )
        .into());
    }
    for &player in players {
        let mut remaining = count;
        while remaining > 0 {
            let amount = maximum.min(remaining);
            remaining -= amount;
            let mut operation = StackOperation::new(level);
            let copy_to_drop = operation.own(player, prototype.copy_with_count(amount))?;
            let added = crate::inventory::add_item_stack(player, copy_to_drop, operation.level)?;
            if added && operation.level.stack(copy_to_drop)?.is_empty() {
                if let Some(mut item) = player::create_item_stack_to_drop(
                    player,
                    prototype.clone(),
                    false,
                    false,
                    operation.level,
                )? {
                    item.make_fake_item();
                    add_item(item, operation.level)?;
                }
                let owner = operation.level.entity_mut::<EntityData>(player)?;
                let position = owner.position;
                let pitch =
                    ((owner.random.next_float() - owner.random.next_float()) * 0.7 + 1.0) * 2.0;
                operation.level.play_sound(
                    None,
                    position,
                    SoundEvent::ENTITY_ITEM_PICKUP,
                    SoundSource::Players,
                    0.2,
                    pitch,
                )?;
                crate::inventory::broadcast_container_menu(player, operation.level)?;
            } else {
                let leftover = operation.level.take_stack_for_move(copy_to_drop)?;
                if let Some(mut item) = player::create_item_stack_to_drop(
                    player,
                    leftover,
                    false,
                    false,
                    operation.level,
                )? {
                    item.pickup_delay = 0;
                    item.target = Some(operation.level.entity::<EntityData>(player)?.uuid);
                    let entity = add_item(item, operation.level)?;
                    let destination = operation.level.stack_ref(ContainerSlot {
                        container: ContainerHandle::Entity(entity),
                        slot: 0,
                    })?;
                    operation
                        .level
                        .forward_moved_stack(copy_to_drop, destination)?;
                }
            }
        }
    }
    let mut args = vec![integer(count), component(prototype.get_display_name()?)];
    let key = if let [player] = players {
        args.push(component(level.entity_display_name(*player)?));
        "commands.give.success.single"
    } else {
        args.push(integer(players.len() as i32));
        "commands.give.success.multiple"
    };
    Ok(CommandResult::success(
        players.len() as i32,
        Component::translatable(key, args),
        true,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::exceptions::error_component,
        entity::player::{GameType, ServerPlayer},
        level::effects::LevelEffect,
    };
    use ferraloom_data::Item;
    use ferraloom_protocol::{
        component::{ItemStack, Persistent},
        play::clientbound as cb,
    };
    use ferraloom_util::{Uuid, Vec3};
    use serde_json::{Value, json};

    #[test]
    fn native_give_inventory_drop_sound_rng_and_feedback_outcomes_match() {
        let rows: Vec<Value> =
            serde_json::from_str(include_str!("vanilla/give-vectors.json")).unwrap();
        for row in &rows {
            let mut level = crate::entity::tests::level(false);
            level.set_unported_behavior(crate::block::UnportedBehavior::Strict);
            crate::level::drop_fixtures::install(&mut level);
            level.install_shared(Box::default());
            let mut players = Vec::new();
            for index in 0..row["targets"].as_i64().unwrap() {
                let entity = level.next_entity_ref();
                let mut state = ServerPlayer::new(
                    Uuid::new(0, index + 1),
                    format!("Player{index}"),
                    level.next_entity_id(),
                    Vec3::new(0.5, 64., 0.5),
                    if row["creative"] == true {
                        GameType::Creative
                    } else {
                        GameType::Survival
                    },
                );
                state.update_game_mode_abilities();
                state.random.set_seed(42);
                if row["fill"] != "empty" {
                    for slot in &mut state.inventory.items {
                        *slot = ItemStack::new(Item::COBBLESTONE, 64, Default::default());
                    }
                }
                match row["fill"].as_str().unwrap() {
                    "partial" => {
                        state.inventory.items[0] =
                            ItemStack::new(Item::STONE, 63, Default::default())
                    }
                    "one-slot" => state.inventory.items[0] = ItemStack::default(),
                    _ => {}
                }
                level
                    .add_entity(entity, EntityState::Player(Box::new(state)))
                    .unwrap();
                players.push(entity);
            }
            level.take_effects();
            let input = super::super::arguments::items::parse(
                &mut ferraloom_brigadier::StringReader::new(row["item"].as_str().unwrap()),
            )
            .unwrap();
            let result = run(
                &input,
                row["count"].as_i64().unwrap() as i32,
                &players,
                &mut level,
            );
            let mut actual = serde_json::Map::new();
            match result {
                Ok(result) => {
                    actual.insert("value".into(), json!(result.value));
                    actual.insert("feedback".into(),Value::Array(result.feedback.into_iter().map(|f|json!({"message":serde_json::from_str::<Value>(&f.message.to_json().unwrap()).unwrap(),"broadcast":f.broadcast})).collect()));
                }
                Err(GiveError::Syntax(error)) => {
                    actual.insert(
                        "error".into(),
                        serde_json::from_str(&error_component(&error).to_json().unwrap()).unwrap(),
                    );
                    actual.insert("feedback".into(), json!([]));
                }
                Err(error) => panic!("{row}: {error:?}"),
            }
            let inventories: Vec<_> = players
                .iter()
                .map(|&player| {
                    let player = level.entity::<ServerPlayer>(player).unwrap();
                    player
                        .inventory
                        .items
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| !s.is_empty())
                        .map(|(i, s)| json!([i, s.to_json().unwrap()]))
                        .collect::<Vec<_>>()
                })
                .collect();
            actual.insert("inventory".into(), json!(inventories));
            let randoms: Vec<_> = players
                .iter()
                .map(|&p| {
                    level
                        .entity_mut::<EntityData>(p)
                        .unwrap()
                        .random
                        .next_long()
                })
                .collect();
            actual.insert("random".into(), json!(randoms));
            let drops: Vec<_> = level
                .all_entities()
                .into_iter()
                .filter_map(|id| level.entity::<ItemEntity>(id).ok())
                .map(|item| {
                    let mut value = serde_json::Map::from_iter([
                        ("stack".into(), item.item().to_json().unwrap()),
                        ("age".into(), json!(item.age)),
                        ("delay".into(), json!(item.pickup_delay)),
                    ]);
                    if let Some(owner) = item.target {
                        value.insert("target".into(), json!(owner.to_string()));
                    }
                    Value::Object(value)
                })
                .collect();
            actual.insert("drops".into(), json!(drops));
            let sounds: Vec<_> = level
                .take_effects()
                .into_iter()
                .filter_map(|effect| match effect {
                    LevelEffect::PlayerPacket { packet, .. } => match *packet {
                        cb::Packet::Sound(sound) => {
                            Some(json!({"pitch":format!("{:x}",sound.pitch.to_bits())}))
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            actual.insert("sounds".into(), json!(sounds));
            assert_eq!(level.unported_behavior_stats().skipped_calls, 0, "{row}");
            for (key, value) in actual {
                assert_eq!(value, row[&key], "{key}: {row}");
            }
        }
        assert_eq!(rows.len(), 14);
    }
}
