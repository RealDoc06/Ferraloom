//! CombatTracker's canonical history and synchronous enter/leave transitions.
use super::DamageSource;
use crate::{
    entity::{Entity, EntityRef, LivingEntity},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, tags::BlockTags};

/// Port of FallLocation's finite set of native location identifiers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FallLocation {
    Generic,
    Ladder,
    Vines,
    WeepingVines,
    TwistingVines,
    Scaffolding,
    OtherClimbable,
    Water,
}
impl FallLocation {
    pub fn id(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Ladder => "ladder",
            Self::Vines => "vines",
            Self::WeepingVines => "weeping_vines",
            Self::TwistingVines => "twisting_vines",
            Self::Scaffolding => "scaffolding",
            Self::OtherClimbable => "other_climbable",
            Self::Water => "water",
        }
    }
    /// Port of FallLocation#getCurrentFallLocation. Retained climbing state wins
    /// over water and requires the current block and tag read at that position.
    pub fn current(entity: EntityRef, level: &Level) -> Result<Option<Self>, LevelError> {
        let living = level.entity::<LivingEntity>(entity)?;
        let Some(pos) = living.last_climbable_pos.get() else {
            return Ok(living.was_touching_water.then_some(Self::Water));
        };
        let block = level.get_block_state(pos)?.block();
        Ok(Some(
            if block == Block::LADDER
                || level
                    .block_tags()?
                    .contains_key(BlockTags::TRAPDOORS, block)
            {
                Self::Ladder
            } else if block == Block::VINE {
                Self::Vines
            } else if block == Block::WEEPING_VINES || block == Block::WEEPING_VINES_PLANT {
                Self::WeepingVines
            } else if block == Block::TWISTING_VINES || block == Block::TWISTING_VINES_PLANT {
                Self::TwistingVines
            } else if block == Block::SCAFFOLDING {
                Self::Scaffolding
            } else {
                Self::OtherClimbable
            },
        ))
    }
}
/// Port of CombatEntry. Sources retain their canonical direct/causing objects.
#[derive(Clone, Debug)]
pub struct CombatEntry {
    pub source: DamageSource,
    pub damage: f32,
    pub fall_location: Option<FallLocation>,
    pub fall_distance: f32,
}
/// Port of CombatTracker. The embedding LivingEntity supplies the receiver
/// handle; this state never stores another copy or pointer to its owner.
#[derive(Debug, Default)]
pub struct CombatTracker {
    entries: Vec<CombatEntry>,
    last_damage_time: i32,
    combat_start_time: i32,
    combat_end_time: i32,
    in_combat: bool,
    taking_damage: bool,
}
impl CombatTracker {
    pub fn entries(&self) -> &[CombatEntry] {
        &self.entries
    }
    pub fn is_in_combat(&self) -> bool {
        self.in_combat
    }
    pub fn is_taking_damage(&self) -> bool {
        self.taking_damage
    }
    /// Port of CombatTracker#getCombatDuration, including Java int wrapping.
    pub fn combat_duration(&self, tick: i32) -> i32 {
        (if self.in_combat {
            tick
        } else {
            self.combat_end_time
        })
        .wrapping_sub(self.combat_start_time)
    }
    pub(crate) fn trace_entity_refs(&self, references: &mut Vec<EntityRef>) {
        for entry in &self.entries {
            entry.source.trace_entity_refs(references);
        }
    }
}
/// Port of CombatTracker#recheckStatus. Leave callbacks see the changed flags
/// and the original entries; entries appended by that callback are cleared too.
pub fn recheck_status<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let living = level.entity::<LivingEntity>(entity)?;
    if !living.combat_tracker.taking_damage {
        return Ok(());
    }
    let timeout = if living.combat_tracker.in_combat {
        300
    } else {
        100
    };
    if behavior.is_alive(entity, level)?
        && living
            .tick_count
            .wrapping_sub(living.combat_tracker.last_damage_time)
            <= timeout
    {
        return Ok(());
    }
    let living = level.entity_mut::<LivingEntity>(entity)?;
    let was_in_combat = living.combat_tracker.in_combat;
    living.combat_tracker.taking_damage = false;
    living.combat_tracker.in_combat = false;
    living.combat_tracker.combat_end_time = living.tick_count;
    if was_in_combat {
        behavior.on_leave_combat(entity, level)?;
    }
    level
        .entity_mut::<LivingEntity>(entity)?
        .combat_tracker
        .entries
        .clear();
    Ok(())
}
/// Port of CombatTracker#recordDamage. Every state borrow ends before a world
/// query or callback; entry insertion and flags precede onEnterCombat.
pub fn record_damage<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    source: DamageSource,
    damage: f32,
    level: &mut Level,
) -> Result<(), LevelError> {
    recheck_status(behavior, entity, level)?;
    let location = FallLocation::current(entity, level)?;
    let living = level.entity_mut::<LivingEntity>(entity)?;
    living.combat_tracker.entries.push(CombatEntry {
        source: source.clone(),
        damage,
        fall_location: location,
        fall_distance: living.fall_distance as f32,
    });
    living.combat_tracker.last_damage_time = living.tick_count;
    living.combat_tracker.taking_damage = true;
    if !living.combat_tracker.in_combat && behavior.is_alive(entity, level)? {
        let enter = source
            .causing_entity()
            .map(|id| {
                level
                    .entity_state(id)
                    .map(|state| state.as_living().is_some())
            })
            .transpose()?
            .unwrap_or(false);
        if enter {
            let living = level.entity_mut::<LivingEntity>(entity)?;
            living.combat_tracker.in_combat = true;
            living.combat_tracker.combat_start_time = living.tick_count;
            living.combat_tracker.combat_end_time = living.tick_count;
            behavior.on_enter_combat(entity, level)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
