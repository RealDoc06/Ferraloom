//! StructurePlacement and ChunkGeneratorStructureState placement calculations.
use crate::{Error, Identifier, codec::*};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource, WorldgenRandom};
use serde_json::Value;
use std::collections::BTreeMap;

/// Port of RandomSpreadType.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RandomSpreadType {
    Linear,
    Triangular,
}
impl RandomSpreadType {
    fn evaluate(self, random: &mut impl RandomSource, limit: i32) -> i32 {
        match self {
            Self::Linear => random.next_int_bound(limit),
            Self::Triangular => (random.next_int_bound(limit) + random.next_int_bound(limit)) / 2,
        }
    }
}
/// Port of AbstractSpreadingStructurePlacement.FrequencyReductionMethod.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrequencyReductionMethod {
    Default,
    LegacyType1,
    LegacyType2,
    LegacyType3,
}
impl FrequencyReductionMethod {
    pub fn should_generate(self, seed: i64, salt: i32, [x, z]: [i32; 2], probability: f32) -> bool {
        let mut random = WorldgenRandom::new(LegacyRandomSource::new(0));
        match self {
            Self::Default => {
                // The unusual argument order is vanilla's probabilityReducer.
                random.set_large_feature_with_salt(seed, salt, x, z);
                random.next_float() < probability
            }
            Self::LegacyType1 => {
                random.set_seed(i64::from((x >> 4) ^ (z >> 4).wrapping_shl(4)) ^ seed);
                random.next_int();
                random.next_int_bound((1.0_f32 / probability) as i32) == 0
            }
            Self::LegacyType2 => {
                random.set_large_feature_with_salt(seed, x, z, 10387320);
                random.next_float() < probability
            }
            Self::LegacyType3 => {
                random.set_large_feature_seed(seed, x, z);
                random.next_double() < f64::from(probability)
            }
        }
    }
}
/// Port of StructureSet's holder codec: named or directly embedded sets.
#[derive(Clone, Debug, PartialEq)]
pub enum StructureSetHolder {
    Reference(Identifier),
    Direct(Box<StructureSet>),
}
/// Port of AbstractSpreadingStructurePlacement.ExclusionZone.
#[derive(Clone, Debug, PartialEq)]
pub struct ExclusionZone {
    pub other_set: StructureSetHolder,
    pub chunk_count: i32,
}
/// Shared settings of random-spread and ring placements.
#[derive(Clone, Debug, PartialEq)]
pub struct Spreading {
    pub locate_offset: [i32; 3],
    pub frequency_reduction_method: FrequencyReductionMethod,
    pub frequency: f32,
    pub salt: i32,
    pub exclusion_zone: Option<ExclusionZone>,
}
impl Spreading {
    fn decode(v: &Value) -> Result<Self, Error> {
        let locate_offset = optional(v, "locate_offset", [0; 3], |v| {
            let values = list(v)?;
            if values.len() != 3 {
                return Err(Error("locate_offset must have three coordinates".into()));
            }
            Ok([
                int_range(&values[0], -16, 16)?,
                int_range(&values[1], -16, 16)?,
                int_range(&values[2], -16, 16)?,
            ])
        })?;
        let method = optional(
            v,
            "frequency_reduction_method",
            FrequencyReductionMethod::Default,
            |v| match string(v)? {
                "default" => Ok(FrequencyReductionMethod::Default),
                "legacy_type_1" => Ok(FrequencyReductionMethod::LegacyType1),
                "legacy_type_2" => Ok(FrequencyReductionMethod::LegacyType2),
                "legacy_type_3" => Ok(FrequencyReductionMethod::LegacyType3),
                name => Err(Error(format!("unknown frequency reduction method {name}"))),
            },
        )?;
        let frequency = optional(v, "frequency", 1.0, |v| {
            let n = float(v)?;
            if (0.0..=1.0).contains(&n) {
                Ok(n)
            } else {
                Err(Error("structure frequency outside [0,1]".into()))
            }
        })?;
        let exclusion_zone = v
            .get("exclusion_zone")
            .map(|v| {
                let other = field(v, "other_set")?;
                Ok::<_, Error>(ExclusionZone {
                    other_set: if other.is_string() {
                        StructureSetHolder::Reference(id(other)?)
                    } else {
                        StructureSetHolder::Direct(Box::new(StructureSet::decode(other)?))
                    },
                    chunk_count: int_range(field(v, "chunk_count")?, 1, 16)?,
                })
            })
            .transpose()?;
        Ok(Self {
            locate_offset,
            frequency_reduction_method: method,
            frequency,
            salt: int_range(field(v, "salt")?, 0, i32::MAX)?,
            exclusion_zone,
        })
    }
    pub fn apply_additional_chunk_restrictions(&self, seed: i64, chunk: [i32; 2]) -> bool {
        self.frequency >= 1.0
            || self.frequency_reduction_method.should_generate(
                seed,
                self.salt,
                chunk,
                self.frequency,
            )
    }
}
/// Port of RandomSpreadStructurePlacement.
#[derive(Clone, Debug, PartialEq)]
pub struct RandomSpread {
    pub spreading: Spreading,
    pub spacing: i32,
    pub separation: i32,
    pub spread_type: RandomSpreadType,
}
impl RandomSpread {
    pub fn potential_structure_chunk(&self, seed: i64, chunk: [i32; 2]) -> [i32; 2] {
        let grid = chunk.map(|v| v.div_euclid(self.spacing));
        let mut random = WorldgenRandom::new(LegacyRandomSource::new(0));
        random.set_large_feature_with_salt(seed, grid[0], grid[1], self.spreading.salt);
        let limit = self.spacing - self.separation;
        grid.map(|v| {
            v.wrapping_mul(self.spacing)
                .wrapping_add(self.spread_type.evaluate(&mut random, limit))
        })
    }
}
/// Port of ConcentricRingsStructurePlacement. Biomes may be a tag or a list.
#[derive(Clone, Debug, PartialEq)]
pub struct ConcentricRings {
    pub spreading: Spreading,
    pub distance: i32,
    pub spread: i32,
    pub count: i32,
    pub preferred_biomes: BiomeSet,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BiomeSet {
    Tag(Identifier),
    Direct(Vec<Identifier>),
}
impl BiomeSet {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        if let Some(tag) = v.as_str().and_then(|s| s.strip_prefix('#')) {
            Ok(Self::Tag(tag.parse()?))
        } else if v.is_string() {
            Ok(Self::Direct(vec![id(v)?]))
        } else {
            Ok(Self::Direct(
                list(v)?.iter().map(id).collect::<Result<_, _>>()?,
            ))
        }
    }
}
impl ConcentricRings {
    /// Port of ChunkGeneratorStructureState#generateRingPositions. The search
    /// callback gets vanilla's independently forked RNG, so scheduling cannot
    /// affect either later ring locations or reservoir sampling within a ring.
    pub fn generate_positions(
        &self,
        seed: i64,
        mut search: impl FnMut([i32; 2], &BiomeSet, &mut Random) -> Result<Option<[i32; 2]>, Error>,
    ) -> Result<Vec<[i32; 2]>, Error> {
        let mut random = LegacyRandomSource::new(seed);
        let mut angle = random.next_double() * std::f64::consts::PI * 2.0;
        let mut position = 0;
        let mut circle = 0;
        let mut spread = self.spread;
        let mut result = Vec::with_capacity(self.count as usize);
        for i in 0..self.count {
            let distance = f64::from(4 * self.distance + self.distance * circle * 6)
                + (random.next_double() - 0.5) * (f64::from(self.distance) * 2.5);
            let initial = [
                (angle.cos() * distance + 0.5).floor() as i32,
                (angle.sin() * distance + 0.5).floor() as i32,
            ];
            let mut fork = random.fork();
            result.push(search(initial, &self.preferred_biomes, &mut fork)?.unwrap_or(initial));
            angle += std::f64::consts::TAU / f64::from(spread);
            position += 1;
            if position == spread {
                circle += 1;
                position = 0;
                spread += 2 * spread / (circle + 1);
                spread = spread.min(self.count - i);
                angle += random.next_double() * std::f64::consts::PI * 2.0;
            }
        }
        Ok(result)
    }
}
/// Port of StructurePlacement's 26.3 dispatch, including dimension_origin.
#[derive(Clone, Debug, PartialEq)]
pub enum StructurePlacement {
    RandomSpread(RandomSpread),
    ConcentricRings(ConcentricRings),
    DimensionOrigin,
}
impl StructurePlacement {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        match kind(v)?.to_string().as_str() {
            "minecraft:random_spread" => {
                let spreading = Spreading::decode(v)?;
                let spacing = int_range(field(v, "spacing")?, 0, 4096)?;
                let separation = int_range(field(v, "separation")?, 0, 4096)?;
                if spacing <= separation {
                    return Err(Error("Spacing has to be larger than separation".into()));
                }
                let spread_type =
                    optional(
                        v,
                        "spread_type",
                        RandomSpreadType::Linear,
                        |v| match string(v)? {
                            "linear" => Ok(RandomSpreadType::Linear),
                            "triangular" => Ok(RandomSpreadType::Triangular),
                            s => Err(Error(format!("unknown spread type {s}"))),
                        },
                    )?;
                Ok(Self::RandomSpread(RandomSpread {
                    spreading,
                    spacing,
                    separation,
                    spread_type,
                }))
            }
            "minecraft:concentric_rings" => Ok(Self::ConcentricRings(ConcentricRings {
                spreading: Spreading::decode(v)?,
                distance: int_range(field(v, "distance")?, 0, 1023)?,
                spread: int_range(field(v, "spread")?, 0, 1023)?,
                count: int_range(field(v, "count")?, 1, 4095)?,
                preferred_biomes: BiomeSet::decode(field(v, "preferred_biomes")?)?,
            })),
            "minecraft:dimension_origin" => Ok(Self::DimensionOrigin),
            s => Err(Error(format!("unknown structure placement {s}"))),
        }
    }
    pub fn spreading(&self) -> Option<&Spreading> {
        match self {
            Self::RandomSpread(p) => Some(&p.spreading),
            Self::ConcentricRings(p) => Some(&p.spreading),
            Self::DimensionOrigin => None,
        }
    }
    pub fn locate_pos(&self, [x, z]: [i32; 2]) -> [i32; 3] {
        let offset = self.spreading().map_or([0; 3], |p| p.locate_offset);
        [
            x.wrapping_mul(16).wrapping_add(offset[0]),
            offset[1],
            z.wrapping_mul(16).wrapping_add(offset[2]),
        ]
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StructureSelectionEntry {
    pub structure: Identifier,
    pub weight: i32,
}
/// Port of StructureSet. Selection removes failures and redraws with the same
/// RNG; eagerly shuffling choices would consume a different random stream.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureSet {
    pub structures: Vec<StructureSelectionEntry>,
    pub placement: StructurePlacement,
}
impl StructureSet {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let structures = list(field(v, "structures")?)?
            .iter()
            .map(|e| {
                Ok(StructureSelectionEntry {
                    structure: id(field(e, "structure")?)?,
                    weight: int_range(field(e, "weight")?, 1, i32::MAX)?,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            structures,
            placement: StructurePlacement::decode(field(v, "placement")?)?,
        })
    }
    pub fn try_select(
        &self,
        seed: i64,
        chunk: [i32; 2],
        mut generate: impl FnMut(&StructureSelectionEntry) -> Result<bool, Error>,
    ) -> Result<bool, Error> {
        if self.structures.len() == 1 {
            return generate(&self.structures[0]);
        }
        let mut remaining = self.structures.iter().collect::<Vec<_>>();
        let mut total = remaining.iter().try_fold(0_i32, |total, e| {
            total
                .checked_add(e.weight)
                .ok_or_else(|| Error("structure selection weight overflow".into()))
        })?;
        let mut random = WorldgenRandom::new(LegacyRandomSource::new(0));
        random.set_large_feature_seed(seed, chunk[0], chunk[1]);
        while !remaining.is_empty() {
            let mut choice = random.next_int_bound(total);
            let index = remaining
                .iter()
                .position(|e| {
                    choice -= e.weight;
                    choice < 0
                })
                .unwrap();
            let selected = remaining[index];
            if generate(selected)? {
                return Ok(true);
            }
            total -= selected.weight;
            remaining.remove(index);
        }
        Ok(false)
    }
}
/// Placement-only portion of ChunkGeneratorStructureState. Ring positions are
/// computed once before sharing, including biome search; queries are immutable.
#[derive(Clone, Debug)]
pub struct ChunkGeneratorStructureState {
    pub seed: i64,
    pub origin: [i32; 2],
    pub sets: BTreeMap<Identifier, StructureSet>,
    pub rings: Vec<(ConcentricRings, Vec<[i32; 2]>)>,
}
impl ChunkGeneratorStructureState {
    pub fn load(resources: &ResourceManager, seed: i64, origin: [i32; 2]) -> Result<Self, Error> {
        let sets = resources
            .registry("worldgen/structure_set")
            .into_iter()
            .map(|(id, r)| Ok((id, StructureSet::decode(&r.json()?)?)))
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            seed,
            origin,
            sets,
            rings: Vec::new(),
        })
    }
    pub fn is_structure_chunk(
        &self,
        placement: &StructurePlacement,
        chunk: [i32; 2],
    ) -> Result<bool, Error> {
        self.check(placement, chunk, &mut Vec::new())
    }
    fn check(
        &self,
        placement: &StructurePlacement,
        chunk: [i32; 2],
        path: &mut Vec<Identifier>,
    ) -> Result<bool, Error> {
        let potential = match placement {
            StructurePlacement::RandomSpread(p) => {
                p.potential_structure_chunk(self.seed, chunk) == chunk
            }
            StructurePlacement::ConcentricRings(p) => self
                .rings
                .iter()
                .find(|(r, _)| r == p)
                .is_some_and(|(_, v)| v.contains(&chunk)),
            StructurePlacement::DimensionOrigin => self.origin == chunk,
        };
        if !potential {
            return Ok(false);
        }
        let Some(p) = placement.spreading() else {
            return Ok(true);
        };
        if !p.apply_additional_chunk_restrictions(self.seed, chunk) {
            return Ok(false);
        }
        if let Some(zone) = &p.exclusion_zone {
            let other = match &zone.other_set {
                StructureSetHolder::Reference(id) => {
                    if path.contains(id) {
                        return Err(Error(format!("cyclic structure exclusion zone {id}")));
                    }
                    path.push(id.clone());
                    self.sets
                        .get(id)
                        .ok_or_else(|| Error(format!("missing structure set {id}")))?
                }
                StructureSetHolder::Direct(set) => set,
            };
            let radius = zone.chunk_count;
            let forbidden = (|| {
                for x in chunk[0] - radius..=chunk[0] + radius {
                    for z in chunk[1] - radius..=chunk[1] + radius {
                        if self.check(&other.placement, [x, z], path)? {
                            return Ok::<_, Error>(true);
                        }
                    }
                }
                Ok(false)
            })();
            if matches!(zone.other_set, StructureSetHolder::Reference(_)) {
                path.pop();
            }
            if forbidden? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

/// BiomeSource#findBiomeHorizontal as called by the ring placement search.
/// FixedBiomeSource overrides the reservoir algorithm with two block draws.
pub fn find_ring_biome(
    generator: &crate::generator::NoiseBasedChunkGenerator,
    chunk: [i32; 2],
    preferred: &BiomeSet,
    random: &mut Random,
) -> Result<Option<[i32; 2]>, Error> {
    use crate::{biome::BiomeSource, climate::SearchContext, density::SamplerContext};
    let allowed = match preferred {
        BiomeSet::Direct(ids) => ids.as_slice(),
        BiomeSet::Tag(id) => generator
            .data
            .biome_tags
            .get(id)
            .ok_or_else(|| Error(format!("missing biome tag {id}")))?
            .as_slice(),
    };
    let block = chunk.map(|v| v.wrapping_mul(16).wrapping_add(8));
    if let BiomeSource::Fixed(id) = &generator.source {
        return Ok(allowed
            .contains(id)
            .then(|| block.map(|v| (v - 112 + random.next_int_bound(225)) >> 4)));
    }
    let quart = block.map(|v| v >> 2);
    let mut context = SamplerContext::cached();
    let mut search = SearchContext::default();
    let mut result = None;
    let mut found = 0;
    for z in -28..=28 {
        for x in -28..=28 {
            let p = [quart[0] + x, 0, quart[1] + z];
            let biome = generator.source.sample(
                &generator.random.compiler,
                generator.random.router,
                &mut context,
                &mut search,
                p,
            );
            if allowed.contains(&biome) {
                if result.is_none() || random.next_int_bound(found + 1) == 0 {
                    result = Some([p[0] >> 2, p[2] >> 2]);
                }
                found += 1;
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
#[path = "placement_tests.rs"]
mod tests;
