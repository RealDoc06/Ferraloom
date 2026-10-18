//! Canonical FoodData state, persistence and native server food arithmetic.
use super::ServerPlayer;
use crate::{
    entity::{Entity, EntityRef},
    level::{Level, LevelError},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{component::FoodProperties, play::Difficulty};
use ferraloom_util::java;

/// Port of FoodData. Timers and unclamped loaded values are canonical state.
#[derive(Clone, Debug)]
pub struct FoodData {
    food_level: i32,
    saturation_level: f32,
    exhaustion_level: f32,
    tick_timer: i32,
}
impl Default for FoodData {
    fn default() -> Self {
        Self {
            food_level: 20,
            saturation_level: 5.0,
            exhaustion_level: 0.0,
            tick_timer: 0,
        }
    }
}
impl FoodData {
    fn add(&mut self, food: i32, saturation: f32) {
        self.food_level = self.food_level.wrapping_add(food).clamp(0, 20);
        let saturation = self.saturation_level + saturation;
        self.saturation_level = if saturation < 0.0 {
            0.0
        } else {
            java::min_f32(saturation, self.food_level as f32)
        };
    }
    /// Port of FoodData#eat(int,float) and FoodConstants#saturationByModifier.
    pub fn eat(&mut self, food: i32, saturation_modifier: f32) {
        self.add(food, food as f32 * saturation_modifier * 2.0);
    }
    /// Port of FoodData#eat(FoodProperties); saturation is already absolute.
    pub fn eat_properties(&mut self, food: &FoodProperties) {
        self.add(food.nutrition.0, food.saturation.0);
    }
    pub fn food_level(&self) -> i32 {
        self.food_level
    }
    pub fn saturation_level(&self) -> f32 {
        self.saturation_level
    }
    pub fn exhaustion_level(&self) -> f32 {
        self.exhaustion_level
    }
    pub fn tick_timer(&self) -> i32 {
        self.tick_timer
    }
    pub fn has_enough_food(&self) -> bool {
        self.food_level as f32 > 6.0
    }
    pub fn needs_food(&self) -> bool {
        self.food_level < 20
    }
    pub fn set_food_level(&mut self, food: i32) {
        self.food_level = food;
    }
    pub fn set_saturation(&mut self, saturation: f32) {
        self.saturation_level = saturation;
    }
    pub fn add_exhaustion(&mut self, amount: f32) {
        self.exhaustion_level = java::min_f32(self.exhaustion_level + amount, 40.0);
    }
    /// Port of FoodData#readAdditionalSaveData, preserving native numeric reads.
    pub fn load(&mut self, tag: &CompoundTag) {
        self.food_level = tag.get_int_or("foodLevel", 20);
        self.tick_timer = tag.get_int_or("foodTickTimer", 0);
        self.saturation_level = tag.get_float_or("foodSaturationLevel", 5.0);
        self.exhaustion_level = tag.get_float_or("foodExhaustionLevel", 0.0);
    }
    pub fn save(&self, tag: &mut CompoundTag) {
        // ValueOutput#putFloat uses FloatTag.valueOf's positive ZERO singleton.
        let saturation = if self.saturation_level == 0.0 {
            0.0
        } else {
            self.saturation_level
        };
        let exhaustion = if self.exhaustion_level == 0.0 {
            0.0
        } else {
            self.exhaustion_level
        };
        tag.insert("foodLevel", Tag::Int(self.food_level));
        tag.insert("foodTickTimer", Tag::Int(self.tick_timer));
        tag.insert("foodSaturationLevel", Tag::Float(saturation));
        tag.insert("foodExhaustionLevel", Tag::Float(exhaustion));
    }
}

/// Port of LivingEntity#heal, used by the native food and peaceful hooks.
fn heal(player: EntityRef, amount: f32, level: &mut Level) -> Result<(), LevelError> {
    let player = level.entity_mut::<ServerPlayer>(player)?;
    let health = player.health();
    if health > 0.0 {
        player.set_health(health + amount);
    }
    Ok(())
}
fn is_hurt(player: EntityRef, level: &Level) -> Result<bool, LevelError> {
    let player = level.entity::<ServerPlayer>(player)?;
    Ok(player.health() > 0.0 && player.health() < player.max_health())
}

/// Port of FoodData#tick. End each food-state borrow before healing or the
/// virtual starvation damage callback, then resume on the same canonical data.
pub fn tick<E: Entity + ?Sized>(
    behavior: &E,
    player: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let difficulty = level.difficulty();
    {
        let food = &mut level.entity_mut::<ServerPlayer>(player)?.food_data;
        if food.exhaustion_level > 4.0 {
            food.exhaustion_level -= 4.0;
            if food.saturation_level > 0.0 {
                food.saturation_level = java::max_f32(food.saturation_level - 1.0, 0.0);
            } else if difficulty != Difficulty::Peaceful {
                food.food_level = food.food_level.wrapping_sub(1).max(0);
            }
        }
    }
    let natural_regen = level.rules().natural_health_regeneration;
    let food = &level.entity::<ServerPlayer>(player)?.food_data;
    let fast = natural_regen
        && food.saturation_level > 0.0
        && is_hurt(player, level)?
        && food.food_level >= 20;
    if fast {
        let food = &mut level.entity_mut::<ServerPlayer>(player)?.food_data;
        food.tick_timer = food.tick_timer.wrapping_add(1);
        if food.tick_timer >= 10 {
            let spent = java::min_f32(food.saturation_level, 6.0);
            heal(player, spent / 6.0, level)?;
            let food = &mut level.entity_mut::<ServerPlayer>(player)?.food_data;
            food.add_exhaustion(spent);
            food.tick_timer = 0;
        }
    } else {
        let food = &level.entity::<ServerPlayer>(player)?.food_data;
        let slow = natural_regen && food.food_level >= 18 && is_hurt(player, level)?;
        if slow {
            let food = &mut level.entity_mut::<ServerPlayer>(player)?.food_data;
            food.tick_timer = food.tick_timer.wrapping_add(1);
            if food.tick_timer >= 80 {
                heal(player, 1.0, level)?;
                let food = &mut level.entity_mut::<ServerPlayer>(player)?.food_data;
                food.add_exhaustion(6.0);
                food.tick_timer = 0;
            }
        } else if level.entity::<ServerPlayer>(player)?.food_data.food_level <= 0 {
            let food = &mut level.entity_mut::<ServerPlayer>(player)?.food_data;
            food.tick_timer = food.tick_timer.wrapping_add(1);
            if food.tick_timer >= 80 {
                let health = level.entity::<ServerPlayer>(player)?.health();
                if health > 10.0
                    || difficulty == Difficulty::Hard
                    || health > 1.0 && difficulty == Difficulty::Normal
                {
                    let source = level.damage_sources().starve();
                    behavior.hurt_server(player, level, source, 1.0)?;
                }
                level
                    .entity_mut::<ServerPlayer>(player)?
                    .food_data
                    .tick_timer = 0;
            }
        } else {
            level
                .entity_mut::<ServerPlayer>(player)?
                .food_data
                .tick_timer = 0;
        }
    }
    Ok(())
}

/// Port of ServerPlayer#tickRegeneration, before Inventory#tick in aiStep.
pub fn tick_regeneration(player: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    if level.difficulty() == Difficulty::Peaceful && level.rules().natural_health_regeneration {
        let ticks = level.entity::<ServerPlayer>(player)?.tick_count;
        if ticks % 20 == 0 {
            if level.entity::<ServerPlayer>(player)?.health()
                < level.entity::<ServerPlayer>(player)?.max_health()
            {
                heal(player, 1.0, level)?;
            }
            let food = &mut level.entity_mut::<ServerPlayer>(player)?.food_data;
            let saturation = food.saturation_level;
            if saturation < 20.0 {
                food.set_saturation(saturation + 1.0);
            }
        }
        if ticks % 10 == 0 {
            let food = &mut level.entity_mut::<ServerPlayer>(player)?.food_data;
            if food.needs_food() {
                food.set_food_level(food.food_level.wrapping_add(1));
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests;
