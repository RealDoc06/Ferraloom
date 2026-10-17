//! Fixed, multi-noise, and The End biome sources.
use crate::{Error, Identifier, climate::*, codec::*, density::*, router::CompiledRouter};
use ferraloom_data::datapack::ResourceManager;
use serde_json::Value;
use std::io::Read;

/// Port of the biome-source codecs used by NoiseBasedChunkGenerator.
#[derive(Clone, Debug)]
pub enum BiomeSource {
    Fixed(Identifier),
    MultiNoise(ParameterList<Identifier>),
    TheEnd,
}
impl BiomeSource {
    pub fn decode(v: &Value, resources: &ResourceManager) -> Result<Self, Error> {
        let ty = kind(v)?;
        match ty.to_string().as_str() {
            "minecraft:fixed" => Ok(Self::Fixed(biome_id(field(v, "biome")?, resources)?)),
            "minecraft:the_end" => Ok(Self::TheEnd),
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
        let volume = DensityVolume {
            min: min.map(|n| n.wrapping_shl(2)),
            size,
            step: [4; 3],
        };
        if let Self::MultiNoise(parameters) = self {
            router
                .sample_climate_volume(compiler, samplers, volume)
                .into_iter()
                .map(|target| parameters.find(target, search).clone())
                .collect()
        } else {
            (0..volume.len())
                .map(|i| {
                    self.sample(
                        compiler,
                        router,
                        samplers,
                        search,
                        volume.position(i).map(|n| n >> 2),
                    )
                })
                .collect()
        }
    }
}
fn biome_id(v: &Value, resources: &ResourceManager) -> Result<Identifier, Error> {
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
