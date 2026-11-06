//! `NaturalSpawner`: which mob spawns where and when. The spawn state
//! (`createState`: category counts, the spawn potential's charges, the
//! per-player local caps), the filtered categories and their global and local
//! caps, `spawnForChunk` and `spawnCategoryForPosition` (the three packs of up
//! to four attempts, the distance and respawn checks, `getRandomSpawnMobAt`
//! over `mobsAt`: the nether fortress bounds, structure spawn overrides by
//! piece or bounding box, then the biome's natural mob spawns), the per-type
//! checks of `isValidSpawnPostitionForType` and `isValidPositionForMob`
//! (each class's `checkSpawnRules` walk target value, `checkSpawnObstruction`,
//! `removeWhenFarAway`, cluster and group sizes), and
//! `spawnMobsForChunkGeneration`. Everything reads the level through
//! [`NaturalLevel`] so the game and the differential test world
//! (tools/test_natural_spawn_parity.py) run the same code; random draws follow
//! vanilla's order.
use super::Species;
use super::player_index::PlayerIndex;
use super::spawn_rules::{self, Reason, SpawnLevel};
use crate::biome::SpawnEntry;
use crate::fast_hash::FxHashMap;
use horizon_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};
use serde_json::Value;
use std::{collections::HashMap, fs, ops::ControlFlow, path::Path};

/// A `natural_mob_spawns` / `spawn_overrides` entry list.
pub fn parse_entries(list: &Value) -> Vec<SpawnEntry> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let count = &e["count"];
            let (min, max, constant) = match count {
                Value::Number(n) => {
                    let n = n.as_u64().unwrap_or(1) as u32;
                    (n, n, true)
                }
                c if c["type"].as_str() == Some("minecraft:constant") => {
                    let n = c["value"].as_u64().unwrap_or(1) as u32;
                    (n, n, true)
                }
                c => (
                    c["min_inclusive"].as_u64().unwrap_or(1) as u32,
                    c["max_inclusive"].as_u64().unwrap_or(1) as u32,
                    false,
                ),
            };
            Some(SpawnEntry {
                entity: e["type"].as_str()?.to_owned(),
                weight: e["weight"].as_u64().unwrap_or(1) as u32,
                min,
                max,
                constant,
            })
        })
        .collect()
}
/// `MobSpawnSettings.MobSpawnCost`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnCost {
    pub energy_budget: f64,
    pub charge: f64,
}
/// The `spawn_costs` of a `natural_mob_spawns` value.
pub fn parse_costs(costs: &Value) -> HashMap<String, SpawnCost> {
    costs
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, c)| {
            (
                k.clone(),
                SpawnCost {
                    energy_budget: c["energy_budget"].as_f64().unwrap_or(0.0),
                    charge: c["charge"].as_f64().unwrap_or(0.0),
                },
            )
        })
        .collect()
}

/// `MobCategory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Monster,
    Creature,
    Ambient,
    Axolotls,
    UndergroundWaterCreature,
    WaterCreature,
    WaterAmbient,
    Misc,
}
impl Category {
    pub const ALL: [Self; 8] = [
        Self::Monster,
        Self::Creature,
        Self::Ambient,
        Self::Axolotls,
        Self::UndergroundWaterCreature,
        Self::WaterCreature,
        Self::WaterAmbient,
        Self::Misc,
    ];
    /// `NaturalSpawner.SPAWNING_CATEGORIES`: all but MISC, in enum order.
    pub const SPAWNING: [Self; 7] = [
        Self::Monster,
        Self::Creature,
        Self::Ambient,
        Self::Axolotls,
        Self::UndergroundWaterCreature,
        Self::WaterCreature,
        Self::WaterAmbient,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Monster => "monster",
            Self::Creature => "creature",
            Self::Ambient => "ambient",
            Self::Axolotls => "axolotls",
            Self::UndergroundWaterCreature => "underground_water_creature",
            Self::WaterCreature => "water_creature",
            Self::WaterAmbient => "water_ambient",
            Self::Misc => "misc",
        }
    }
    pub fn from_name(name: &str) -> Self {
        match name {
            "monster" => Self::Monster,
            "creature" => Self::Creature,
            "ambient" => Self::Ambient,
            "axolotls" => Self::Axolotls,
            "underground_water_creature" => Self::UndergroundWaterCreature,
            "water_creature" => Self::WaterCreature,
            "water_ambient" => Self::WaterAmbient,
            _ => Self::Misc,
        }
    }
    fn index(self) -> usize {
        self as usize
    }
    /// The category's bit in a set of categories.
    pub const fn bit(self) -> u8 {
        1 << self as usize
    }
    /// `getMaxInstancesPerChunk`.
    pub fn max(self) -> i32 {
        match self {
            Self::Monster => 70,
            Self::Creature => 10,
            Self::Ambient => 15,
            Self::Axolotls | Self::UndergroundWaterCreature | Self::WaterCreature => 5,
            Self::WaterAmbient => 20,
            Self::Misc => -1,
        }
    }
    pub fn friendly(self) -> bool {
        self != Self::Monster
    }
    pub fn persistent(self) -> bool {
        matches!(self, Self::Creature | Self::Misc)
    }
    pub fn despawn_distance(self) -> i32 {
        if self == Self::WaterAmbient { 64 } else { 128 }
    }
}

/// `PathfinderMob.checkSpawnRules`: the class's `getWalkTargetValue` at the
/// spawn position must not be negative (plain `Mob`s accept any position).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalkTarget {
    NotPathfinder,
    Zero,
    Animal,
    Mooshroom,
    Turtle,
    Monster,
    Giant,
    Guardian,
    Silverfish,
    Strider,
    Hoglin,
    Bee,
    HappyGhast,
}
fn walk_target(s: Species) -> WalkTarget {
    use Species as S;
    use WalkTarget as W;
    match s {
        S::Bat | S::Slime | S::MagmaCube | S::SulfurCube | S::Ghast | S::Phantom => {
            W::NotPathfinder
        }
        // Monster subclasses that override it with 0.
        S::Enderman | S::Creaking | S::Pillager | S::Warden => W::Zero,
        S::Giant => W::Giant,
        S::Guardian | S::ElderGuardian => W::Guardian,
        S::Silverfish => W::Silverfish,
        S::Zombie
        | S::Husk
        | S::Drowned
        | S::ZombieVillager
        | S::ZombifiedPiglin
        | S::Skeleton
        | S::Stray
        | S::Bogged
        | S::Parched
        | S::WitherSkeleton
        | S::Creeper
        | S::Spider
        | S::CaveSpider
        | S::Endermite
        | S::Witch
        | S::Vindicator
        | S::Evoker
        | S::Illusioner
        | S::Ravager
        | S::Piglin
        | S::PiglinBrute
        | S::Zoglin
        | S::Blaze
        | S::Vex
        | S::Breeze
        | S::Wither => W::Monster,
        S::Mooshroom => W::Mooshroom,
        S::Turtle => W::Turtle,
        S::Strider => W::Strider,
        S::Hoglin => W::Hoglin,
        S::Bee => W::Bee,
        S::HappyGhast => W::HappyGhast,
        // Animal subclasses with their own value.
        S::Axolotl | S::Nautilus | S::ZombieNautilus => W::Zero,
        S::Pig
        | S::Cow
        | S::Sheep
        | S::Chicken
        | S::Rabbit
        | S::Horse
        | S::Donkey
        | S::Mule
        | S::Llama
        | S::TraderLlama
        | S::SkeletonHorse
        | S::ZombieHorse
        | S::Camel
        | S::CamelHusk
        | S::Goat
        | S::Panda
        | S::PolarBear
        | S::Fox
        | S::Wolf
        | S::Cat
        | S::Ocelot
        | S::Parrot
        | S::Armadillo
        | S::Frog
        | S::Sniffer => W::Animal,
        // Other PathfinderMobs: water animals, golems, villagers, allays...
        _ => W::Zero,
    }
}
/// Each class's `checkSpawnObstruction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Obstruction {
    /// `Mob`: no liquid in the box and no entity blocking it.
    NoLiquidUnobstructed,
    Unobstructed,
    Ocelot,
    NoLiquid,
}
fn obstruction(s: Species) -> Obstruction {
    use Species as S;
    match s {
        S::Squid
        | S::GlowSquid
        | S::Dolphin
        | S::Axolotl
        | S::Cod
        | S::Salmon
        | S::TropicalFish
        | S::Pufferfish
        | S::Tadpole
        | S::Nautilus
        | S::ZombieNautilus
        | S::Guardian
        | S::ElderGuardian
        | S::Strider
        | S::Drowned => Obstruction::Unobstructed,
        S::Ocelot => Obstruction::Ocelot,
        S::Ravager => Obstruction::NoLiquid,
        // ZombifiedPiglin's override is the Mob default's two tests; iron
        // golems and wardens never spawn naturally.
        _ => Obstruction::NoLiquidUnobstructed,
    }
}
/// `Mob.removeWhenFarAway` of a freshly created mob.
fn removed_when_far(s: Species) -> bool {
    use Species as S;
    match s {
        // Animal, golems, villagers, traders, allays and wardens never;
        // cats and ocelots only past 2400 ticks; chickens as jockeys.
        S::Axolotl
        | S::Cod
        | S::Salmon
        | S::TropicalFish
        | S::Pufferfish
        | S::Tadpole
        | S::CamelHusk
        | S::ZombieHorse
        | S::Hoglin
        | S::Nautilus
        | S::ZombieNautilus => true,
        S::Villager
        | S::WanderingTrader
        | S::IronGolem
        | S::SnowGolem
        | S::Shulker
        | S::Allay
        | S::Warden => false,
        _ => !is_animal(s),
    }
}
fn is_animal(s: Species) -> bool {
    use Species as S;
    matches!(walk_target(s), WalkTarget::Animal)
        || matches!(
            s,
            S::Axolotl
                | S::Nautilus
                | S::ZombieNautilus
                | S::Mooshroom
                | S::Turtle
                | S::Strider
                | S::Hoglin
                | S::Bee
                | S::HappyGhast
        )
}
/// `Mob.getMaxSpawnClusterSize`.
pub fn max_cluster_size(s: Species) -> u32 {
    use Species as S;
    match s {
        S::Horse
        | S::Donkey
        | S::Mule
        | S::Llama
        | S::TraderLlama
        | S::SkeletonHorse
        | S::ZombieHorse
        | S::Camel
        | S::CamelHusk => 6,
        S::Cod | S::TropicalFish | S::Pufferfish | S::Tadpole | S::Wolf => 8,
        S::Salmon => 5,
        S::Ghast | S::HappyGhast | S::Pillager => 1,
        _ => 4,
    }
}
/// `EntityType.canSpawnFarFromPlayer`: creatures, misc, pillagers, shulkers.
fn spawns_far_from_player(s: Species, category: Category) -> bool {
    matches!(category, Category::Creature | Category::Misc)
        || matches!(s, Species::Pillager | Species::Shulker)
}

/// What natural spawning reads from a level beyond the spawn rules.
/// An entity type's species, width, height and category.
pub type MobType = (Species, f32, f32, Category);

pub trait NaturalLevel: SpawnLevel {
    fn min_y(&self) -> i32;
    /// `getMaxY()`: the highest buildable block.
    fn max_y(&self) -> i32;
    /// `DimensionType.ambientLight`.
    fn ambient_light(&self) -> f32;
    fn has_ceiling(&self) -> bool;
    /// The entity type's species, width, height and category.
    fn mob_type(&self, entity: &str) -> Option<MobType>;
    /// The `natural_mob_spawns` lists by category at a position.
    fn biome_spawns(&self, x: i32, y: i32, z: i32) -> Option<&HashMap<String, Vec<SpawnEntry>>>;
    /// The biome's list for a category at a position: what `biome_spawns`
    /// holds under the category's name, or nothing.
    fn natural_mobs(&self, category: Category, x: i32, y: i32, z: i32) -> &[SpawnEntry] {
        self.biome_spawns(x, y, z)
            .and_then(|b| b.get(category.name()))
            .map_or(&[], Vec::as_slice)
    }
    /// `#reduce_water_ambient_spawns` of the biome at a position.
    fn reduces_water_ambient(&self, x: i32, y: i32, z: i32) -> bool {
        self.biome_tag("minecraft:reduce_water_ambient_spawns", x, y, z)
    }
    /// False when no structure reaching the column can change `category`'s
    /// list, so [`mobs_at`] needs no structure lookup.
    fn structures_may_override(&self, _category: Category, _x: i32, _z: i32) -> bool {
        true
    }
    /// The `natural_mob_spawns` spawn cost of a type at a position.
    fn spawn_cost(&self, entity: &str, x: i32, y: i32, z: i32) -> Option<SpawnCost>;
    /// The `creature_world_gen_spawn_probability` attribute at a position.
    fn creature_probability(&self, x: i32, y: i32, z: i32) -> f32;
    /// Every structure whose references reach the position's chunk and
    /// whose id `wanted` takes, in the chunk's reference order.
    fn structures_at(
        &self,
        x: i32,
        y: i32,
        z: i32,
        wanted: &dyn Fn(&str) -> bool,
    ) -> Vec<StructureAt>;
    fn structure_spawns(&self) -> &StructureSpawns;
    /// The world spawn point, when it is in this dimension.
    fn respawn(&self) -> Option<[i32; 3]>;
    /// `ServerLevel.canSpawnEntitiesInChunk`.
    fn can_spawn_in_chunk(&self, cx: i32, cz: i32) -> bool;
    /// `noCollision(null, box)`: blocks under an empty collision context and
    /// entities that can be collided with.
    fn no_collision(&self, bounds: Aabb) -> bool;
    /// `isUnobstructed(entity)`: no entity blocking building overlaps.
    fn unobstructed(&self, bounds: Aabb) -> bool;
    /// The `spawn_mobs` game rule.
    fn spawn_mobs(&self) -> bool;
    /// `getHeight(heightmap, x, z)`.
    fn heightmap(&self, kind: Heightmap, x: i32, z: i32) -> i32;
    /// `isPathfindable(PathComputationType.LAND)` of the block.
    fn pathfindable_land(&self, x: i32, y: i32, z: i32) -> bool;
    /// A hint that the block at a position will be read soon: the level may
    /// start loading it. Must not change anything a read answers.
    fn prefetch_block(&self, _x: i32, _y: i32, _z: i32) {}
    /// A hint that the section holding a position will be read soon (its
    /// header, which [`Self::prefetch_block`] then reads). Must not change
    /// anything a read answers.
    fn prefetch_section(&self, _x: i32, _y: i32, _z: i32) {}
}

/// The heightmap types `SpawnPlacements` registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heightmap {
    WorldSurface,
    OceanFloor,
    MotionBlocking,
    MotionBlockingNoLeaves,
}
impl Heightmap {
    pub fn from_name(name: &str) -> Self {
        match name {
            "WORLD_SURFACE" => Self::WorldSurface,
            "OCEAN_FLOOR" => Self::OceanFloor,
            "MOTION_BLOCKING" => Self::MotionBlocking,
            _ => Self::MotionBlockingNoLeaves,
        }
    }
}

/// `Heightmap.Types`' predicates: WORLD_SURFACE (not air), OCEAN_FLOOR
/// (`#blocks_motion_in_heightmap`), MOTION_BLOCKING (that or a fluid) and
/// MOTION_BLOCKING_NO_LEAVES (`#blocks_motion_in_heightmap_no_leaves` or a fluid).
pub fn heightmap_blocks<L: SpawnLevel>(l: &L, kind: Heightmap, x: i32, y: i32, z: i32) -> bool {
    let state = l.state(x, y, z);
    let f = l.facts().get(state);
    let fluid = f.fluid != crate::blocks::Fluid::None;
    match kind {
        Heightmap::WorldSurface => !l.facts().is_air(state),
        Heightmap::OceanFloor => l.block_tag("minecraft:blocks_motion_in_heightmap", &f.name),
        Heightmap::MotionBlocking => {
            fluid || l.block_tag("minecraft:blocks_motion_in_heightmap", &f.name)
        }
        Heightmap::MotionBlockingNoLeaves => {
            fluid || l.block_tag("minecraft:blocks_motion_in_heightmap_no_leaves", &f.name)
        }
    }
}
/// `LevelReader.getLightLevelDependentMagicValue`.
fn magic_light<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> f32 {
    let raw = l
        .block_light(x, y, z)
        .max(l.sky_light(x, y, z) - l.sky_darken());
    let v = raw as f32 / 15.0;
    let curved = v / (4.0 - 3.0 * v);
    curved + l.ambient_light() * (1.0 - curved)
}
fn cost_from_light<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> f32 {
    magic_light(l, x, y, z) - 0.5
}
fn name_at<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> &str {
    &l.facts().get(l.state(x, y, z)).name
}
fn fluid_at<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> crate::blocks::Fluid {
    l.facts().get(l.state(x, y, z)).fluid
}
/// `checkSpawnRules(level, NATURAL)` of a freshly created mob at a block.
pub(crate) fn mob_spawn_rules<L: NaturalLevel>(s: Species, l: &L, x: i32, y: i32, z: i32) -> bool {
    use crate::blocks::Fluid;
    let value = match walk_target(s) {
        WalkTarget::NotPathfinder | WalkTarget::Zero => 0.0,
        WalkTarget::Animal => {
            if name_at(l, x, y - 1, z) == "minecraft:grass_block" {
                10.0
            } else {
                cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Mooshroom => {
            if name_at(l, x, y - 1, z) == "minecraft:mycelium" {
                10.0
            } else {
                cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Turtle => {
            if fluid_at(l, x, y, z) == Fluid::Water
                || l.block_tag("minecraft:sand", name_at(l, x, y - 1, z))
            {
                10.0
            } else {
                cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Monster => -cost_from_light(l, x, y, z),
        WalkTarget::Giant => cost_from_light(l, x, y, z),
        WalkTarget::Guardian => {
            if fluid_at(l, x, y, z) == Fluid::Water {
                10.0 + cost_from_light(l, x, y, z)
            } else {
                -cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Silverfish => {
            if infested_host(name_at(l, x, y - 1, z)) {
                10.0
            } else {
                -cost_from_light(l, x, y, z)
            }
        }
        // A new strider is not in lava yet.
        WalkTarget::Strider => {
            if fluid_at(l, x, y, z) == Fluid::Lava {
                10.0
            } else {
                0.0
            }
        }
        // A new hoglin remembers no repellent.
        WalkTarget::Hoglin => {
            if name_at(l, x, y - 1, z) == "minecraft:crimson_nylium" {
                10.0
            } else {
                0.0
            }
        }
        WalkTarget::Bee => {
            if l.facts().is_air(l.state(x, y, z)) {
                10.0
            } else {
                0.0
            }
        }
        WalkTarget::HappyGhast => {
            let air = |dy| l.facts().is_air(l.state(x, y + dy, z));
            if !air(0) {
                0.0
            } else if air(-1) && !air(-2) {
                10.0
            } else {
                5.0
            }
        }
    };
    value >= 0.0
}
/// `InfestedBlock.isCompatibleHostBlock`.
pub fn infested_host(block: &str) -> bool {
    matches!(
        block,
        "minecraft:stone"
            | "minecraft:cobblestone"
            | "minecraft:stone_bricks"
            | "minecraft:mossy_stone_bricks"
            | "minecraft:cracked_stone_bricks"
            | "minecraft:chiseled_stone_bricks"
            | "minecraft:deepslate"
    )
}
/// `LevelReader.containsAnyLiquid`: the blocks from the box's minimum corner
/// to the block holding its maximum corner.
pub fn contains_any_liquid<L: SpawnLevel>(l: &L, b: Aabb) -> bool {
    let (lo, hi) = (b.min_position(), b.max_position());
    let (x0, y0, z0) = (
        lo.x.floor() as i32,
        lo.y.floor() as i32,
        lo.z.floor() as i32,
    );
    let (x1, y1, z1) = (
        hi.x.floor() as i32,
        hi.y.floor() as i32,
        hi.z.floor() as i32,
    );
    (x0..=x1).any(|x| {
        (y0..=y1).any(|y| {
            (z0..=z1).any(|z| l.facts().get(l.state(x, y, z)).fluid != crate::blocks::Fluid::None)
        })
    })
}
/// `EntityDimensions.makeBoundingBox`: the half width is a float.
pub fn spawn_box(x: f64, y: f64, z: f64, width: f32, height: f32) -> Aabb {
    let half = f64::from(width / 2.0);
    Aabb::new(
        Vec3::new(x - half, y, z - half),
        Vec3::new(x + half, y + f64::from(height), z + half),
    )
}
/// `EntityType.spawnDimensionsScale`: cube mobs are checked for room at
/// their largest spawn size.
fn spawn_dimensions_scale(s: Species) -> f32 {
    match s {
        Species::Slime | Species::MagmaCube => 4.0,
        Species::SulfurCube => 2.0,
        _ => 1.0,
    }
}
/// `EntityType.getSpawnAABB`.
pub fn spawn_aabb(s: Species, x: f64, y: f64, z: f64, width: f32, height: f32) -> Aabb {
    let scale = spawn_dimensions_scale(s);
    let half = f64::from(scale * width / 2.0);
    Aabb::new(
        Vec3::new(x - half, y, z - half),
        Vec3::new(x + half, y + f64::from(scale * height), z + half),
    )
}
/// `checkSpawnObstruction` of a freshly created mob with the given box.
pub(crate) fn spawn_obstruction<L: NaturalLevel>(
    s: Species,
    l: &L,
    bounds: Aabb,
    spawned: &[Aabb],
    x: i32,
    y: i32,
    z: i32,
) -> bool {
    let unobstructed =
        |l: &L| l.unobstructed(bounds) && !spawned.iter().any(|b| b.intersects(bounds));
    match obstruction(s) {
        Obstruction::NoLiquidUnobstructed => !contains_any_liquid(l, bounds) && unobstructed(l),
        Obstruction::Unobstructed => unobstructed(l),
        Obstruction::NoLiquid => !contains_any_liquid(l, bounds),
        Obstruction::Ocelot => {
            if unobstructed(l) && !contains_any_liquid(l, bounds) {
                if y < l.sea_level() {
                    return false;
                }
                let below = name_at(l, x, y - 1, z);
                if below == "minecraft:grass_block" || l.block_tag("minecraft:leaves", below) {
                    return true;
                }
            }
            false
        }
    }
}

/// A structure's spawn override for one category.
#[derive(Clone, Debug)]
pub struct Override {
    /// `bounding_box: piece` (else the whole structure's box).
    pub piece: bool,
    pub spawns: Vec<SpawnEntry>,
}
/// Every structure's `spawn_overrides`, by structure id then category.
#[derive(Clone, Debug, Default)]
pub struct StructureSpawns {
    /// Each category's override by category index. Fx-hashed, one look-up a
    /// structure: natural spawning asks about every structure reaching every
    /// position it tries.
    pub overrides: FxHashMap<String, [Option<Override>; Category::ALL.len()]>,
}
impl StructureSpawns {
    /// The overrides of the resource root first asked for, loaded once.
    pub fn shared(root: &Path) -> &'static Self {
        static SHARED: std::sync::OnceLock<StructureSpawns> = std::sync::OnceLock::new();
        SHARED.get_or_init(|| Self::load(root))
    }
    pub fn load(root: &Path) -> Self {
        let mut overrides = FxHashMap::default();
        let dir = root.join("data/minecraft/worldgen/structure");
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(v) = fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            else {
                continue;
            };
            let mut spawns = Self::default();
            for (category, o) in v["spawn_overrides"].as_object().into_iter().flatten() {
                // A category no spawner asks about is never looked up.
                if let Some(c) = Category::ALL.into_iter().find(|c| c.name() == category) {
                    let o = Override {
                        piece: o["bounding_box"].as_str() == Some("piece"),
                        spawns: parse_entries(&o["spawns"]),
                    };
                    spawns.insert(format!("minecraft:{stem}"), c, o);
                }
            }
            overrides.extend(spawns.overrides);
        }
        Self { overrides }
    }
    /// Sets a structure's override for a category.
    pub fn insert(&mut self, structure: String, category: Category, o: Override) {
        self.overrides.entry(structure).or_default()[category.index()] = Some(o);
    }
    pub fn get(&self, structure: &str, category: Category) -> Option<&Override> {
        self.overrides.get(structure)?[category.index()].as_ref()
    }
    /// The bits ([`Category::bit`]) of the categories whose [`mobs_at`] looks
    /// a structure up: those it overrides, and monsters for a fortress.
    pub fn category_bits(&self, structure: &str) -> u8 {
        let fortress = if structure == "minecraft:fortress" {
            Category::Monster.bit()
        } else {
            0
        };
        self.overrides.get(structure).map_or(fortress, |map| {
            Category::ALL
                .into_iter()
                .filter(|c| map[c.index()].is_some())
                .fold(fortress, |bits, c| bits | c.bit())
        })
    }
}

/// A structure whose references reach a position's chunk.
#[derive(Clone, Debug)]
pub struct StructureAt {
    pub id: String,
    /// A piece of one of its starts holds the position.
    pub in_piece: bool,
    /// One of its starts' bounding boxes holds the position.
    pub in_bounds: bool,
}

/// `NetherFortressStructure.FORTRESS_ENEMIES`.
pub fn fortress_enemies() -> &'static [SpawnEntry] {
    static ENEMIES: std::sync::LazyLock<Vec<SpawnEntry>> = std::sync::LazyLock::new(|| {
        let e = |entity: &str, weight, min, max, constant| SpawnEntry {
            entity: entity.to_owned(),
            weight,
            min,
            max,
            constant,
        };
        vec![
            e("minecraft:blaze", 10, 2, 3, false),
            e("minecraft:zombified_piglin", 5, 4, 4, true),
            e("minecraft:wither_skeleton", 8, 5, 5, true),
            e("minecraft:skeleton", 2, 5, 5, true),
            e("minecraft:magma_cube", 3, 4, 4, true),
        ]
    });
    &ENEMIES
}

/// `NaturalSpawner.mobsAt`: the fortress bounds on nether bricks, then
/// `ChunkGenerator.getMobsAt` (the first referenced structure whose override
/// for the category holds the position), else the biome's list. Borrowed from
/// the level: a spawn attempt looks it up up to twice.
pub fn mobs_at<L: NaturalLevel>(
    l: &L,
    category: Category,
    x: i32,
    y: i32,
    z: i32,
) -> &[SpawnEntry] {
    // Only a fortress or a structure overriding this category's spawns can
    // change the list, so only theirs are looked up (the references reaching
    // most chunks are mineshafts, which override nothing).
    if !l.structures_may_override(category, x, z) {
        return l.natural_mobs(category, x, y, z);
    }
    let overrides = l.structure_spawns();
    let monster = category == Category::Monster;
    let structures = l.structures_at(x, y, z, &|id| {
        (monster && id == "minecraft:fortress") || overrides.get(id, category).is_some()
    });
    if category == Category::Monster
        && structures
            .iter()
            .any(|s| s.id == "minecraft:fortress" && s.in_bounds)
        && name_at(l, x, y - 1, z) == "minecraft:nether_bricks"
    {
        return fortress_enemies();
    }
    for s in &structures {
        if let Some(o) = overrides.get(&s.id, category)
            && (if o.piece { s.in_piece } else { s.in_bounds })
        {
            return &o.spawns;
        }
    }
    l.natural_mobs(category, x, y, z)
}

/// `WeightedList.getRandom`.
pub fn weighted_pick<'e>(
    list: &'e [SpawnEntry],
    random: &mut impl RandomSource,
) -> Option<&'e SpawnEntry> {
    let total: i64 = list.iter().map(|e| i64::from(e.weight)).sum();
    if total <= 0 {
        return None;
    }
    let mut selection = i64::from(random.next_int_bound(total as i32).ok()?);
    for e in list {
        selection -= i64::from(e.weight);
        if selection < 0 {
            return Some(e);
        }
    }
    None
}

/// `SpawnerData.count().sample(random)`: a constant draws nothing.
pub fn sample_count(entry: &SpawnEntry, random: &mut impl RandomSource) -> u32 {
    if entry.constant {
        entry.min
    } else {
        let span = entry.max.saturating_sub(entry.min) as i32 + 1;
        entry.min + random.next_int_bound(span).unwrap_or(0) as u32
    }
}
/// `SpawnerData.equals`: the type and the count provider (the weight is not
/// part of it; a constant and a one-value uniform differ).
fn same_spawner(a: &SpawnEntry, b: &SpawnEntry) -> bool {
    a.entity == b.entity && a.constant == b.constant && a.min == b.min && a.max == b.max
}

/// An entity counted by `NaturalSpawner.createState`.
#[derive(Clone, Debug)]
pub struct Counted<'a> {
    pub entity: &'a str,
    pub category: Category,
    /// `blockPosition()`.
    pub block: [i32; 3],
    pub is_mob: bool,
    /// `isPersistenceRequired() || requiresCustomPersistence()`.
    pub persistent: bool,
}
/// A counted entity's contribution to `createState`, its charge already read
/// from the biome at its block (`None`: the type has no spawn cost there).
#[derive(Clone, Copy, Debug)]
pub struct CountedCharge {
    pub block: [i32; 3],
    pub category: Category,
    pub is_mob: bool,
    pub charge: Option<f64>,
}
/// Whether `createState` counts it: not a persistent mob, not MISC. The chunk
/// must also be loaded.
pub fn counted_by_state(category: Category, is_mob: bool, persistent: bool) -> bool {
    !(is_mob && persistent || category == Category::Misc)
}
/// `NaturalSpawner.SpawnState` with its `PotentialCalculator` and
/// `LocalMobCapCalculator`.
#[derive(Clone, Debug)]
pub struct SpawnState {
    spawnable_chunks: i32,
    counts: [i32; 8],
    charges: Vec<([i32; 3], f64)>,
    /// The non-spectators, each labelled with the categories below their
    /// local cap ([`Self::open_categories`]).
    players: PlayerIndex,
    /// `LocalMobCapCalculator.playerMobCounts`, by the index's slots.
    player_counts: Vec<[i32; 8]>,
    /// The last chunk asked about, the categories asked about, and those of
    /// them open to its players. A count only rises in a pass, so this holds
    /// until a player's cap fills.
    local: Option<((i32, i32), u8, u8)>,
    /// The answers [`Self::prepare_local`] worked out for the chunks to come,
    /// last chunk first, until a cap fills.
    prepared: Vec<((i32, i32), u8, u8)>,
    /// Reused across the tick's positions; holds the chunk's players.
    nearest: Nearest,
}
/// Every spawning category's bit.
const SPAWNING_BITS: u8 = {
    let mut bits = 0;
    let mut i = 0;
    while i < Category::SPAWNING.len() {
        bits |= 1 << Category::SPAWNING[i] as usize;
        i += 1;
    }
    bits
};
impl SpawnState {
    /// `NaturalSpawner.createState`: every non-persistent entity of a
    /// spawning category in a loaded chunk adds its biome's spawn charge, its
    /// category count and (for mobs) the local caps of the players near it.
    /// `players` holds the level's non-spectators.
    pub fn new<L: NaturalLevel>(
        l: &L,
        spawnable_chunks: i32,
        players: PlayerIndex,
        entities: &[Counted],
        chunk_loaded: impl Fn(i32, i32) -> bool,
    ) -> Self {
        let counted = entities
            .iter()
            .filter(|e| {
                counted_by_state(e.category, e.is_mob, e.persistent)
                    && chunk_loaded(e.block[0] >> 4, e.block[2] >> 4)
            })
            .map(|e| {
                let [x, y, z] = e.block;
                CountedCharge {
                    block: e.block,
                    category: e.category,
                    is_mob: e.is_mob,
                    charge: l.spawn_cost(e.entity, x, y, z).map(|c| c.charge),
                }
            });
        Self::from_counted(spawnable_chunks, players, counted)
    }
    /// The rest of `createState`, over what it counts in the level's order
    /// (the charges' order is the potential's floating-point sum).
    pub fn from_counted(
        spawnable_chunks: i32,
        players: PlayerIndex,
        counted: impl IntoIterator<Item = CountedCharge>,
    ) -> Self {
        let mut state = Self {
            spawnable_chunks,
            counts: [0; 8],
            charges: Vec::new(),
            player_counts: vec![[0; 8]; players.len()],
            players,
            local: None,
            prepared: Vec::new(),
            nearest: Nearest::default(),
        };
        // The local caps are sums, so a chunk's mobs can go in together.
        let mut mobs_by_chunk = FxHashMap::<(i32, i32), [i32; 8]>::default();
        for e in counted {
            let [x, _, z] = e.block;
            if let Some(charge) = e.charge {
                state.add_charge(e.block, charge);
            }
            if e.is_mob {
                mobs_by_chunk.entry((x >> 4, z >> 4)).or_insert([0; 8])[e.category.index()] += 1;
            }
            state.counts[e.category.index()] += 1;
        }
        let Self {
            players,
            player_counts,
            ..
        } = &mut state;
        for (chunk, mobs) in mobs_by_chunk {
            players.for_each_close_for_spawning(chunk, |p| {
                for (count, added) in player_counts[p].iter_mut().zip(mobs) {
                    *count += added;
                }
            });
        }
        state.label_open_categories();
        state
    }
    fn add_charge(&mut self, pos: [i32; 3], charge: f64) {
        if charge != 0.0 {
            self.charges.push((pos, charge));
        }
    }
    /// `LocalMobCapCalculator.MobCounts.canSpawn` of every spawning category.
    fn open_categories(counts: &[i32; 8]) -> u8 {
        Category::SPAWNING
            .into_iter()
            .filter(|c| counts[c.index()] < c.max())
            .fold(0, |bits, c| bits | 1 << c.index())
    }
    fn label_open_categories(&mut self) {
        let counts = &self.player_counts;
        self.players
            .set_labels(|slot| Self::open_categories(&counts[slot]));
    }
    /// `LocalMobCapCalculator.addMob`: the players close to the chunk
    /// (`ChunkMap.getPlayersCloseForSpawning`). The natural spawn chunk
    /// tracker's gate there needs no check: a player within 128 blocks of a
    /// chunk's centre is within the tracker's 8 chunks of it.
    fn add_local(&mut self, chunk: (i32, i32), category: Category) {
        let i = category.index();
        // Where the players whose cap this fills are.
        let mut filled: Vec<[f64; 3]> = Vec::new();
        let counts = &mut self.player_counts;
        self.players.for_each_point_close_for_spawning(chunk, |p| {
            let count = &mut counts[p.slot as usize][i];
            *count += 1;
            if *count == category.max() {
                filled.push(p.position);
            }
        });
        if !filled.is_empty() {
            self.local = None;
            self.label_open_categories();
            // Only the chunks close to those players can answer otherwise.
            let players = &self.players;
            for (at, asked, open) in &mut self.prepared {
                if filled
                    .iter()
                    .any(|&p| super::player_index::close_for_spawning(*at, p))
                {
                    *open = players.close_for_spawning_labels(*at, *asked);
                }
            }
        }
    }
    /// Works out on the pool what [`spawn_for_chunk`] will ask about `chunks`,
    /// in the order it will ask, for `categories`.
    pub fn prepare_local(&mut self, chunks: &[(i32, i32)], categories: &[Category]) {
        let asked = Self::asked(categories);
        self.prepared.clear();
        self.prepared
            .extend(chunks.iter().rev().map(|&chunk| (chunk, asked, 0)));
        let players = &self.players;
        horizon_threads::pool::for_each_mut(&mut self.prepared, 256, |(chunk, asked, open)| {
            *open = players.close_for_spawning_labels(*chunk, *asked);
        });
    }
    /// What [`Self::can_spawn_local_of`] would answer for every category in
    /// `asked` at `chunk` now, without asking or keeping anything.
    fn open_for(&self, chunk: (i32, i32), asked: u8) -> u8 {
        [self.local, self.prepared.last().copied()]
            .into_iter()
            .flatten()
            .find(|&(at, known, _)| at == chunk && known == asked)
            .map_or_else(
                || self.players.close_for_spawning_labels(chunk, asked),
                |(_, _, open)| open,
            )
    }
    /// The chunk [`Self::prepare_local`] lists after `chunk`, with what it
    /// worked out for `asked` there (a guess once a cap fills).
    fn next_open(&self, chunk: (i32, i32), asked: u8) -> Option<((i32, i32), u8)> {
        self.prepared
            .iter()
            .rev()
            .find(|&&(at, _, _)| at != chunk)
            .filter(|&&(_, known, _)| known == asked)
            .map(|&(at, _, open)| (at, open))
    }
    fn asked(categories: &[Category]) -> u8 {
        categories.iter().fold(0, |bits, c| bits | c.bit())
    }
    /// `canSpawnForCategoryGlobal`.
    pub fn can_spawn_global(&self, category: Category) -> bool {
        let max = category.max() * self.spawnable_chunks / 289;
        self.counts[category.index()] < max
    }
    /// `canSpawnForCategoryLocal` (`LocalMobCapCalculator.canSpawn`), for a
    /// spawning category: any close player below their cap.
    pub fn can_spawn_local(&mut self, category: Category, cx: i32, cz: i32) -> bool {
        self.can_spawn_local_of(SPAWNING_BITS, category, (cx, cz))
    }
    /// The same, knowing the chunk will be asked about no category outside
    /// `asked`: a player's full cap in another (creatures, most ticks) cannot
    /// keep the search going.
    fn can_spawn_local_of(&mut self, asked: u8, category: Category, chunk: (i32, i32)) -> bool {
        let open = match self.local {
            Some((at, known, open)) if at == chunk && known == asked => open,
            _ => {
                let open = match self.prepared.last() {
                    Some(&(at, known, open)) if at == chunk && known == asked => {
                        self.prepared.pop();
                        open
                    }
                    _ => self.players.close_for_spawning_labels(chunk, asked),
                };
                self.local = Some((chunk, asked, open));
                open
            }
        };
        open & category.bit() != 0
    }
    /// `getFilteredSpawningCategories`.
    pub fn filtered_categories(
        &self,
        spawn_enemies: bool,
        spawn_persistent: bool,
    ) -> Vec<Category> {
        Category::SPAWNING
            .into_iter()
            .filter(|&c| {
                (spawn_enemies || c.friendly())
                    && (spawn_persistent || !c.persistent())
                    && self.can_spawn_global(c)
            })
            .collect()
    }
    /// `SpawnState.canSpawn`: the spawn potential's energy change must stay
    /// within the type's budget where it has a spawn cost. Gives the charge
    /// [`Self::after_spawn`] adds (vanilla's `lastCharge`) if it may spawn.
    fn can_spawn<L: NaturalLevel>(&self, l: &L, entity: &str, pos: [i32; 3]) -> Option<f64> {
        match l.spawn_cost(entity, pos[0], pos[1], pos[2]) {
            None => Some(0.0),
            Some(cost) => {
                (self.energy_change(pos, cost.charge) <= cost.energy_budget).then_some(cost.charge)
            }
        }
    }
    /// `PotentialCalculator.getPotentialEnergyChange`.
    fn energy_change(&self, pos: [i32; 3], charge: f64) -> f64 {
        if charge == 0.0 {
            return 0.0;
        }
        let mut change = 0.0;
        for (p, c) in &self.charges {
            let d = [
                f64::from(p[0] - pos[0]),
                f64::from(p[1] - pos[1]),
                f64::from(p[2] - pos[2]),
            ];
            let dist = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            change += if dist == 0.0 {
                f64::INFINITY
            } else {
                c / dist.sqrt()
            };
        }
        change * charge
    }
    /// `SpawnState.afterSpawn`, right after [`Self::can_spawn`] at the same
    /// position for the same type (so vanilla's `lastCheckedPos` always holds).
    fn after_spawn(&mut self, category: Category, pos: [i32; 3], charge: f64) {
        self.add_charge(pos, charge);
        self.counts[category.index()] += 1;
        self.add_local((pos[0] >> 4, pos[2] >> 4), category);
    }
}

/// What a spawn pass did, in vanilla's order (for differential tests).
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `getFilteredSpawningCategories`.
    Categories(Vec<Category>),
    /// `canSpawnForCategoryLocal` for the chunk.
    Local(Category, bool),
    /// `getRandomPosWithin`'s start.
    Start(Category, [i32; 3]),
    /// An attempt at a position (the nearest player lookup).
    Try(i32, i32),
    /// The type passed `isValidSpawnPostitionForType`; the spawn state's
    /// potential test answered.
    Test(String, [i32; 3], bool),
    /// A mob spawned.
    Spawn(String, [f64; 3], f32),
}
/// A mob the spawner is about to add: the level runs `finalizeSpawn` and
/// adds it, returning its finalized box and whether a tropical fish schools.
#[derive(Clone, Debug)]
pub struct SpawnRequest<'a> {
    pub entity: &'a str,
    pub species: Species,
    pub position: [f64; 3],
    pub yaw: f32,
    /// Mobs of this group spawned before (the `SpawnGroupData`'s size).
    pub group_index: u32,
    pub reason: Reason,
}
#[derive(Clone, Copy, Debug)]
pub struct Spawned {
    pub bounds: Aabb,
    /// `TropicalFish.isSchool`.
    pub school: bool,
}

/// A random natural spawning can look ahead in without drawing from it: the
/// 48-bit state of a `java.util.Random`, or `None` to look nowhere.
pub trait Foresee {
    fn legacy_state(&self) -> Option<u64>;
}
impl Foresee for horizon_core::random::LegacyRandom {
    fn legacy_state(&self) -> Option<u64> {
        Some(self.state())
    }
}
/// `java.util.Random`'s `next(31)` and `nextInt(bound)` on a copy of its
/// state: a lookahead that neither moves nor traces the real random.
#[derive(Clone, Copy)]
struct Lcg(u64);
impl Lcg {
    fn bits31(&mut self) -> i32 {
        self.0 = self.0.wrapping_mul(0x5_deec_e66d).wrapping_add(11) & ((1 << 48) - 1);
        (self.0 >> 17) as i32
    }
    /// `nextInt(bound)`, and nothing drawn for a bound below 1 (the callers'
    /// `unwrap_or(0)`).
    fn next_int(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            return 0;
        }
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.bits31())) >> 31) as i32;
        }
        loop {
            let bits = self.bits31();
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }
}
/// The starts `spawn_category_for_chunk` will pick for `categories` (those
/// of them `open`) from random state `seed`, should each attempt end at its
/// first read, as most do (a solid block, or the bottom of the world): their
/// surfaces are read and their blocks prefetched together, so the serial
/// pass meets them in cache. `after` gets the state each category should
/// leave the random in; once the real one differs, the rest are guesses.
///
/// The guess goes on into `next`, the chunk to come with its open categories
/// and all of `spawn_for_chunk`'s, from the state this one should leave: when
/// every attempt here ends at its first read, that is where the next starts.
/// The sections' headers of every start are hinted first, and only then
/// their storage, so all their misses overlap instead of queueing one start
/// after another. Hints only: no read or draw changes.
fn foresee_starts<L: NaturalLevel>(
    l: &L,
    seed: u64,
    open: u8,
    categories: &[Category],
    chunk: (i32, i32),
    after: &mut [u64],
    next: Option<((i32, i32), u8, &[Category])>,
) {
    let mut random = Lcg(seed);
    let min = l.min_y();
    let start = |random: &mut Lcg, (cx, cz): (i32, i32)| {
        let x = cx * 16 + random.next_int(16);
        let z = cz * 16 + random.next_int(16);
        let top = l.heightmap(Heightmap::WorldSurface, x, z) + 1;
        let y = min + random.next_int(top - min + 1);
        (y > min).then(|| {
            l.prefetch_section(x, y, z);
            (x, y, z)
        })
    };
    let mut starts = [None; 2 * Category::ALL.len()];
    let (here, ahead) = starts.split_at_mut(Category::ALL.len());
    for ((category, after), at) in categories.iter().zip(after).zip(here) {
        if open & category.bit() != 0 {
            *at = start(&mut random, chunk);
        }
        *after = random.0;
    }
    if let Some((next, open, categories)) = next {
        for (category, at) in categories.iter().zip(ahead) {
            if open & category.bit() != 0 {
                *at = start(&mut random, next);
            }
        }
    }
    for (x, y, z) in starts.into_iter().flatten() {
        l.prefetch_block(x, y, z);
    }
}

/// `NaturalSpawner.spawnForChunk` for the categories already filtered.
#[allow(clippy::too_many_arguments)]
pub fn spawn_for_chunk<L: NaturalLevel, R: RandomSource + Foresee>(
    l: &L,
    random: &mut R,
    state: &mut SpawnState,
    categories: &[Category],
    chunk: (i32, i32),
    spawn: &mut dyn FnMut(&L, &SpawnRequest) -> Option<Spawned>,
    trace: &mut (impl FnMut(Event) + ?Sized),
) {
    let asked = SpawnState::asked(categories);
    // The random state foreseen after each category ([`foresee_starts`]).
    let mut after = [0u64; Category::ALL.len()];
    for (i, &category) in categories.iter().enumerate() {
        if let Some(now) = random.legacy_state()
            && (i == 0 || after.get(i - 1) != Some(&now))
        {
            let open = state.open_for(chunk, asked);
            let next = state
                .next_open(chunk, asked)
                .map(|(next, open)| (next, open, categories));
            let rest = after.get_mut(i..).unwrap_or_default();
            foresee_starts(l, now, open, &categories[i..], chunk, rest, next);
        }
        let local = state.can_spawn_local_of(asked, category, chunk);
        trace(Event::Local(category, local));
        if local {
            spawn_category_for_chunk(l, random, state, category, chunk, spawn, trace);
        }
    }
}
/// `NaturalSpawner.spawnCategoryForChunk`: `getRandomPosWithin` (a column of
/// the chunk, a height up to one above the world surface).
#[inline(never)]
pub fn spawn_category_for_chunk<L: NaturalLevel, R: RandomSource>(
    l: &L,
    random: &mut R,
    state: &mut SpawnState,
    category: Category,
    chunk: (i32, i32),
    spawn: &mut dyn FnMut(&L, &SpawnRequest) -> Option<Spawned>,
    trace: &mut (impl FnMut(Event) + ?Sized),
) {
    let x = chunk.0 * 16 + random.next_int_bound(16).unwrap_or(0);
    let z = chunk.1 * 16 + random.next_int_bound(16).unwrap_or(0);
    let top = l.heightmap(Heightmap::WorldSurface, x, z) + 1;
    let min = l.min_y();
    let y = min + random.next_int_bound(top - min + 1).unwrap_or(0);
    trace(Event::Start(category, [x, y, z]));
    if y > min {
        spawn_category_for_position(l, random, state, category, chunk, [x, y, z], spawn, trace);
    }
}
/// `NaturalSpawner.spawnCategoryForPosition` with the spawn state's
/// `canSpawn` and `afterSpawn`.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
pub fn spawn_category_for_position<L: NaturalLevel, R: RandomSource>(
    l: &L,
    random: &mut R,
    state: &mut SpawnState,
    category: Category,
    chunk: (i32, i32),
    start: [i32; 3],
    spawn: &mut dyn FnMut(&L, &SpawnRequest) -> Option<Spawned>,
    trace: &mut (impl FnMut(Event) + ?Sized),
) {
    let [x0, y, z0] = start;
    if l.mob_data().state(l.state(x0, y, z0)).flags & super::data::REDSTONE_CONDUCTOR != 0 {
        return;
    }
    let mut spawned: Vec<Aabb> = Vec::new();
    let mut cluster = 0u32;
    for _ in 0..3 {
        let (mut x, mut z) = (x0, z0);
        // The group's entry, its type and where it was picked.
        let mut current: Option<(&SpawnEntry, Option<MobType>, [i32; 3])> = None;
        let mut group_index = 0u32;
        let mut max = (random.next_float() * 4.0).ceil() as i32;
        let mut ll = 0;
        while ll < max {
            ll += 1;
            x += random.next_int_bound(6).unwrap_or(0) - random.next_int_bound(6).unwrap_or(0);
            z += random.next_int_bound(6).unwrap_or(0) - random.next_int_bound(6).unwrap_or(0);
            let (xx, zz) = (f64::from(x) + 0.5, f64::from(z) + 0.5);
            trace(Event::Try(x, z));
            let Some(distance) =
                state
                    .nearest
                    .distance_sqr(&state.players, chunk, [xx, f64::from(y), zz])
            else {
                continue;
            };
            if !right_distance(l, chunk, [x, y, z], distance) {
                continue;
            }
            if current.is_none() {
                let list = if category == Category::WaterAmbient
                    && l.reduces_water_ambient(x, y, z)
                    && random.next_float() < 0.98
                {
                    None
                } else {
                    weighted_pick(mobs_at(l, category, x, y, z), random)
                };
                let Some(entry) = list else {
                    break;
                };
                max = sample_count(entry, random) as i32;
                let kind = l.mob_type(&entry.entity);
                current = Some((entry, kind, [x, y, z]));
            }
            let (entry, kind, picked_at) = current.unwrap_or_else(|| unreachable!());
            // The list the entry was picked from holds it where it was picked.
            let listed = picked_at == [x, y, z];
            let valid = valid_position_for_type(
                l,
                random,
                category,
                entry,
                kind,
                listed,
                [x, y, z],
                distance,
            );
            let Some((species, width, height, type_category)) = kind.filter(|_| valid) else {
                continue;
            };
            let charge = state.can_spawn(l, &entry.entity, [x, y, z]);
            trace(Event::Test(
                entry.entity.clone(),
                [x, y, z],
                charge.is_some(),
            ));
            let Some(charge) = charge else {
                continue;
            };
            // getMobForSpawn (a type the game cannot create, which would end
            // the pass, never passes isValidSpawnPostitionForType).
            let yaw = random.next_float() * 360.0;
            let bounds = spawn_box(xx, f64::from(y), zz, width, height);
            let despawn = f64::from(type_category.despawn_distance().pow(2));
            let far = distance > despawn && removed_when_far(species);
            if far
                || !mob_spawn_rules(species, l, x, y, z)
                || !spawn_obstruction(species, l, bounds, &spawned, x, y, z)
            {
                continue;
            }
            let request = SpawnRequest {
                entity: &entry.entity,
                species,
                position: [xx, f64::from(y), zz],
                yaw,
                group_index,
                reason: Reason::Natural,
            };
            let Some(result) = spawn(l, &request) else {
                continue;
            };
            group_index += 1;
            spawned.push(result.bounds);
            cluster += 1;
            state.after_spawn(type_category, [x, y, z], charge);
            trace(Event::Spawn(
                entry.entity.clone(),
                [xx, f64::from(y), zz],
                yaw,
            ));
            if cluster >= max_cluster_size(species) {
                return;
            }
            // TropicalFish.isMaxGroupSizeReached.
            if species == Species::TropicalFish && !result.school {
                break;
            }
        }
    }
}
/// `getNearestPlayer(x, y, z, -1, false)`'s squared distance for the
/// attempts in one chunk, which come in their thousands a tick. The spawn
/// checks only compare it with [`Nearest::CHECKS`], so the few players that can
/// be within 128 blocks of an attempt near the chunk are kept and measured:
/// when none is, the nearest reads as infinitely far, which every check answers
/// as it answers the real distance. In a crowd too big to keep, those kept give
/// the index's search a distance to beat; an attempt that wanders farther asks
/// the index outright. Either way the answer is good enough for the checks.
#[derive(Clone, Debug)]
struct Nearest {
    chunk: Option<(i32, i32)>,
    centre: (f64, f64),
    /// The kept players' positions by axis, then (in a crowd, which keeps
    /// [`Self::KEPT`]) the players the index's searches found last: an
    /// attempt's next tries are a few blocks away, where the same player is
    /// often near enough to end the search. Every other slot holds a player
    /// infinitely far, so whole blocks of [`Self::LANES`] can be measured.
    axes: [[f64; Self::SLOTS]; 3],
    kept: usize,
    found_len: usize,
    /// The found slot the next search's player replaces (the oldest).
    found_next: usize,
    crowded: bool,
}
impl Default for Nearest {
    fn default() -> Self {
        Self {
            chunk: None,
            centre: (0.0, 0.0),
            axes: [[f64::INFINITY; Self::SLOTS]; 3],
            kept: 0,
            found_len: 0,
            found_next: 0,
            crowded: false,
        }
    }
}
impl Nearest {
    /// `right_distance`'s 24² and the despawn distances, 64² and 128².
    const CHECKS: [f64; 3] = [576.0, 4096.0, 16384.0];
    const KEPT: usize = 16;
    const FOUND: usize = 4;
    const SLOTS: usize = Self::KEPT + Self::FOUND;
    /// Players measured together.
    const LANES: usize = 4;
    /// An attempt this far from the chunk's centre on both axes uses the
    /// kept players.
    const MARGIN: f64 = 48.0;
    /// 128 blocks past the corner of that square.
    const RADIUS_SQR: f64 = 196.0 * 196.0;
    fn distance_sqr(
        &mut self,
        players: &PlayerIndex,
        chunk: (i32, i32),
        at: [f64; 3],
    ) -> Option<f64> {
        if players.is_empty() {
            return None;
        }
        if self.chunk != Some(chunk) {
            self.keep_players(players, chunk);
        }
        let (cx, cz) = self.centre;
        if (at[0] - cx).abs() > Self::MARGIN || (at[2] - cz).abs() > Self::MARGIN {
            return Some(players.nearest_against(at, &Self::CHECKS));
        }
        let kept = self.measure(at);
        if !self.crowded {
            return Some(kept);
        }
        let (distance, found) = players.nearest_point_against_from(kept, at, &Self::CHECKS);
        if let Some(position) = found {
            let slot = Self::KEPT + self.found_next;
            for (axis, v) in self.axes.iter_mut().zip(position) {
                axis[slot] = v;
            }
            self.found_next = (self.found_next + 1) % Self::FOUND;
            self.found_len = (self.found_len + 1).min(Self::FOUND);
        }
        Some(distance)
    }
    /// The squared distance of the nearest kept or found player, or
    /// infinity. The slots past them hold players infinitely far, which no
    /// minimum takes.
    fn measure(&self, at: [f64; 3]) -> f64 {
        let used = if self.found_len > 0 {
            Self::KEPT + self.found_len
        } else {
            self.kept
        };
        let [xs, ys, zs] = &self.axes;
        let mut best = [f64::INFINITY; Self::LANES];
        for block in 0..used.div_ceil(Self::LANES) {
            for (lane, best) in best.iter_mut().enumerate() {
                let i = block * Self::LANES + lane;
                let d = (xs[i] - at[0]).powi(2) + (ys[i] - at[1]).powi(2) + (zs[i] - at[2]).powi(2);
                *best = best.min(d);
            }
        }
        best.into_iter().fold(f64::INFINITY, f64::min)
    }
    fn keep_players(&mut self, players: &PlayerIndex, chunk: (i32, i32)) {
        *self = Self {
            chunk: Some(chunk),
            centre: (f64::from(chunk.0 * 16 + 8), f64::from(chunk.1 * 16 + 8)),
            ..Self::default()
        };
        let (x, z) = self.centre;
        let _ = players.visit_within(x, z, Self::RADIUS_SQR, &mut |p| {
            if self.kept == Self::KEPT {
                self.crowded = true;
                return ControlFlow::Break(());
            }
            for (axis, v) in self.axes.iter_mut().zip(p.position) {
                axis[self.kept] = v;
            }
            self.kept += 1;
            ControlFlow::Continue(())
        });
    }
}
/// `isRightDistanceToPlayerAndSpawnPoint`.
fn right_distance<L: NaturalLevel>(l: &L, chunk: (i32, i32), pos: [i32; 3], distance: f64) -> bool {
    if distance <= 576.0 {
        return false;
    }
    if let Some(r) = l.respawn() {
        let d = [
            f64::from(r[0]) + 0.5 - (f64::from(pos[0]) + 0.5),
            f64::from(r[1]) + 0.5 - f64::from(pos[1]),
            f64::from(r[2]) + 0.5 - (f64::from(pos[2]) + 0.5),
        ];
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 24.0 * 24.0 {
            return false;
        }
    }
    let (cx, cz) = (pos[0] >> 4, pos[2] >> 4);
    (cx, cz) == chunk || l.can_spawn_in_chunk(cx, cz)
}
/// `isValidSpawnPostitionForType`, for the entry's type (`mob_type`).
/// `listed`: the position is where the entry was picked from `mobsAt`.
#[allow(clippy::too_many_arguments)]
fn valid_position_for_type<L: NaturalLevel, R: RandomSource>(
    l: &L,
    random: &mut R,
    category: Category,
    entry: &SpawnEntry,
    kind: Option<MobType>,
    listed: bool,
    pos: [i32; 3],
    distance: f64,
) -> bool {
    let Some((species, width, height, type_category)) = kind else {
        return false;
    };
    if type_category == Category::Misc {
        return false;
    }
    let despawn = f64::from(type_category.despawn_distance().pow(2));
    if !spawns_far_from_player(species, type_category) && distance > despawn {
        return false;
    }
    let [x, y, z] = pos;
    if !listed
        && !mobs_at(l, category, x, y, z)
            .iter()
            .any(|e| same_spawner(e, entry))
    {
        return false;
    }
    if !spawn_rules::is_spawn_position_ok(species, l, x, y, z) {
        return false;
    }
    if !spawn_rules::check_spawn_rules(species, l, Reason::Natural, random, x, y, z) {
        return false;
    }
    l.no_collision(spawn_aabb(
        species,
        f64::from(x) + 0.5,
        f64::from(y),
        f64::from(z) + 0.5,
        width,
        height,
    ))
}

/// What chunk generation spawning did (for differential tests).
#[derive(Clone, Debug, PartialEq)]
pub enum GenEvent {
    /// An attempt at a column (the heightmap lookup).
    Column(i32, i32),
    Spawn(String, [f64; 3], f32),
}
/// `NaturalSpawner.spawnMobsForChunkGeneration`: creature packs placed on
/// the chunk's surface. `rules` is the level's own random, which the spawn
/// predicates draw from.
#[allow(clippy::too_many_arguments)]
pub fn spawn_mobs_for_chunk_generation<L: NaturalLevel, R: RandomSource, Q: RandomSource>(
    l: &L,
    source: [i32; 3],
    chunk: (i32, i32),
    random: &mut R,
    rules: &mut Q,
    spawn: &mut dyn FnMut(&L, &SpawnRequest) -> Option<Spawned>,
    trace: &mut dyn FnMut(GenEvent),
) {
    if !l.spawn_mobs() {
        return;
    }
    let [sx, sy, sz] = source;
    let mobs = l
        .biome_spawns(sx, sy, sz)
        .and_then(|b| b.get("creature"))
        .cloned()
        .unwrap_or_default();
    if mobs.iter().map(|e| e.weight).sum::<u32>() == 0 {
        return;
    }
    let probability = l.creature_probability(sx, sy, sz);
    let (xo, zo) = (chunk.0 * 16, chunk.1 * 16);
    while random.next_float() < probability {
        let Some(entry) = weighted_pick(&mobs, random) else {
            continue;
        };
        let count = sample_count(entry, random);
        let mut group_index = 0u32;
        let mut spawned: Vec<Aabb> = Vec::new();
        let mut x = xo + random.next_int_bound(16).unwrap_or(0);
        let mut z = zo + random.next_int_bound(16).unwrap_or(0);
        let (start_x, start_z) = (x, z);
        let kind = l.mob_type(&entry.entity);
        for _ in 0..count {
            let mut success = false;
            let mut attempts = 0;
            while !success && attempts < 4 {
                attempts += 1;
                let Some((species, width, height, _)) = kind else {
                    break;
                };
                let pos = top_non_colliding(l, &entry.entity, x, z);
                trace(GenEvent::Column(x, z));
                if spawn_rules::is_spawn_position_ok(species, l, pos[0], pos[1], pos[2]) {
                    let fx = f64::from(x).clamp(
                        f64::from(xo) + f64::from(width),
                        f64::from(xo) + 16.0 - f64::from(width),
                    );
                    let fz = f64::from(z).clamp(
                        f64::from(zo) + f64::from(width),
                        f64::from(zo) + 16.0 - f64::from(width),
                    );
                    let fy = f64::from(pos[1]);
                    let (bx, bz) = (fx.floor() as i32, fz.floor() as i32);
                    if !l.no_collision(spawn_aabb(species, fx, fy, fz, width, height))
                        || !spawn_rules::check_spawn_rules(
                            species,
                            l,
                            Reason::ChunkGeneration,
                            rules,
                            bx,
                            pos[1],
                            bz,
                        )
                    {
                        continue;
                    }
                    let yaw = random.next_float() * 360.0;
                    let bounds = spawn_box(fx, fy, fz, width, height);
                    if mob_spawn_rules(species, l, bx, pos[1], bz)
                        && spawn_obstruction(species, l, bounds, &spawned, bx, pos[1], bz)
                    {
                        let request = SpawnRequest {
                            entity: &entry.entity,
                            species,
                            position: [fx, fy, fz],
                            yaw,
                            group_index,
                            reason: Reason::ChunkGeneration,
                        };
                        if let Some(result) = spawn(l, &request) {
                            group_index += 1;
                            spawned.push(result.bounds);
                            trace(GenEvent::Spawn(entry.entity.clone(), [fx, fy, fz], yaw));
                            success = true;
                        }
                    }
                }
                x += random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
                z += random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
                while x < xo || x >= xo + 16 || z < zo || z >= zo + 16 {
                    x = start_x + random.next_int_bound(5).unwrap_or(0)
                        - random.next_int_bound(5).unwrap_or(0);
                    z = start_z + random.next_int_bound(5).unwrap_or(0)
                        - random.next_int_bound(5).unwrap_or(0);
                }
            }
        }
    }
}
/// `getTopNonCollidingPos`: the type's heightmap, under the ceiling in
/// dimensions that have one, then the placement type's adjustment.
fn top_non_colliding<L: NaturalLevel>(l: &L, entity: &str, x: i32, z: i32) -> [i32; 3] {
    let heightmap = Heightmap::from_name(l.mob_data().heightmap(entity));
    let mut y = l.heightmap(heightmap, x, z);
    if l.has_ceiling() {
        loop {
            y -= 1;
            if l.facts().is_air(l.state(x, y, z)) {
                break;
            }
        }
        loop {
            y -= 1;
            if !(l.facts().is_air(l.state(x, y, z)) && y > l.min_y()) {
                break;
            }
        }
    }
    if l.mob_data().placement(entity) == super::data::Placement::OnGround
        && l.pathfindable_land(x, y - 1, z)
    {
        y -= 1;
    }
    [x, y, z]
}

#[cfg(test)]
mod tests {
    use super::Event;

    /// The lookahead draws what `LegacyRandom` draws, rejections included,
    /// and ends in its state.
    #[test]
    fn lookahead_draws_as_the_random_does() {
        use horizon_core::random::{LegacyRandom, RandomSource};
        let mut real = LegacyRandom::new(0x5eed);
        let mut copy = super::Lcg(real.state());
        for bound in [16, 1, 2, 3, 100, 385, 1 << 30, (1 << 30) + 1, i32::MAX, 0] {
            for _ in 0..64 {
                assert_eq!(
                    copy.next_int(bound),
                    real.next_int_bound(bound).unwrap_or(0)
                );
                assert_eq!(copy.0, real.state(), "{bound}");
            }
        }
    }

    /// `add_local` and `can_spawn_local` against vanilla's lists: every chunk's
    /// players found by a scan, their counts bumped one at a time, caps filling.
    #[test]
    fn local_caps_match_the_players_close_list() {
        use super::{Category, Nearest, PlayerIndex, SpawnState};
        let mut seed = 0x9e37_79b9_u64;
        let mut draw = |range: i32| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ((seed >> 33) as i32).rem_euclid(range)
        };
        let players: Vec<[f64; 3]> = (0..60)
            .map(|_| [f64::from(draw(600)), 64.0, f64::from(draw(600))])
            .collect();
        let mut state = SpawnState {
            spawnable_chunks: 289,
            counts: [0; 8],
            charges: Vec::new(),
            players: PlayerIndex::new(players.iter().copied()),
            player_counts: vec![[0; 8]; players.len()],
            local: None,
            prepared: Vec::new(),
            nearest: Nearest::default(),
        };
        state.label_open_categories();
        let close = |(cx, cz): (i32, i32)| -> Vec<usize> {
            let (x, z) = (f64::from(cx * 16 + 8), f64::from(cz * 16 + 8));
            (0..players.len())
                .filter(|&i| {
                    let (dx, dz) = (x - players[i][0], z - players[i][2]);
                    dx * dx + dz * dz < 16384.0
                })
                .collect()
        };
        let mut counts = vec![[0; 8]; players.len()];
        let mut filled = 0;
        for _ in 0..30_000 {
            let chunk = (draw(40), draw(40));
            let category = Category::SPAWNING[draw(7) as usize];
            let expected = close(chunk)
                .iter()
                .any(|&p| counts[p][category.index()] < category.max());
            if draw(5) == 0 {
                // Asked for in advance, as a spawn pass does.
                state.prepare_local(&[chunk, (draw(40), draw(40))], &Category::SPAWNING);
            }
            assert_eq!(state.can_spawn_local(category, chunk.0, chunk.1), expected);
            filled += usize::from(!expected);
            if draw(2) == 0 {
                state.add_local(chunk, category);
                for p in close(chunk) {
                    counts[p][category.index()] += 1;
                }
            }
        }
        assert!(filled > 1000, "the caps must fill: {filled}");
    }

    #[test]
    fn category_names_and_structure_bits_answer_as_the_lookups_do() {
        use super::{Category, Override, StructureSpawns};
        for c in Category::ALL {
            assert_eq!(Category::from_name(c.name()), c);
        }
        assert_eq!(Category::from_name("other"), Category::Misc);
        let mut spawns = StructureSpawns::default();
        let piece = || Override {
            piece: true,
            spawns: Vec::new(),
        };
        for (id, category) in [
            ("minecraft:monument", Category::Monster),
            ("minecraft:monument", Category::Axolotls),
            ("minecraft:fortress", Category::Creature),
        ] {
            spawns.insert(id.to_owned(), category, piece());
        }
        for id in [
            "minecraft:monument",
            "minecraft:fortress",
            "minecraft:mineshaft",
        ] {
            for c in Category::ALL {
                // The structures `mobs_at` wants.
                let wanted = c == Category::Monster && id == "minecraft:fortress"
                    || spawns.get(id, c).is_some();
                assert_eq!(
                    spawns.category_bits(id) & c.bit() != 0,
                    wanted,
                    "{id} {c:?}"
                );
            }
        }
    }

    #[test]
    fn nearest_answers_every_check_as_the_exact_distance() {
        use super::{Nearest, PlayerIndex};
        let mut seed = 0x29ab_7435_u64;
        let mut draw = |range: i32| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ((seed >> 33) as i32).rem_euclid(range)
        };
        // A few nearby, then more than the chunk keeps, and none at all near.
        for (count, spread) in [(3, 400), (40, 64), (300, 2000), (2, 100_000)] {
            let players: Vec<[f64; 3]> = (0..count)
                .map(|_| {
                    [
                        f64::from(draw(spread) - spread / 2),
                        f64::from(draw(100)),
                        f64::from(draw(spread) - spread / 2),
                    ]
                })
                .collect();
            let index = PlayerIndex::new(players.iter().copied());
            let mut nearest = Nearest::default();
            for _ in 0..2000 {
                let chunk = (draw(64) - 32, draw(64) - 32);
                for _ in 0..4 {
                    // Mostly in the chunk, sometimes past the kept players' reach.
                    let wander = if draw(8) == 0 { 400 } else { 60 };
                    let at = [
                        f64::from(chunk.0 * 16 + draw(wander) - wander / 2) + 0.5,
                        f64::from(draw(100)),
                        f64::from(chunk.1 * 16 + draw(wander) - wander / 2) + 0.5,
                    ];
                    let actual = nearest.distance_sqr(&index, chunk, at).unwrap();
                    let exact = players
                        .iter()
                        .map(|p| {
                            (p[0] - at[0]).powi(2) + (p[1] - at[1]).powi(2) + (p[2] - at[2]).powi(2)
                        })
                        .fold(f64::INFINITY, f64::min);
                    for cutoff in Nearest::CHECKS {
                        assert_eq!(actual <= cutoff, exact <= cutoff, "{chunk:?} {at:?}");
                    }
                }
            }
        }
        assert_eq!(
            Nearest::default().distance_sqr(&PlayerIndex::new([]), (0, 0), [0.0; 3]),
            None
        );
    }

    #[test]
    #[ignore = "focused release trace micro-benchmark; run with --release --ignored --nocapture"]
    fn bench_spawn_trace() {
        use std::{hint::black_box, time::Instant};
        // Model the event construction at a successful type check. The old
        // callback's vtable hides the game's no-op sink from this function.
        #[inline(never)]
        fn dynamic(trace: &mut dyn FnMut(Event), entity: &str) {
            trace(Event::Test(entity.to_owned(), [0, 64, 0], true));
        }
        fn specialized(trace: &mut impl FnMut(Event), entity: &str) {
            trace(Event::Test(entity.to_owned(), [0, 64, 0], true));
        }
        for _ in 0..3 {
            for optimized in [false, true] {
                let started = Instant::now();
                let mut trace = |_| {};
                for _ in 0..2_000_000 {
                    let entity = black_box("minecraft:zombie");
                    if optimized {
                        specialized(&mut trace, entity);
                    } else {
                        dynamic(black_box(&mut trace as &mut dyn FnMut(Event)), entity);
                    }
                }
                eprintln!("optimized={optimized}: {:?}", started.elapsed());
            }
        }
    }
}
