//! Typed codecs for 26.3 material rules (formerly SurfaceRules).
use crate::{Error, Identifier, codec::*, density::DensityFunction, density::Function};
use ferraloom_data::{
    BlockState,
    datapack::{ResourceManager, decode_block_state},
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

pub type Rule = Arc<MaterialRule>;
pub type Condition = Arc<MaterialCondition>;
/// Port of VerticalAnchor#CODEC, whose alternatives are exclusive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerticalAnchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
    RelativeToSeaLevel(i32),
}
impl VerticalAnchor {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let alternatives = [
            ("absolute", Self::Absolute as fn(i32) -> Self),
            ("above_bottom", Self::AboveBottom),
            ("below_top", Self::BelowTop),
            ("relative_to_sea_level", Self::RelativeToSeaLevel),
        ];
        let mut decoded = Vec::new();
        for (name, ctor) in alternatives {
            if let Some(value) = v.get(name)
                && let Ok(n) = int_range(value, -2032, 2031)
            {
                decoded.push(ctor(n));
            }
        }
        if decoded.len() != 1 {
            return Err(Error(
                "vertical anchor needs exactly one valid alternative".into(),
            ));
        }
        Ok(decoded[0])
    }
    pub fn resolve(self, min_y: i32, height: i32, sea_level: i32) -> i32 {
        match self {
            Self::Absolute(n) => n,
            Self::AboveBottom(n) => min_y.wrapping_add(n),
            Self::BelowTop(n) => height.wrapping_sub(1).wrapping_add(min_y).wrapping_sub(n),
            Self::RelativeToSeaLevel(n) => sea_level.wrapping_add(n),
        }
    }
}
#[derive(Clone, Debug)]
pub enum BiomeSet {
    Direct(Vec<Identifier>),
    Tag(Identifier),
}
#[derive(Clone, Debug)]
pub enum MaterialRule {
    Reference(Identifier),
    Block(BlockState),
    Sequence(Vec<Rule>),
    Condition {
        condition: Condition,
        then: Rule,
    },
    Bandlands,
    OreVein {
        ore: BlockState,
        raw: BlockState,
        filler: BlockState,
        raw_chance: f32,
        density: Function,
        richness: Function,
        filler_gap: Function,
    },
}
#[derive(Clone, Debug)]
pub enum MaterialCondition {
    Reference(Identifier),
    Biome(BiomeSet),
    NoiseThreshold {
        noise: Identifier,
        min: f64,
        max: f64,
        is_3d: bool,
    },
    Not(Condition),
    YAbove {
        anchor: VerticalAnchor,
        surface_multiplier: i32,
        add_stone: bool,
    },
    Water {
        offset: i32,
        surface_multiplier: i32,
        add_stone: bool,
    },
    StoneDepth {
        offset: i32,
        add_surface: bool,
        secondary_range: i32,
        ceiling: bool,
    },
    VerticalGradient {
        random_name: Identifier,
        below: VerticalAnchor,
        above: VerticalAnchor,
    },
    Temperature,
    Steep,
    Hole,
    AbovePreliminarySurface,
}
impl MaterialRule {
    pub fn decode(v: &Value) -> Result<Rule, Error> {
        if v.is_string() {
            return Ok(Arc::new(Self::Reference(id(v)?)));
        }
        let ty = kind(v)?;
        let result = match ty.to_string().as_str() {
            "minecraft:block" => Self::Block(decode_block_state(field(v, "result_state")?)?),
            "minecraft:sequence" => Self::Sequence(
                list(field(v, "sequence")?)?
                    .iter()
                    .map(Self::decode)
                    .collect::<Result<_, _>>()?,
            ),
            "minecraft:condition" => Self::Condition {
                condition: MaterialCondition::decode(field(v, "if_true")?)?,
                then: Self::decode(field(v, "then_run")?)?,
            },
            "minecraft:bandlands" => Self::Bandlands,
            "minecraft:ore_vein" => Self::OreVein {
                ore: decode_block_state(field(v, "ore_block")?)?,
                raw: decode_block_state(field(v, "raw_ore_block")?)?,
                filler: decode_block_state(field(v, "filler_block")?)?,
                raw_chance: {
                    let n = float(field(v, "raw_ore_chance")?)?;
                    if n.total_cmp(&0.0).is_lt() || n.total_cmp(&1.0).is_gt() {
                        return Err(Error("ore chance outside [0,1]".into()));
                    }
                    n
                },
                density: DensityFunction::decode(field(v, "density")?)?,
                richness: DensityFunction::decode(field(v, "richness")?)?,
                filler_gap: DensityFunction::decode(field(v, "filler_gap")?)?,
            },
            _ => return Err(Error(format!("unknown material rule {ty}"))),
        };
        Ok(Arc::new(result))
    }
}
impl MaterialCondition {
    pub fn decode(v: &Value) -> Result<Condition, Error> {
        if v.is_string() {
            return Ok(Arc::new(Self::Reference(id(v)?)));
        }
        let ty = kind(v)?;
        let result = match ty.to_string().as_str() {
            "minecraft:biome" => {
                let biomes = field(v, "biome_is")?;
                let set = if let Some(name) = biomes.as_str() {
                    if let Some(tag) = name.strip_prefix('#') {
                        BiomeSet::Tag(tag.parse()?)
                    } else {
                        BiomeSet::Direct(vec![id(biomes)?])
                    }
                } else {
                    let values = list(biomes)?
                        .iter()
                        .map(id)
                        .collect::<Result<Vec<_>, _>>()?;
                    BiomeSet::Direct(values)
                };
                Self::Biome(set)
            }
            "minecraft:noise_threshold" => Self::NoiseThreshold {
                noise: id(field(v, "noise")?)?,
                min: double(field(v, "min_threshold")?)?,
                max: double(field(v, "max_threshold")?)?,
                is_3d: optional(v, "is_3d", false, boolean)?,
            },
            "minecraft:not" => Self::Not(Self::decode(field(v, "invert")?)?),
            "minecraft:y_above" => Self::YAbove {
                anchor: VerticalAnchor::decode(field(v, "anchor")?)?,
                surface_multiplier: int_range(field(v, "surface_depth_multiplier")?, -20, 20)?,
                add_stone: boolean(field(v, "add_stone_depth")?)?,
            },
            "minecraft:water" => Self::Water {
                offset: integer(field(v, "offset")?)?,
                surface_multiplier: int_range(field(v, "surface_depth_multiplier")?, -20, 20)?,
                add_stone: boolean(field(v, "add_stone_depth")?)?,
            },
            "minecraft:stone_depth" => Self::StoneDepth {
                offset: integer(field(v, "offset")?)?,
                add_surface: boolean(field(v, "add_surface_depth")?)?,
                secondary_range: integer(field(v, "secondary_depth_range")?)?,
                ceiling: match string(field(v, "surface_type")?)? {
                    "floor" => false,
                    "ceiling" => true,
                    _ => return Err(Error("unknown cave surface".into())),
                },
            },
            "minecraft:vertical_gradient" => Self::VerticalGradient {
                random_name: id(field(v, "random_name")?)?,
                below: VerticalAnchor::decode(field(v, "true_at_and_below")?)?,
                above: VerticalAnchor::decode(field(v, "false_at_and_above")?)?,
            },
            "minecraft:temperature" => Self::Temperature,
            "minecraft:steep" => Self::Steep,
            "minecraft:hole" => Self::Hole,
            "minecraft:above_preliminary_surface" => Self::AbovePreliminarySurface,
            _ => return Err(Error(format!("unknown material condition {ty}"))),
        };
        Ok(Arc::new(result))
    }
}
#[derive(Clone, Debug, Default)]
pub struct MaterialRegistry {
    pub rules: BTreeMap<Identifier, Rule>,
    pub conditions: BTreeMap<Identifier, Condition>,
}
impl MaterialRegistry {
    pub fn load(resources: &ResourceManager) -> Result<Self, Error> {
        let rules = resources
            .registry("worldgen/material_rule")
            .into_iter()
            .map(|(id, r)| Ok((id, MaterialRule::decode(&r.json()?)?)))
            .collect::<Result<_, Error>>()?;
        let conditions = resources
            .registry("worldgen/material_condition")
            .into_iter()
            .map(|(id, r)| Ok((id, MaterialCondition::decode(&r.json()?)?)))
            .collect::<Result<_, Error>>()?;
        Ok(Self { rules, conditions })
    }
}
