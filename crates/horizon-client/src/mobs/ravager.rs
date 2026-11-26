//! Ravagers (`Ravager`): charging beasts that trample leaves, bite for 12,
//! get stunned by a raised shield half the time and answer with a roar
//! that hurts and flings everything within 4 blocks.
use super::{
    Damage, Mob, MobEvent, MobWorld, Species, Who,
    goals::{Float, HurtByTarget, LookAtPlayer, MeleeAttack, NearestTarget, Stroll},
    path::PathType,
};
use horizon_core::geometry::trig::Mth;
use horizon_core::random::RandomSource;

// Ravager state: `counter` attackTick, `counter2` stunnedTick,
// `anim_ticks` roarTick.

pub fn register(m: &mut Mob) {
    m.set_malus(PathType::Leaves, 0.0);
    let g = &mut m.ai.goals;
    g.add(0, Float);
    g.add(4, MeleeAttack::new(1.0, true));
    g.add(5, Stroll::avoiding_water(0.4));
    g.add(6, LookAtPlayer::new(6.0));
    g.add(10, LookAtPlayer::mobs(8.0));
    let mut hurt = HurtByTarget::new().alert();
    hurt.ignore = vec![
        Species::Pillager,
        Species::Vindicator,
        Species::Evoker,
        Species::Illusioner,
        Species::Witch,
        Species::Ravager,
    ];
    m.ai.targets.add(2, hurt);
    m.ai.targets.add(3, NearestTarget::player(true));
    m.ai.targets.add(
        4,
        NearestTarget::new(
            super::goals::Prey::Filter(|s| {
                matches!(s.species, Species::Villager | Species::WanderingTrader) && !s.baby
            }),
            true,
        ),
    );
    m.ai.targets
        .add(4, NearestTarget::species(&[Species::IronGolem], true));
}
/// `Ravager.isImmobile`: biting, stunned or roaring.
pub fn immobile(m: &Mob) -> bool {
    m.extra.counter > 0 || m.extra.counter2 > 0 || m.extra.anim_ticks > 0
}
/// `Ravager.doHurtTarget`: the bite animation (entity event 4 for clients).
pub fn on_attack(m: &mut Mob, events: &mut Vec<MobEvent>) {
    m.extra.counter = 10;
    events.push(MobEvent::EntityEvent {
        mob: m.id,
        event: 4,
        at: [m.position.x, m.position.y, m.position.z],
    });
    events.push(MobEvent::Sound(
        "entity.ravager.attack",
        [m.position.x, m.position.y, m.position.z],
        1.0,
        1.0,
    ));
}
/// `Ravager.blockedByItem`: a raised shield stuns it half the time (else
/// flings the defender); returns whether it was stunned.
pub fn blocked(m: &mut Mob, events: &mut Vec<MobEvent>) -> bool {
    if m.extra.anim_ticks != 0 {
        return false;
    }
    if m.random.next_double() < 0.5 {
        m.extra.counter2 = 40;
        // Entity event 39: the stun starts on clients.
        events.push(MobEvent::EntityEvent {
            mob: m.id,
            event: 39,
            at: [m.position.x, m.position.y, m.position.z],
        });
        events.push(MobEvent::Sound(
            "entity.ravager.stunned",
            [m.position.x, m.position.y, m.position.z],
            1.0,
            1.0,
        ));
        true
    } else {
        false
    }
}
/// `Ravager.aiStep`: speed toward 0.35 while hunting, leaf trampling, and
/// the bite, stun and roar clocks.
pub fn ai_step(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    if !m.alive() {
        return;
    }
    let max = if immobile(m) {
        0.0
    } else if m.ai.target.is_some() {
        0.35
    } else {
        0.3
    };
    m.attrs.speed = if max == 0.0 {
        0.0
    } else {
        m.attrs.speed + (max - m.attrs.speed) * 0.1
    };
    if m.horizontal_collision && w.mob_griefing {
        let b = m
            .bounds()
            .inflate(horizon_core::geometry::Vec3::new(0.2, 0.2, 0.2));
        let (lo, hi) = (b.min_position(), b.max_position());
        let mut destroyed = false;
        for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
            for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
                for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                    let name = w.block_name(x, y, z);
                    if name.ends_with("_leaves") {
                        events.push(MobEvent::BreakBlock {
                            pos: [x, y, z],
                            drop: true,
                        });
                        destroyed = true;
                    }
                }
            }
        }
        if !destroyed && m.on_ground {
            m.ai.jumping = true;
        }
    }
    if m.extra.anim_ticks > 0 {
        m.extra.anim_ticks -= 1;
        if m.extra.anim_ticks == 10 {
            roar(m, w, events);
        }
    }
    if m.extra.counter > 0 {
        m.extra.counter -= 1;
    }
    if m.extra.counter2 > 0 {
        m.extra.counter2 -= 1;
        if m.random.next_int_bound(6).unwrap_or(1) == 0 {
            let yaw = m.body_yaw.to_radians();
            let width = f64::from(m.width());
            events.push(MobEvent::Particles {
                kind: "entity_effect",
                at: [
                    m.position.x - width * f64::from(yaw.msin()),
                    m.position.y + f64::from(m.height()) - 0.3,
                    m.position.z + width * f64::from(yaw.mcos()),
                ],
                count: 0,
            });
        }
        if m.extra.counter2 == 0 {
            events.push(MobEvent::Sound(
                "entity.ravager.roar",
                [m.position.x, m.position.y, m.position.z],
                1.0,
                1.0,
            ));
            m.extra.anim_ticks = 20;
        }
    }
}
/// `Ravager.roar`: 6 damage to everything but illagers within 4 blocks,
/// and a strong knockback (players too, through the client).
fn roar(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    let area = m
        .bounds()
        .inflate(horizon_core::geometry::Vec3::new(4.0, 4.0, 4.0));
    let from = [m.position.x, m.position.y, m.position.z];
    let fling = |p: horizon_core::geometry::Vec3| {
        let (xd, zd) = (p.x - m.position.x, p.z - m.position.z);
        let dd = (xd * xd + zd * zd).max(0.001);
        [xd / dd * 4.0, 0.2, zd / dd * 4.0]
    };
    if let Some(p) = &w.player
        && p.alive
        && p.bounds.intersects(area)
    {
        events.push(MobEvent::AttackPlayer(6.0, from));
        events.push(MobEvent::Push {
            target: Who::Player,
            velocity: fling(p.position),
        });
    }
    for s in w.others_near(area) {
        if s.id == m.id || !s.alive || s.species == Species::Ravager || !s.bounds.intersects(area) {
            continue;
        }
        let illager = matches!(
            s.species,
            Species::Pillager | Species::Vindicator | Species::Evoker | Species::Illusioner
        );
        if !illager {
            events.push(MobEvent::AttackMob {
                target: s.id,
                amount: 6.0,
                source: Damage::mob(m.id, from),
            });
        }
        events.push(MobEvent::Push {
            target: Who::Mob(s.id),
            velocity: fling(s.position),
        });
    }
    // Entity event 69: a puff of smoke, and the client's own push. A client
    // of this server (the window of the server thread) gets the event;
    // this window draws the puff itself.
    events.push(MobEvent::EntityEvent {
        mob: m.id,
        event: 69,
        at: from,
    });
    events.push(MobEvent::Particles {
        kind: "poof",
        at: [
            m.position.x,
            m.position.y + f64::from(m.height()) / 2.0,
            m.position.z,
        ],
        count: 40,
    });
}
