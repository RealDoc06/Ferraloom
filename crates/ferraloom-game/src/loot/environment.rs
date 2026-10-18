use super::{Identifier, Result, codec};
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
