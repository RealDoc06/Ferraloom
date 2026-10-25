use super::{Identifier, Result, codec};
use crate::loot::codec::DecodeInput;
use ferraloom_protocol::chat::Component;

/// Values compared by EnvironmentAttributeCheck. Float equality follows boxed
/// Java Float (including signed zero), and colors preserve their float channels.
#[derive(Clone, Debug, PartialEq)]
pub enum EnvironmentValue {
    Float(f32),
    Boolean(bool),
    Color(Vec<f32>),
    Enum(String),
    BedRule {
        can_sleep: String,
        can_set_spawn: String,
        destroy_on_use: bool,
        destroy_on_leave: bool,
        error_message: Option<Box<Component>>,
    },
}
impl EnvironmentValue {
    pub(crate) fn same_value(&self, other: &Self) -> bool {
        let bits = |v: f32| {
            if v.is_nan() {
                f32::NAN.to_bits()
            } else {
                v.to_bits()
            }
        };
        match (self, other) {
            (Self::Float(a), Self::Float(b)) => bits(*a) == bits(*b),
            (Self::Color(a), Self::Color(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| bits(*a) == bits(*b))
            }
            _ => self == other,
        }
    }
}

/// Port of EnvironmentAttribute#valueCodec, including the attribute's range.
/// Complex particle, music and spawn values remain explicit unsupported codecs.
pub(crate) fn check_value<I: DecodeInput>(
    id: &Identifier,
    v: &I,
) -> Result<(EnvironmentValue, bool)> {
    use EnvironmentValue as E;
    let name = id.as_str();
    if let Ok(positional) = numeric_attribute(id, true) {
        let n = codec::float(v)?;
        let range = match name {
            "minecraft:visual/fog_end_distance"
            | "minecraft:visual/sky_fog_end_distance"
            | "minecraft:visual/cloud_fog_end_distance"
            | "minecraft:visual/water_fog_end_distance" => Some((0.0, f32::INFINITY)),
            "minecraft:visual/star_brightness"
            | "minecraft:visual/sky_light_factor"
            | "minecraft:audio/music_volume"
            | "minecraft:gameplay/turtle_egg_hatch_chance"
            | "minecraft:gameplay/surface_slime_spawn_chance"
            | "minecraft:gameplay/cat_waking_up_gift_chance" => Some((0.0, 1.0)),
            "minecraft:gameplay/creature_world_gen_spawn_probability" => Some((0.0, 0.9999999)),
            "minecraft:gameplay/sky_light_level" => Some((0.0, 15.0)),
            _ => None,
        };
        if range.is_some_and(|(min, max)| !(min..=max).contains(&n)) {
            return Err(codec::error("environment value outside attribute range"));
        }
        return Ok((E::Float(n), positional));
    }
    let value = match name {
        "minecraft:audio/firefly_bush_sounds"
        | "minecraft:gameplay/can_start_raid"
        | "minecraft:gameplay/water_evaporates"
        | "minecraft:gameplay/respawn_anchor_works"
        | "minecraft:gameplay/nether_portal_spawns_piglin"
        | "minecraft:gameplay/fast_lava"
        | "minecraft:gameplay/increased_fire_burnout"
        | "minecraft:gameplay/piglins_zombify"
        | "minecraft:gameplay/snow_golem_melts"
        | "minecraft:gameplay/creaking_active"
        | "minecraft:gameplay/bees_stay_in_hive"
        | "minecraft:gameplay/monsters_burn"
        | "minecraft:gameplay/can_pillager_patrol_spawn" => E::Boolean(codec::boolean(v)?),
        "minecraft:gameplay/eyeblossom_open" => {
            let value = if let Some(b) = v.as_bool() {
                if b { "true" } else { "false" }
            } else {
                codec::string(v)?
            };
            if !matches!(value, "true" | "false" | "default") {
                return Err(codec::error("unknown tri state"));
            }
            E::Enum(value.to_owned())
        }
        "minecraft:visual/moon_phase" => {
            let value = codec::string(v)?;
            if !matches!(
                value,
                "full_moon"
                    | "waning_gibbous"
                    | "third_quarter"
                    | "waning_crescent"
                    | "new_moon"
                    | "waxing_crescent"
                    | "first_quarter"
                    | "waxing_gibbous"
            ) {
                return Err(codec::error("unknown moon phase"));
            }
            E::Enum(value.to_owned())
        }
        "minecraft:gameplay/villager_activity" | "minecraft:gameplay/baby_villager_activity" => {
            let value = codec::id(v)?;
            if !ferraloom_data::registry_entries("minecraft:activity")
                .is_some_and(|entries| entries.contains(&value.as_str()))
            {
                return Err(codec::error("unknown activity"));
            }
            E::Enum(value.to_string())
        }
        "minecraft:visual/fog_color"
        | "minecraft:visual/water_fog_color"
        | "minecraft:visual/sky_color"
        | "minecraft:visual/block_light_tint"
        | "minecraft:visual/sky_light_color"
        | "minecraft:visual/night_vision_color"
        | "minecraft:visual/ambient_light_color" => E::Color(color(v, 3)?),
        "minecraft:visual/sunrise_sunset_color" | "minecraft:visual/cloud_color" => {
            E::Color(color(v, 4)?)
        }
        "minecraft:gameplay/bed_rule" | "minecraft:gameplay/straw_bed_rule" => {
            let rule = |key| -> Result<String> {
                let value = codec::string(codec::field(v, key)?)?;
                if !matches!(value, "always" | "when_dark" | "never") {
                    return Err(codec::error("unknown bed rule"));
                }
                Ok(value.to_owned())
            };
            E::BedRule {
                can_sleep: rule("can_sleep")?,
                can_set_spawn: rule("can_set_spawn")?,
                destroy_on_use: codec::optional(v, "destroy_on_use", codec::boolean)?
                    .unwrap_or(false),
                destroy_on_leave: codec::optional(v, "destroy_on_leave", codec::boolean)?
                    .unwrap_or(false),
                error_message: codec::optional(v, "error_message", codec::persistent)?,
            }
        }
        "minecraft:visual/default_dripstone_particle"
        | "minecraft:visual/ambient_particles"
        | "minecraft:audio/background_music"
        | "minecraft:audio/ambient_sounds"
        | "minecraft:gameplay/natural_mob_spawns" => {
            return Err(super::LootError::Unsupported(format!(
                "environment_attribute.value_codec.{id}"
            )));
        }
        _ => return Err(codec::error(format!("unknown environment attribute {id}"))),
    };
    Ok((value, name != "minecraft:gameplay/fast_lava"))
}
fn color<I: DecodeInput>(v: &I, size: usize) -> Result<Vec<f32>> {
    if v.is_array() {
        let values = codec::list(v, codec::float)?;
        return if values.len() == size {
            Ok(values)
        } else {
            Err(codec::error("wrong color vector length"))
        };
    }
    let n = if let Some(text) = v.as_str() {
        let hex = text
            .strip_prefix('#')
            .ok_or_else(|| codec::error("expected #hex color"))?;
        if hex.len() != size * 2 {
            return Err(codec::error("wrong color hex length"));
        }
        u32::from_str_radix(hex, 16).map_err(|_| codec::error("invalid color hex"))?
    } else {
        codec::int(v)? as u32
    };
    let mut channels = vec![
        ((n >> 16) & 255) as f32 / 255.0,
        ((n >> 8) & 255) as f32 / 255.0,
        (n & 255) as f32 / 255.0,
    ];
    if size == 4 {
        channels.push(((n >> 24) & 255) as f32 / 255.0);
    }
    Ok(channels)
}
/// Port of EnvironmentAttributeProvider's numeric conversion codec checks.
/// 26.3 registers no environment attribute whose type has a toInt conversion.
/// Float/angle attributes have toFloat; the other attribute types have neither.
pub(crate) fn numeric_attribute(id: &Identifier, float: bool) -> Result<bool> {
    let positional = match id.as_str() {
        "minecraft:visual/fog_start_distance"
        | "minecraft:visual/fog_end_distance"
        | "minecraft:visual/sky_fog_end_distance"
        | "minecraft:visual/cloud_fog_end_distance"
        | "minecraft:visual/water_fog_start_distance"
        | "minecraft:visual/water_fog_end_distance"
        | "minecraft:visual/cloud_height"
        | "minecraft:visual/sun_angle"
        | "minecraft:visual/moon_angle"
        | "minecraft:visual/star_angle"
        | "minecraft:visual/star_brightness"
        | "minecraft:visual/sky_light_factor"
        | "minecraft:audio/music_volume"
        | "minecraft:gameplay/turtle_egg_hatch_chance"
        | "minecraft:gameplay/surface_slime_spawn_chance"
        | "minecraft:gameplay/cat_waking_up_gift_chance"
        | "minecraft:gameplay/creature_world_gen_spawn_probability" => true,
        "minecraft:gameplay/sky_light_level" => false,
        _ => {
            return Err(codec::error(format!(
                "attribute {id} cannot be converted to a number"
            )));
        }
    };
    if !float {
        return Err(codec::error(format!(
            "attribute {id} cannot be converted to an integer"
        )));
    }
    Ok(positional)
}
