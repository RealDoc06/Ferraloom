//! What mobs read from the level: block solidity, ray clipping for line of
//! sight (`ClipContext.Block.COLLIDER`), sky visibility, light-dependent
//! values and the positions of other entities.
use super::{MobWorld, Seen, Who, data, tick_loop};
use crate::{
    blocks::{BlockFacts, Fluid},
    collisions::SceneCollisions,
};
use horizon_core::{
    geometry::{Aabb, Vec3, traverse_blocks},
    position::BlockPos,
};
use horizon_world::{
    block_collision::{BlockCollisionRegistry, DynamicCollisionWorld},
    registry::BlockStateId,
};

/// The other mobs a mob's tick sees ([`MobWorld::others`]). Reading them
/// all goes through here, so a speculative tick knows it did (`tick_loop`);
/// [`MobWorld::others_near`] and [`MobWorld::seen`] read some.
#[derive(Clone, Copy, Default)]
pub struct Others<'a>(&'a [Seen]);
impl<'a> Others<'a> {
    pub fn new(seen: &'a [Seen]) -> Self {
        Self(seen)
    }
    /// Every one of them, in snapshot order.
    pub fn iter(self) -> std::slice::Iter<'a, Seen> {
        tick_loop::note(tick_loop::Reads::all);
        self.0.iter()
    }
    pub fn len(self) -> usize {
        self.0.len()
    }
    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }
}
impl<'a> IntoIterator for Others<'a> {
    type Item = &'a Seen;
    type IntoIter = std::slice::Iter<'a, Seen>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Where a target entity is this tick.
#[derive(Clone, Copy, Debug)]
pub struct TargetInfo {
    pub position: Vec3,
    pub eye_y: f64,
    pub bounds: Aabb,
    pub alive: bool,
    pub can_be_seen_as_enemy: bool,
    pub is_player: bool,
    pub creative: bool,
    /// `isPassenger`.
    pub passenger: bool,
}
impl MobWorld<'_> {
    /// The biome's base temperature at a position.
    pub fn temperature(&self, x: i32, y: i32, z: i32) -> f32 {
        self.world
            .chunks
            .get(&(x >> 4, z >> 4))
            .map_or(0.8, |c| self.biomes.get(c.biome(x, y, z)).temperature)
    }
    /// `Heightmap.Types.MOTION_BLOCKING`: above the highest blocking block.
    pub fn motion_blocking_height(&self, x: i32, z: i32) -> i32 {
        let top = crate::chunk::MIN_Y + crate::chunk::HEIGHT;
        (crate::chunk::MIN_Y..top)
            .rev()
            .find(|&y| {
                let f = self.facts.get(self.world.block(x, y, z));
                f.full_collision || f.fluid != crate::blocks::Fluid::None
            })
            .map_or(crate::chunk::MIN_Y, |y| y + 1)
    }
    /// BlockState.isSolid (the legacy solid flag).
    pub fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        self.data.state(self.world.block(x, y, z)).flags & data::SOLID != 0
    }
    pub fn fluid(&self, x: i32, y: i32, z: i32) -> Fluid {
        self.facts.get(self.world.block(x, y, z)).fluid
    }
    pub fn block_name(&self, x: i32, y: i32, z: i32) -> &str {
        &self.facts.get(self.world.block(x, y, z)).name
    }
    /// A block state property's value.
    pub fn prop(&self, x: i32, y: i32, z: i32, key: &str) -> Option<&str> {
        self.registry
            .state(self.world.block(x, y, z))
            .and_then(|s| s.properties.get(key))
            .map(String::as_str)
    }
    /// Whether `block` is in the block tag `tag`. While [`keep_tag_answers`] holds,
    /// the answers are kept by the strings' contents: the tag table hashes
    /// both with SipHash, and the mob ticks ask the same few many times.
    pub fn has_tag(&self, tag: &str, block: &str) -> bool {
        let ask = || self.block_tags.get(tag).is_some_and(|s| s.contains(block));
        TAGS.with(|cache| {
            let mut cache = cache.borrow_mut();
            let Some(answers) = cache.as_mut() else {
                return ask();
            };
            if let Some(&known) = answers.get(tag).and_then(|blocks| blocks.get(block)) {
                return known;
            }
            let known = ask();
            answers
                .entry(tag.into())
                .or_default()
                .insert(block.into(), known);
            known
        })
    }
    /// `findBlocksInBoxByManhattanDistance(at, range_xz, range_y).filterState(..).findFirst()`:
    /// the first block `matches` accepts (given its position and name), going
    /// out from `at` by Manhattan distance as `BlockPos.withinBoxByManhattanDistance` does.
    pub fn nearest_block(
        &self,
        at: [i32; 3],
        range_xz: i32,
        range_y: i32,
        matches: impl Fn([i32; 3], &str) -> bool,
    ) -> Option<[i32; 3]> {
        // Visiting order: distance, then x, then y, then +z before -z.
        let mut best: Option<((i32, i32, i32, bool), [i32; 3])> = None;
        for dx in -range_xz..=range_xz {
            for dy in -range_y..=range_y {
                for dz in -range_xz..=range_xz {
                    let key = (dx.abs() + dy.abs() + dz.abs(), dx, dy, dz < 0);
                    if best.is_some_and(|(k, _)| k <= key) {
                        continue;
                    }
                    let p = [at[0] + dx, at[1] + dy, at[2] + dz];
                    let state = self.world.block(p[0], p[1], p[2]);
                    if !self.facts.is_air(state) && matches(p, &self.facts.get(state).name) {
                        best = Some((key, p));
                    }
                }
            }
        }
        best.map(|(_, p)| p)
    }
    /// Level.clip with COLLIDER shapes (optionally stopping at any fluid): true on a miss.
    pub fn clear_between(&self, from: Vec3, to: Vec3, fluids_block: bool) -> bool {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.collision,
            subject: None,
        };
        let result = traverse_blocks(from, to, 512, |p: BlockPos| {
            let state = self.world.block(p.x, p.y, p.z);
            if fluids_block && self.facts.get(state).fluid != Fluid::None {
                return Some(());
            }
            if self.facts.is_air(state) {
                return None;
            }
            let shape = self
                .collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?;
            shape.clip(from, to, p).map(|_| ())
        });
        matches!(result, Ok(None))
    }
    /// Level.clip with VISUAL shapes and no fluids: true on a miss.
    pub fn visual_clear_between(&self, from: Vec3, to: Vec3) -> bool {
        self.visual_clip(from, to) == Some(None)
    }
    /// Level.clip with VISUAL shapes and no fluids: the block the segment
    /// first hits, if it does.
    pub fn visual_hit(&self, from: Vec3, to: Vec3) -> Option<[i32; 3]> {
        self.visual_clip(from, to).flatten()
    }
    fn visual_clip(&self, from: Vec3, to: Vec3) -> Option<Option<[i32; 3]>> {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.collision,
            subject: None,
        };
        let state = |x, y, z| self.world.block(x, y, z);
        visual_clip(self.facts, self.collision, &mut scene, &state, from, to)
    }
    /// Level.clip with COLLIDER shapes and no fluids: where the segment
    /// first hits a block, if it does.
    pub fn clip_block(&self, from: Vec3, to: Vec3) -> Option<Vec3> {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.collision,
            subject: None,
        };
        let result = traverse_blocks(from, to, 512, |p: BlockPos| {
            let state = self.world.block(p.x, p.y, p.z);
            if self.facts.is_air(state) {
                return None;
            }
            let shape = self
                .collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?;
            shape.clip(from, to, p).map(|hit| hit.location)
        });
        result.ok().flatten()
    }
    /// Level.clip with COLLIDER shapes and no fluids: the block the segment
    /// first hits, if it does.
    pub fn clip_block_at(&self, from: Vec3, to: Vec3) -> Option<[i32; 3]> {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.collision,
            subject: None,
        };
        let result = traverse_blocks(from, to, 512, |p: BlockPos| {
            let state = self.world.block(p.x, p.y, p.z);
            if self.facts.is_air(state) {
                return None;
            }
            let shape = self
                .collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?;
            shape.clip(from, to, p).map(|_| [p.x, p.y, p.z])
        });
        result.ok().flatten()
    }
    /// The bounds of the block's collision shape, relative to the block.
    pub fn collision_bounds(&self, p: [i32; 3]) -> Option<horizon_core::geometry::Aabb> {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.collision,
            subject: None,
        };
        let state = self.world.block(p[0], p[1], p[2]);
        self.collision
            .collision_shape(&mut scene, state, p)
            .ok()?
            .bounds()
    }
    /// LivingEntity.hasLineOfSight: eye to eye within 128 blocks.
    pub fn line_of_sight(&self, from_eye: Vec3, to: Vec3) -> bool {
        let d =
            (to.x - from_eye.x).powi(2) + (to.y - from_eye.y).powi(2) + (to.z - from_eye.z).powi(2);
        if d > 128.0 * 128.0 {
            return false;
        }
        self.clear_between(from_eye, to, false)
    }
    /// Level.canSeeSky: sky light 15 at the position (no roof above).
    pub fn can_see_sky(&self, x: i32, y: i32, z: i32) -> bool {
        if self.nether {
            return false;
        }
        self.light_ahead(x, y, z) >> 4 == 15
    }
    /// Level.precipitationAt: raining, open to the sky, nothing motion
    /// blocking above, then the biome's precipitation at the position:
    /// `Some(true)` rain, `Some(false)` snow, `None` nothing.
    pub fn precipitation_at(&self, x: i32, y: i32, z: i32) -> Option<bool> {
        if !self.raining || !self.can_see_sky(x, y, z) || self.motion_blocking_height(x, z) > y {
            return None;
        }
        let chunk = self.world.chunks.get(&(x >> 4, z >> 4))?;
        let biome = self.biomes.get(chunk.biome(x, y, z));
        if !biome.precipitation {
            return None;
        }
        Some(biome.temperature_at(BlockPos::new(x, y, z), self.sea_level) >= 0.15)
    }
    /// Level.isRainingAt: the precipitation at the position is rain, not snow
    /// or none.
    pub fn raining_at(&self, x: i32, y: i32, z: i32) -> bool {
        self.precipitation_at(x, y, z) == Some(true)
    }
    /// Level.getLightLevelDependentMagicValue: the dimension's brightness curve
    /// at the raw (sky darkened) light level.
    pub fn brightness(&self, x: i32, y: i32, z: i32) -> f32 {
        let light = self.light_ahead(x, y, z);
        let sky = i32::from(light >> 4) - self.sky_darken();
        let level = sky.max(i32::from(light & 15)).clamp(0, 15) as f32;
        // DimensionType.getBrightness with ambient light 0 (overworld) / 0.1 (nether).
        let f = level / 15.0;
        let base = f / (4.0 - 3.0 * f);
        if self.nether { 0.1 + base * 0.9 } else { base }
    }
    pub fn sky_darken(&self) -> i32 {
        if self.daylight { 0 } else { 11 }
    }
    pub fn target_info(&self, who: Who) -> Option<TargetInfo> {
        match who {
            Who::Player => self.player.as_ref().map(|p| TargetInfo {
                position: p.position,
                eye_y: p.eye[1],
                bounds: p.bounds,
                alive: p.alive,
                can_be_seen_as_enemy: true,
                is_player: true,
                creative: p.creative,
                passenger: p.passenger,
            }),
            Who::Mob(id) => self.seen(id).map(|s| TargetInfo {
                position: s.position,
                eye_y: s.eye_y,
                bounds: s.bounds,
                alive: s.alive,
                can_be_seen_as_enemy: !s.playing_dead,
                is_player: false,
                creative: false,
                passenger: s.passenger,
            }),
        }
    }
    /// The block at `x, y, z`: from [`MobWorld::blocks_ahead`] when it was
    /// read ahead there (the blocks hold still while the mobs tick), else
    /// from the world.
    pub fn block_ahead(&self, x: i32, y: i32, z: i32) -> BlockStateId {
        match &self.blocks_ahead {
            Some(read) if read.eye == [x, y, z] => read.eye_block,
            Some(read) if read.below == [x, y, z] => read.below_block,
            _ => self.world.block(x, y, z),
        }
    }
    /// The packed light at `x, y, z` ([`crate::world::World::light`]), from
    /// [`MobWorld::blocks_ahead`] when it was read ahead there.
    pub fn light_ahead(&self, x: i32, y: i32, z: i32) -> u8 {
        match &self.blocks_ahead {
            Some(read) if read.eye == [x, y, z] => read.eye_light,
            _ => self.world.light(x, y, z),
        }
    }
    /// The other mobs whose boxes may meet `area` (callers test the boxes),
    /// in snapshot order: through the grid when there is one, else all.
    pub fn others_near(&self, area: Aabb) -> impl Iterator<Item = &Seen> {
        let mut near = NEAR.take();
        let indexed = self
            .others_grid
            .is_some_and(|grid| grid.candidates(area, &mut near));
        if !indexed {
            near.clear();
        }
        tick_loop::note(|r| {
            r.query(
                indexed
                    .then(|| self.others_grid.and_then(|g| g.columns_for(area)))
                    .flatten(),
            )
        });
        Near {
            list: near,
            at: 0,
            others: self.others.0,
            all: (!indexed).then(|| self.others.0.iter()),
        }
    }
    /// Whether `others` may hold a mob of one of `species`: false only when
    /// its grid keeps their species and none is among them, so a search
    /// for them finds nothing.
    pub fn others_may_hold(&self, species: &[super::Species]) -> bool {
        self.others_grid.is_none_or(|grid| {
            tick_loop::note(tick_loop::Reads::species);
            grid.may_hold(species)
        })
    }
    pub fn seen(&self, id: u32) -> Option<&Seen> {
        match self.others_index {
            Some(index) => {
                let slot = *index.get(&id)?;
                tick_loop::note(|r| r.slot(slot));
                self.others.0.get(slot as usize).filter(|s| s.id == id)
            }
            None => self.others.iter().find(|s| s.id == id),
        }
    }
    /// Living mobs of the given species within `range` of `from`, nearest first.
    pub fn nearby<'s>(
        &'s self,
        from: Vec3,
        range: f64,
        pred: impl Fn(&Seen) -> bool + 's,
    ) -> impl Iterator<Item = &'s Seen> + 's {
        // A position within `range` puts the box's bottom face in this area
        // (one block wider, as the box test is strict).
        let r = range + 1.0;
        let area = Aabb::new(
            Vec3::new(from.x - r, from.y - r, from.z - r),
            Vec3::new(from.x + r, from.y + r, from.z + r),
        );
        let distance = |s: &Seen| {
            (s.position.x - from.x).powi(2)
                + (s.position.y - from.y).powi(2)
                + (s.position.z - from.z).powi(2)
        };
        let found: Vec<(f64, &Seen)> = self
            .others_near(area)
            .filter(|s| {
                s.alive
                    && (s.position.x - from.x).abs() <= range
                    && (s.position.y - from.y).abs() <= range
                    && (s.position.z - from.z).abs() <= range
            })
            .filter(move |s| pred(s))
            .map(|s| (distance(s), s))
            .collect();
        Nearest::new(found)
    }
}

/// `nearby`'s answer: its candidates nearest first, as a stable sort by
/// distance (`total_cmp`) orders them. The first is found by one scan, and
/// the rest are sorted only if asked for: most callers take one.
struct Nearest<'a> {
    found: Vec<(f64, &'a Seen)>,
    /// The candidates left, in the order they are given out; None until
    /// the second is asked for.
    sorted: Option<std::vec::IntoIter<(f64, &'a Seen)>>,
    first: bool,
}
impl<'a> Nearest<'a> {
    fn new(found: Vec<(f64, &'a Seen)>) -> Self {
        Self {
            found,
            sorted: None,
            first: true,
        }
    }
}
impl<'a> Iterator for Nearest<'a> {
    type Item = &'a Seen;
    fn next(&mut self) -> Option<&'a Seen> {
        if let Some(rest) = &mut self.sorted {
            return rest.next().map(|(_, s)| s);
        }
        if std::mem::take(&mut self.first) {
            // The first of the least distance, as the stable sort puts it.
            let mut best: Option<usize> = None;
            for (k, (d, _)) in self.found.iter().enumerate() {
                if best.is_none_or(|b| d.total_cmp(&self.found[b].0).is_lt()) {
                    best = Some(k);
                }
            }
            return best.map(|k| self.found.remove(k).1);
        }
        // The rest keep their order among equals, as the stable sort does.
        let mut rest = std::mem::take(&mut self.found);
        rest.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut rest = rest.into_iter();
        let next = rest.next().map(|(_, s)| s);
        self.sorted = Some(rest);
        next
    }
}
/// `MobWorld::has_tag`'s answers by tag then block, while kept.
type TagAnswers =
    crate::fast_hash::FxHashMap<Box<str>, crate::fast_hash::FxHashMap<Box<str>, bool>>;
thread_local! {
    static TAGS: std::cell::RefCell<Option<TagAnswers>> = const { std::cell::RefCell::new(None) };
}
/// `MobWorld::has_tag`'s answers kept on this thread, fresh, until the
/// guard drops: for a stretch in which the block tags cannot change (the
/// mob ticks).
pub fn keep_tag_answers() -> KeptTags {
    TAGS.with(|cache| *cache.borrow_mut() = Some(Default::default()));
    KeptTags(())
}
/// The guard [`keep_tag_answers`] returns.
pub struct KeptTags(());
impl Drop for KeptTags {
    fn drop(&mut self) {
        TAGS.with(|cache| *cache.borrow_mut() = None);
    }
}
thread_local! {
    /// The candidate list of the thread's last `others_near`, kept for its
    /// capacity: queries run for most mobs every tick.
    static NEAR: std::cell::Cell<Vec<u32>> = const { std::cell::Cell::new(Vec::new()) };
}
/// `others_near`'s answer: the grid's candidates, else every entry.
struct Near<'a> {
    list: Vec<u32>,
    at: usize,
    others: &'a [Seen],
    all: Option<std::slice::Iter<'a, Seen>>,
}
impl<'a> Iterator for Near<'a> {
    type Item = &'a Seen;
    fn next(&mut self) -> Option<&'a Seen> {
        if let Some(&i) = self.list.get(self.at) {
            self.at += 1;
            return Some(&self.others[i as usize]);
        }
        self.all.as_mut()?.next()
    }
}
impl Drop for Near<'_> {
    fn drop(&mut self) {
        let mut list = std::mem::take(&mut self.list);
        list.clear();
        NEAR.set(list);
    }
}
/// Level.clip with VISUAL shapes and no fluids: the block the segment first
/// hits, None inside when it hits nothing and outside when the segment is
/// too long to walk. Glass and copper grates (`TransparentBlock`) and bars and panes
/// (`IronBarsBlock`) have no visual shape; mud, soul sand and powder snow
/// look like full blocks.
pub fn visual_clip<W: DynamicCollisionWorld>(
    facts: &BlockFacts,
    collision: &BlockCollisionRegistry,
    scene: &mut W,
    state: &dyn Fn(i32, i32, i32) -> BlockStateId,
    from: Vec3,
    to: Vec3,
) -> Option<Option<[i32; 3]>> {
    let full = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
    let result = traverse_blocks(from, to, 512, |p: BlockPos| {
        let id = state(p.x, p.y, p.z);
        if facts.is_air(id) {
            return None;
        }
        let name = facts.get(id).name.trim_start_matches("minecraft:");
        if name.ends_with("glass")
            || name.ends_with("glass_pane")
            || name.ends_with("_bars")
            || name.ends_with("copper_grate")
        {
            return None;
        }
        if matches!(name, "mud" | "soul_sand" | "powder_snow") {
            return full
                .move_by(Vec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z)))
                .clip(from, to)
                .map(|_| [p.x, p.y, p.z]);
        }
        let shape = collision.collision_shape(scene, id, [p.x, p.y, p.z]).ok()?;
        shape.clip(from, to, p).map(|_| [p.x, p.y, p.z])
    });
    result.ok()
}
