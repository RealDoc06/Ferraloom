//! Port of ParticleTypes.CODEC and each ParticleType#codec.
mod input;
use crate::language::java_float_string;
use ferraloom_data::{Block, BlockState, Item, registries::particle_type};
use ferraloom_protocol::{
    BlockPos, Identifier, Wire, Writer,
    codec::{RegistryContext, VarInt},
    component::ItemStackTemplate,
    entity_data::{
        Dust, DustTransition, GeyserBase, Particle, PositionSource, Spell, Trail, Vec3, Vibration,
    },
};
pub use input::{Expected, Input};
use std::fmt;

/// A native codec error, kept separate from the command adapter's translated wrapper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError(pub String);
impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for DecodeError {}

/// Port of ParticleTypes.CODEC's type-dispatched map. Both input formats enter
/// this same schema; numeric and component reads retain their own DynamicOps.
pub fn decode<I: Input>(input: &I) -> Result<Particle, DecodeError> {
    if !input.is_map() {
        return Err(DecodeError(input.type_error(Expected::Map)));
    }
    let kind = input
        .get("type")
        .ok_or_else(|| DecodeError(format!("No key type in MapLike[{input}]")))?;
    let kind = text(kind).map_err(DecodeError)?;
    decode_options(&kind, input)
}

// Simple particle types share the protocol's dispatch. Only that built-in
// registry is consulted; no command source or dynamic game service is captured.
struct ParticleRegistry;
impl RegistryContext for ParticleRegistry {
    fn entry_id(&self, registry: &Identifier, entry: &Identifier) -> Option<i32> {
        (registry.as_str() == "minecraft:particle_type")
            .then(|| particle_type::Id::from_name(entry.as_str()).map(|v| v.protocol_id() as i32))
            .flatten()
    }
    fn entry_key(&self, registry: &Identifier, id: i32) -> Option<Identifier> {
        if registry.as_str() != "minecraft:particle_type" {
            return None;
        }
        ferraloom_data::registry_entries(registry.as_str())?
            .get(usize::try_from(id).ok()?)
            .and_then(|v| Identifier::parse(v).ok())
    }
    fn has_tag(&self, _: &Identifier, _: &Identifier) -> bool {
        false
    }
}

/// RecordCodecBuilder aggregates failures in reverse field order.
struct Fields<'a, I> {
    map: &'a I,
    errors: Vec<String>,
}
impl<I: Input> Fields<'_, I> {
    fn value<T>(
        &mut self,
        key: &str,
        default: T,
        optional: bool,
        decode: impl FnOnce(&I) -> Result<T, String>,
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
/// Port of ParticleType#codec for an already selected particle type. Commands
/// pass the SNBT options map here after resolving the identifier separately.
pub fn decode_options<I: Input>(id: &str, map: &I) -> Result<Particle, DecodeError> {
    decode_options_inner(id, map).map_err(DecodeError)
}
fn decode_options_inner<I: Input>(id: &str, map: &I) -> Result<Particle, String> {
    if !map.is_map() {
        return Err(map.type_error(Expected::Map));
    }
    let id = resource_location(id)?;
    let particle_type = particle_type::Id::from_name(id.as_str()).ok_or_else(|| {
        format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:particle_type]: {id}"
        )
    })?;
    let id = id.as_str();
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
            |v| {
                v.persistent::<ItemStackTemplate>()
                    .map_err(|error| error.to_string())
            },
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
            let mut bytes = Writer::new();
            bytes.var_int(particle_type.protocol_id() as i32);
            let bytes = bytes.into_inner();
            Particle::decode(&mut ferraloom_protocol::Reader::with_context(
                &bytes,
                &ParticleRegistry,
            ))
            .expect("unit particle codec")
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
fn integer<I: Input>(tag: &I) -> Result<i32, String> {
    tag.persistent()
        .map_err(|_| tag.type_error(Expected::Number))
}
fn float<I: Input>(tag: &I) -> Result<f32, String> {
    tag.persistent()
        .map_err(|_| tag.type_error(Expected::Number))
}
fn double<I: Input>(tag: &I) -> Result<f64, String> {
    tag.persistent()
        .map_err(|_| tag.type_error(Expected::Number))
}
fn positive<I: Input>(tag: &I) -> Result<i32, String> {
    let value = integer(tag)?;
    if value >= 1 {
        Ok(value)
    } else {
        Err(format!("Value must be positive: {value}"))
    }
}
fn scale<I: Input>(tag: &I) -> Result<f32, String> {
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
fn vector<I: Input, T>(
    tag: &I,
    size: usize,
    decode: impl Fn(&I) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let values = tag.list().ok_or_else(|| tag.type_error(Expected::List))?;
    let values = values.iter().map(decode).collect::<Result<Vec<_>, _>>()?;
    if values.len() == size {
        Ok(values)
    } else {
        Err(format!("Input is not a list of {size} elements"))
    }
}
fn color<I: Input>(tag: &I, channels: usize) -> Result<i32, String> {
    let number_error = match integer(tag) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    let values = vector(tag, channels, float).map_err(|message| {
        // A too-long fixed-size list has a partial result. EitherCodec then
        // reports that branch's failure rather than combining both alternatives.
        if tag.list().is_some_and(|values| {
            values.len() > channels && values.iter().all(|v| float(v).is_ok())
        }) {
            message
        } else {
            format!("Failed to parse either. First: {number_error}; Second: {message}")
        }
    })?;
    let channel = |index: usize| (values[index] * 255.).floor() as i32;
    let alpha = if channels == 4 { channel(3) } else { 255 };
    Ok(alpha << 24 | channel(0) << 16 | channel(1) << 8 | channel(2))
}
fn text<I: Input>(tag: &I) -> Result<String, String> {
    tag.persistent()
        .map_err(|_| tag.type_error(Expected::String))
}
fn block_state<I: Input>(tag: &I) -> Result<BlockState, String> {
    // BlockState.CODEC is either(either(block holder, full state), legacy string).
    let block = |value: &I| {
        let name = text(value)?;
        let id = resource_location(&name)?;
        Block::from_name(id.as_str()).ok_or_else(|| {
            format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:block]: {id}")
        })
    };
    let simple_error = match block(tag) {
        Ok(block) => return Ok(block.default_state()),
        Err(error) => error,
    };
    let state_error = if tag.is_map() {
        let id = tag
            .get("id")
            .ok_or_else(|| format!("No key id in MapLike[{tag}]"));
        match id.and_then(block) {
            Ok(_) => {
                let json = tag.to_json().map_err(|error| error.to_string())?;
                return ferraloom_data::datapack::decode_block_state(&json)
                    .map_err(|error| error.to_string());
            }
            Err(error) => error,
        }
    } else {
        tag.type_error(Expected::Map)
    };
    Err(format!(
        "Failed to parse either. First: Failed to parse either. First: {simple_error}; Second: {state_error}; Second: {simple_error}"
    ))
}
fn position_source<I: Input>(tag: &I) -> Result<PositionSource, String> {
    if !tag.is_map() {
        return Err(tag.type_error(Expected::Map));
    }
    let map = tag;
    let kind = map
        .get("type")
        .ok_or_else(|| format!("No key type in MapLike[{tag}]"))?;
    let kind = resource_location(&text(kind)?)?;
    match kind.as_str() {
        "minecraft:block" | "block" => {
            let pos = map
                .get("pos")
                .ok_or_else(|| format!("No key pos in MapLike[{tag}]"))?;
            let values = pos.list().ok_or_else(|| pos.type_error(Expected::List))?;
            let pos = values.iter().map(integer).collect::<Result<Vec<_>, _>>()?;
            if pos.len() != 3 {
                return Err("Input is not a list of 3 ints".into());
            }
            Ok(PositionSource::Block(BlockPos::new(pos[0], pos[1], pos[2])))
        }
        "minecraft:entity" | "entity" => {
            let entity = map
                .get("source_entity")
                .ok_or_else(|| format!("No key source_entity in MapLike[{tag}]"))?;
            let _ = entity
                .persistent::<ferraloom_protocol::Uuid>()
                .map_err(|error| error.to_string())?;
            if let Some(offset) = map.get("y_offset") {
                float(offset)?;
            }
            Err("Entity position sources are not allowed".into())
        }
        _ => Err(format!(
            "Unknown registry key in ResourceKey[minecraft:root / minecraft:position_source_type]: {kind}"
        )),
    }
}

/// Port of Identifier#read's codec-facing validation message.
fn resource_location(value: &str) -> Result<Identifier, String> {
    let (namespace, path) = match value.split_once(':') {
        Some(("", path)) => ("minecraft", path),
        Some(parts) => parts,
        None => ("minecraft", value),
    };
    let valid = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c);
    let reason = if namespace == ".." || !namespace.bytes().all(valid) {
        Some(format!(
            "Non [a-z0-9_.-] character in namespace of identifier: {namespace}:{path}"
        ))
    } else if !path.bytes().all(|c| valid(c) || c == b'/') {
        Some(format!(
            "Non [a-z0-9/._-] character in path of location: {namespace}:{path}"
        ))
    } else {
        None
    };
    if let Some(reason) = reason {
        Err(format!("Not a valid resource location: {value} {reason}"))
    } else {
        Identifier::parse(value).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests;
