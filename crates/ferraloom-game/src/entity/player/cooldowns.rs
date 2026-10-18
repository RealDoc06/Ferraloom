//! Port of ItemCooldowns and ServerItemCooldowns. Cooldowns are never persisted.
use super::{ServerPlayer, inventory};
use crate::{
    entity::EntityRef,
    level::{Level, LevelError, effects::LevelEffect},
};
use ferraloom_data::{Item, registries::data_component_type::Id as ComponentType};
use ferraloom_protocol::{
    Identifier,
    codec::VarInt,
    component::{DataComponent, ItemStack},
    play::clientbound as cb,
};
use ferraloom_util::{
    collections::{JavaHash, JavaHashMap},
    mth,
};
use std::{cmp::Ordering, ops::Deref};

#[derive(Clone, Debug, Eq, PartialEq)]
struct Group(Identifier);
impl JavaHash for Group {
    fn java_hash_code(&self) -> i32 {
        self.0
            .namespace()
            .java_hash_code()
            .wrapping_mul(31)
            .wrapping_add(self.0.path().java_hash_code())
    }
    fn java_compare(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.0
                .path()
                .cmp(other.0.path())
                .then_with(|| self.0.namespace().cmp(other.0.namespace())),
        )
    }
}
#[derive(Clone, Copy, Debug)]
struct CooldownInstance {
    start_time: i32,
    end_time: i32,
}
#[derive(Debug, Default)]
pub struct ItemCooldowns {
    cooldowns: JavaHashMap<Group, CooldownInstance>,
    tick_count: i32,
}
impl ItemCooldowns {
    /// Port of ItemCooldowns#getCooldownGroup, including component patches.
    pub fn get_cooldown_group(stack: &ItemStack) -> Identifier {
        if stack.is_empty() {
            return Identifier::parse(Item::AIR.name()).expect("registered air identifier");
        }
        let item = stack.0.as_ref().unwrap().item;
        let default = || Identifier::parse(item.name()).expect("registered item identifier");
        match inventory::component(stack, ComponentType::USE_COOLDOWN) {
            Some(DataComponent::UseCooldown(value)) => {
                value.cooldown_group.clone().unwrap_or_else(default)
            }
            _ => default(),
        }
    }
    pub fn is_on_cooldown(&self, stack: &ItemStack) -> bool {
        self.get_cooldown_percent(stack, 0.0) > 0.0
    }
    pub fn get_cooldown_percent(&self, stack: &ItemStack, partial_tick: f32) -> f32 {
        let group = Group(Self::get_cooldown_group(stack));
        let Some(cooldown) = self.cooldowns.peek(&group) else {
            return 0.0;
        };
        let duration = cooldown.end_time.wrapping_sub(cooldown.start_time) as f32;
        let remaining = cooldown.end_time as f32 - (self.tick_count as f32 + partial_tick);
        mth::clamp_f32(remaining / duration, 0.0, 1.0)
    }
    /// Expirations retain HashMap entry-iterator order, including iterator removal.
    pub fn tick(&mut self) -> Vec<Identifier> {
        self.tick_count = self.tick_count.wrapping_add(1);
        let mut ended = Vec::new();
        let mut cursor = self.cooldowns.cursor();
        while let Some((_, cooldown)) = cursor.next_entry() {
            if cooldown.end_time <= self.tick_count {
                ended.push(cursor.remove().unwrap().0.0);
            }
        }
        ended
    }
    pub fn add_cooldown(&mut self, stack: &ItemStack, duration: i32) {
        self.add_cooldown_group(Self::get_cooldown_group(stack), duration);
    }
    pub fn add_cooldown_group(&mut self, group: Identifier, duration: i32) {
        self.cooldowns.insert(
            Group(group),
            CooldownInstance {
                start_time: self.tick_count,
                end_time: self.tick_count.wrapping_add(duration),
            },
        );
    }
    pub fn remove_cooldown(&mut self, group: &Identifier) {
        self.cooldowns.remove(&Group(group.clone()));
    }
}
/// One embedded ItemCooldowns parent. Mutations use handle operations below so
/// every ServerItemCooldowns callback enqueues its real packet synchronously.
#[derive(Debug, Default)]
pub struct ServerItemCooldowns {
    parent: ItemCooldowns,
}
impl Deref for ServerItemCooldowns {
    type Target = ItemCooldowns;
    fn deref(&self) -> &ItemCooldowns {
        &self.parent
    }
}
fn send(player: EntityRef, group: Identifier, duration: i32, level: &mut Level) {
    level.region_mut().effects.push(LevelEffect::PlayerPacket {
        recipients: vec![player],
        packet: Box::new(cb::Packet::Cooldown(cb::Cooldown {
            cooldown_group: group,
            duration: VarInt(duration),
        })),
    });
}
/// Port of ServerItemCooldowns#tick, after the ordinary Player tick body.
pub fn tick(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let ended = level
        .entity_mut::<ServerPlayer>(player)?
        .cooldowns
        .parent
        .tick();
    for group in ended {
        send(player, group, 0, level);
    }
    Ok(())
}
pub fn add_cooldown(
    player: EntityRef,
    stack: &ItemStack,
    duration: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    add_cooldown_group(
        player,
        ItemCooldowns::get_cooldown_group(stack),
        duration,
        level,
    )
}
pub fn add_cooldown_group(
    player: EntityRef,
    group: Identifier,
    duration: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    level
        .entity_mut::<ServerPlayer>(player)?
        .cooldowns
        .parent
        .add_cooldown_group(group.clone(), duration);
    send(player, group, duration, level);
    Ok(())
}
pub fn remove_cooldown(
    player: EntityRef,
    group: Identifier,
    level: &mut Level,
) -> Result<(), LevelError> {
    level
        .entity_mut::<ServerPlayer>(player)?
        .cooldowns
        .parent
        .remove_cooldown(&group);
    send(player, group, 0, level);
    Ok(())
}

#[cfg(test)]
mod tests;
