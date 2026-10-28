//! Mob#serverAiStep's immediate component order. Components own their state;
//! each call below starts a fresh canonical borrow through its owning module.
use crate::{
    entity::{
        EntityData, EntityRef, Mob,
        ai::{self, goal::SelectorKind},
    },
    level::{Level, LevelError},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    Sensing,
    Target(bool),
    Goal(bool),
    Navigation,
    Custom,
    Move,
    Look,
    Jump,
}

/// Port of Mob#serverAiStep. The callback is immediate, not a queue: it models
/// the component/virtual call boundaries and never holds a Mob borrow over one.
fn run_steps(
    entity: EntityRef,
    level: &mut Level,
    mut call: impl FnMut(Step, EntityRef, &mut Level) -> Result<(), LevelError>,
) -> Result<(), LevelError> {
    let living = &mut level.entity_mut::<Mob>(entity)?.parent;
    living.set_no_action_time(living.no_action_time().wrapping_add(1));
    call(Step::Sensing, entity, level)?;
    // Sensing can call back into the world. Native reads both inputs afterward,
    // then captures this cadence once for BOTH target and ordinary selectors.
    let base = level.entity::<EntityData>(entity)?;
    let reduced = base.tick_count > 1 && base.tick_count.wrapping_add(base.network_id) % 2 != 0;
    call(Step::Target(reduced), entity, level)?;
    call(Step::Goal(reduced), entity, level)?;
    for step in [
        Step::Navigation,
        Step::Custom,
        Step::Move,
        Step::Look,
        Step::Jump,
    ] {
        call(step, entity, level)?;
    }
    Ok(())
}

pub fn server_ai_step(entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
    run_steps(entity, level, |step, entity, level| match step {
        Step::Sensing => ai::sensing::tick(entity, level),
        Step::Target(true) => {
            ai::goal::tick_running_goals(entity, SelectorKind::Target, false, level)
        }
        Step::Target(false) => ai::goal::tick_selector(entity, SelectorKind::Target, level),
        Step::Goal(true) => ai::goal::tick_running_goals(entity, SelectorKind::Goal, false, level),
        Step::Goal(false) => ai::goal::tick_selector(entity, SelectorKind::Goal, level),
        Step::Navigation => ai::navigation::tick(entity, level),
        Step::Custom => level
            .entity_behavior(entity)?
            .custom_server_ai_step(entity, level),
        Step::Move => ai::control::tick_move(entity, level),
        Step::Look => ai::control::tick_look(entity, level),
        Step::Jump => ai::control::tick_jump(entity, level),
    })
}

#[cfg(test)]
mod tests;
