//! Initial world spawn and PlayerSpawnFinder's ordered searches.
use crate::{
    Error,
    climate::quantize,
    density::SamplerContext,
    generator::{ChunkGeneratorAccess, RandomState},
    region::Heightmap,
    settings::DimensionType,
};
use ferraloom_util::random::RandomSource;

/// Block#isFaceFull for the upward face of a cached collision shape. Dynamic
/// shapes must be obtained from the live block behavior instead.
pub fn cached_collision_up_face_full(state: ferraloom_data::BlockState) -> Option<bool> {
    if state.cached_collision_full_block()? {
        return Some(true);
    }
    let boxes = state
        .data()
        .collision
        .boxes()
        .iter()
        .filter(|bounds| bounds.min_y <= 0.9999999 && bounds.max_y > 0.9999999)
        .collect::<Vec<_>>();
    if boxes.iter().any(|bounds| {
        bounds.min_x < 0.0 || bounds.max_x > 1.0 || bounds.min_z < 0.0 || bounds.max_z > 1.0
    }) {
        return Some(false);
    }
    let mut xs = vec![0.0, 1.0];
    let mut zs = vec![0.0, 1.0];
    for bounds in &boxes {
        xs.extend([bounds.min_x, bounds.max_x]);
        zs.extend([bounds.min_z, bounds.max_z]);
    }
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    zs.sort_by(f64::total_cmp);
    zs.dedup();
    Some(xs.windows(2).all(|x| {
        zs.windows(2).all(|z| {
            boxes.iter().any(|bounds| {
                bounds.min_x <= x[0]
                    && bounds.max_x >= x[1]
                    && bounds.min_z <= z[0]
                    && bounds.max_z >= z[1]
            })
        })
    }))
}

/// Port of NoiseSpawnFinder. Density samples are quart-aligned; distance bias
/// uses the original block coordinates and Java's wrapping long arithmetic.
pub fn noise_spawn_position(random: &RandomState) -> [i32; 2] {
    if random.spawn_targets.is_empty() {
        return [0, 0];
    }
    let mut context = SamplerContext::cached();
    let mut fitness = |position: [i32; 2]| {
        let p = [(position[0] >> 2) << 2, 0, (position[1] >> 2) << 2];
        let climate = random
            .spawn_targets
            .iter()
            .map(|target| {
                target.iter().fold(0i64, |sum, (node, range)| {
                    let distance = range.distance(quantize(random.compiler.sample_value(
                        *node,
                        &mut context,
                        p,
                    )));
                    sum.wrapping_add(distance.wrapping_mul(distance))
                })
            })
            .min()
            .unwrap();
        climate
            .wrapping_mul(2048 * 2048)
            .wrapping_add(i64::from(position[0]).pow(2))
            .wrapping_add(i64::from(position[1]).pow(2))
    };
    let mut best = ([0, 0], fitness([0, 0]));
    for (max_radius, increment) in [(2048f32, 512f32), (512f32, 32f32)] {
        let origin = best.0;
        let mut angle = 0f32;
        let mut radius = increment;
        while radius <= max_radius {
            let position = [
                origin[0].wrapping_add((f64::from(angle).sin() * f64::from(radius)) as i32),
                origin[1].wrapping_add((f64::from(angle).cos() * f64::from(radius)) as i32),
            ];
            let candidate = fitness(position);
            if candidate < best.1 {
                best = (position, candidate);
            }
            angle += increment / radius;
            if f64::from(angle) > std::f64::consts::TAU {
                angle = 0.0;
                radius += increment;
            }
        }
    }
    best.0
}

/// Game-backed queries used by MinecraftServer#setInitialSpawn and
/// PlayerSpawnFinder. Heights are the highest matching block Y, as returned by
/// LevelChunk#getHeight (one less than region.height_at). Loading must finish
/// in request order and retain the actual generated chunks and their writes.
pub trait SpawnWorld {
    fn dimension(&self) -> DimensionType;
    fn ensure_chunk(&mut self, position: [i32; 2]) -> Result<(), Error>;
    fn height(&mut self, map: Heightmap, x: i32, z: i32) -> Result<i32, Error>;
    fn has_fluid(&mut self, p: [i32; 3]) -> Result<bool, Error>;
    fn collision_up_face_full(&mut self, p: [i32; 3]) -> Result<bool, Error>;
    /// PLAYER's real bounding box at the block's bottom center, liquids included.
    fn no_collision_no_liquid(&mut self, p: [i32; 3]) -> Result<bool, Error>;
    fn place_bonus_chest(&mut self, _p: [i32; 3]) -> Result<(), Error> {
        Err(Error("Unsupported: bonus chest generation".into()))
    }
}

/// Port of PlayerSpawnFinder#getLevelRespawnPos.
pub fn level_respawn_position(
    world: &mut impl SpawnWorld,
    generator: &dyn ChunkGeneratorAccess,
    x: i32,
    z: i32,
) -> Result<Option<[i32; 3]>, Error> {
    let dimension = world.dimension();
    let top = if dimension.has_ceiling {
        generator.get_spawn_height(dimension.min_y, dimension.height)
    } else {
        world.height(Heightmap::MotionBlocking, x, z)?
    };
    if top < dimension.min_y {
        return Ok(None);
    }
    let surface = world.height(Heightmap::WorldSurface, x, z)?;
    if surface <= top && surface > world.height(Heightmap::OceanFloor, x, z)? {
        return Ok(None);
    }
    for y in (dimension.min_y..=top + 1).rev() {
        let p = [x, y, z];
        if world.has_fluid(p)? {
            break;
        }
        if world.collision_up_face_full(p)? {
            return Ok(Some([x, y + 1, z]));
        }
    }
    Ok(None)
}

pub fn spawn_position_in_chunk(
    world: &mut impl SpawnWorld,
    generator: &dyn ChunkGeneratorAccess,
    position: [i32; 2],
) -> Result<Option<[i32; 3]>, Error> {
    world.ensure_chunk(position)?;
    for x in position[0] * 16..=position[0] * 16 + 15 {
        for z in position[1] * 16..=position[1] * 16 + 15 {
            if let Some(p) = level_respawn_position(world, generator, x, z)? {
                return Ok(Some(p));
            }
        }
    }
    Ok(None)
}

/// Port of MinecraftServer#setInitialSpawn's origin and 121-chunk spiral.
pub fn initial_spawn(
    world: &mut impl SpawnWorld,
    generator: &dyn ChunkGeneratorAccess,
    debug: bool,
    bonus_chest: bool,
) -> Result<[i32; 3], Error> {
    if debug {
        return Ok([0, 80, 0]);
    }
    let origin = generator.get_origin()?;
    let dimension = world.dimension();
    let mut height = generator.get_spawn_height(dimension.min_y, dimension.height);
    if height < dimension.min_y {
        world.ensure_chunk(origin)?;
        height = world.height(
            Heightmap::WorldSurface,
            origin[0] * 16 + 8,
            origin[1] * 16 + 8,
        )? + 1;
    }
    let mut result = [origin[0] * 16 + 8, height, origin[1] * 16 + 8];
    let (mut x, mut z, mut dx, mut dz) = (0, 0, 0, -1);
    for _ in 0..121 {
        if (-5..=5).contains(&x) && (-5..=5).contains(&z) {
            if let Some(position) =
                spawn_position_in_chunk(world, generator, [origin[0] + x, origin[1] + z])?
            {
                result = position;
                break;
            }
        }
        if x == z || (x < 0 && x == -z) || (x > 0 && x == 1 - z) {
            (dx, dz) = (-dz, dx);
        }
        x += dx;
        z += dz;
    }
    if bonus_chest {
        world.place_bonus_chest(result)?;
    }
    Ok(result)
}

/// Port of PlayerSpawnFinder's candidate permutation. A caller-owned RNG makes
/// the fresh thread-local vanilla draw injectable for parity comparisons.
#[derive(Clone, Debug)]
pub struct PlayerSpawnFinder {
    suggestion: [i32; 3],
    radius: i32,
    count: i32,
    coprime: i32,
    offset: i32,
    next: i32,
}
impl PlayerSpawnFinder {
    pub fn new(
        suggestion: [i32; 3],
        radius: i32,
        border_distance: f64,
        random: &mut impl RandomSource,
    ) -> Self {
        let mut radius = radius.max(0);
        let border = border_distance.floor() as i32;
        if border < radius {
            radius = border;
        }
        if border <= 1 {
            radius = 1;
        }
        let side = i64::from(radius) * 2 + 1;
        let count = (side * side).min(1024) as i32;
        Self {
            suggestion,
            radius,
            count,
            coprime: if count <= 16 { count - 1 } else { 17 },
            offset: random.next_int_bound(count),
            next: 0,
        }
    }
    pub fn next_candidate(&mut self) -> Option<[i32; 2]> {
        if self.next >= self.count {
            return None;
        }
        let value = (self.offset + self.coprime * self.next) % self.count;
        self.next += 1;
        let side = self.radius * 2 + 1;
        Some([
            self.suggestion[0] + value % side - self.radius,
            self.suggestion[2] + value / side - self.radius,
        ])
    }
    pub fn find_spawn(
        &mut self,
        world: &mut impl SpawnWorld,
        generator: &dyn ChunkGeneratorAccess,
    ) -> Result<[f64; 3], Error> {
        while let Some([x, z]) = self.next_candidate() {
            world.ensure_chunk([x >> 4, z >> 4])?;
            if let Some(p) = level_respawn_position(world, generator, x, z)?
                && world.no_collision_no_liquid(p)?
            {
                return Ok(bottom_center(p));
            }
        }
        world.ensure_chunk([self.suggestion[0] >> 4, self.suggestion[2] >> 4])?;
        fixup_spawn_height(world, self.suggestion)
    }
}

/// Adventure mode goes directly here without constructing a finder or drawing RNG.
pub fn fixup_spawn_height(world: &mut impl SpawnWorld, mut p: [i32; 3]) -> Result<[f64; 3], Error> {
    let dimension = world.dimension();
    while !world.no_collision_no_liquid(p)? && p[1] < dimension.min_y + dimension.height - 1 {
        p[1] += 1;
    }
    p[1] -= 1;
    while world.no_collision_no_liquid(p)? && p[1] > dimension.min_y {
        p[1] -= 1;
    }
    p[1] += 1;
    Ok(bottom_center(p))
}
fn bottom_center(p: [i32; 3]) -> [f64; 3] {
    [
        f64::from(p[0]) + 0.5,
        f64::from(p[1]),
        f64::from(p[2]) + 0.5,
    ]
}
