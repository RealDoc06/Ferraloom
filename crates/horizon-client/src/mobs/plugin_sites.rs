//! Ferraloom: Bukkit plugin events fired from inside mob code
//! (docs/ferraloom/plugins.md, "Parallel sections").
//!
//! A site here runs only behind `ferraloom_plugins::listens`, so without a
//! listener nothing in this module runs. With one, the rule of the parallel
//! mob pass holds: in a speculative tick a site calls
//! [`tick_loop::note_impure`] and goes on as if no plugin listened (the tick
//! is thrown away and the mob ticks again in its serial turn); in a serial
//! turn, or outside the pass, it calls the plugin through the game's
//! [`Sites`]. Those calls get a restricted host: it reads the mob in hand
//! and what the pass holds, and the changes plugins make to other entities
//! wait in [`queue`], in mob order, for the game to apply after the pass.
//!
//! The lib cannot see the game, so the game installs [`Sites`] when plugins
//! start; without plugins they are never set.
use super::{Damage, Mob, MobEvent, MobWorld, Seen, Who, finalize, species, tick_loop};
use ferraloom_plugins::{
    Actor, Change, EntityRef, EntityState, Event, Location, Named, entity_flags,
};
use horizon_core::random::LegacyRandom;
use std::{cell::RefCell, sync::OnceLock};

/// The game's side of the sites.
pub struct Sites {
    /// `EntityDamageEvent` for mob `m` taking `amount` (after armor; `base`
    /// before it) from `source`: the damage to deal, none when cancelled.
    pub damage: fn(m: &mut Mob, source: &Damage, base: f32, amount: f32) -> Option<f32>,
    /// `EntityTargetEvent` for mob `m`, whose tick changed its target from
    /// `was`, in its serial turn of the pass (`w`): the target it keeps.
    pub target: fn(m: &mut Mob, was: Option<Who>, w: &MobWorld<'_>) -> Option<Who>,
    /// `EntityCombustEvent` for mob `m`, whose tick set it burning for
    /// `ticks`: the ticks it burns, none when cancelled.
    pub combust: fn(m: &mut Mob, ticks: i32, w: &MobWorld<'_>) -> Option<i32>,
    /// `EntityChangeBlockEvent` for mob `m` changing the block at `pos` to
    /// state `to` (`minecraft:air`): true when cancelled.
    pub change_block: fn(m: &mut Mob, pos: [i32; 3], to: &str, w: &MobWorld<'_>) -> bool,
}

static SITES: OnceLock<Sites> = OnceLock::new();

/// Called by the game once plugins run.
pub fn install(sites: Sites) {
    let _ = SITES.set(sites);
}

/// `LivingEntity.actuallyHurt`'s `EntityDamageEvent` site (`Mob::damage_with`).
pub fn damage(m: &mut Mob, source: &Damage, base: f32, amount: f32) -> Option<f32> {
    if tick_loop::note_impure() {
        return Some(amount);
    }
    match SITES.get() {
        Some(sites) => (sites.damage)(m, source, base, amount),
        None => Some(amount),
    }
}

/// The events [`tick_watched`] watches a tick for.
pub const WATCHED: u64 = Event::mask(&[Event::EntityTarget, Event::EntityCombust]);

/// Mob `m`'s tick in the pass while a plugin listens to one of [`WATCHED`]
/// (`tick_loop`), its events at the end of the tick that set what they
/// guard, as these are fields set in many places: `Mob.setTarget`'s
/// `EntityTargetEvent` for a changed target (a cancel puts the old one
/// back), `igniteForTicks`'s `EntityCombustEvent` for fire that grew (a
/// cancel leaves the fire as it would have burned down). A speculative tick
/// that set one ticks again in turn.
pub fn tick_watched(
    m: &mut Mob,
    w: &MobWorld,
    random: &mut LegacyRandom,
    events: &mut Vec<MobEvent>,
) -> bool {
    let (target, fire) = (m.ai.target, m.fire_ticks);
    let kept = m.tick(w, random, events);
    let targeted = m.ai.target != target && ferraloom_plugins::listens(Event::EntityTarget);
    let ignited = m.fire_ticks > fire && ferraloom_plugins::listens(Event::EntityCombust);
    if !(targeted || ignited) || tick_loop::note_impure() {
        return kept;
    }
    let Some(sites) = SITES.get() else {
        return kept;
    };
    if ignited {
        let ticks = m.fire_ticks;
        m.fire_ticks = match (sites.combust)(m, ticks, w) {
            Some(ticks) => ticks,
            None => (fire - 1).max(fire.min(0)),
        };
    }
    if targeted {
        m.ai.target = (sites.target)(m, target, w);
    }
    kept
}

/// `EntityChangeBlockEvent` for the blocks mob `m`'s tick changes: its
/// events from `from` on (`tick_loop`'s commit of the mob, in mob order, so
/// the speculative and serial passes ask alike and no tick runs again for
/// it). A cancelled change's event goes; an enderman keeps what it held.
/// The mob's other state stays as its tick left it (a sheep that ate the
/// grass it was refused is still fed).
pub fn change_blocks(m: &mut Mob, events: &mut Vec<MobEvent>, from: usize, w: &MobWorld) {
    let Some(sites) = SITES.get() else {
        return;
    };
    let name = |state| w.registry.state(state).map(|s| s.name.clone());
    let mut i = from;
    while i < events.len() {
        let change = match &events[i] {
            MobEvent::SetBlock { pos, state } => name(*state).map(|n| (*pos, n)),
            MobEvent::SetBlockName { pos, name, .. } => Some((*pos, (*name).to_owned())),
            MobEvent::BreakBlock { pos, .. } | MobEvent::BreakDoor { pos } => {
                Some((*pos, "minecraft:air".to_owned()))
            }
            _ => None,
        };
        let Some((pos, to)) = change else {
            i += 1;
            continue;
        };
        if !(sites.change_block)(m, pos, &to, w) {
            i += 1;
            continue;
        }
        if let MobEvent::SetBlock { state, .. } = events.remove(i)
            && m.species() == super::Species::Enderman
        {
            // EndermanTakeBlockGoal and LeaveBlockGoal change the carried block only once allowed.
            m.extra.carried = if to == "minecraft:air" {
                None
            } else {
                Some(state)
            };
        }
    }
}

thread_local! {
    /// Changes plugins made to entities other than the one in hand, in the order made.
    static QUEUE: RefCell<Vec<(EntityRef, Change)>> = const { RefCell::new(Vec::new()) };
}

/// A change to another entity than the one in hand, for after the pass.
pub fn queue(e: EntityRef, change: Change) {
    QUEUE.with_borrow_mut(|q| q.push((e, change)));
}

/// The queued changes, in the order made.
pub fn take_queued() -> Vec<(EntityRef, Change)> {
    QUEUE.take()
}

/// How many changes are queued (`tick_loop`'s check mode puts its serial run's back).
pub(crate) fn queued() -> usize {
    QUEUE.with_borrow(Vec::len)
}

pub(crate) fn unqueue_after(n: usize) {
    QUEUE.with_borrow_mut(|q| q.truncate(n));
}

/// The type key a mob's handle carries (`zombie`).
pub fn kind(m: &Mob) -> &'static str {
    species::def(m.species()).name
}

/// A mob as the Bukkit API names it.
pub fn named(m: &Mob) -> Named {
    Named::entity(m.id, kind(m))
}

/// Whether `e` names mob `m`.
pub fn is(e: EntityRef, m: &Mob) -> bool {
    e.is(m.id, kind(m))
}

/// A mob as the bridge reads it, in world `world`; its target and killer as
/// `who` names them (the player a mob sees is the game's to say).
pub fn state(m: &Mob, world: usize, who: impl Fn(Who) -> Option<Named>) -> EntityState {
    use entity_flags as f;
    let mut s = EntityState::at(Location {
        world,
        x: m.position.x,
        y: m.position.y,
        z: m.position.z,
        yaw: m.yaw,
        pitch: m.head_pitch,
    });
    s.velocity = [m.velocity.x, m.velocity.y, m.velocity.z];
    let bits = [
        (m.on_ground, f::ON_GROUND),
        (m.name_visible, f::NAME_VISIBLE),
        (m.silent, f::SILENT),
        (m.glowing, f::GLOWING),
        (m.invulnerable_flag, f::INVULNERABLE),
        (!m.no_gravity, f::GRAVITY),
        (true, f::LIVING),
        (!m.alive() || m.removed, f::DEAD),
        (!m.no_ai, f::AI),
        (m.persistent, f::PERSISTENT),
        (m.extra.tame, f::TAMED),
        (m.is_baby(), f::BABY),
        (m.invisible, f::INVISIBLE),
        (m.extra.charged, f::POWERED),
    ];
    s.flags = bits
        .iter()
        .filter(|(on, _)| *on)
        .fold(0, |a, (_, bit)| a | bit);
    s.fire_ticks = m.fire_ticks;
    s.ticks_lived = m.age as i32;
    s.fall_distance = m.fall_distance;
    s.health = f64::from(m.health);
    s.max_health = f64::from(finalize::max_health(m));
    s.no_damage_ticks = m.invulnerable;
    s.custom_name = m.custom_name.clone();
    s.target = m.ai.target.and_then(&who);
    s.owner = m.extra.owner.map(uuid);
    s
}

/// What the pass's snapshot holds of another mob: less than the mob itself
/// (no name, no flags but the living ones).
pub fn seen_state(s: &Seen, world: usize, who: impl Fn(Who) -> Option<Named>) -> EntityState {
    use entity_flags as f;
    let mut state = EntityState::at(Location {
        world,
        x: s.position.x,
        y: s.position.y,
        z: s.position.z,
        yaw: s.yaw,
        pitch: 0.0,
    });
    state.velocity = [s.velocity.x, s.velocity.y, s.velocity.z];
    state.flags = f::LIVING | f::AI | f::GRAVITY;
    if !s.alive {
        state.flags |= f::DEAD;
    }
    if s.tame {
        state.flags |= f::TAMED;
    }
    if s.baby {
        state.flags |= f::BABY;
    }
    state.health = f64::from(s.health);
    state.target = s.target.and_then(who);
    state.owner = s.owner.map(uuid);
    state
}

/// A UUID as vanilla's int array holds it.
pub fn uuid(ints: [i32; 4]) -> u128 {
    ints.iter()
        .fold(0_u128, |a, &i| (a << 32) | u128::from(i as u32))
}

/// Makes a plugin's change to mob `m`; false for one this mob cannot take
/// (a teleport to another world: the game's to do).
pub fn apply(m: &mut Mob, world: usize, change: Change) -> bool {
    match change {
        Change::Velocity([x, y, z]) => {
            m.velocity = horizon_core::geometry::Vec3::new(x, y, z);
            m.needs_sync = true;
        }
        Change::Teleport(to) => {
            if to.world != world {
                return false;
            }
            m.position = horizon_core::geometry::Vec3::new(to.x, to.y, to.z);
            m.yaw = to.yaw;
            m.head_pitch = to.pitch;
            m.needs_sync = true;
        }
        Change::Remove => m.removed = true,
        Change::CustomName(name) => {
            m.custom_name = name;
            m.custom_name_text = None;
        }
        Change::NameVisible(on) => m.name_visible = on,
        Change::FireTicks(t) => m.fire_ticks = t,
        Change::Health(h) => m.health = (h as f32).clamp(0.0, finalize::max_health(m)),
        Change::Target(t) => {
            m.ai.target = t.map(|t| match t {
                Actor::Player(_) => Who::Player,
                Actor::Entity(e) => Who::Mob(e.id),
            })
        }
        Change::Ai(on) => m.no_ai = !on,
        Change::Persistent(on) => m.persistent = on,
        Change::Silent(on) => m.silent = on,
        Change::Glowing(on) => m.glowing = on,
        Change::Gravity(on) => m.no_gravity = !on,
        Change::Invulnerable(on) => m.invulnerable_flag = on,
        Change::Tamed(on) => m.extra.tame = on,
        Change::Baby(on) => finalize::set_baby(m, on),
        Change::Powered(on) => m.extra.charged = on,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_read_as_vanilla_writes_them() {
        let ints = [0x0123_4567, 0x89ab_cdef_u32 as i32, -1, 1];
        assert_eq!(uuid(ints), 0x0123_4567_89ab_cdef_ffff_ffff_0000_0001);
    }
}
