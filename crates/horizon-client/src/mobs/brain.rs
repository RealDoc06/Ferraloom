//! Pieces of vanilla's `Brain` that species implemented as brains share:
//! memories that expire, behaviour durations, walk targets and
//! `MoveToTargetSink`.
use super::{Mob, MobWorld, Who};
use horizon_core::{
    geometry::Vec3,
    random::{LegacyRandom, RandomSource},
};

/// `Behavior`'s bookkeeping in the brains ported behaviour for behaviour
/// (villager, animal, monster, warden): running, and until when.
#[derive(Clone, Copy, Debug)]
pub struct Timed {
    pub running: bool,
    pub end: i64,
    pub min: i32,
    pub max: i32,
}
impl Timed {
    pub fn new(min: i32, max: i32) -> Self {
        Self {
            running: false,
            end: 0,
            min,
            max,
        }
    }
    /// `Behavior.tryStart`'s duration draw from the level random, in Java's
    /// int arithmetic (made even when it cannot vary).
    pub fn begin(&mut self, lr: &mut LegacyRandom, now: i64) {
        self.running = true;
        let bound = self.max.wrapping_add(1).wrapping_sub(self.min);
        let d = self.min.wrapping_add(lr.next_int_bound(bound).unwrap_or(0));
        self.end = now + i64::from(d);
    }
    /// `Behavior.timedOut`.
    pub fn timed_out(&self, now: i64) -> bool {
        now > self.end
    }
}
/// `NearestLivingEntitySensor`'s scan: the living entities (the player
/// among them) whose boxes meet the follow range around `m`, nearest first.
pub fn living_near(m: &Mob, w: &MobWorld) -> Vec<Who> {
    let r = m.attrs.follow_range;
    let area = m.bounds().inflate(Vec3::new(r, r, r));
    let mut found: Vec<(Who, f64)> = Vec::new();
    for o in w.others_near(area) {
        if o.id != m.id && o.alive && o.bounds.intersects(area) {
            found.push((Who::Mob(o.id), m.distance_sqr(o.position)));
        }
    }
    if let Some(p) = &w.player
        && p.alive
        && p.bounds.intersects(area)
    {
        found.push((Who::Player, m.distance_sqr(p.position)));
    }
    found.sort_by(|a, b| a.1.total_cmp(&b.1));
    found.into_iter().map(|(w, _)| w).collect()
}
