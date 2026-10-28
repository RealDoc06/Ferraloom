//! Generation-relevant dimension and noise-generator settings codecs.
use crate::{
    Error, Identifier, climate::Parameter, codec::*, density::*, material::*, router::NoiseRouter,
};
use ferraloom_data::{BlockState, datapack::decode_block_state};
use serde_json::Value;
use std::collections::BTreeMap;

/// Port of NoiseSettings#CODEC and guardY.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NoiseSettings {
    pub min_y: i32,
    pub height: i32,
}
impl NoiseSettings {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let min_y = int_range(field(v, "min_y")?, -2032, 2031)?;
        let height = int_range(field(v, "height")?, 0, 4064)?;
        if min_y + height > 2032 || height % 16 != 0 || min_y % 16 != 0 {
            return Err(Error(
                "noise height and min_y must align to sections within the dimension bounds".into(),
            ));
        }
        Ok(Self { min_y, height })
    }
}
/// Port of Aquifer.Config.
#[derive(Clone, Debug)]
pub struct AquiferConfig {
    pub functions: [Function; 6],
}
impl AquiferConfig {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let names = [
            "barrier",
            "fluid_level_floodedness",
            "fluid_level_spread",
            "lava",
            "exclusion",
            "surface_level",
        ];
        let functions = names
            .iter()
            .map(|name| DensityFunction::decode(field(v, name)?))
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap();
        Ok(Self { functions })
    }
}
#[derive(Clone, Debug)]
pub struct NoiseGeneratorSettings {
    pub noise: NoiseSettings,
    pub default_block: BlockState,
    pub default_fluid: BlockState,
    pub router: NoiseRouter,
    pub material_rule: Rule,
    pub spawn_target: Vec<BTreeMap<Identifier, Parameter>>,
    pub sea_level: i32,
    pub disable_mob_generation: bool,
    pub aquifers: Option<AquiferConfig>,
    pub legacy_random_source: bool,
    pub debug_functions: Vec<(String, Function)>,
}
impl NoiseGeneratorSettings {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let spawn_target = list(field(v, "spawn_target")?)?
            .iter()
            .map(|v| {
                let entries = v
                    .as_object()
                    .ok_or_else(|| Error("expected spawn target function map".into()))?;
                entries
                    .iter()
                    .map(|(id, v)| Ok((id.parse()?, Parameter::decode(v)?)))
                    .collect::<Result<_, Error>>()
            })
            .collect::<Result<_, _>>()?;
        let debug_functions = optional(v, "debug_functions", Vec::new(), |v| {
            list(v)?
                .iter()
                .map(|v| {
                    Ok((
                        string(field(v, "label")?)?.to_owned(),
                        DensityFunction::decode(field(v, "function")?)?,
                    ))
                })
                .collect::<Result<_, Error>>()
        })?;
        Ok(Self {
            noise: NoiseSettings::decode(field(v, "noise")?)?,
            default_block: decode_block_state(field(v, "default_block")?)?,
            default_fluid: decode_block_state(field(v, "default_fluid")?)?,
            router: NoiseRouter::decode(field(v, "noise_router")?)?,
            material_rule: MaterialRule::decode(field(v, "material_rule")?)?,
            spawn_target,
            sea_level: integer(field(v, "sea_level")?)?,
            disable_mob_generation: boolean(field(v, "disable_mob_generation")?)?,
            aquifers: v.get("aquifers").map(AquiferConfig::decode).transpose()?,
            legacy_random_source: boolean(field(v, "legacy_random_source")?)?,
            debug_functions,
        })
    }
}

/// Port of DimensionType#DIRECT_CODEC's generation and infiniburn fields.
#[derive(Clone, Debug)]
pub struct DimensionType {
    pub min_y: i32,
    pub height: i32,
    pub logical_height: i32,
    pub has_ceiling: bool,
    pub has_skylight: bool,
    /// FAST_LAVA's dimension constant, after applying the dimension modifier.
    pub fast_lava: bool,
    pub coordinate_scale: f64,
    /// DimensionType#infiniburn: the decoded block holder set, independent of the level key.
    pub infiniburn: crate::structure::processor::BlockSet,
}
impl DimensionType {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let noise = NoiseSettings::decode(v)?;
        let logical_height = int_range(field(v, "logical_height")?, 0, 4064)?;
        if noise.height < 16 || logical_height > noise.height {
            return Err(Error("invalid dimension logical height".into()));
        }
        Ok(Self {
            min_y: noise.min_y,
            height: noise.height,
            logical_height,
            has_ceiling: boolean(field(v, "has_ceiling")?)?,
            has_skylight: boolean(field(v, "has_skylight")?)?,
            fast_lava: dimension_fast_lava(v)?,
            coordinate_scale: bounded(
                field(v, "coordinate_scale")?,
                9.999999747378752E-6,
                30000000.0,
            )?,
            infiniburn: crate::structure::processor::BlockSet::decode(field(v, "infiniburn")?)?,
        })
    }
}

/// Port of WorldGenRegion's static EnvironmentAttributeSystem dimension layer.
/// FAST_LAVA is non-positional and starts at false; biome layers do not affect it.
fn dimension_fast_lava(v: &Value) -> Result<bool, Error> {
    let Some(attributes) = v.get("attributes").filter(|value| !value.is_null()) else {
        return Ok(false);
    };
    let attributes = attributes
        .as_object()
        .ok_or_else(|| Error("expected dimension environment attribute map".into()))?;
    let fast_lava: Identifier = "minecraft:gameplay/fast_lava".parse()?;
    let mut entry = None;
    for (name, value) in attributes {
        if name.parse::<Identifier>()? == fast_lava && entry.replace(value).is_some() {
            return Err(Error("duplicate FAST_LAVA dimension attribute".into()));
        }
    }
    let Some(entry) = entry else {
        return Ok(false);
    };
    if !entry.is_object() {
        return boolean(entry);
    }
    let modifier = string(field(entry, "modifier")?)?;
    let argument = boolean(field(entry, "argument")?)?;
    Ok(crate::environment::BooleanModifier::decode(modifier)?.apply(false, argument))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn dimension_fast_lava_matches_vanilla_attributes_and_modifiers() {
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../tests/golden/fast_lava.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let decoded = DimensionType::decode(&case["config"]);
            assert_eq!(
                decoded.is_ok(),
                case["success"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
            if let Ok(dimension) = decoded {
                assert_eq!(
                    dimension.fast_lava,
                    case["fast_lava"].as_bool().unwrap(),
                    "{}",
                    case["name"]
                );
            }
        }
    }
}
