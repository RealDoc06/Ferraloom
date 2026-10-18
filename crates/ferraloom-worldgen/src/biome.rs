//! Fixed, multi-noise, and The End biome sources.
use crate::{Error, Identifier, climate::*, codec::*, density::*, router::CompiledRouter};
use ferraloom_data::datapack::ResourceManager;
use serde_json::Value;
use std::io::Read;

/// Port of the biome-source codecs used by NoiseBasedChunkGenerator.
#[derive(Clone, Debug)]
pub enum BiomeSource {
    Fixed(Identifier),
    Checkerboard { biomes: Vec<Identifier>, scale: i32 },
    MultiNoise(ParameterList<Identifier>),
    TheEnd,
}
impl BiomeSource {
    pub fn possible_biomes(&self) -> Vec<Identifier> {
        match self {
            Self::Fixed(id) => vec![id.clone()],
            Self::Checkerboard { biomes, .. } => {
                let mut seen = std::collections::BTreeSet::new();
                let mut result = Vec::new();
                for id in biomes {
                    if seen.insert(id.clone()) {
                        result.push(id.clone());
                    }
                }
                result
            }
            Self::MultiNoise(parameters) => {
                // BiomeSource collects into an ImmutableSet: first encounter
                // order determines FeatureSorter's global per-step indices.
                let mut seen = std::collections::BTreeSet::new();
                let mut result = Vec::new();
                for (_, id) in parameters.values() {
                    if seen.insert(id.clone()) {
                        result.push(id.clone());
                    }
                }
                result
            }
            Self::TheEnd => [
                "the_end",
                "end_highlands",
                "end_midlands",
                "small_end_islands",
                "end_barrens",
            ]
            .map(|name| name.parse().unwrap())
            .to_vec(),
        }
    }
    pub fn decode(v: &Value, resources: &ResourceManager) -> Result<Self, Error> {
        let ty = kind(v)?;
        match ty.to_string().as_str() {
            "minecraft:fixed" => Ok(Self::Fixed(biome_id(field(v, "biome")?, resources)?)),
            "minecraft:the_end" => Ok(Self::TheEnd),
            "minecraft:checkerboard" => {
                let entries = field(v, "biomes")?;
                let biomes = if let Some(tag) = entries.as_str().and_then(|s| s.strip_prefix('#')) {
                    let names = resources.registry("worldgen/biome");
                    let tags = ferraloom_data::datapack::load_tags(
                        resources,
                        "worldgen/biome",
                        |id, _| names.contains_key(id).then(|| id.clone()),
                    );
                    if !tags.diagnostics.is_empty() {
                        return Err(Error("invalid biome source tags".into()));
                    }
                    tags.values
                        .get(&tag.parse()?)
                        .cloned()
                        .ok_or_else(|| Error(format!("missing biome tag {tag}")))?
                } else if entries.is_string() || entries.is_object() {
                    vec![biome_id(entries, resources)?]
                } else {
                    list(entries)?
                        .iter()
                        .map(|v| biome_id(v, resources))
                        .collect::<Result<_, _>>()?
                };
                Ok(Self::Checkerboard {
                    biomes,
                    scale: optional(v, "scale", 2, |v| int_range(v, 0, 62))?,
                })
            }
            "minecraft:multi_noise" => {
                if let Some(biomes) = v.get("biomes") {
                    let values = list(biomes)?
                        .iter()
                        .map(|v| {
                            Ok((
                                ParameterPoint::decode(field(v, "parameters")?)?,
                                biome_id(field(v, "biome")?, resources)?,
                            ))
                        })
                        .collect::<Result<_, Error>>()?;
                    Ok(Self::MultiNoise(ParameterList::new(values)?))
                } else {
                    let preset = field(v, "preset")?;
                    let definition = if preset.is_string() {
                        resources
                            .require(&id(preset)?.with_path(format!(
                                "worldgen/multi_noise_biome_source_parameter_list/{}.json",
                                id(preset)?.path()
                            ))?)?
                            .json()?
                    } else {
                        preset.clone()
                    };
                    let name = id(field(&definition, "preset")?)?;
                    Ok(Self::MultiNoise(preset_parameters(&name)?))
                }
            }
            _ => Err(Error(format!("unsupported biome source {ty}"))),
        }
    }
    pub fn sample(
        &self,
        compiler: &DensityFunctionCompiler,
        router: CompiledRouter,
        samplers: &mut SamplerContext,
        search: &mut SearchContext,
        quart: [i32; 3],
    ) -> Identifier {
        match self {
            Self::Fixed(id) => id.clone(),
            Self::Checkerboard { biomes, scale } => {
                let shift = (scale + 2) as u32;
                let index = quart[0]
                    .wrapping_shr(shift)
                    .wrapping_add(quart[2].wrapping_shr(shift))
                    .rem_euclid(biomes.len() as i32);
                biomes[index as usize].clone()
            }
            Self::MultiNoise(parameters) => parameters
                .find(router.sample_climate(compiler, samplers, quart), search)
                .clone(),
            Self::TheEnd => {
                let block = quart.map(|v| v.wrapping_shl(2));
                let cx = block[0] >> 4;
                let cz = block[2] >> 4;
                let name = if i64::from(cx) * i64::from(cx) + i64::from(cz) * i64::from(cz) <= 4096
                {
                    "the_end"
                } else {
                    let x = cx.wrapping_mul(2).wrapping_add(1).wrapping_mul(8);
                    let z = cz.wrapping_mul(2).wrapping_add(1).wrapping_mul(8);
                    let height =
                        compiler.sample_value(router.functions[3], samplers, [x, block[1], z]);
                    if height > 0.25 {
                        "end_highlands"
                    } else if height >= -0.0625 {
                        "end_midlands"
                    } else if height < -0.21875 {
                        "small_end_islands"
                    } else {
                        "end_barrens"
                    }
                };
                name.parse().unwrap()
            }
        }
    }
    /// Port of MultiNoiseBiomeSource#createResolverForChunk. Climate volume
    /// sampling is required here; scalar sampling can select a different biome.
    pub fn sample_quart_grid(
        &self,
        compiler: &DensityFunctionCompiler,
        router: CompiledRouter,
        samplers: &mut SamplerContext,
        search: &mut SearchContext,
        min: [i32; 3],
        size: [usize; 3],
    ) -> Vec<Identifier> {
        self.sample_quart_grid_with_blender(compiler, router, samplers, search, min, size, None)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn sample_quart_grid_with_blender(
        &self,
        compiler: &DensityFunctionCompiler,
        router: CompiledRouter,
        samplers: &mut SamplerContext,
        search: &mut SearchContext,
        min: [i32; 3],
        size: [usize; 3],
        blender: Option<&crate::blending::Blender>,
    ) -> Vec<Identifier> {
        let volume = DensityVolume {
            min: min.map(|n| n.wrapping_shl(2)),
            size,
            step: [4; 3],
        };
        if let Self::MultiNoise(parameters) = self {
            let targets = router.sample_climate_volume(compiler, samplers, volume);
            let mut result = vec![parameters.values()[0].1.clone(); volume.len()];
            for section_y in (0..size[1]).step_by(4) {
                for x in 0..size[0] {
                    for y in section_y..(section_y + 4).min(size[1]) {
                        for z in 0..size[2] {
                            let index = volume.index(x, y, z);
                            result[index] = blender
                                .and_then(|b| {
                                    b.blend_biome([
                                        min[0] + x as i32,
                                        min[1] + y as i32,
                                        min[2] + z as i32,
                                    ])
                                })
                                .unwrap_or_else(|| parameters.find(targets[index], search).clone());
                        }
                    }
                }
            }
            result
        } else {
            (0..volume.len())
                .map(|i| {
                    let quart = volume.position(i).map(|n| n >> 2);
                    blender
                        .and_then(|b| b.blend_biome(quart))
                        .unwrap_or_else(|| self.sample(compiler, router, samplers, search, quart))
                })
                .collect()
        }
    }
}
fn biome_id(v: &Value, resources: &ResourceManager) -> Result<Identifier, Error> {
    if v.is_object() {
        return Err(Error(
            "Unsupported: inline biome holders require section and protocol biome identity".into(),
        ));
    }
    let id = id(v)?;
    resources.require(&id.with_path(format!("worldgen/biome/{}.json", id.path()))?)?;
    Ok(id)
}

/// Numeric facts extracted by our Java harness from Preset#knownPresets, like
/// the generated block registries. No vanilla source or JSON assets are embedded.
pub fn preset_parameters(name: &Identifier) -> Result<ParameterList<Identifier>, Error> {
    let mut bytes = String::new();
    flate2::read::GzDecoder::new(include_bytes!("biome_presets.json.gz").as_slice())
        .read_to_string(&mut bytes)
        .map_err(|e| Error(e.to_string()))?;
    let presets: Value = serde_json::from_str(&bytes)?;
    let values = presets
        .get(name.to_string())
        .ok_or_else(|| Error(format!("unknown biome preset {name}")))?;
    let values = list(values)?
        .iter()
        .map(|v| {
            let point = list(v)?;
            let mut ranges = [Parameter { min: 0, max: 0 }; 6];
            for i in 0..6 {
                ranges[i] = Parameter {
                    min: point[i][0]
                        .as_i64()
                        .ok_or_else(|| Error("invalid generated parameter".into()))?,
                    max: point[i][1]
                        .as_i64()
                        .ok_or_else(|| Error("invalid generated parameter".into()))?,
                };
            }
            Ok((
                ParameterPoint {
                    ranges,
                    offset: point[6][0].as_i64().unwrap(),
                },
                id(&point[7])?,
            ))
        })
        .collect::<Result<_, Error>>()?;
    ParameterList::new(values)
}
