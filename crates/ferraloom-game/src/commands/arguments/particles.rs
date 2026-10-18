//! Port of ParticleArgument and the codecs selected by ParticleType#codec.
use super::{identifier, nbt};
use crate::commands::{
    context::CommandBuildContext,
    exceptions::{error, error_at, java_float_string},
};
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_data::{Block, BlockState, Item, registries::particle_type};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    BlockPos, Identifier, Wire, Writer,
    codec::VarInt,
    component::{ItemStackTemplate, Persistent},
    entity_data::{
        Dust, DustTransition, GeyserBase, Particle, PositionSource, Spell, Trail, Vec3, Vibration,
    },
};

pub fn parse(
    reader: &mut StringReader,
    context: &CommandBuildContext,
) -> Result<Particle, CommandSyntaxException> {
    let id = identifier(reader)?;
    if !context.contains("minecraft:particle_type", &id) {
        return Err(error_at(reader, "particle.notFound", &[id]));
    }
    let options = if reader.can_read() && reader.peek() == b'{' as u16 {
        nbt(reader, false)?
    } else {
        Tag::Compound(CompoundTag::new())
    };
    let Tag::Compound(map) = options else {
        unreachable!("particle options begin with a compound")
    };
    decode(&id, &map).map_err(|message| error("particle.invalidOptions", &[message]))
}
/// RecordCodecBuilder aggregates failures in reverse field order.
struct Fields<'a> {
    map: &'a CompoundTag,
    errors: Vec<String>,
}
impl Fields<'_> {
    fn value<T>(
        &mut self,
        key: &str,
        default: T,
        optional: bool,
        decode: impl FnOnce(&Tag) -> Result<T, String>,
    ) -> T {
        let failure = match self.map.get(key) {
            Some(tag) => match decode(tag) {
                Ok(value) => return value,
                Err(error) => error,
            },
            None if optional => return default,
            None => format!("No key {key} in MapLike[{}]", self.map),
        };
        self.errors.push(failure);
        default
    }
}
fn decode(id: &str, map: &CompoundTag) -> Result<Particle, String> {
    let mut fields = Fields {
        map,
        errors: Vec::new(),
    };
    let value = match id.strip_prefix("minecraft:").unwrap_or(id) {
        "geyser" | "geyser_plume" => {
            let water_blocks = fields.value("water_blocks", 0, false, positive);
            if id.ends_with(":geyser") {
                Particle::Geyser(water_blocks)
            } else {
                Particle::GeyserPlume(water_blocks)
            }
        }
        "geyser_base" | "geyser_poof" => {
            let water_blocks = fields.value("water_blocks", 0, false, positive);
            let burst_impulse_base = fields.value("burst_impulse_base", 0., false, float);
            let value = GeyserBase {
                water_blocks,
                burst_impulse_base,
            };
            if id.ends_with(":geyser_base") {
                Particle::GeyserBase(value)
            } else {
                Particle::GeyserPoof(value)
            }
        }
        "dragon_breath" => Particle::DragonBreath(fields.value("power", 1., true, float)),
        "dust" => {
            let color = fields.value("color", 0, false, |v| color(v, 3));
            let scale = fields.value("scale", 1., false, scale);
            Particle::Dust(Dust { color, scale })
        }
        "dust_color_transition" => {
            let from_color = fields.value("from_color", 0, false, |v| color(v, 3));
            let to_color = fields.value("to_color", 0, false, |v| color(v, 3));
            let scale = fields.value("scale", 1., false, scale);
            Particle::DustColorTransition(DustTransition {
                from_color,
                to_color,
                scale,
            })
        }
        "effect" | "instant_effect" => {
            let color = fields.value("color", -1, true, |v| color(v, 3));
            let power = fields.value("power", 1., true, float);
            let value = Spell { color, power };
            if id.ends_with(":effect") {
                Particle::Effect(value)
            } else {
                Particle::InstantEffect(value)
            }
        }
        "entity_effect" | "tinted_leaves" | "flash" => {
            let color = fields.value("color", 0, false, |v| color(v, 4));
            match id {
                "minecraft:entity_effect" => Particle::EntityEffect(color),
                "minecraft:tinted_leaves" => Particle::TintedLeaves(color),
                _ => Particle::Flash(color),
            }
        }
        "sculk_charge" => Particle::SculkCharge(fields.value("roll", 0., false, float)),
        "shriek" => Particle::Shriek(VarInt(fields.value("delay", 0, false, integer))),
        "trail" => {
            let target = fields.value(
                "target",
                Vec3 {
                    x: 0.,
                    y: 0.,
                    z: 0.,
                },
                false,
                |v| {
                    let vector = vector(v, 3, double)?;
                    Ok(Vec3 {
                        x: vector[0],
                        y: vector[1],
                        z: vector[2],
                    })
                },
            );
            let color = fields.value("color", 0, false, |v| color(v, 3));
            let duration = VarInt(fields.value("duration", 1, false, positive));
            Particle::Trail(Trail {
                target,
                color,
                duration,
            })
        }
        "vibration" => {
            let destination = fields.value(
                "destination",
                PositionSource::Block(BlockPos::new(0, 0, 0)),
                false,
                position_source,
            );
            let arrival_in_ticks = VarInt(fields.value("arrival_in_ticks", 0, false, integer));
            Particle::Vibration(Vibration {
                destination,
                arrival_in_ticks,
            })
        }
        "item" => Particle::Item(fields.value(
            "item",
            ItemStackTemplate::new(Item::STONE, 1, Default::default()).unwrap(),
            false,
            |v| ItemStackTemplate::from_nbt(v).map_err(|error| error.to_string()),
        )),
        "block" | "block_marker" | "falling_dust" | "dust_pillar" | "block_crumble" => {
            let state = fields.value(
                "block_state",
                Block::AIR.default_state(),
                false,
                block_state,
            );
            match id {
                "minecraft:block" => Particle::Block(state),
                "minecraft:block_marker" => Particle::BlockMarker(state),
                "minecraft:falling_dust" => Particle::FallingDust(state),
                "minecraft:dust_pillar" => Particle::DustPillar(state),
                _ => Particle::BlockCrumble(state),
            }
        }
        _ => {
            // Unit codecs accept any map and have no option payload. Reuse the canonical
            // protocol dispatch rather than maintaining another list of simple particles.
            let id = particle_type::Id::from_name(id).expect("registered vanilla particle type");
            let mut bytes = Writer::new();
            bytes.var_int(id.protocol_id() as i32);
            Particle::from_bytes(&bytes.into_inner()).expect("unit particle codec")
        }
    };
    if fields.errors.is_empty() {
        Ok(value)
    } else {
        Err(fields
            .errors
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("; "))
    }
}
fn integer(tag: &Tag) -> Result<i32, String> {
    match tag {
        Tag::Float(value) => Some(*value as i32),
        Tag::Double(value) => Some(*value as i32),
        _ => tag.as_int(),
    }
    .ok_or_else(|| "Not a number".into())
}
fn float(tag: &Tag) -> Result<f32, String> {
    tag.as_float().ok_or_else(|| "Not a number".into())
}
fn double(tag: &Tag) -> Result<f64, String> {
    tag.as_double().ok_or_else(|| "Not a number".into())
}
fn positive(tag: &Tag) -> Result<i32, String> {
    let value = integer(tag)?;
    if value >= 1 {
        Ok(value)
    } else {
        Err(format!("Value must be positive: {value}"))
    }
}
fn scale(tag: &Tag) -> Result<f32, String> {
    let value = float(tag)?;
    if (0.01..=4.).contains(&value) {
        Ok(value)
    } else {
        Err(format!(
            "Value must be within range [0.01;4.0]: {}",
            java_float_string(value)
        ))
    }
}
fn vector<T>(
    tag: &Tag,
    size: usize,
    decode: impl Fn(&Tag) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let values = match tag {
        Tag::List(values) => values.0.clone(),
        Tag::ByteArray(values) => values.iter().copied().map(Tag::Byte).collect(),
        Tag::IntArray(values) => values.iter().copied().map(Tag::Int).collect(),
        Tag::LongArray(values) => values.iter().copied().map(Tag::Long).collect(),
        _ => return Err(format!("Not a list: {tag}")),
    };
    let values = values.iter().map(decode).collect::<Result<Vec<_>, _>>()?;
    if values.len() == size {
        Ok(values)
    } else {
        Err(format!("Input is not a list of {size} elements"))
    }
}
fn color(tag: &Tag, channels: usize) -> Result<i32, String> {
    if let Ok(value) = integer(tag) {
        return Ok(value);
    }
    let values = vector(tag, channels, float).map_err(|message| {
        format!("Failed to parse either. First: Not a number; Second: {message}")
    })?;
    let channel = |index| (values[index] * 255.).floor() as i32;
    let alpha = if channels == 4 { channel(3) } else { 255 };
    Ok(alpha << 24 | channel(0) << 16 | channel(1) << 8 | channel(2))
}
fn text(tag: &Tag) -> Result<String, String> {
    tag.as_string()
        .map(|s| s.to_string_lossy())
        .ok_or_else(|| "Not a string".into())
}
fn block_state(tag: &Tag) -> Result<BlockState, String> {
    let name = if let Tag::String(_) = tag {
        text(tag)?
    } else if let Tag::Compound(map) = tag {
        map.get("id").map(text).unwrap_or_else(||Err(format!("No key id in MapLike[{tag}]"))).map_err(|message|format!("Failed to parse either. First: Failed to parse either. First: Not a string; Second: {message}; Second: Not a string"))?
    } else {
        return Err(format!(
            "Failed to parse either. First: Failed to parse either. First: Not a string; Second: Not a map: {tag}; Second: Not a string"
        ));
    };
    let id =
        Identifier::parse(&name).map_err(|_| format!("Not a valid resource location: {name}"))?;
    if Block::from_name(id.as_str()).is_none() {
        return Err(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:block]: {id}"
        ));
    }
    let json = ferraloom_protocol::ops::encode_nbt(tag, &ferraloom_protocol::ops::JsonOps)
        .map_err(|error| error.to_string())?;
    ferraloom_data::datapack::decode_block_state(&json).map_err(|error| error.to_string())
}
fn position_source(tag: &Tag) -> Result<PositionSource, String> {
    let Tag::Compound(map) = tag else {
        return Err(format!("Not a map: {tag}"));
    };
    let kind = map
        .get("type")
        .ok_or_else(|| format!("No key type in MapLike[{tag}]"))?;
    let kind = text(kind)?;
    match kind.as_str() {
        "minecraft:block" | "block" => {
            let pos = map
                .get("pos")
                .ok_or_else(|| format!("No key pos in MapLike[{tag}]"))?;
            let pos = vector(pos, 3, integer)?;
            Ok(PositionSource::Block(BlockPos::new(pos[0], pos[1], pos[2])))
        }
        "minecraft:entity" | "entity" => Err("Entity position sources are not allowed".into()),
        _ => Err(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:position_source_type]: {kind}"
        )),
    }
}
