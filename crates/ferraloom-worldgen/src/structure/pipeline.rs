//! Structure-start and reference chunk statuses. The generation job owns the
//! target chunk; registries, placements and dependency starts are read-only.
use super::{
    TerrainAdjustment,
    decoration::StructureStep,
    placement::{BiomeSet, ChunkGeneratorStructureState},
    references::StructureReferences,
    start::StructureStart,
    template::StructureTemplateManager,
};
use crate::{
    Error, Identifier, decoration::GenerationStep, feature::PlacedFeature,
    generator::GenerationData,
};
use ferraloom_data::{datapack::ResourceManager, registries::worldgen_structure_type};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{ChunkPos, mth};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub struct StructureDefinition {
    pub kind: worldgen_structure_type::Id,
    pub biomes: Vec<Identifier>,
    pub step: GenerationStep,
    pub terrain: TerrainAdjustment,
}
#[derive(Clone, Debug, Default)]
pub struct StructureRegistry {
    pub structures: BTreeMap<Identifier, StructureDefinition>,
}
impl StructureRegistry {
    pub fn load(resources: &ResourceManager, data: &GenerationData) -> Result<Self, Error> {
        let steps = StructureStep::load(resources)?
            .into_iter()
            .map(|s| (s.id, s.step))
            .collect::<BTreeMap<_, _>>();
        let structures = resources
            .registry("worldgen/structure")
            .into_iter()
            .map(|(id, resource)| {
                let value = resource.json()?;
                let kind = crate::codec::id(crate::codec::field(&value, "type")?)?;
                let kind = worldgen_structure_type::Id::from_name(&kind.to_string())
                    .ok_or_else(|| Error(format!("unknown structure type {kind}")))?;
                let biomes = match BiomeSet::decode(crate::codec::field(&value, "biomes")?)? {
                    BiomeSet::Direct(ids) => ids,
                    BiomeSet::Tag(tag) => data
                        .biome_tags
                        .get(&tag)
                        .cloned()
                        .ok_or_else(|| Error(format!("unknown biome tag {tag}")))?,
                };
                if let Some(id) = biomes.iter().find(|id| !data.biome_ids.contains_key(*id)) {
                    return Err(Error(format!("unknown structure biome {id}")));
                }
                let terrain = value
                    .get("terrain_adaptation")
                    .map(|v| TerrainAdjustment::decode(crate::codec::string(v)?))
                    .transpose()?
                    .unwrap_or(TerrainAdjustment::None);
                let step = steps[&id];
                Ok((
                    id,
                    StructureDefinition {
                        kind,
                        biomes,
                        step,
                        terrain,
                    },
                ))
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self { structures })
    }
    pub fn terrain_settings(&self) -> BTreeMap<Identifier, TerrainAdjustment> {
        self.structures
            .iter()
            .map(|(id, s)| (id.clone(), s.terrain))
            .collect()
    }
    /// Port of hasBiomesForStructureSet; the remaining sets keep registry order.
    pub fn possible_structure_sets(
        &self,
        state: &ChunkGeneratorStructureState,
        possible_biomes: &[Identifier],
    ) -> Result<Vec<Identifier>, Error> {
        let mut possible = Vec::new();
        for (id, set) in &state.sets {
            let mut matches = false;
            for entry in &set.structures {
                let structure = self
                    .structures
                    .get(&entry.structure)
                    .ok_or_else(|| Error(format!("unknown structure {}", entry.structure)))?;
                matches |= structure
                    .biomes
                    .iter()
                    .any(|id| possible_biomes.contains(id));
            }
            if matches {
                possible.push(id.clone());
            }
        }
        Ok(possible)
    }
}
/// The typed payload for a chunk's structure_starts/structure_references stages.
/// Its NBT boundary is storage::schema::StructureData's structures compound.
#[derive(Clone, Debug)]
pub struct ChunkStructures {
    pub position: [i32; 2],
    pub starts: BTreeMap<Identifier, StructureStart>,
    pub references: BTreeMap<Identifier, StructureReferences>,
}
impl ChunkStructures {
    pub fn new(position: [i32; 2]) -> Self {
        Self {
            position,
            starts: BTreeMap::new(),
            references: BTreeMap::new(),
        }
    }
    pub fn to_nbt(&self) -> Result<CompoundTag, Error> {
        let starts = self
            .starts
            .iter()
            .map(|(id, start)| Ok((id.to_string(), Tag::Compound(start.to_nbt(self.position)?))))
            .collect::<Result<CompoundTag, Error>>()?;
        let references = self
            .references
            .iter()
            .filter(|(_, set)| !set.is_empty())
            .map(|(id, set)| (id.to_string(), Tag::LongArray(set.iter().collect())))
            .collect::<CompoundTag>();
        Ok([
            ("starts", Tag::Compound(starts)),
            ("References", Tag::Compound(references)),
        ]
        .into_iter()
        .collect())
    }
    pub fn from_nbt(
        position: [i32; 2],
        tag: &CompoundTag,
        registry: &StructureRegistry,
        templates: &StructureTemplateManager,
        resources: &ResourceManager,
        decode_feature: &mut impl FnMut(&serde_json::Value) -> Result<Arc<PlacedFeature>, Error>,
    ) -> Result<Self, Error> {
        let mut chunk = Self::new(position);
        let settings = registry.terrain_settings();
        if let Some(starts) = tag.get("starts").and_then(Tag::as_compound) {
            for (key, value) in starts.iter() {
                let Ok(id) = key.to_string_lossy().parse::<Identifier>() else {
                    continue;
                };
                if registry.structures.contains_key(&id)
                    && let Some(value) = value.as_compound()
                    && let Some(start) = StructureStart::from_nbt(
                        value,
                        &settings,
                        templates,
                        resources,
                        decode_feature,
                    )?
                {
                    chunk.starts.insert(id, start);
                }
            }
        }
        if let Some(references) = tag.get("References").and_then(Tag::as_compound) {
            for (key, value) in references.iter() {
                let Ok(id) = key.to_string_lossy().parse::<Identifier>() else {
                    continue;
                };
                if !registry.structures.contains_key(&id) {
                    continue;
                }
                if let Some(values) = value.as_long_array() {
                    let values = values
                        .iter()
                        .copied()
                        .filter(|&value| {
                            let source = ChunkPos::unpack(value);
                            mth::chessboard_distance(position[0], position[1], source.x, source.z)
                                <= 8
                        })
                        .collect::<Vec<_>>();
                    chunk
                        .references
                        .insert(id, StructureReferences::from_array(&values));
                }
            }
        }
        Ok(chunk)
    }
    pub fn known_valid_starts(&self) -> BTreeMap<Identifier, i32> {
        self.starts
            .iter()
            .filter(|(_, s)| s.is_valid())
            .map(|(id, s)| (id.clone(), s.references))
            .collect()
    }
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
