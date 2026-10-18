//! Typed NbtOps encoders used by saved structure pieces. Numeric codec output
//! keeps the codec's tag type (FLOAT/INT), rather than JSON's narrowest number.
use super::{
    processor::{BlockSet, ProcessorRule, RuleBlockEntityModifier, StructureProcessor},
    rule::{PosRuleTest, RuleTest},
};
use crate::{placement::providers::IntProvider, region::Heightmap};
use ferraloom_data::BlockState;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};

/// NbtOps#getNumberValue followed by Number#intValue. Floating-point codecs
/// truncate toward zero; CompoundTag#getInt uses the separate NBT floor rule.
pub(super) fn ops_int(tag: &Tag) -> Option<i32> {
    match tag {
        Tag::Float(value) => Some(*value as i32),
        Tag::Double(value) => Some(*value as i32),
        _ => tag.as_int(),
    }
}
pub(super) fn string(value: impl ToString) -> Tag {
    Tag::String(value.to_string().into())
}
pub(super) fn compound<const N: usize>(values: [(&str, Tag); N]) -> Tag {
    Tag::Compound(values.into_iter().collect())
}
pub(super) fn sequence(values: impl IntoIterator<Item = Tag>) -> Tag {
    Tag::List(ListTag(values.into_iter().collect()))
}
fn dispatched(kind: &str, ty: &str, fields: impl IntoIterator<Item = (&'static str, Tag)>) -> Tag {
    let mut out: CompoundTag = fields.into_iter().collect();
    out.insert(kind, string(format!("minecraft:{ty}")));
    Tag::Compound(out)
}
pub(super) fn block_state(state: BlockState) -> Tag {
    if state == state.block().default_state() {
        return string(state.block().name());
    }
    compound([
        ("id", string(state.block().name())),
        (
            "properties",
            Tag::Compound(state.properties().map(|(k, v)| (k, string(v))).collect()),
        ),
    ])
}
impl RuleTest {
    pub fn to_nbt(&self) -> Tag {
        let mut fields = Vec::new();
        let name = match self {
            Self::AlwaysTrue => "always_true",
            Self::BlockMatch(block) => {
                fields.push(("block", string(block.name())));
                "block_match"
            }
            Self::BlockStateMatch(state) => {
                fields.push(("block_state", block_state(*state)));
                "blockstate_match"
            }
            Self::TagMatch(id) => {
                fields.push(("tag", string(id)));
                "tag_match"
            }
            Self::HeightMatch { min, max } => {
                fields.extend([
                    ("min_inclusive", Tag::Int(*min)),
                    ("max_inclusive", Tag::Int(*max)),
                ]);
                "height_match"
            }
            Self::RandomBlockMatch { block, probability } => {
                fields.extend([
                    ("block", string(block.name())),
                    ("probability", Tag::Float(*probability)),
                ]);
                "random_block_match"
            }
            Self::RandomBlockStateMatch { state, probability } => {
                fields.extend([
                    ("block_state", block_state(*state)),
                    ("probability", Tag::Float(*probability)),
                ]);
                "random_blockstate_match"
            }
            Self::AllOf(rules) | Self::AnyOf(rules) => {
                fields.push(("rules", sequence(rules.iter().map(Self::to_nbt))));
                if matches!(self, Self::AllOf(_)) {
                    "all_of"
                } else {
                    "any_of"
                }
            }
            Self::Not(rule) => {
                fields.push(("rule", rule.to_nbt()));
                "not"
            }
        };
        dispatched("predicate_type", name, fields)
    }
}
impl PosRuleTest {
    pub fn to_nbt(&self) -> Tag {
        let (name, min_chance, max_chance, min_dist, max_dist, axis) = match *self {
            Self::AlwaysTrue => return dispatched("predicate_type", "always_true", []),
            Self::Linear {
                min_chance,
                max_chance,
                min_dist,
                max_dist,
            } => ("linear_pos", min_chance, max_chance, min_dist, max_dist, 1),
            Self::AxisAlignedLinear {
                min_chance,
                max_chance,
                min_dist,
                max_dist,
                axis,
            } => (
                "axis_aligned_linear_pos",
                min_chance,
                max_chance,
                min_dist,
                max_dist,
                axis,
            ),
        };
        let mut fields = Vec::new();
        if min_chance.to_bits() != 0 {
            fields.push(("min_chance", Tag::Float(min_chance)));
        }
        if max_chance.to_bits() != 0 {
            fields.push(("max_chance", Tag::Float(max_chance)));
        }
        if min_dist != 0 {
            fields.push(("min_dist", Tag::Int(min_dist)));
        }
        if max_dist != 0 {
            fields.push(("max_dist", Tag::Int(max_dist)));
        }
        if axis != 1 {
            fields.push(("axis", string(["x", "y", "z"][axis])));
        }
        dispatched("predicate_type", name, fields)
    }
}
impl BlockSet {
    pub fn to_nbt(&self) -> Tag {
        match self {
            Self::Tag(id) => string(format!("#{id}")),
            Self::Direct(blocks) => match blocks.as_slice() {
                [block] => string(block.name()),
                _ => sequence(blocks.iter().map(|b| string(b.name()))),
            },
        }
    }
}
impl RuleBlockEntityModifier {
    pub fn to_nbt(&self) -> Tag {
        let (name, fields) = match self {
            Self::Passthrough => ("passthrough", Vec::new()),
            Self::Clear => ("clear", Vec::new()),
            Self::AppendLoot(id) => ("append_loot", vec![("loot_table", string(id))]),
            Self::AppendStatic(tag) => {
                ("append_static", vec![("data", Tag::Compound(tag.clone()))])
            }
        };
        dispatched("type", name, fields)
    }
}
impl ProcessorRule {
    pub fn to_nbt(&self) -> Tag {
        let mut tag = CompoundTag::new();
        tag.insert("input_predicate", self.input.to_nbt());
        tag.insert("location_predicate", self.location.to_nbt());
        tag.insert("output_state", block_state(self.output));
        if !matches!(self.position, PosRuleTest::AlwaysTrue) {
            tag.insert("position_predicate", self.position.to_nbt());
        }
        if !matches!(
            self.block_entity_modifier,
            RuleBlockEntityModifier::Passthrough
        ) {
            tag.insert("block_entity_modifier", self.block_entity_modifier.to_nbt());
        }
        Tag::Compound(tag)
    }
}
pub(super) fn heightmap_name(map: Heightmap) -> &'static str {
    match map {
        Heightmap::WorldSurfaceWg => "WORLD_SURFACE_WG",
        Heightmap::WorldSurface => "WORLD_SURFACE",
        Heightmap::OceanFloorWg => "OCEAN_FLOOR_WG",
        Heightmap::OceanFloor => "OCEAN_FLOOR",
        Heightmap::MotionBlocking => "MOTION_BLOCKING",
        Heightmap::MotionBlockingNoLeaves => "MOTION_BLOCKING_NO_LEAVES",
    }
}
impl StructureProcessor {
    pub fn to_nbt(&self) -> Tag {
        let mut fields = Vec::new();
        let name = match self {
            Self::Nop => "nop",
            Self::BlockIgnore(blocks) => {
                fields.push(("blocks", sequence(blocks.iter().map(|b| string(b.name())))));
                "block_ignore"
            }
            Self::BlockRot {
                integrity,
                rottable,
            } => {
                fields.push(("integrity", Tag::Float(*integrity)));
                if let Some(blocks) = rottable {
                    fields.push(("rottable_blocks", blocks.to_nbt()));
                }
                "block_rot"
            }
            Self::BlockAge { mossiness } => {
                fields.push(("mossiness", Tag::Float(*mossiness)));
                "block_age"
            }
            Self::BlackstoneReplace => "blackstone_replace",
            Self::Gravity { heightmap, offset } => {
                if *heightmap != Heightmap::WorldSurfaceWg {
                    fields.push(("heightmap", string(heightmap_name(*heightmap))));
                }
                if *offset != 0 {
                    fields.push(("offset", Tag::Int(*offset)));
                }
                "gravity"
            }
            Self::JigsawReplacement => "jigsaw_replacement",
            Self::LavaSubmergedBlock => "lava_submerged_block",
            Self::ProtectedBlocks(blocks) => {
                fields.push(("value", blocks.to_nbt()));
                "protected_blocks"
            }
            Self::Rule(rules) => {
                fields.push(("rules", sequence(rules.iter().map(ProcessorRule::to_nbt))));
                "rule"
            }
            Self::Capped { delegate, limit } => {
                fields.extend([
                    ("delegate", delegate.to_nbt()),
                    ("limit", int_provider(limit)),
                ]);
                "capped"
            }
        };
        dispatched("processor_type", name, fields)
    }
}
fn int_provider(provider: &IntProvider) -> Tag {
    let mut fields = Vec::new();
    let (name, range) = match provider {
        IntProvider::Constant(value) => return Tag::Int(*value),
        IntProvider::Uniform(min, max) => ("uniform", Some((*min, *max))),
        IntProvider::BiasedToBottom(min, max) => ("biased_to_bottom", Some((*min, *max))),
        IntProvider::VeryBiasedToBottom(min, max) => ("very_biased_to_bottom", Some((*min, *max))),
        IntProvider::Clamped { source, min, max } => {
            fields.push(("source", int_provider(source)));
            ("clamped", Some((*min, *max)))
        }
        IntProvider::ClampedNormal {
            mean,
            deviation,
            min,
            max,
        } => {
            fields.extend([
                ("mean", Tag::Float(*mean)),
                ("deviation", Tag::Float(*deviation)),
            ]);
            ("clamped_normal", Some((*min, *max)))
        }
        IntProvider::WeightedList(list) => {
            fields.push((
                "distribution",
                sequence(list.entries().iter().map(|(data, weight)| {
                    compound([("data", int_provider(data)), ("weight", Tag::Int(*weight))])
                })),
            ));
            ("weighted_list", None)
        }
        IntProvider::Trapezoid { min, max, plateau } => {
            fields.extend([
                ("min", Tag::Int(*min)),
                ("max", Tag::Int(*max)),
                ("plateau", Tag::Int(*plateau)),
            ]);
            ("trapezoid", None)
        }
    };
    if let Some((min, max)) = range {
        fields.extend([
            ("min_inclusive", Tag::Int(min)),
            ("max_inclusive", Tag::Int(max)),
        ]);
    }
    dispatched("type", name, fields)
}

impl super::pool::StructurePoolElement {
    pub fn to_nbt(&self) -> Result<Tag, crate::Error> {
        use super::{
            Projection,
            pool::{PoolFeature, ProcessorList},
            template::LiquidSettings,
        };
        fn processors(list: &ProcessorList) -> Tag {
            list.id.as_ref().map_or_else(
                || {
                    compound([(
                        "processors",
                        sequence(list.processors.iter().map(StructureProcessor::to_nbt)),
                    )])
                },
                string,
            )
        }
        fn projection(p: Projection) -> Tag {
            string(if p == Projection::Rigid {
                "rigid"
            } else {
                "terrain_matching"
            })
        }
        let mut fields = Vec::new();
        let name = match self {
            Self::Empty => "empty_pool_element",
            Self::Single {
                location,
                processors: list,
                projection: p,
                liquid,
                legacy,
            } => {
                fields.extend([
                    ("location", string(location)),
                    ("processors", processors(list)),
                    ("projection", projection(*p)),
                ]);
                if let Some(liquid) = liquid {
                    fields.push((
                        "override_liquid_settings",
                        string(if *liquid == LiquidSettings::ApplyWaterlogging {
                            "apply_waterlogging"
                        } else {
                            "ignore_waterlogging"
                        }),
                    ));
                }
                if *legacy {
                    "legacy_single_pool_element"
                } else {
                    "single_pool_element"
                }
            }
            Self::List {
                elements,
                projection: p,
            } => {
                fields.extend([
                    (
                        "elements",
                        sequence(
                            elements
                                .iter()
                                .map(|v| v.to_nbt())
                                .collect::<Result<Vec<_>, _>>()?,
                        ),
                    ),
                    ("projection", projection(*p)),
                ]);
                "list_pool_element"
            }
            Self::Feature {
                feature,
                projection: p,
            } => {
                fields.extend([
                    (
                        "feature",
                        match feature {
                            PoolFeature::Reference(id) => string(id),
                            PoolFeature::Direct { source, .. } => {
                                super::processor::json_nbt(source)?
                            }
                        },
                    ),
                    ("projection", projection(*p)),
                ]);
                "feature_pool_element"
            }
        };
        Ok(dispatched("element_type", name, fields))
    }
}
/// NbtOps#convertTo(JsonOps) for codec inputs. Inline feature saves use the
/// reverse conversion; full feature codec default-field normalization is pending.
pub(super) fn to_json(tag: &Tag) -> Result<serde_json::Value, crate::Error> {
    use serde_json::{Number, Value};
    Ok(match tag {
        Tag::End => return Err(crate::Error("End tag is not a codec value".into())),
        Tag::Byte(v) => Value::from(*v),
        Tag::Short(v) => Value::from(*v),
        Tag::Int(v) => Value::from(*v),
        Tag::Long(v) => Value::from(*v),
        Tag::Float(v) => Value::Number(
            Number::from_f64(f64::from(*v))
                .ok_or_else(|| crate::Error("nonfinite NBT codec number".into()))?,
        ),
        Tag::Double(v) => Value::Number(
            Number::from_f64(*v)
                .ok_or_else(|| crate::Error("nonfinite NBT codec number".into()))?,
        ),
        Tag::String(v) => Value::String(v.to_string_lossy()),
        Tag::Compound(v) => Value::Object(
            v.iter()
                .map(|(k, v)| Ok((k.to_string_lossy(), to_json(v)?)))
                .collect::<Result<_, crate::Error>>()?,
        ),
        Tag::List(v) => Value::Array(v.iter().map(to_json).collect::<Result<_, _>>()?),
        Tag::ByteArray(v) => Value::Array(v.iter().map(|&v| Value::from(v)).collect()),
        Tag::IntArray(v) => Value::Array(v.iter().map(|&v| Value::from(v)).collect()),
        Tag::LongArray(v) => Value::Array(v.iter().map(|&v| Value::from(v)).collect()),
    })
}

#[cfg(test)]
#[path = "nbt_tests.rs"]
mod tests;
