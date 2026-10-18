//! Entity impulses and EntitySelector's explicit scoreboard read boundary.
use super::{Entity, EntityData, EntityRef, EntityState, LivingEntity, synched_data};
use crate::level::{Level, LevelError};
pub use crate::scoreboard::TeamRead;
use ferraloom_protocol::play::TeamCollisionRule;
use ferraloom_util::random::RandomSource;
use ferraloom_util::{Vec3, mth};

fn entity_team<T: TeamRead>(
    entity: EntityRef,
    level: &Level,
    teams: &T,
) -> Result<Option<T::Team>, LevelError> {
    let state = level.entity_state(entity)?;
    let name = match state {
        EntityState::Player(player) => Some(player.name.as_str()),
        _ => None,
    };
    let name = crate::scoreboard::scoreboard_name(name, state.as_entity().uuid);
    Ok(teams.team_by_scoreboard_name(&name))
}

/// Port of EntitySelector#pushableBy on the server. Capture the pusher's team
/// and rule once; each target's team/rule is resolved at its predicate call.
pub struct PushableBy<T: TeamRead> {
    teams: T,
    own_team: Option<T::Team>,
    own_rule: TeamCollisionRule,
}
impl<T: TeamRead> PushableBy<T> {
    pub fn new(entity: EntityRef, level: &Level, teams: T) -> Result<Self, LevelError> {
        let own_team = entity_team(entity, level, &teams)?;
        let own_rule =
            own_team.map_or(TeamCollisionRule::Always, |team| teams.collision_rule(team));
        Ok(Self {
            teams,
            own_team,
            own_rule,
        })
    }
    pub fn test(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        if self.own_rule == TeamCollisionRule::Never {
            return Ok(false);
        }
        let behavior = level.entity_behavior(entity)?;
        if behavior.is_spectator(entity, level)? || !behavior.is_pushable(entity, level)? {
            return Ok(false);
        }
        let their_team = entity_team(entity, level, &self.teams)?;
        let their_rule = their_team.map_or(TeamCollisionRule::Always, |team| {
            self.teams.collision_rule(team)
        });
        if their_rule == TeamCollisionRule::Never {
            return Ok(false);
        }
        let allied = self
            .own_team
            .is_some_and(|team| self.teams.is_allied_to(team, their_team));
        if (self.own_rule == TeamCollisionRule::PushOwnTeam
            || their_rule == TeamCollisionRule::PushOwnTeam)
            && allied
        {
            Ok(false)
        } else {
            Ok(self.own_rule != TeamCollisionRule::PushOtherTeams
                && their_rule != TeamCollisionRule::PushOtherTeams
                || allied)
        }
    }
}

/// Port of Entity#push(Entity). Every virtual query/impulse ends its canonical
/// borrow before the next call, preserving first-entity then second-entity order.
pub fn push<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    other: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    if super::collision::same_vehicle(entity, other, level)? {
        return Ok(());
    }
    let base = level.entity::<EntityData>(entity)?;
    let target = level.entity::<EntityData>(other)?;
    if target.no_physics || base.no_physics {
        return Ok(());
    }
    let mut x = target.position.x - base.position.x;
    let mut z = target.position.z - base.position.z;
    let distance = mth::abs_max(x, z);
    if distance >= 0.009_999_999_776_482_582 {
        let distance = distance.sqrt();
        x /= distance;
        z /= distance;
        let mut power = 1.0 / distance;
        if power > 1.0 {
            power = 1.0;
        }
        x *= power;
        z *= power;
        x *= 0.050_000_000_745_058_06;
        z *= 0.050_000_000_745_058_06;
        if level.entity::<EntityData>(entity)?.passengers.is_empty()
            && behavior.is_pushable(entity, level)?
        {
            behavior.push_impulse(entity, Vec3::new(-x, 0.0, -z), level)?;
        }
        let other_behavior = level.entity_behavior(other)?;
        if level.entity::<EntityData>(other)?.passengers.is_empty()
            && other_behavior.is_pushable(other, level)?
        {
            other_behavior.push_impulse(other, Vec3::new(x, 0.0, z), level)?;
        }
    }
    Ok(())
}

/// Port of LivingEntity#push(Entity), delegating to the Entity parent body.
pub fn living_push<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    other: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    if level
        .entity::<LivingEntity>(entity)?
        .entity_data
        .get(synched_data::SLEEPING_POS)
        .is_none()
    {
        push(behavior, entity, other, level)?;
    }
    Ok(())
}

/// Port of Level#getPushableEntities. The pusher's team rule is captured before
/// the section query; target virtual queries retain the native traversal order.
pub fn pushable_entities(entity: EntityRef, level: &Level) -> Result<Vec<EntityRef>, LevelError> {
    let bounds = level.entity::<EntityData>(entity)?.bounding_box;
    let predicate = PushableBy::new(entity, level, level.scoreboard())?;
    let mut pushable = Vec::new();
    for other in level.entities_in(bounds, |id, _| id != entity) {
        if predicate.test(other, level)? {
            pushable.push(other);
        }
    }
    Ok(pushable)
}

/// Port of LivingEntity#pushEntities. Capture the query before damage or push
/// callbacks; every captured target is visited even when an earlier call removes it.
pub fn living_push_entities<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    let pushable = pushable_entities(entity, level)?;
    push_captured_entities(behavior, entity, pushable, level)
}

fn push_captured_entities<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    pushable: Vec<EntityRef>,
    level: &mut Level,
) -> Result<(), LevelError> {
    if pushable.is_empty() {
        return Ok(());
    }
    let max_cramming = level.rules().max_entity_cramming;
    if max_cramming > 0
        && pushable.len() > (max_cramming - 1) as usize
        && level
            .entity_mut::<EntityData>(entity)?
            .random
            .next_int_bound(4)
            == 0
    {
        let mut count = 0;
        for &other in &pushable {
            if level.entity::<EntityData>(other)?.vehicle.get().is_none() {
                count += 1;
            }
        }
        if count > max_cramming - 1 {
            let source = level.damage_sources().cramming();
            behavior.hurt_server(entity, level, source, 6.0)?;
        }
    }
    for other in pushable {
        behavior.do_push(entity, other, level)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
