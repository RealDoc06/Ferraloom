//! Port of ArgumentTypeInfos network templates.
use ferraloom_protocol::{
    Identifier,
    play::{
        CommandArgument, EntityArgumentFlags, NumericBounds, ScoreHolderFlags, StringArgumentType,
    },
};
use serde_json::Value;

pub fn from_report(parser: &str, properties: &Value) -> CommandArgument {
    match parser {
        "brigadier:bool" => CommandArgument::Bool {},
        "brigadier:float" => CommandArgument::Float {
            bounds: NumericBounds {
                min: properties["min"].as_f64().map(|v| v as f32),
                max: properties["max"].as_f64().map(|v| v as f32),
            },
        },
        "brigadier:double" => CommandArgument::Double {
            bounds: NumericBounds {
                min: properties["min"].as_f64(),
                max: properties["max"].as_f64(),
            },
        },
        "brigadier:integer" => CommandArgument::Integer {
            bounds: NumericBounds {
                min: properties["min"].as_i64().map(|v| v as i32),
                max: properties["max"].as_i64().map(|v| v as i32),
            },
        },
        "brigadier:long" => CommandArgument::Long {
            bounds: NumericBounds {
                min: properties["min"].as_i64(),
                max: properties["max"].as_i64(),
            },
        },
        "brigadier:string" => CommandArgument::String {
            string_type: match properties["type"].as_str().unwrap() {
                "word" => StringArgumentType::SingleWord,
                "phrase" => StringArgumentType::QuotablePhrase,
                "greedy" => StringArgumentType::GreedyPhrase,
                _ => unreachable!(),
            },
        },
        "minecraft:entity" => CommandArgument::Entity {
            flags: EntityArgumentFlags {
                single: properties["amount"] == "single",
                players_only: properties["type"] == "players",
            },
        },
        "minecraft:game_profile" => CommandArgument::GameProfile {},
        "minecraft:block_pos" => CommandArgument::BlockPos {},
        "minecraft:column_pos" => CommandArgument::ColumnPos {},
        "minecraft:vec3" => CommandArgument::Vec3 {},
        "minecraft:vec2" => CommandArgument::Vec2 {},
        "minecraft:block_state" => CommandArgument::BlockState {},
        "minecraft:block_predicate" => CommandArgument::BlockPredicate {},
        "minecraft:item_stack" => CommandArgument::ItemStack {},
        "minecraft:item_predicate" => CommandArgument::ItemPredicate {},
        "minecraft:team_color" => CommandArgument::TeamColor {},
        "minecraft:hex_color" => CommandArgument::HexColor {},
        "minecraft:component" => CommandArgument::Component {},
        "minecraft:style" => CommandArgument::Style {},
        "minecraft:message" => CommandArgument::Message {},
        "minecraft:nbt_compound_tag" => CommandArgument::NbtCompoundTag {},
        "minecraft:nbt_tag" => CommandArgument::NbtTag {},
        "minecraft:nbt_path" => CommandArgument::NbtPath {},
        "minecraft:objective" => CommandArgument::Objective {},
        "minecraft:objective_criteria" => CommandArgument::ObjectiveCriteria {},
        "minecraft:operation" => CommandArgument::Operation {},
        "minecraft:particle" => CommandArgument::Particle {},
        "minecraft:angle" => CommandArgument::Angle {},
        "minecraft:rotation" => CommandArgument::Rotation {},
        "minecraft:scoreboard_slot" => CommandArgument::ScoreboardSlot {},
        "minecraft:score_holder" => CommandArgument::ScoreHolder {
            flags: ScoreHolderFlags {
                multiple: properties["amount"] == "multiple",
            },
        },
        "minecraft:swizzle" => CommandArgument::Swizzle {},
        "minecraft:team" => CommandArgument::Team {},
        "minecraft:item_slot" => CommandArgument::ItemSlot {},
        "minecraft:item_slots" => CommandArgument::ItemSlots {},
        "minecraft:resource_location" => CommandArgument::ResourceLocation {},
        "minecraft:function" => CommandArgument::Function {},
        "minecraft:entity_anchor" => CommandArgument::EntityAnchor {},
        "minecraft:int_range" => CommandArgument::IntRange {},
        "minecraft:float_range" => CommandArgument::FloatRange {},
        "minecraft:dimension" => CommandArgument::Dimension {},
        "minecraft:gamemode" => CommandArgument::Gamemode {},
        "minecraft:time" => CommandArgument::Time {
            min: properties["min"].as_i64().unwrap_or(0) as i32,
        },
        "minecraft:resource_or_tag" => CommandArgument::ResourceOrTag {
            registry: Identifier::parse(properties["registry"].as_str().unwrap())
                .expect("vanilla registry id"),
        },
        "minecraft:resource_or_tag_key" => CommandArgument::ResourceOrTagKey {
            registry: Identifier::parse(properties["registry"].as_str().unwrap())
                .expect("vanilla registry id"),
        },
        "minecraft:resource" => CommandArgument::Resource {
            registry: Identifier::parse(properties["registry"].as_str().unwrap())
                .expect("vanilla registry id"),
        },
        "minecraft:resource_key" => CommandArgument::ResourceKey {
            registry: Identifier::parse(properties["registry"].as_str().unwrap())
                .expect("vanilla registry id"),
        },
        "minecraft:resource_selector" => CommandArgument::ResourceSelector {
            registry: Identifier::parse(properties["registry"].as_str().unwrap())
                .expect("vanilla registry id"),
        },
        "minecraft:template_mirror" => CommandArgument::TemplateMirror {},
        "minecraft:template_rotation" => CommandArgument::TemplateRotation {},
        "minecraft:heightmap" => CommandArgument::Heightmap {},
        "minecraft:loot_table" => CommandArgument::LootTable {},
        "minecraft:loot_predicate" => CommandArgument::LootPredicate {},
        "minecraft:loot_modifier" => CommandArgument::LootModifier {},
        "minecraft:context_float_provider" => CommandArgument::ContextFloatProvider {},
        "minecraft:context_int_provider" => CommandArgument::ContextIntProvider {},
        "minecraft:slot_source" => CommandArgument::SlotSource {},
        "minecraft:dialog" => CommandArgument::Dialog {},
        "minecraft:feature" => CommandArgument::Feature {},
        "minecraft:swing_animation" => CommandArgument::SwingAnimation {},
        "minecraft:uuid" => CommandArgument::Uuid {},
        _ => panic!("Unknown vanilla argument type {parser}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_protocol::Wire;
    #[derive(serde::Deserialize)]
    struct Vector {
        parser: String,
        properties: Value,
        wire: String,
    }
    #[test]
    fn every_argument_network_template_matches_java() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("../vanilla/argument-wire.json")).unwrap();
        assert_eq!(vectors.len(), 62);
        for vector in vectors {
            let expected = vector
                .wire
                .as_bytes()
                .chunks_exact(2)
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect::<Vec<_>>();
            let argument = from_report(&vector.parser, &vector.properties);
            assert_eq!(argument.to_bytes().unwrap(), expected, "{}", vector.parser);
        }
    }
}
