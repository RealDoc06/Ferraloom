//! The game's side of `NaturalSpawner` (`mobs::spawn_selection`): the level
//! it reads, the spawning chunks and caps of `ServerChunkCache.tickChunks`,
//! and the mobs it adds (finalized with the game's `finalizeSpawn`).
use super::{Dim, Game};
use horizon_client::{
    biome::{BiomeInfo, SpawnEntry},
    blocks::BlockFacts,
    chunk::Chunk,
    chunk_map::ChunkMap,
    collisions::{SceneCollisions, no_collision_empty},
    fast_hash::FxHashMap,
    mobs::{
        Mob, MobWorld, Species,
        data::{self, MobData},
        player_index::PlayerIndex,
        seen_grid::SeenGrid,
        spawn_rules::{Dimension, SpawnLevel},
        spawn_selection::{
            self, Category, CountedCharge, Heightmap, MobType, NaturalLevel, SpawnCost,
            SpawnRequest, SpawnState, Spawned, StructureAt, StructureSpawns, counted_by_state,
        },
    },
};
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::{LegacyRandom, RandomSource, WorldgenRandom},
};
use horizon_world::registry::BlockStateId;
use std::{cell::Cell, collections::HashMap, ops::ControlFlow};

/// A player in the level in place, as spawning and despawning see it.
#[derive(Clone, Copy)]
pub(super) struct LevelPlayer {
    pub(super) position: Vec3,
    pub(super) bounds: Aabb,
    pub(super) alive: bool,
    pub(super) creative: bool,
    pub(super) spectator: bool,
}
impl LevelPlayer {
    fn distance_sqr(&self, x: f64, y: f64, z: f64) -> f64 {
        (self.position.x - x).powi(2)
            + (self.position.y - y).powi(2)
            + (self.position.z - z).powi(2)
    }
}
/// The non-spectators, in level order: who `EntityGetter.getNearestPlayer(x,
/// y, z, -1, false)` (alive or not) and `ChunkMap.getPlayersCloseForSpawning`
/// read.
pub(super) fn non_spectators(players: &[LevelPlayer]) -> PlayerIndex {
    PlayerIndex::new(
        players
            .iter()
            .filter(|p| !p.spectator)
            .map(|p| [p.position.x, p.position.y, p.position.z]),
    )
}

/// The chunks collected so far by `spawning_chunks`, as a row of bits per
/// chunk column in regions of 64 by 64. A player's window is 17 rows, most of
/// them already marked by the players around them.
#[derive(Default)]
struct ChunkBits {
    regions: horizon_client::fast_hash::FxHashMap<(i32, i32), Box<[u64; 64]>>,
}
impl ChunkBits {
    /// Marks 17 chunks of column `x` from `z` up, and adds those new to the
    /// set to `out` in that order.
    fn mark_column(&mut self, x: i32, z: i32, out: &mut Vec<(i32, i32)>) {
        let (mut z, end) = (z, z + 17);
        while z < end {
            let bit = z & 63;
            let len = (end - z).min(64 - bit);
            let mask = ((1u64 << len) - 1) << bit;
            let region = self
                .regions
                .entry((x >> 6, z >> 6))
                .or_insert_with(|| Box::new([0; 64]));
            let word = &mut region[(x & 63) as usize];
            let mut new = mask & !*word;
            *word |= mask;
            while new != 0 {
                out.push((x, (z & !63) + new.trailing_zeros() as i32));
                new &= new - 1;
            }
            z += len;
        }
    }
}

/// The column the last read landed in: a spawn attempt reads one column many
/// times, and the map only changes when the level's borrow of the game ends.
#[derive(Clone, Copy)]
struct LastColumn<'g> {
    key: (i32, i32),
    chunk: Option<&'g Chunk>,
    /// The categories the structures reaching it may change the lists of
    /// ([`StructureSpawns::category_bits`]), once asked for.
    override_bits: Option<u8>,
}
#[derive(Default)]
struct ColumnMemo<'g>(Cell<Option<LastColumn<'g>>>);
impl<'g> ColumnMemo<'g> {
    /// What `chunks.get` gives for the column holding block `x`, `z`.
    fn get(&self, chunks: &'g ChunkMap, x: i32, z: i32) -> LastColumn<'g> {
        let key = (x >> 4, z >> 4);
        match self.0.get() {
            Some(last) if last.key == key => last,
            _ => {
                let last = LastColumn {
                    key,
                    chunk: chunks.get(&key).map(|c| &**c),
                    override_bits: None,
                };
                self.0.set(Some(last));
                last
            }
        }
    }
    /// Makes `chunk` (what `chunks.get` gives for `key`) the last column.
    fn enter(&self, key: (i32, i32), chunk: Option<&'g Chunk>) {
        self.0.set(Some(LastColumn {
            key,
            chunk,
            override_bits: None,
        }));
    }
    /// The categories whose lists the structures reaching the column may change.
    fn override_bits(&self, chunks: &'g ChunkMap, spawns: &StructureSpawns, x: i32, z: i32) -> u8 {
        let mut last = self.get(chunks, x, z);
        if let Some(bits) = last.override_bits {
            return bits;
        }
        let bits = last.chunk.map_or(0, |chunk| {
            chunk
                .structure_refs
                .iter()
                .fold(0, |bits, (id, _)| bits | spawns.category_bits(id))
        });
        last.override_bits = Some(bits);
        self.0.set(Some(last));
        bits
    }
}

/// A biome's answers to the questions every attempt asks, by category index.
struct BiomeRow<'g> {
    mobs: [&'g [SpawnEntry]; Category::ALL.len()],
    reduces_water_ambient: bool,
}
/// A biome's list for every category, by category index.
fn mobs_by_category(
    spawns: &HashMap<String, Vec<SpawnEntry>>,
) -> [&[SpawnEntry]; Category::ALL.len()] {
    Category::ALL.map(|c| spawns.get(c.name()).map_or(&[][..], Vec::as_slice))
}
impl<'g> BiomeRow<'g> {
    fn new(biome: &'g BiomeInfo, data: &MobData) -> Self {
        Self {
            mobs: mobs_by_category(&biome.spawns),
            reduces_water_ambient: data
                .biome_tags
                .get("minecraft:reduce_water_ambient_spawns")
                .is_some_and(|members| members.contains(&biome.name)),
        }
    }
}

/// The game level as natural spawning reads it.
pub(super) struct GameLevel<'g, 'a> {
    pub(super) game: &'g Game<'a>,
    pub(super) w: MobWorld<'g>,
    column: ColumnMemo<'g>,
    /// The column the pass spawns in next, read without leaving the last
    /// column for it (what `chunks.get` gives for the key).
    ahead: Cell<Option<((i32, i32), Option<&'g Chunk>)>>,
    /// Every biome's [`BiomeRow`], in the biome list's order, for this pass.
    biome_rows: std::cell::OnceCell<Vec<BiomeRow<'g>>>,
    /// The generator's structure starts by owning chunk, once asked, for
    /// this pass (each look-up takes two locks and the generator's cache).
    starts: std::cell::RefCell<FxHashMap<(i32, i32), Starts>>,
    locator:
        std::cell::OnceCell<Option<std::sync::Arc<horizon_client::vanillagen::VanillaTerrain>>>,
    /// Entity types as [`NaturalLevel::mob_type`] answers, for this pass.
    mob_types: std::cell::RefCell<FxHashMap<String, Option<MobType>>>,
    /// Natural spawning snapshots players once for its pass. Other spawning
    /// checks read the immutably borrowed level without collecting a list.
    players: Option<Vec<LevelPlayer>>,
    /// Boxes of the mobs this pass created, not yet in the game's list.
    pub(super) pending: std::cell::RefCell<Vec<Aabb>>,
    /// The types and boxes of what a spawner created this tick.
    pub(super) pending_ids: std::cell::RefCell<Vec<(String, Aabb)>>,
    /// The game's mobs by box, built once a pass has asked more than
    /// [`SCANNED_QUERIES`] box queries (the mobs do not change while it
    /// holds the game); the first scans, as the build costs about a scan.
    pub(super) boxes: std::cell::OnceCell<MobBoxes>,
    pub(super) box_queries: std::cell::Cell<u32>,
}
const SCANNED_QUERIES: u32 = 1;
type Starts =
    std::rc::Rc<[std::sync::Arc<horizon_world::worldgen::structure::start::StructureStart>]>;
/// How many chunks or mobs a pool thread takes at a time in this pass's reads.
const PASS_GRAIN: usize = 512;
/// A mob not removed: its index in `Game::mobs`, box, alive, and whether
/// it is a living shulker.
type MobBox = (usize, Aabb, bool, bool);
fn mob_box(i: usize, m: &Mob) -> MobBox {
    let alive = m.alive();
    (
        i,
        m.bounds(),
        alive,
        alive && m.species() == Species::Shulker,
    )
}
/// The boxes of the game's mobs not removed, and a grid over them.
pub(super) struct MobBoxes {
    entries: Vec<MobBox>,
    grid: SeenGrid,
}
impl MobBoxes {
    fn new(mobs: &[Mob]) -> Self {
        let entries: Vec<_> = mobs
            .iter()
            .enumerate()
            .filter(|(_, m)| !m.removed)
            .map(|(i, m)| mob_box(i, m))
            .collect();
        let grid = SeenGrid::new(entries.iter().map(|e| e.1));
        Self { entries, grid }
    }
    /// The entries whose boxes meet `area` (and possibly a few more).
    fn near(&self, area: Aabb) -> impl Iterator<Item = MobBox> + '_ {
        let mut near = Vec::new();
        let indexed = self.grid.candidates(area, &mut near);
        let all = (!indexed).then(|| self.entries.iter());
        near.into_iter()
            .map(|i| self.entries[i as usize])
            .chain(all.into_iter().flatten().copied())
    }
}
impl<'g> GameLevel<'g, '_> {
    pub(super) fn players(&self) -> impl Iterator<Item = LevelPlayer> + '_ {
        let snapshot = self.players.iter().flatten().copied();
        let live = self
            .players
            .is_none()
            .then(|| self.game.iter_level_players());
        snapshot.chain(live.into_iter().flatten())
    }
    /// The mobs not removed whose boxes may meet `area` (callers test them).
    fn mobs_near(&self, area: Aabb) -> Box<dyn Iterator<Item = MobBox> + '_> {
        let asked = self.box_queries.get() + 1;
        self.box_queries.set(asked);
        if asked <= SCANNED_QUERIES && self.boxes.get().is_none() {
            return Box::new(
                self.game
                    .mobs
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| !m.removed)
                    .map(|(i, m)| mob_box(i, m)),
            );
        }
        Box::new(
            self.boxes
                .get_or_init(|| MobBoxes::new(&self.game.mobs))
                .near(area),
        )
    }
    fn chunk(&self, x: i32, z: i32) -> Option<&'g Chunk> {
        let key = (x >> 4, z >> 4);
        match (self.column.0.get(), self.ahead.get()) {
            (Some(last), _) if last.key == key => last.chunk,
            (_, Some((at, chunk))) if at == key => chunk,
            _ => self.column.get(&self.w.world.chunks, x, z).chunk,
        }
    }
    fn biome(&self, x: i32, y: i32, z: i32) -> Option<&BiomeInfo> {
        let chunk = self.chunk(x, z)?;
        Some(self.game.res.biomes().get(chunk.biome(x, y, z)))
    }
    /// `Level.getRawBrightness`'s packed sky and block light at a block.
    fn light(&self, x: i32, y: i32, z: i32) -> u8 {
        self.chunk(x, z)
            .map_or(0xF0, |chunk| chunk.light(x & 15, y, z & 15))
    }
    fn biome_row(&self, x: i32, y: i32, z: i32) -> Option<&BiomeRow<'g>> {
        let chunk = self.chunk(x, z)?;
        let rows = self.biome_rows.get_or_init(|| {
            let biomes: &'g _ = self.w.biomes;
            biomes
                .list
                .iter()
                .map(|b| BiomeRow::new(b, self.w.data))
                .collect()
        });
        // Biomes::get's clamp.
        rows.get(usize::from(chunk.biome(x, y, z)).min(rows.len() - 1))
    }
}
impl SpawnLevel for GameLevel<'_, '_> {
    fn state(&self, x: i32, y: i32, z: i32) -> BlockStateId {
        self.chunk(x, z)
            .map_or(self.w.world.air, |chunk| chunk.block(x & 15, y, z & 15))
    }
    fn facts(&self) -> &BlockFacts {
        self.w.facts
    }
    fn mob_data(&self) -> &MobData {
        self.w.data
    }
    fn block_tag(&self, tag: &str, block: &str) -> bool {
        self.w.block_tag(tag, block)
    }
    fn biome_tag(&self, tag: &str, x: i32, y: i32, z: i32) -> bool {
        self.w.biome_tag(tag, x, y, z)
    }
    fn sky_light(&self, x: i32, y: i32, z: i32) -> i32 {
        if self.w.nether || self.w.end {
            return 0;
        }
        i32::from(self.light(x, y, z) >> 4)
    }
    fn block_light(&self, x: i32, y: i32, z: i32) -> i32 {
        i32::from(self.light(x, y, z) & 15)
    }
    fn sky_darken(&self) -> i32 {
        SpawnLevel::sky_darken(&self.w)
    }
    fn thundering(&self) -> bool {
        self.w.thundering
    }
    fn sea_level(&self) -> i32 {
        self.w.sea_level()
    }
    fn world_surface(&self, x: i32, z: i32) -> i32 {
        self.chunk(x, z)
            .map_or(horizon_client::chunk::MIN_Y, |chunk| {
                chunk.world_surface(x, z, |v| self.w.facts.is_air(BlockStateId(v)))
            })
    }
    fn peaceful(&self) -> bool {
        self.w.peaceful()
    }
    fn dimension(&self) -> Dimension {
        self.w.dimension()
    }
    fn surface_slime_chance(&self, x: i32, y: i32, z: i32) -> f32 {
        self.w.surface_slime_chance(x, y, z)
    }
    fn world_seed(&self) -> i64 {
        self.w.world_seed
    }
    /// `getNearestPlayer(x, y, z, distance, true)`, over every player.
    fn player_near(&self, x: f64, y: f64, z: f64, distance: f64) -> bool {
        self.players().any(|p| {
            p.alive && !p.creative && !p.spectator && p.distance_sqr(x, y, z) < distance * distance
        })
    }
}
impl NaturalLevel for GameLevel<'_, '_> {
    fn min_y(&self) -> i32 {
        horizon_client::chunk::MIN_Y
    }
    fn max_y(&self) -> i32 {
        horizon_client::chunk::MIN_Y + horizon_client::chunk::HEIGHT - 1
    }
    fn ambient_light(&self) -> f32 {
        match self.game.dim {
            Dim::Nether => 0.1,
            Dim::End => 0.25,
            _ => 0.0,
        }
    }
    fn has_ceiling(&self) -> bool {
        self.game.dim == Dim::Nether
    }
    fn mob_type(&self, entity: &str) -> Option<MobType> {
        if let Some(known) = self.mob_types.borrow().get(entity) {
            return *known;
        }
        let kind = self.game.res.mob_types().get(entity).map(|t| {
            (
                t.species,
                t.width as f32,
                t.height as f32,
                Category::from_name(&t.category),
            )
        });
        self.mob_types.borrow_mut().insert(entity.to_owned(), kind);
        kind
    }
    fn natural_mobs(&self, category: Category, x: i32, y: i32, z: i32) -> &[SpawnEntry] {
        self.biome_row(x, y, z)
            .map_or(&[], |row| row.mobs[category as usize])
    }
    fn reduces_water_ambient(&self, x: i32, y: i32, z: i32) -> bool {
        self.biome_row(x, y, z)
            .is_some_and(|row| row.reduces_water_ambient)
    }
    fn structures_may_override(&self, category: Category, x: i32, z: i32) -> bool {
        let spawns = self.structure_spawns();
        self.column
            .override_bits(&self.w.world.chunks, spawns, x, z)
            & category.bit()
            != 0
    }
    fn biome_spawns(&self, x: i32, y: i32, z: i32) -> Option<&HashMap<String, Vec<SpawnEntry>>> {
        self.biome(x, y, z).map(|b| &b.spawns)
    }
    fn spawn_cost(&self, entity: &str, x: i32, y: i32, z: i32) -> Option<SpawnCost> {
        self.biome(x, y, z)?.spawn_costs.get(entity).copied()
    }
    fn creature_probability(&self, x: i32, y: i32, z: i32) -> f32 {
        self.biome(x, y, z).map_or(0.1, |b| b.creature_probability)
    }
    fn structures_at(
        &self,
        x: i32,
        y: i32,
        z: i32,
        wanted: &dyn Fn(&str) -> bool,
    ) -> Vec<StructureAt> {
        let Some(chunk) = self.chunk(x, z).filter(|c| !c.structure_refs.is_empty()) else {
            return Vec::new();
        };
        let Some(locator) = self.locator.get_or_init(|| self.game.end.locator()) else {
            return Vec::new();
        };
        super::structures::structures_reaching_in(chunk, [x, y, z], wanted, |sx, sz| {
            let mut starts = self.starts.borrow_mut();
            let owned = starts
                .entry((sx, sz))
                .or_insert_with(|| locator.starts_at(sx, sz).into());
            Starts::clone(owned)
        })
        .into_iter()
        .map(|(id, in_piece)| StructureAt {
            id,
            in_piece,
            in_bounds: true,
        })
        .collect()
    }
    fn structure_spawns(&self) -> &StructureSpawns {
        StructureSpawns::shared(&self.game.options.resources)
    }
    fn respawn(&self) -> Option<[i32; 3]> {
        // The world spawn point lives in the overworld.
        if self.game.dim != Dim::Overworld {
            return None;
        }
        self.game.level.spawn.map(|s| {
            [
                s[0].floor() as i32,
                s[1].floor() as i32,
                s[2].floor() as i32,
            ]
        })
    }
    fn can_spawn_in_chunk(&self, cx: i32, cz: i32) -> bool {
        self.chunk(cx * 16, cz * 16).is_some()
    }
    fn no_collision(&self, bounds: Aabb) -> bool {
        let mut scene = SceneCollisions {
            world: &self.game.world,
            binding: self.w.collision,
            subject: None,
        };
        // getEntityCollisions(null, box): living shulkers are solid.
        let grown = bounds.inflate(Vec3::new(1e-7, 1e-7, 1e-7));
        no_collision_empty(&mut scene, bounds)
            && !self
                .mobs_near(grown)
                .any(|(_, b, _, shulker)| shulker && b.intersects(grown))
            && !self
                .pending_ids
                .borrow()
                .iter()
                .any(|(id, b)| id == "minecraft:shulker" && b.intersects(grown))
    }
    fn unobstructed(&self, bounds: Aabb) -> bool {
        let player = self
            .players()
            .any(|p| p.alive && !p.spectator && p.bounds.intersects(bounds));
        !player
            && !self.pending.borrow().iter().any(|b| b.intersects(bounds))
            && !self
                .mobs_near(bounds)
                .any(|(_, b, alive, _)| alive && b.intersects(bounds))
    }
    fn spawn_mobs(&self) -> bool {
        self.game.ui.rules.bool("spawn_mobs")
    }
    /// WORLD_SURFACE (every spawn attempt's) is the chunk's kept heightmap.
    /// The others are scanned down the column: sections of air alone are
    /// skipped (air counts for no heightmap). An unloaded column reads as air.
    fn heightmap(&self, kind: Heightmap, x: i32, z: i32) -> i32 {
        let world = self.w.world;
        if kind == Heightmap::WorldSurface {
            return self.world_surface(x, z);
        }
        self.chunk(x, z)
            .and_then(|chunk| {
                chunk.highest(world.air.0, |y| {
                    spawn_selection::heightmap_blocks(self, kind, x, y, z)
                })
            })
            .map_or(horizon_client::chunk::MIN_Y, |y| y + 1)
    }
    fn pathfindable_land(&self, x: i32, y: i32, z: i32) -> bool {
        self.w.data.state(self.state(x, y, z)).flags & data::LAND != 0
    }
    fn prefetch_block(&self, x: i32, y: i32, z: i32) {
        if let Some(chunk) = self.chunk(x, z) {
            chunk.prefetch_block(x, y, z);
        }
    }
}
impl horizon_client::mobs::spawner::SpawnerLevel for GameLevel<'_, '_> {
    /// `Level.hasNearbyAlivePlayer`, over every player.
    fn alive_player_near(&self, x: f64, y: f64, z: f64, range: f64) -> bool {
        if let Some(players) = &self.game.block_entity_players {
            if range < 0.0 {
                return !players.is_empty();
            }
            let reach = range * range;
            return players
                .visit_within(x, z, reach, &mut |p| {
                    if p.distance_sqr([x, y, z]) < reach {
                        ControlFlow::Break(())
                    } else {
                        ControlFlow::Continue(())
                    }
                })
                .is_break();
        }
        self.players().any(|p| {
            p.alive && !p.spectator && (range < 0.0 || p.distance_sqr(x, y, z) < range * range)
        })
    }
    fn spawner_blocks_work(&self) -> bool {
        self.game.ui.rules.bool("spawner_blocks_work")
    }
    fn entity_exists(&self, id: &str) -> bool {
        self.game.res.mob_types().contains_key(id) || self.game.entity_type_exists(id)
    }
    fn count_entities(&self, id: &str, bounds: Aabb) -> usize {
        let mobs = self
            .mobs_near(bounds)
            .filter(|&(i, b, ..)| self.game.mobs[i].kind.id == id && b.intersects(bounds))
            .count();
        let pending = self
            .pending_ids
            .borrow()
            .iter()
            .filter(|(k, b)| k == id && b.intersects(bounds))
            .count();
        mobs + pending
    }
}
impl<'a> Game<'a> {
    pub(super) fn natural_level(&self) -> GameLevel<'_, 'a> {
        self.natural_level_with_players(None)
    }
    fn natural_level_with_players(&self, players: Option<Vec<LevelPlayer>>) -> GameLevel<'_, 'a> {
        GameLevel {
            game: self,
            w: self.mob_world_base(&[]),
            column: ColumnMemo::default(),
            ahead: Cell::new(None),
            biome_rows: Default::default(),
            starts: Default::default(),
            locator: Default::default(),
            mob_types: Default::default(),
            players,
            pending: Default::default(),
            pending_ids: Default::default(),
            boxes: Default::default(),
            box_queries: Default::default(),
        }
    }
    /// Creates a mob the spawner accepted, finalized the game's way; the
    /// caller adds it with [`Game::add_spawned`].
    fn finalize_spawned(&self, r: &SpawnRequest, pending: &mut Vec<Mob>) -> Option<Spawned> {
        let kind = self.res.mob_types().get(r.entity)?.clone();
        let [x, y, z] = r.position;
        let mut mob = Mob::new(0, kind, Vec3::new(x, y, z), r.yaw);
        let facts = self.biome_facts(x.floor() as i32, y.floor() as i32, z.floor() as i32);
        let difficulty = self.local_difficulty(r.position);
        horizon_client::mobs::spawning::finalize(&mut mob, &facts, &difficulty);
        horizon_client::mobs::raider::finalize(
            &mut mob,
            horizon_client::mobs::raider::Reason::Natural,
        );
        let bounds = mob.bounds();
        pending.push(mob);
        Some(Spawned {
            bounds,
            school: true,
        })
    }
    /// Adds the mobs a spawn pass created: ids, enchanted gear and jockeys.
    fn add_spawned(&mut self, pending: Vec<Mob>, persistent: bool) {
        for mut mob in pending {
            self.next_entity += 1;
            mob.id = self.next_entity;
            mob.persistent = persistent;
            let id = mob.id;
            self.mobs.push(mob);
            self.enchant_spawn_equipment(id);
            if !persistent {
                self.spawn_jockeys(id, true);
            }
        }
    }
    /// Every player in the level in place: the local player, and the LAN
    /// guests or server players in it.
    pub(super) fn level_players(&self) -> Vec<LevelPlayer> {
        let mut players =
            Vec::with_capacity(self.lan.as_ref().map_or(0, |h| h.hub.guests.len()) + 1);
        players.extend(self.iter_level_players());
        players
    }
    /// The first player in `ServerLevel.players`, without collecting the level.
    pub(super) fn first_level_player(&self) -> Option<LevelPlayer> {
        self.iter_level_players().next()
    }
    fn iter_level_players(&self) -> impl Iterator<Item = LevelPlayer> + '_ {
        let local = self.player_view().map(|p| LevelPlayer {
            position: p.position,
            bounds: p.bounds,
            alive: p.alive,
            creative: p.creative,
            spectator: p.spectator,
        });
        let guests = self.lan.iter().flat_map(|h| {
            h.hub
                .guests
                .iter()
                .filter(|g| h.hub.here(g))
                .map(|g| LevelPlayer {
                    position: Vec3::new(g.position[0], g.position[1], g.position[2]),
                    bounds: super::lan_mobs::guest_bounds(g),
                    alive: !g.dead,
                    creative: g.creative(),
                    spectator: g.spectator(),
                })
        });
        local.into_iter().chain(guests)
    }
    /// `ChunkMap.collectSpawningChunks`: the natural spawn chunk tracker's
    /// chunks (within 8 chunks of a player, `getSpawnCandidateChunks`) that
    /// are ticking and within 128 blocks of a non-spectator. Also returns how
    /// many chunks the tracker holds (`getNaturalSpawnChunkCount`).
    /// `close` is [`non_spectators`] of `players`.
    pub(super) fn spawning_chunks(
        &self,
        players: &[LevelPlayer],
        close: &PlayerIndex,
    ) -> (i32, Vec<(i32, i32)>) {
        let mut candidates: Vec<(i32, i32)> = Vec::new();
        let mut seen = ChunkBits::default();
        for p in players {
            let (pcx, pcz) = (
                p.position.x.floor() as i32 >> 4,
                p.position.z.floor() as i32 >> 4,
            );
            for x in pcx - 8..=pcx + 8 {
                seen.mark_column(x, pcz - 8, &mut candidates);
            }
        }
        let count = candidates.len() as i32;
        let levels = self.tickets.levels();
        let chunks = &self.world.chunks;
        let mut kept: Vec<((i32, i32), bool)> = candidates.iter().map(|&c| (c, false)).collect();
        horizon_threads::pool::for_each_mut(&mut kept, PASS_GRAIN, |((cx, cz), keep)| {
            *keep = levels.loaded_ticking(*cx, *cz)
                && super::ticking_column(chunks, *cx, *cz)
                && close.any_close_for_spawning((*cx, *cz));
        });
        candidates.clear();
        candidates.extend(kept.iter().filter(|(_, keep)| *keep).map(|(c, _)| *c));
        (count, candidates)
    }
    /// `tickSpawningChunk`'s inhabited time.
    #[inline(never)]
    fn tick_inhabited(&mut self, chunks: &[(i32, i32)]) {
        for &key in chunks {
            *self.world.inhabited.entry(key).or_insert(0) += 1;
        }
    }
    /// What `createState` counts of the loaded mobs, in their order. Mobs are
    /// read here (they cannot be shared with the pool); the columns and biomes
    /// each of them needs are read on the pool, whichever thread takes which.
    fn counted_mobs(&self) -> impl Iterator<Item = CountedCharge> {
        let (chunks, biomes) = (&self.world.chunks, self.res.biomes());
        let mut rows: Vec<(&str, Category, [i32; 3], Option<CountedCharge>)> = self
            .mobs
            .iter()
            .filter_map(|m| {
                let category = Category::from_name(&m.kind.category);
                let persistent =
                    m.persistent || horizon_client::mobs::ai::requires_custom_persistence(m);
                counted_by_state(category, true, persistent).then(|| {
                    let block = [
                        m.position.x.floor() as i32,
                        m.position.y.floor() as i32,
                        m.position.z.floor() as i32,
                    ];
                    (m.kind.id.as_str(), category, block, None)
                })
            })
            .collect();
        horizon_threads::pool::for_each_mut(&mut rows, PASS_GRAIN, |(id, category, block, out)| {
            let [x, y, z] = *block;
            if let Some(chunk) = chunks.get(&(x >> 4, z >> 4)) {
                *out = Some(CountedCharge {
                    block: *block,
                    category: *category,
                    is_mob: true,
                    charge: biomes
                        .get(chunk.biome(x, y, z))
                        .spawn_costs
                        .get(*id)
                        .map(|c| c.charge),
                });
            }
        });
        rows.into_iter().filter_map(|row| row.3)
    }
    /// `ServerChunkCache.tickChunks`: the spawn state over the loaded mobs,
    /// the categories under their global caps (creatures every 400 ticks),
    /// then `tickSpawningChunk` in the shuffled spawning chunks of every
    /// player: inhabited time, then `spawnForChunk`.
    #[inline(never)]
    pub(super) fn spawn_natural(&mut self) {
        let players = self.level_players();
        if players.is_empty() {
            return;
        }
        let close = non_spectators(&players);
        let (spawnable, mut chunks) = horizon_client::spans::time("spawning_chunks", || {
            self.spawning_chunks(&players, &close)
        });
        self.tick_inhabited(&chunks);
        let spawn_mobs = self.ui.rules.bool("spawn_mobs");
        let spawn_enemies = self.ui.rules.bool("spawn_monsters");
        let mut random = std::mem::replace(&mut self.world.random, LegacyRandom::new(0));
        let mut pending = Vec::new();
        {
            let level = self.natural_level_with_players(Some(players));
            let mut state = SpawnState::from_counted(spawnable, close, self.counted_mobs());
            let categories = if spawn_mobs {
                state.filtered_categories(spawn_enemies, self.world.game_time % 400 == 0)
            } else {
                Vec::new()
            };
            // Util.shuffle.
            for i in (1..chunks.len()).rev() {
                let j = random.next_int_bound(i as i32 + 1).unwrap_or(0) as usize;
                chunks.swap(i, j);
            }
            if !categories.is_empty() {
                let _span = horizon_client::spans::scope("spawn_chunks");
                state.prepare_local(&chunks, &categories);
                let columns = self.spawning_columns(&chunks);
                for (i, &chunk) in chunks.iter().enumerate() {
                    // Hints only: the coming columns' headers, surfaces and
                    // section headers load while this one spawns, ready for
                    // the next column's foreseen starts.
                    if let Some(Some(next)) = columns.get(i + 4) {
                        next.prefetch_header();
                    }
                    if let Some(Some(next)) = columns.get(i + 3) {
                        next.prefetch_surface();
                    }
                    if let Some(Some(next)) = columns.get(i + 2) {
                        next.prefetch_sections();
                    }
                    level.column.enter(chunk, columns[i]);
                    level
                        .ahead
                        .set(columns.get(i + 1).map(|&next| (chunks[i + 1], next)));
                    let mut spawn = |l: &GameLevel, r: &SpawnRequest| {
                        let spawned = l.game.finalize_spawned(r, &mut pending);
                        if let Some(s) = &spawned {
                            l.pending.borrow_mut().push(s.bounds);
                        }
                        spawned
                    };
                    spawn_selection::spawn_for_chunk(
                        &level,
                        &mut random,
                        &mut state,
                        &categories,
                        chunk,
                        &mut spawn,
                        &mut |_| {},
                    );
                }
            }
        }
        self.world.random = random;
        self.add_spawned(pending, false);
    }
    /// The columns of `chunks`, in their order, looked up on the pool with
    /// their WORLD_SURFACE heightmaps built (the map and the blocks do not
    /// change while a pass holds the game, and the heightmap is a cache).
    fn spawning_columns(&self, chunks: &[(i32, i32)]) -> Vec<Option<&Chunk>> {
        let (map, facts) = (&self.world.chunks, self.res.facts());
        let mut columns: Vec<_> = chunks.iter().map(|&key| (key, None)).collect();
        horizon_threads::pool::for_each_mut(&mut columns, PASS_GRAIN, |(key, column)| {
            *column = map.get(key).map(|c| &**c);
            if let Some(chunk) = column {
                chunk.world_surface(0, 0, |v| facts.is_air(BlockStateId(v)));
            }
        });
        columns.into_iter().map(|(_, column)| column).collect()
    }
    /// `NoiseBasedChunkGenerator.spawnOriginalMobs`: creature packs for a
    /// freshly generated chunk, from the biome at its corner column's top,
    /// with the chunk's decoration-seeded random (mobs are not generated in
    /// the End).
    pub(super) fn spawn_generation_creatures(&mut self, cx: i32, cz: i32) {
        if !self.world.chunks.contains_key(&(cx, cz)) || self.dim == Dim::End {
            return;
        }
        let mut random = WorldgenRandom::new(LegacyRandom::new(0));
        random.set_decoration_seed(self.level.seed, cx * 16, cz * 16);
        // The region's own random: no creature's spawn predicate draws from it.
        let mut rules = LegacyRandom::new(self.level.seed ^ (i64::from(cx) << 32 | i64::from(cz)));
        let mut pending = Vec::new();
        {
            let level = self.natural_level();
            let source = [cx * 16, level.max_y(), cz * 16];
            let mut spawn = |l: &GameLevel, r: &SpawnRequest| {
                let spawned = l.game.finalize_spawned(r, &mut pending);
                if let Some(s) = &spawned {
                    l.pending.borrow_mut().push(s.bounds);
                }
                spawned
            };
            spawn_selection::spawn_mobs_for_chunk_generation(
                &level,
                source,
                (cx, cz),
                &mut random,
                &mut rules,
                &mut spawn,
                &mut |_| {},
            );
        }
        self.add_spawned(pending, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The memo answers as the map does, across column changes, with no
    /// structure's bits carried over from the column before.
    #[test]
    fn column_memo_answers_as_the_map_does() {
        let mut spawns = StructureSpawns::default();
        spawns.overrides.insert(
            "minecraft:monument".to_owned(),
            [(
                Category::Monster.name().to_owned(),
                horizon_client::mobs::spawn_selection::Override {
                    piece: true,
                    spawns: Vec::new(),
                },
            )]
            .into_iter()
            .collect(),
        );
        let mut chunks = ChunkMap::default();
        for x in 0..4 {
            let mut chunk = Chunk::empty(x, 0);
            if x % 2 == 0 {
                chunk
                    .structure_refs
                    .push(("minecraft:monument".into(), (x, 0)));
                chunk
                    .structure_refs
                    .push(("minecraft:mineshaft".into(), (x, 0)));
            }
            chunks.insert((x, 0), std::sync::Arc::new(chunk));
        }
        let memo = ColumnMemo::default();
        for block_x in [0, 15, 16, 17, 31, 32, 48, 64, 64, -1, -1, 3] {
            let key = (block_x >> 4, 0);
            let expected = chunks.get(&key).map(|c| &**c as *const Chunk);
            let bits = chunks.get(&key).map_or(0, |c| {
                c.structure_refs
                    .iter()
                    .fold(0, |b, (id, _)| b | spawns.category_bits(id))
            });
            // Bits first, or chunk first: both orders see the column's own.
            if block_x % 2 == 0 {
                assert_eq!(memo.override_bits(&chunks, &spawns, block_x, 5), bits);
            }
            let got = memo
                .get(&chunks, block_x, 5)
                .chunk
                .map(|c| c as *const Chunk);
            assert_eq!(got, expected, "{block_x}");
            assert_eq!(memo.override_bits(&chunks, &spawns, block_x, 5), bits);
        }
    }

    #[test]
    fn biome_rows_hold_each_categorys_list() {
        let entry = |entity: &str| SpawnEntry {
            entity: entity.to_owned(),
            weight: 1,
            min: 1,
            max: 1,
            constant: true,
        };
        let spawns: HashMap<String, Vec<SpawnEntry>> = [
            ("monster".to_owned(), vec![entry("minecraft:zombie")]),
            ("water_ambient".to_owned(), vec![entry("minecraft:cod")]),
            ("not_a_category".to_owned(), vec![entry("minecraft:pig")]),
        ]
        .into_iter()
        .collect();
        let row = mobs_by_category(&spawns);
        for c in Category::ALL {
            let expected = spawns.get(c.name()).map_or(&[][..], Vec::as_slice);
            assert_eq!(row[c as usize].len(), expected.len(), "{c:?}");
            assert!(std::ptr::eq(row[c as usize], expected), "{c:?}");
        }
    }

    fn player(x: f64, spectator: bool) -> LevelPlayer {
        LevelPlayer {
            position: Vec3::new(x, 64.0, 0.0),
            bounds: Aabb::new(Vec3::ZERO, Vec3::ZERO),
            alive: false,
            creative: true,
            spectator,
        }
    }

    #[test]
    fn non_spectators_keep_first_tie_and_dead_creative_players() {
        let players = [player(0.0, true), player(-24.0, false), player(24.0, false)];
        let nearest = non_spectators(&players).nearest([0.0, 64.0, 0.0]).unwrap();
        assert_eq!(nearest.position[0], -24.0);
        assert!(non_spectators(&players[..1]).nearest([0.0; 3]).is_none());
        assert!(non_spectators(&[]).nearest([0.0; 3]).is_none());
    }

    /// The set's order is what `Util.shuffle` starts from: the chunks in the
    /// order a set of pairs would first meet them.
    #[test]
    fn chunk_bits_keep_the_first_seen_order_across_regions() {
        let mut seed = 0x2545_f491_u64;
        let mut draw = |range: i32| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ((seed >> 33) as i32).rem_euclid(range)
        };
        let (mut bits, mut actual) = (ChunkBits::default(), Vec::new());
        let (mut set, mut expected) = (std::collections::HashSet::new(), Vec::new());
        for _ in 0..200 {
            let (pcx, pcz) = (draw(300) - 150, draw(300) - 150);
            for x in pcx - 8..=pcx + 8 {
                bits.mark_column(x, pcz - 8, &mut actual);
                for z in pcz - 8..=pcz + 8 {
                    if set.insert((x, z)) {
                        expected.push((x, z));
                    }
                }
            }
        }
        assert_eq!(actual, expected);
    }
}
