//! Horses, donkeys, mules and the undead horses (`AbstractHorse`): random
//! stats, coat colours and markings, eating, rearing, taming by riding,
//! the ridden movement and jump, offspring stats and `AbstractEquineModel`.
use super::{
    Mob, MobEvent, MobWorld, Species,
    goals::{
        Breed, Cx, Float, FollowParent, Goal, LookAtPlayer, MOVE, Panic, RandomLookAround, Stroll,
        Tempt, goal_clone,
    },
};
use horizon_core::geometry::trig::Mth;
use horizon_core::random::RandomSource;
use std::f32::consts::PI;

// AnimState slots.
pub const EAT: usize = 0;
pub const STAND: usize = 1;
pub const MOUTH: usize = 2;
/// `AbstractHorse.tailCounter`: client-side, it only drives the model.
pub const TAIL: usize = 3;

/// What a rider asks of its mount this tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rider {
    pub strafe: f32,
    pub forward: f32,
    pub yaw: f32,
    pub pitch: f32,
    /// `playerJumpPendingScale` from the jump bar.
    pub jump: f32,
    /// Holding the mount's steering stick (pigs, striders).
    pub steer: bool,
    /// Sprinting (camels run faster).
    pub sprint: bool,
    /// Holding the jump key (a happy ghast rises).
    pub jumping: bool,
}

pub fn is_equine(s: Species) -> bool {
    matches!(
        s,
        Species::Horse
            | Species::Donkey
            | Species::Mule
            | Species::SkeletonHorse
            | Species::ZombieHorse
            | Species::Camel
            | Species::CamelHusk
            | Species::Llama
            | Species::TraderLlama
    )
}
/// `AbstractHorse.getMaxTemper`: 100, 30 for llamas.
pub fn max_temper(s: Species) -> i32 {
    if super::llama::is_llama(s) {
        super::llama::MAX_TEMPER
    } else {
        100
    }
}
/// `canPerformRearing`: not donkeys or mules.
fn can_rear(s: Species) -> bool {
    matches!(
        s,
        Species::Horse | Species::SkeletonHorse | Species::ZombieHorse
    )
}
pub fn register(m: &mut Mob) {
    if super::camel::is_camel(m.species()) {
        return super::camel::register(m);
    }
    if super::llama::is_llama(m.species()) {
        return super::llama::register(m);
    }
    let tempt = m.kind.tempt.clone();
    let species = m.species();
    let rears = can_rear(species);
    let stand_interval = m.kind.ambient_interval;
    let g = &mut m.ai.goals;
    g.add(1, RunAroundLikeCrazy);
    g.add(2, Breed::new(1.0));
    g.add(4, FollowParent::new(1.0));
    g.add(6, Stroll::avoiding_water(0.7));
    g.add(7, LookAtPlayer::new(6.0));
    g.add(8, RandomLookAround::default());
    if rears {
        // resetStandInterval: -getAmbientStandInterval (400 for every horse).
        g.add(
            9,
            RandomStand {
                next: -stand_interval,
            },
        );
    }
    // addBehaviourGoals, after the rest: none for the skeleton horse (it
    // sinks), no mount panic for the zombie horse.
    if species != Species::SkeletonHorse {
        g.add(0, Float);
        if species != Species::ZombieHorse {
            g.add(1, MountPanic(Panic::new(1.2)));
        }
        g.add(3, Tempt::new(1.25, tempt, false));
    }
}
/// `generateMaxHealth`, `generateSpeed`, `generateJumpStrength` and the coat.
pub fn finalize(m: &mut Mob) {
    let r = &mut m.random;
    let health =
        15.0 + r.next_int_bound(8).unwrap_or(0) as f32 + r.next_int_bound(9).unwrap_or(0) as f32;
    let speed =
        (0.45 + r.next_double() * 0.3 + r.next_double() * 0.3 + r.next_double() * 0.3) * 0.25;
    let jump = 0.4 + r.next_double() * 0.2 + r.next_double() * 0.2 + r.next_double() * 0.2;
    match m.species() {
        Species::Horse => {
            m.attrs.max_health = health;
            m.attrs.speed = speed;
            m.extra.vector[0] = jump;
            m.extra.variant = m.random.next_int_bound(7).unwrap_or(0) as u8;
            m.extra.variant2 = m.random.next_int_bound(5).unwrap_or(0) as u8;
        }
        Species::Donkey | Species::Mule => {
            // AbstractChestedHorse.randomizeAttributes: fixed speed, generated health.
            m.attrs.max_health = health;
            m.extra.vector[0] = jump;
        }
        _ => m.extra.vector[0] = jump,
    }
    m.health = m.attrs.max_health;
}
/// `AbstractHorse.createOffspringAttribute`.
pub fn offspring(a: f64, b: f64, min: f64, max: f64, r: &mut impl RandomSource) -> f64 {
    let (a, b) = (a.clamp(min, max), b.clamp(min, max));
    let margin = 0.15 * (max - min);
    let range = (a - b).abs() + margin * 2.0;
    let quality = (r.next_double() + r.next_double() + r.next_double()) / 3.0 - 0.5;
    let v = (a + b) / 2.0 + range * quality;
    if v > max {
        max - (v - max)
    } else if v < min {
        min + (min - v)
    } else {
        v
    }
}
/// `AbstractHorse.setOffspringAttributes` from two parents.
pub fn inherit(child: &mut Mob, a: &Mob, b: &Mob, r: &mut impl RandomSource) {
    child.attrs.max_health = offspring(
        f64::from(a.attrs.max_health),
        f64::from(b.attrs.max_health),
        15.0,
        30.0,
        r,
    ) as f32;
    child.health = child.attrs.max_health;
    child.extra.vector[0] = offspring(a.extra.vector[0], b.extra.vector[0], 0.4, 1.0, r);
    child.attrs.speed = offspring(a.attrs.speed, b.attrs.speed, 0.1125, 0.3375, r);
}
/// The rider controls the mount: saddled, with a player on it.
pub fn controlled(m: &Mob) -> bool {
    let Some(r) = m.rider else {
        return false;
    };
    match m.species() {
        // Pig/Strider.getControllingPassenger: saddled, the rider holding the stick.
        Species::Pig | Species::Strider => m.extra.saddled && r.steer,
        Species::HappyGhast => super::happy_ghast::controlled(m),
        Species::Nautilus | Species::ZombieNautilus => super::nautilus::controlled(m),
        _ => m.extra.saddled || m.species() == Species::SkeletonHorse && m.extra.tame,
    }
}
/// `ItemBasedSteering.boost`: 140 to 980 ticks of extra speed.
pub fn boost(m: &mut Mob) -> bool {
    if m.extra.counter2 > 0 {
        return false;
    }
    m.extra.counter2 = m.random.next_int_bound(841).unwrap_or(0) + 140;
    m.extra.counter = 0;
    true
}
/// `ItemBasedSteering.tickBoost` and `boostFactor`.
fn boost_factor(m: &mut Mob) -> f32 {
    if m.extra.counter2 <= 0 {
        return 1.0;
    }
    m.extra.counter += 1;
    if m.extra.counter > m.extra.counter2 {
        m.extra.counter2 = 0;
        return 1.0;
    }
    1.0 + 1.15 * (m.extra.counter as f32 / m.extra.counter2 as f32 * PI).msin()
}
/// `AbstractHorse.MountPanicGoal`: only without a rider.
#[derive(Clone, Debug)]
struct MountPanic(Panic);
impl Goal for MountPanic {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.rider.is_none() && self.0.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.rider.is_none() && self.0.can_continue(m, cx)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx)
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx)
    }
}
/// `RunAroundLikeCrazyGoal`: an untamed horse with a rider bolts about until
/// it is tamed (temper) or throws the rider off.
#[derive(Clone, Debug)]
pub(super) struct RunAroundLikeCrazy;
impl Goal for RunAroundLikeCrazy {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if m.extra.tame || m.rider.is_none() {
            return false;
        }
        let r = &mut m.random;
        let (dx, dz) = (
            r.next_int_bound(11).unwrap_or(5) - 5,
            r.next_int_bound(11).unwrap_or(5) - 5,
        );
        let dy = r.next_int_bound(9).unwrap_or(4) - 4;
        let t = [
            m.position.x + f64::from(dx),
            m.position.y + f64::from(dy),
            m.position.z + f64::from(dz),
        ];
        m.nav_move_to(cx.w, t, 1.2)
    }
    fn can_continue(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        !m.extra.tame && !m.ai.nav.is_done() && m.rider.is_some()
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if m.extra.tame || m.random.next_int_bound(50).unwrap_or(1) != 0 {
            return;
        }
        let temper = i32::from(m.extra.variant3);
        let max = max_temper(m.species());
        if m.random.next_int_bound(max).unwrap_or(0) < temper {
            // tameWithName
            m.extra.tame = true;
            cx.events.push(MobEvent::Tamed { mob: m.id });
            return;
        }
        m.extra.variant3 = (temper + 5).min(max) as u8;
        cx.events.push(MobEvent::Buck { mob: m.id });
        make_mad(m, cx.events);
    }
}
/// `AbstractHorse.makeMad`: rear with the angry sound.
pub fn make_mad(m: &mut Mob, events: &mut Vec<MobEvent>) {
    if m.extra.counter > 0 {
        return;
    }
    if can_rear(m.species()) {
        m.extra.counter = 20;
        m.extra.flag = false;
    }
    let sound = match m.species() {
        Species::Donkey => "entity.donkey.angry",
        Species::Mule => "entity.mule.angry",
        Species::Llama | Species::TraderLlama => "entity.llama.angry",
        _ => "entity.horse.angry",
    };
    events.push(MobEvent::Sound(sound, m.center(), 1.0, 1.0));
}
/// `RandomStandGoal`.
#[derive(Clone, Debug)]
struct RandomStand {
    next: i32,
}
impl Goal for RandomStand {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
    fn can_use(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        self.next += 1;
        if self.next > 0 && m.random.next_int_bound(1000).unwrap_or(1000) < self.next {
            self.next = -m.kind.ambient_interval;
            return m.random.next_int_bound(10).unwrap_or(1) == 0;
        }
        false
    }
    fn can_continue(&mut self, _: &mut Mob, _: &mut Cx) -> bool {
        false
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        m.extra.counter = 20;
        m.extra.flag = false;
        if m.species() == Species::Horse {
            cx.events
                .push(MobEvent::Sound("entity.horse.angry", m.center(), 1.0, 1.0));
        }
    }
}
/// The mount takes its rider's input (`getRiddenInput`, `tickRidden`).
pub fn ridden_step(m: &mut Mob) {
    let Some(r) = m.rider else {
        return;
    };
    // AbstractNautilus.tickRidden turns half way toward the rider.
    if super::nautilus::is_nautilus(m.species()) {
        return super::nautilus::ridden(m);
    }
    if m.species() == Species::HappyGhast {
        return super::happy_ghast::ridden(m);
    }
    m.yaw = r.yaw;
    m.head_pitch = r.pitch * 0.5;
    m.body_yaw = r.yaw;
    m.head_yaw = r.yaw;
    m.ai.nav.stop();

    if matches!(m.species(), Species::Pig | Species::Strider) {
        // Pig/Strider: always forward along the rider's look, at a fraction
        // of the movement speed, boosted by the stick.
        m.head_pitch = r.pitch * 0.5;
        // Strider.getRiddenSpeed: slower while it shivers out of the warmth.
        let factor = if m.species() == Species::Pig {
            0.225
        } else if super::strider::suffocating(m) {
            0.35
        } else {
            0.55
        };
        let boost = boost_factor(m);
        // travelRidden: setSpeed(getRiddenSpeed), then travel(getRiddenInput);
        // the input is set after the speed (Mob.setSpeed also sets zza).
        // getAttributeValue(MOVEMENT_SPEED): with the cold strider's modifier.
        m.set_speed((m.movement_speed() * factor) as f32 * boost);
        m.ai.xxa = 0.0;
        m.ai.zza = 1.0;
        return;
    }
    if super::camel::is_camel(m.species()) {
        // Camel.tickRidden / getRiddenInput: forward stands it up; no input
        // while sitting; the jump is a dash.
        if r.forward > 0.0 && super::camel::sitting(m) {
            let mut ev = Vec::new();
            super::camel::stand_up(m, &mut ev);
        }
        if super::camel::refuse_to_move(m) {
            m.ai.xxa = 0.0;
            m.ai.zza = 0.0;
            return;
        }
        let forward = if r.forward <= 0.0 {
            r.forward * 0.25
        } else {
            r.forward
        };
        let bonus = if r.sprint && m.extra.anim_ticks == 0 {
            0.1
        } else {
            0.0
        };
        m.set_speed(m.movement_speed() as f32 + bonus);
        m.ai.xxa = r.strafe * 0.5;
        m.ai.zza = forward;
        if r.jump > 0.0 {
            let mut ev = Vec::new();
            super::camel::dash(m, r.jump, &mut ev);
            if let Some(rider) = m.rider.as_mut() {
                rider.jump = 0.0;
            }
        }
        return;
    }
    let standing = m.extra.counter > 0;
    if m.on_ground && r.jump == 0.0 && standing {
        m.ai.xxa = 0.0;
        m.ai.zza = 0.0;
        return;
    }
    let forward = if r.forward <= 0.0 {
        r.forward * 0.25
    } else {
        r.forward
    };
    let speed = m.movement_speed() as f32;
    m.set_speed(speed);
    m.ai.xxa = r.strafe * 0.5;
    m.ai.zza = forward;
    if m.on_ground && r.jump > 0.0 {
        // executeRidersJump
        let impulse = m.extra.vector[0] as f32 * r.jump;
        m.velocity.y = f64::from(impulse);
        if forward > 0.0 {
            let (s, c) = (m.yaw.to_radians().msin(), m.yaw.to_radians().mcos());
            m.velocity.x += f64::from(-0.4 * s * r.jump);
            m.velocity.z += f64::from(0.4 * c * r.jump);
        }
        if let Some(rider) = m.rider.as_mut() {
            rider.jump = 0.0;
        }
    }
}
/// `AbstractHorse.aiStep`'s `moveTail` (one in 200 ticks) and the counter's
/// step in `tick` (it runs to 8, then stops). A client runs it on its own
/// random, as vanilla's client-side `aiStep` does.
pub fn tail_tick(m: &mut Mob) {
    if m.random.next_int_bound(200).unwrap_or(1) == 0 {
        m.anim.v[TAIL] = 1.0;
    }
    if m.anim.v[TAIL] > 0.0 {
        m.anim.v[TAIL] += 1.0;
        if m.anim.v[TAIL] > 8.0 {
            m.anim.v[TAIL] = 0.0;
        }
    }
}
/// `AbstractHorse.aiStep` and `tick`: healing, grazing, the rear and the
/// eating/standing/mouth animations.
pub fn ai_step(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    if super::camel::is_camel(m.species()) {
        return super::camel::ai_step(m, events);
    }
    if super::llama::is_llama(m.species()) {
        return super::llama::ai_step(m, w);
    }
    if m.alive() && m.random.next_int_bound(900).unwrap_or(1) == 0 && m.death_time == 0 {
        m.health = (m.health + 1.0).min(m.attrs.max_health);
    }
    // Grazing (canEatGrass: horses and donkeys alike).
    if m.alive() && m.rider.is_none() {
        let below = w.facts.get(w.world.block(
            m.position.x.floor() as i32,
            (m.position.y - 0.5).floor() as i32,
            m.position.z.floor() as i32,
        ));
        if !m.extra.flag
            && m.random.next_int_bound(300).unwrap_or(1) == 0
            && &*below.name == "minecraft:grass_block"
        {
            m.extra.flag = true;
            m.extra.counter2 = 0;
        }
    }
    if m.extra.flag {
        m.extra.counter2 += 1;
        if m.extra.counter2 > 50 {
            m.extra.counter2 = 0;
            m.extra.flag = false;
        }
    }
    if m.extra.counter > 0 {
        m.extra.counter -= 1;
    }
    animate(m);
}
/// `AbstractHorse.tick`'s eat, stand and mouth animations, which follow the
/// eating and standing flags (`flag` and `counter > 0`).
pub fn animate(m: &mut Mob) {
    let (eating, standing) = (m.extra.flag, m.extra.counter > 0);
    let v = &mut m.anim.v;
    if standing {
        v[EAT] = 0.0;
        m.anim.vo[EAT] = 0.0;
        v[STAND] = (v[STAND] + (1.0 - v[STAND]) * 0.4 + 0.05).min(1.0);
    } else {
        if eating {
            v[EAT] = (v[EAT] + (1.0 - v[EAT]) * 0.4 + 0.05).min(1.0);
        } else {
            v[EAT] = (v[EAT] - v[EAT] * 0.4 - 0.05).max(0.0);
        }
        let s = v[STAND];
        v[STAND] = (s + (0.8 * s * s * s - s) * 0.6 - 0.05).max(0.0);
    }
    v[MOUTH] = (v[MOUTH] - v[MOUTH] * 0.7 - 0.05).max(0.0);
}
/// Whether `ai_step` above runs this species' grazing and rearing, so that
/// the server's flags 16 and 32 mean eating and standing.
pub fn grazes(s: Species) -> bool {
    is_equine(s) && !super::camel::is_camel(s) && !super::llama::is_llama(s)
}
