//! The mobs' turn in `ServerLevel.tick`'s entity pass: each mob ticks in
//! order against a snapshot of the others, and the ones after it see it as
//! it is after its tick (its snapshot entry, its pushes and the brain
//! memories it set on them).
//!
//! The loop runs serially, or speculatively: first every mob ticks at once
//! on the pool, in place (keeping a copy of itself as it was) with a copy of
//! the level random, against the snapshot as it stood before any tick,
//! recording what it read of the snapshot ([`Reads`]). Then, in mob order, a
//! mob keeps its speculative tick only when the serial loop would have given
//! it the same inputs: nothing changed it since (pushes, brain memories), no
//! entry it read differs from the one it read, and the level random is as it
//! was when it drew from it. Otherwise it is put back and ticks again in
//! turn, as the serial loop does.
//! Either way the bookkeeping after each tick runs in mob order, so both
//! give the same mobs, events and random to the bit.
//!
//! What else a mob tick touches outside its mob, and why a speculative tick
//! may: the blocks, the tag table and every resource hold still while the
//! mobs tick; `MobWorld::has_tag`'s answers and `others_near`'s list are
//! kept per thread (each worker keeps its own for the pass); the POI lists,
//! interned sound names and lazily built tables are caches whose answers do
//! not depend on who asks first; the raiders' hurt counter
//! (`raider::next_hurt_seq`) is the one shared counter, and a speculative
//! tick that would draw from it ticks again in turn instead.
use super::{
    BlocksAhead, Mob, MobEvent, MobWorld, Others, PendingPushes, PlayerView, Seen, ai, living,
    raider,
    seen_grid::{ColumnSet, SeenGrid, SpeciesSet},
    spawning::LocalDifficulty,
};
use crate::fast_hash::FxHashMap;
use horizon_core::{
    geometry::Vec3,
    random::{LegacyRandom, RandomSource},
};
use std::{cell::RefCell, collections::HashSet};

/// How the mob ticks run: `FERRALOOM_SERIAL_MOBS=1` serially,
/// `FERRALOOM_CHECK_MOBS=1` both ways, asserting they agree; else
/// speculatively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Serial,
    Speculative,
    Check,
}
impl Mode {
    pub fn from_env() -> Self {
        static MODE: std::sync::OnceLock<Mode> = std::sync::OnceLock::new();
        *MODE.get_or_init(|| {
            let on = |name| std::env::var(name).is_ok_and(|v| v == "1");
            if on("FERRALOOM_CHECK_MOBS") {
                Mode::Check
            } else if on("FERRALOOM_SERIAL_MOBS") {
                Mode::Serial
            } else {
                Mode::Speculative
            }
        })
    }
}

/// What the loop reads of each mob before it, on the tick's workers, from
/// the mob as it stands and the blocks: where it stood, the nearest player to
/// it (the despawn distance), the fluids at its box, the block kinds around
/// it, the blocks its tick reads where it stands and the player it would see
/// when bound to none. Each is taken only while its inputs are unchanged.
#[derive(Default)]
pub struct MobAhead {
    pub stood: Vec3,
    pub nearest: Option<Vec3>,
    pub fluids: Option<([u64; 7], living::Fluids)>,
    pub kinds: Option<([u64; 4], u16)>,
    pub guest: Option<u32>,
    pub memo: Option<(u64, bool)>,
    pub blocks: Option<BlocksAhead>,
}

/// The LAN guests' part in the mob ticks (the game's `lan_mobs`).
pub trait Guests: Sync {
    /// The guest whose player mob `m` sees, from the one it was bound to
    /// and the choice read ahead where it stood; None for the host's.
    fn player_for(
        &self,
        m: &Mob,
        host: Option<&PlayerView>,
        bound: Option<u32>,
        ahead: (Vec3, Option<u32>),
    ) -> Option<u32>;
    /// Guest `conn`'s player as mobs see it.
    fn view(&self, conn: u32) -> Option<PlayerView>;
    /// Moves the events from `from` on that act on the player to `out`, for
    /// guest `conn`.
    fn route(
        &self,
        conn: u32,
        events: &mut Vec<MobEvent>,
        from: usize,
        out: &mut Vec<(u32, MobEvent)>,
    );
}

/// What stays the same for every mob of the loop.
pub struct LoopCx<'a, 'w> {
    /// The world every mob sees, but for the others and what is per mob.
    pub base: &'a MobWorld<'w>,
    pub reads: &'a [MobAhead],
    /// The player's UUID (villagers' reputation in the snapshot).
    pub uuid: Option<[i32; 4]>,
    /// `getNearestPlayer`'s position from a point.
    pub nearest_to: &'a (dyn Fn(Vec3) -> Option<Vec3> + Sync),
    /// Whether a column is in an entity-ticking chunk.
    pub ticking: &'a (dyn Fn(i32, i32) -> bool + Sync),
    /// `getCurrentDifficultyAt`.
    pub difficulty: &'a (dyn Fn([f64; 3]) -> LocalDifficulty + Sync),
    /// The guests, when there are any.
    pub guests: Option<&'a dyn Guests>,
}

/// The loop's state: the mobs, what they see of each other, and what their
/// ticks leave for the game.
#[derive(Clone)]
pub struct MobLoop {
    pub mobs: Vec<Mob>,
    /// Each mob's entry, as of its tick for the ones that ticked.
    pub snapshot: Vec<Seen>,
    grid: SeenGrid,
    /// Each id's first slot, as a scan from the front finds it.
    index: FxHashMap<u32, u32>,
    /// Pushes waiting for their mob's tick (and, after the loop, the rest).
    pub pushes: PendingPushes,
    /// The level random, lent to the mobs.
    pub random: LegacyRandom,
    pub events: Vec<MobEvent>,
    /// Whether each mob stays.
    pub keep: Vec<bool>,
    /// Killed during their own tick.
    pub dead: Vec<Mob>,
    /// Killed bodies with effects that act on death.
    pub corpses: Vec<Mob>,
    pub died: HashSet<u32>,
    /// The bodies that go this tick: (id, feet, width, height).
    pub gone: Vec<(u32, Vec3, f64, f64)>,
    /// Each mob's guest, while it sees one.
    pub bindings: FxHashMap<u32, u32>,
    pub guest_events: Vec<(u32, MobEvent)>,
    /// The species of every mob the loop met.
    pub present: SpeciesSet,
}

/// How the speculation went.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// Mobs that ticked speculatively.
    pub speculated: usize,
    /// Of those, the ones that ticked again in turn.
    pub reran: usize,
}

/// What a mob's speculative tick, which ticked the mob in place, left.
struct Spec {
    kept: bool,
    events: Vec<MobEvent>,
    /// The level random after the tick, when it drew from it.
    random: Option<LegacyRandom>,
    reads: Reads,
    /// Its snapshot entry after the tick, and whether it differs from the
    /// one before.
    seen: Seen,
    changed: bool,
    guest: Option<u32>,
}

/// The snapshot entries that differ from what they were before the loop,
/// and the grid columns they were and are in.
struct Dirty {
    slots: Vec<bool>,
    any: bool,
    columns: ColumnSet,
}
impl Dirty {
    /// Entry `i`, which was in column `was` and is in `is`, changed.
    fn mark(&mut self, i: usize, was: (i32, i32), is: (i32, i32)) {
        self.slots[i] = true;
        self.any = true;
        self.columns.insert(was);
        self.columns.insert(is);
    }
    fn has(&self, i: u32) -> bool {
        self.slots[i as usize]
    }
}

/// The commit pass's tracking, or none for the serial loop.
struct Tracking {
    dirty: Dirty,
    /// Mobs whose state another mob's tick changed (brain memories).
    touched: Vec<bool>,
    /// Each speculatively ticked mob as it was before, until its turn.
    backups: Vec<Option<Mob>>,
    /// Mobs no longer needed (backups, discarded speculative ticks), dropped
    /// on the pool after the loop.
    replaced: Vec<Mob>,
}
impl Tracking {
    /// Puts mob `i` back as it was before its speculative tick, if it is not.
    fn restore(&mut self, m: &mut Mob, i: usize) {
        if let Some(before) = self.backups[i].take() {
            self.replaced.push(std::mem::replace(m, before));
        }
    }
}

/// Mobs per work item of the speculative pass.
const GRAIN: usize = 16;

impl MobLoop {
    pub fn new(
        mobs: Vec<Mob>,
        snapshot: Vec<Seen>,
        died: HashSet<u32>,
        bindings: FxHashMap<u32, u32>,
        random: LegacyRandom,
    ) -> Self {
        let mut grid = SeenGrid::new(snapshot.iter().map(|s| s.bounds));
        // Searches for species none of the mobs is of end at once.
        grid.keep_species(snapshot.iter().map(|s| s.species));
        let mut index = FxHashMap::<u32, u32>::default();
        for (i, s) in snapshot.iter().enumerate() {
            index.entry(s.id).or_insert(i as u32);
        }
        Self {
            keep: Vec::with_capacity(mobs.len()),
            mobs,
            snapshot,
            grid,
            index,
            pushes: PendingPushes::default(),
            random,
            events: Vec::new(),
            dead: Vec::new(),
            corpses: Vec::new(),
            died,
            gone: Vec::new(),
            bindings,
            guest_events: Vec::new(),
            present: SpeciesSet::default(),
        }
    }

    /// Ticks every mob once.
    pub fn run(&mut self, cx: &LoopCx, mode: Mode) -> Stats {
        match mode {
            Mode::Serial => {
                let _tags = super::keep_tag_answers();
                for i in 0..self.mobs.len() {
                    self.step(cx, i, None);
                }
                Stats::default()
            }
            Mode::Speculative => self.run_speculative(cx),
            Mode::Check => {
                let mut serial = self.clone();
                let seq = raider::hurt_seq();
                serial.run(cx, Mode::Serial);
                raider::set_hurt_seq(seq);
                let stats = self.run_speculative(cx);
                if let Some(diff) = self.differs(&serial) {
                    panic!("speculative mob ticks differ from the serial loop: {diff}");
                }
                stats
            }
        }
    }

    /// One mob's turn, as the serial loop takes it.
    fn step(&mut self, cx: &LoopCx, i: usize, mut track: Option<&mut Tracking>) {
        // Each tick stalls on its mob's memory: start loading the next
        // mobs', stage by stage (`Mob::prefetch`).
        for stage in 0..4u8 {
            if let Some(next) = self.mobs.get(i + 4 - usize::from(stage)) {
                next.prefetch(stage);
            }
        }
        if let Some(read) = cx.reads.get(i + 1) {
            horizon_threads::prefetch_all(read);
        }
        self.present.insert(self.mobs[i].species());
        let read = &cx.reads[i];
        let m = &mut self.mobs[i];
        let mut w = MobWorld {
            others: Others::new(&self.snapshot),
            others_grid: Some(&self.grid),
            others_index: Some(&self.index),
            ..cx.base.clone()
        };
        set_ahead(&mut w, cx, m, read);
        let guest = cx.guests.and_then(|guests| {
            let bound = self.bindings.get(&m.id).copied();
            let conn = guests.player_for(m, w.player.as_ref(), bound, (read.stood, read.guest));
            match conn {
                Some(conn) => {
                    self.bindings.insert(m.id, conn);
                    w.player = guests.view(conn);
                }
                None => {
                    self.bindings.remove(&m.id);
                }
            }
            conn
        });
        let (x, z) = (m.position.x.floor() as i32, m.position.z.floor() as i32);
        if !w.world.loaded(x, z) {
            self.keep.push(!m.hostile() || m.persistent);
            return;
        }
        // ServerLevel.tick: every loaded entity checks despawning, but only
        // those in entity-ticking chunks (within the simulation distance)
        // tick.
        if !(cx.ticking)(x, z) {
            let kept = ai::despawn_check(m, &w);
            drop(w);
            if !kept {
                self.died.remove(&m.id);
                if let Some(track) = track.as_deref_mut()
                    && self.snapshot[i].alive
                {
                    let at = self.grid.column_of(i);
                    track.dirty.mark(i, at, at);
                }
                self.snapshot[i].alive = false;
            }
            self.keep.push(kept);
            return;
        }
        super::apply_mob_pushes(m, &mut self.pushes);
        let from = self.events.len();
        let kept = m.tick(&w, &mut self.random, &mut self.events);
        // This tick's reads, for the next one over the same blocks.
        m.ai.move_memo.keep_reads(read.fluids, read.kinds);
        drop(w);
        self.finish(cx, i, from, kept, guest, None, track);
    }

    /// The rest of mob `i`'s turn after its tick, which left its events from
    /// `from` on: deaths, pushes, the guest's events, its snapshot entry
    /// (`seen`, when already made, with whether it changed) and the brain
    /// memories it set on others.
    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        cx: &LoopCx,
        i: usize,
        from: usize,
        kept: bool,
        guest: Option<u32>,
        seen: Option<(Seen, bool)>,
        track: Option<&mut Tracking>,
    ) {
        let m = &mut self.mobs[i];
        // A killed mob's body is removed: its effects that act on death run
        // (`LivingEntity.remove` with KILLED).
        if !kept && !m.alive() && !m.removed && !m.extra.effects.is_empty() {
            self.corpses.push(m.clone());
        }
        super::take_mob_pushes(&mut self.events, from, &mut self.pushes);
        // LivingEntity.tickDeath: after twenty ticks the body is removed.
        if !kept && !m.removed && !m.alive() && m.death_time >= 20 {
            let (width, height) = (f64::from(m.width()), f64::from(m.height()));
            self.gone.push((m.id, m.position, width, height));
        }
        if let (Some(conn), Some(guests)) = (guest, cx.guests) {
            guests.route(conn, &mut self.events, from, &mut self.guest_events);
        }
        // Killed during its own tick (drowning, burning, falling...).
        if !m.alive() && !m.removed && self.died.insert(m.id) {
            self.dead.push(m.clone());
        }
        if !kept {
            self.died.remove(&m.id);
        }
        self.keep.push(kept);
        // ServerLevel ticks entities one after another: the ones after this
        // mob see it as it is now.
        let (entry, changed) = seen.unwrap_or_else(|| {
            let mut entry = m.seen(cx.uuid);
            entry.alive &= kept;
            let changed = track.is_some() && !entry.same_as(&self.snapshot[i]);
            (entry, changed)
        });
        self.snapshot[i] = entry;
        let was = self.grid.column_of(i);
        self.grid.update(i, self.snapshot[i].bounds);
        self.grid.note_species(self.snapshot[i].species);
        match track {
            Some(track) => {
                if changed {
                    track.dirty.mark(i, was, self.grid.column_of(i));
                }
                // A mob changed before its turn ticks again from the changed
                // state.
                super::apply_brain_events_noting(
                    &mut self.mobs,
                    &mut self.events,
                    from,
                    Some(&self.index),
                    |m, k| {
                        track.restore(m, k);
                        track.touched[k] = true;
                    },
                );
            }
            None => super::apply_brain_events_indexed(
                &mut self.mobs,
                &mut self.events,
                from,
                Some(&self.index),
            ),
        }
    }

    fn run_speculative(&mut self, cx: &LoopCx) -> Stats {
        let n = self.mobs.len();
        let start = self.random.clone();
        // Pushes and guest bindings go by id: a mob sharing its id with
        // another ticks in turn.
        let shared: HashSet<u32> = if self.index.len() == n {
            HashSet::new()
        } else {
            let mut once = HashSet::new();
            self.mobs
                .iter()
                .map(|m| m.id)
                .filter(|&id| !once.insert(id))
                .collect()
        };
        let mut specs: Vec<Option<Spec>> = (0..n).map(|_| None).collect();
        let mut backups: Vec<Option<Mob>> = (0..n).map(|_| None).collect();
        {
            let _span = crate::spans::scope("creature_spec");
            let (snapshot, grid, index, bindings) =
                (&self.snapshot, &self.grid, &self.index, &self.bindings);
            let mut items: Vec<(usize, &mut Mob, &mut Option<Mob>, &mut Option<Spec>)> = self
                .mobs
                .iter_mut()
                .zip(backups.iter_mut())
                .zip(specs.iter_mut())
                .enumerate()
                .map(|(i, ((m, backup), spec))| (i, m, backup, spec))
                .collect();
            let (start, shared) = (&start, &shared);
            horizon_threads::pool::for_each_mut_with(
                &mut items,
                GRAIN,
                || {
                    let w = MobWorld {
                        others: Others::new(snapshot),
                        others_grid: Some(grid),
                        others_index: Some(index),
                        ..cx.base.clone()
                    };
                    (super::keep_tag_answers(), w)
                },
                |(_, w), (i, m, backup, spec)| {
                    if shared.contains(&m.id) {
                        return;
                    }
                    let (i, before) = (*i, &snapshot[*i]);
                    **spec = speculate(cx, w, m, backup, before, &cx.reads[i], bindings, start);
                },
            );
        }
        let _span = crate::spans::scope("creature_commit");
        let _tags = super::keep_tag_answers();
        let mut track = Tracking {
            dirty: Dirty {
                slots: vec![false; n],
                any: false,
                columns: ColumnSet::default(),
            },
            touched: vec![false; n],
            backups,
            replaced: Vec::new(),
        };
        let grid_before = (self.grid.species_added(), self.grid.reach().to_bits());
        let mut stats = Stats::default();
        for i in 0..n {
            let Some(mut spec) = specs[i].take() else {
                self.step(cx, i, Some(&mut track));
                continue;
            };
            stats.speculated += 1;
            let holds = !track.touched[i]
                && !self.pushes.has(self.mobs[i].id)
                && spec.reads.hold(&self.grid, &track.dirty, grid_before)
                && (spec.random.is_none() || same_random(&self.random, &start));
            if !holds {
                stats.reran += 1;
                track.restore(&mut self.mobs[i], i);
                self.step(cx, i, Some(&mut track));
                continue;
            }
            // The serial turn, from the speculative tick's results.
            if let Some(before) = track.backups[i].take() {
                track.replaced.push(before);
            }
            self.present.insert(self.mobs[i].species());
            if cx.guests.is_some() {
                let id = self.mobs[i].id;
                match spec.guest {
                    Some(conn) => self.bindings.insert(id, conn),
                    None => self.bindings.remove(&id),
                };
            }
            if let Some(random) = spec.random {
                self.random = random;
            }
            let from = self.events.len();
            self.events.append(&mut spec.events);
            let seen = Some((spec.seen, spec.changed));
            self.finish(cx, i, from, spec.kept, spec.guest, seen, Some(&mut track));
        }
        drop(specs);
        let mut replaced: Vec<Option<Mob>> = track.replaced.into_iter().map(Some).collect();
        horizon_threads::pool::for_each_mut(&mut replaced, 64, |m| *m = None);
        stats
    }

    /// Where this loop's results differ from `other`'s, if they do.
    fn differs(&self, other: &MobLoop) -> Option<String> {
        fn first<T: std::fmt::Debug>(name: &str, a: &[T], b: &[T]) -> Option<String> {
            if a.len() != b.len() {
                return Some(format!("{name}: {} != {}", a.len(), b.len()));
            }
            let (i, (x, y)) = a
                .iter()
                .zip(b)
                .map(|(x, y)| (format!("{x:?}"), format!("{y:?}")))
                .enumerate()
                .find(|(_, (x, y))| x != y)?;
            Some(format!("{name}[{i}]:\n{x}\n!=\n{y}"))
        }
        let sorted = |set: &HashSet<u32>| {
            let mut ids: Vec<u32> = set.iter().copied().collect();
            ids.sort_unstable();
            ids
        };
        let bound = |map: &FxHashMap<u32, u32>| {
            let mut pairs: Vec<(u32, u32)> = map.iter().map(|(&k, &v)| (k, v)).collect();
            pairs.sort_unstable();
            pairs
        };
        first("mobs", &self.mobs, &other.mobs)
            .or_else(|| first("events", &self.events, &other.events))
            .or_else(|| first("snapshot", &self.snapshot, &other.snapshot))
            .or_else(|| first("keep", &self.keep, &other.keep))
            .or_else(|| first("dead", &self.dead, &other.dead))
            .or_else(|| first("corpses", &self.corpses, &other.corpses))
            .or_else(|| first("gone", &self.gone, &other.gone))
            .or_else(|| first("guest events", &self.guest_events, &other.guest_events))
            .or_else(|| first("died", &sorted(&self.died), &sorted(&other.died)))
            .or_else(|| first("bindings", &bound(&self.bindings), &bound(&other.bindings)))
            .or_else(|| first("random", &[&self.random], &[&other.random]))
            .or_else(|| first("pushes", &[&self.pushes], &[&other.pushes]))
            .or_else(|| first("present", &[&self.present], &[&other.present]))
    }
}

/// Sets what is mob `m`'s own in its view of the world: its difficulty, what
/// was read ahead of it and the nearest player.
fn set_ahead(w: &mut MobWorld, cx: &LoopCx, m: &Mob, read: &MobAhead) {
    // getCurrentDifficultyAt(blockPosition()): the mob's own chunk, not the
    // local player's (a server thread has no player).
    w.difficulty = (cx.difficulty)([m.position.x, m.position.y, m.position.z]);
    w.fluids_ahead = read.fluids;
    w.kinds_ahead = read.kinds;
    w.memo_ahead = read.memo;
    w.blocks_ahead = read.blocks;
    let same =
        |a: Vec3, b: Vec3| [a.x, a.y, a.z].map(f64::to_bits) == [b.x, b.y, b.z].map(f64::to_bits);
    w.nearest_player = if same(read.stood, m.position) {
        read.nearest
    } else {
        (cx.nearest_to)(m.position)
    };
}

/// Mob `m`'s tick, in place (`backup` keeps it as it was), against the
/// snapshot as it was before any tick (`w`, this thread's), drawing from a
/// copy of the level random as it was then. None for a mob that does not
/// tick (it takes its turn serially) or whose tick panicked (it is put back).
#[allow(clippy::too_many_arguments)]
fn speculate(
    cx: &LoopCx,
    w: &mut MobWorld,
    m: &mut Mob,
    backup: &mut Option<Mob>,
    before: &Seen,
    read: &MobAhead,
    bindings: &FxHashMap<u32, u32>,
    start: &LegacyRandom,
) -> Option<Spec> {
    let (x, z) = (m.position.x.floor() as i32, m.position.z.floor() as i32);
    if !w.world.loaded(x, z) || !(cx.ticking)(x, z) {
        return None;
    }
    set_ahead(w, cx, m, read);
    let guest = cx.guests.and_then(|guests| {
        let bound = bindings.get(&m.id).copied();
        let host = cx.base.player.as_ref();
        let conn = guests.player_for(m, host, bound, (read.stood, read.guest));
        w.player = match conn {
            Some(conn) => guests.view(conn),
            None => cx.base.player.clone(),
        };
        conn
    });
    *backup = Some(m.clone());
    let mut random = start.clone();
    let mut events = Vec::new();
    let ticked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        record(|| m.tick(w, &mut random, &mut events))
    }));
    let Ok((kept, reads)) = ticked else {
        *m = backup.take()?;
        return None;
    };
    m.ai.move_memo.keep_reads(read.fluids, read.kinds);
    let mut seen = m.seen(cx.uuid);
    seen.alive &= kept;
    let changed = !seen.same_as(before);
    let drew = !same_random(&random, start);
    Some(Spec {
        kept,
        events,
        random: drew.then_some(random),
        reads,
        seen,
        changed,
        guest,
    })
}

/// Whether two level randoms give the same draws: the same state, and the
/// same `nextGaussian` kept back (both draw one alike).
fn same_random(a: &LegacyRandom, b: &LegacyRandom) -> bool {
    if a.state() != b.state() {
        return false;
    }
    let (mut a, mut b) = (a.clone(), b.clone());
    a.next_gaussian().to_bits() == b.next_gaussian().to_bits() && a.state() == b.state()
}

/// What a speculative mob tick read of what earlier ticks of the loop can
/// change: the snapshot entries, the grid's answers and species, and
/// anything it must not do speculatively.
#[derive(Debug, Default)]
pub struct Reads {
    /// Read every entry.
    all: bool,
    /// Asked the grid's species.
    species: bool,
    /// Did what only a tick in turn may (`note_impure`).
    impure: bool,
    /// Entries read by slot.
    slots: Vec<u32>,
    /// The grid columns of each grid query.
    queries: Vec<((i32, i32), (i32, i32))>,
}
impl Reads {
    pub(crate) fn all(&mut self) {
        self.all = true;
    }
    pub(crate) fn species(&mut self) {
        self.species = true;
    }
    pub(crate) fn slot(&mut self, i: u32) {
        self.slots.push(i);
    }
    /// A grid query over `area`, with the grid's columns for it, or None
    /// when the grid sent it to every entry.
    pub(crate) fn query(&mut self, columns: Option<((i32, i32), (i32, i32))>) {
        match columns {
            Some(columns) => self.queries.push(columns),
            None => self.all = true,
        }
    }
    /// Whether the serial loop, at this point, would give the tick the same
    /// answers: no entry it read changed, and each grid query reads the same
    /// columns, to which no changed entry came and from which none went, so
    /// it finds the same entries, unchanged (`before`: the grid's count of
    /// added species and its reach, as bits, before the loop).
    fn hold(&self, grid: &SeenGrid, dirty: &Dirty, before: (u32, u64)) -> bool {
        if self.impure || (self.species && grid.species_added() != before.0) {
            return false;
        }
        // The grid changes only with the boxes of changed entries.
        if !dirty.any {
            return true;
        }
        if self.all || self.slots.iter().any(|&i| dirty.has(i)) {
            return false;
        }
        // A wider reach widens every query.
        if !self.queries.is_empty() && grid.reach().to_bits() != before.1 {
            return false;
        }
        self.queries
            .iter()
            .all(|&(lo, hi)| !grid.too_wide(lo, hi) && !dirty.columns.meets(lo, hi))
    }
}

thread_local! {
    /// The reads of the speculative tick running on this thread.
    static READS: RefCell<Option<Reads>> = const { RefCell::new(None) };
}

/// Runs `f`, a speculative tick, recording what it reads.
fn record<T>(f: impl FnOnce() -> T) -> (T, Reads) {
    /// Stops the recording, also when `f` panics.
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            READS.with_borrow_mut(|r| *r = None);
        }
    }
    READS.with_borrow_mut(|r| *r = Some(Reads::default()));
    let _stop = Stop;
    let out = f();
    (out, READS.with_borrow_mut(Option::take).unwrap_or_default())
}

/// Notes a read of the running speculative tick, if one runs on this thread.
pub(crate) fn note(f: impl FnOnce(&mut Reads)) {
    READS.with_borrow_mut(|r| {
        if let Some(r) = r {
            f(r);
        }
    });
}

/// True, and marks the tick to run again in turn, when a speculative tick
/// runs on this thread: for what only a tick in turn may do.
pub(crate) fn note_impure() -> bool {
    READS.with_borrow_mut(|r| match r {
        Some(r) => {
            r.impure = true;
            true
        }
        None => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assets::Assets,
        biome::Biomes,
        blocks::BlockFacts,
        chunk::{Chunk, HEIGHT, MIN_Y},
        items::Items,
        light::{LightTables, apply_lighting, light_chunk},
        mobs::{MobData, MobTypes, Species},
        world::World,
    };
    use std::sync::Arc;

    /// Crowds that push each other and read each other (a cow pen, sheep in
    /// love, zombies after villagers) and parrots drawing from the level
    /// random every tick tick alike serially and speculatively, and the
    /// speculation both keeps ticks and runs some again.
    #[test]
    fn speculative_ticks_match_the_serial_loop() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client/26.3");
        if !root.join("horizon-resources.json").exists() {
            return;
        }
        let assets = Assets::load(&root).unwrap();
        let registry = assets.registry;
        let facts = BlockFacts::load(&root, registry).unwrap();
        let motion = crate::motion::MotionResources::load(&root, &assets).unwrap();
        let data = MobData::load(&root, &facts);
        let types = MobTypes::load(&root).unwrap();
        let biomes = Biomes::load(&root).unwrap();
        let items = Items::load(&root, registry, &facts).unwrap();
        let tables = LightTables::load(&root, registry, &facts).unwrap();
        let collision = crate::collisions::load(&root, registry).unwrap();
        let state = |name: &str| registry.default_state(name).unwrap();
        // A flat, lit plains world around the origin.
        let mut world = World::new(state("minecraft:air"), 0);
        let plains = biomes.id("minecraft:plains");
        let layers = [
            "minecraft:bedrock",
            "minecraft:dirt",
            "minecraft:grass_block",
        ];
        for cz in -3..=3 {
            for cx in -3..=3 {
                let mut chunk = Chunk::empty(cx, cz);
                for qy in 0..HEIGHT / 4 {
                    for qz in 0..4 {
                        for qx in 0..4 {
                            chunk.set_biome_cell(qx, qy, qz, plains);
                        }
                    }
                }
                world.chunks.insert((cx, cz), Arc::new(chunk));
                for z in 0..16 {
                    for x in 0..16 {
                        for (y, layer) in layers.iter().enumerate() {
                            world
                                .set(cx * 16 + x, MIN_Y + y as i32, cz * 16 + z, state(layer))
                                .unwrap();
                        }
                    }
                }
            }
        }
        let keys: Vec<(i32, i32)> = world.chunks.keys().copied().collect();
        let lit: Vec<((i32, i32), Chunk)> = keys
            .iter()
            .map(|&(x, z)| {
                let region: [[Option<&Chunk>; 3]; 3] = std::array::from_fn(|dz| {
                    std::array::from_fn(|dx| {
                        world
                            .chunks
                            .get(&(x + dx as i32 - 1, z + dz as i32 - 1))
                            .map(|c| &**c)
                    })
                });
                let mut c = (*world.chunks[&(x, z)]).clone();
                apply_lighting(&mut c, light_chunk(&region, &tables, true));
                c.lit = true;
                ((x, z), c)
            })
            .collect();
        for (k, c) in lit {
            world.chunks.insert(k, Arc::new(c));
        }
        world.day_time = 18_000;
        world.game_time = 1000;
        let ground = f64::from(MIN_Y + 3);
        let mut mobs = Vec::new();
        let mut add = |kind: &str, x: f64, z: f64, setup: &dyn Fn(&mut Mob)| {
            let id = 1000 + mobs.len() as u32;
            let mut m = Mob::new(
                id,
                Arc::new(types.types[kind].clone()),
                Vec3::new(x, ground, z),
                0.0,
            );
            m.random = LegacyRandom::new(i64::from(id) * 7919);
            if super::super::monster_brain::has_brain(m.species()) {
                super::super::monster_brain::init(&mut m, i64::from(id));
            }
            if m.species() == Species::Villager {
                super::super::villager_brain::init(
                    &mut m,
                    world.game_time,
                    world.day_time,
                    i64::from(id),
                );
            }
            if super::super::animal_brain::kind_of(&m).is_some() {
                super::super::animal_brain::init(&mut m, i64::from(id));
            }
            setup(&mut m);
            mobs.push(m);
        };
        let none = |_: &mut Mob| {};
        let in_love = |m: &mut Mob| m.extra.in_love = 600;
        // A pen of cows standing in each other.
        for k in 0..30 {
            add(
                "minecraft:cow",
                f64::from(k % 6) * 0.6,
                f64::from(k / 6) * 0.6,
                &none,
            );
        }
        for k in 0..8 {
            add(
                "minecraft:sheep",
                12.0 + f64::from(k % 4),
                f64::from(k / 4),
                &in_love,
            );
        }
        for k in 0..4 {
            add("minecraft:villager", -12.0 + f64::from(k), 6.0, &none);
            add("minecraft:zombie", -12.0 + f64::from(k), -2.0, &none);
            add("minecraft:parrot", 4.0, -8.0 + f64::from(k), &none);
            add("minecraft:goat", -6.0 + f64::from(k) * 0.5, 12.0, &none);
        }
        let lid_openers = std::collections::HashMap::new();
        let base = MobWorld {
            world: &world,
            registry,
            facts: &facts,
            fluid_tables: &motion.fluids,
            movement: &motion.blocks,
            collision: &collision,
            data: &data,
            player: None,
            nearest_player: None,
            others: Others::default(),
            others_grid: None,
            others_index: None,
            daylight: false,
            game_time: world.game_time,
            day_time: world.day_time,
            raining: false,
            thundering: false,
            nether: false,
            block_tags: &items.block_tags,
            biomes: &biomes,
            mob_griefing: true,
            mob_drops: true,
            difficulty: LocalDifficulty::new(2, world.day_time, 0, 0.0),
            end: false,
            world_seed: 0,
            sea_level: -63,
            teams: Default::default(),
            items: Some(&items),
            lid_openers: &lid_openers,
            fluids_ahead: None,
            kinds_ahead: None,
            memo_ahead: None,
            blocks_ahead: None,
        };
        let nearest_to = |_: Vec3| Some(Vec3::new(0.0, ground, 0.0));
        let ticking = |_: i32, _: i32| true;
        let difficulty = |_: [f64; 3]| LocalDifficulty::new(2, 18_000, 0, 0.0);
        let mut random = LegacyRandom::new(42);
        let mut totals = Stats::default();
        for tick in 0..80 {
            let reads: Vec<MobAhead> = mobs.iter().map(|_| MobAhead::default()).collect();
            let cx = LoopCx {
                base: &base,
                reads: &reads,
                uuid: None,
                nearest_to: &nearest_to,
                ticking: &ticking,
                difficulty: &difficulty,
                guests: None,
            };
            let snapshot = mobs.iter().map(|m| m.seen(None)).collect();
            let start = MobLoop::new(
                std::mem::take(&mut mobs),
                snapshot,
                HashSet::new(),
                FxHashMap::default(),
                random.clone(),
            );
            let mut serial = start.clone();
            serial.run(&cx, Mode::Serial);
            let mut speculative = start;
            let stats = speculative.run(&cx, Mode::Speculative);
            if let Some(diff) = speculative.differs(&serial) {
                panic!("tick {tick}: {diff}");
            }
            totals.speculated += stats.speculated;
            totals.reran += stats.reran;
            // What the game does after the loop, for the next tick.
            let MobLoop {
                mobs: mut after,
                keep,
                mut pushes,
                random: next,
                ..
            } = speculative;
            let mut flags = keep.into_iter();
            after.retain(|_| flags.next().unwrap_or(true));
            for m in &mut after {
                super::super::apply_mob_pushes(m, &mut pushes);
            }
            mobs = after;
            random = next;
        }
        assert!(totals.reran > 0, "{totals:?}");
        assert!(totals.reran < totals.speculated, "{totals:?}");
        assert!(mobs.iter().any(|m| m.species() == Species::Parrot));
    }
}
