use super::*;
use crate::{
    Behaviors,
    advancements::{Advancement, AdvancementHolder, PlayerAdvancements, ServerAdvancementManager},
    entity::{EntityData, EntityState, item::PrimedTnt, player::GameType},
    level::{LevelContext, Region},
};
use ferraloom_data::Block;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_util::{Uuid, random::RandomSource};
use serde_json::json;

const OLD: Vec3 = Vec3::new(-0.25, 0.5, -0.0);
const SPEED: Vec3 = Vec3::new(0.9, 1.5, 0.9);
const START: Vec3 = Vec3::new(2.0, 80.0, -3.0);
const IMPACT: Vec3 = Vec3::new(-0.0, 70.25, 3.0);
const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn fixture(player: bool) -> (Level, EntityRef) {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
    let entity = level.next_entity_ref();
    let id = level.next_entity_id();
    let state = if player {
        EntityState::Player(Box::new(ServerPlayer::new(
            Uuid::new(1, 2),
            "fall".into(),
            id,
            Vec3::new(2.0, 70.0, -3.0),
            GameType::Survival,
        )))
    } else {
        EntityState::PrimedTnt(Box::new(PrimedTnt::new(id, 123)))
    };
    level.add_entity(entity, state).unwrap();
    (level, entity)
}
fn vector(value: Option<Vec3>) -> String {
    value
        .map(|v| {
            format!(
                "{:x},{:x},{:x}",
                v.x.to_bits(),
                v.y.to_bits(),
                v.z.to_bits()
            )
        })
        .unwrap_or("-".into())
}
fn state(entity: EntityRef, level: &Level) -> String {
    let base = level.entity::<EntityData>(entity).unwrap();
    let result = format!(
        "{:x}\t{}",
        base.fall_distance.to_bits(),
        vector(Some(base.stuck_speed_multiplier))
    );
    if let Ok(player) = level.entity::<ServerPlayer>(entity) {
        // The grace counter has native persistence, without a second query API.
        let saved = player.save();
        format!(
            "{result}\t{}\t{}\t{}",
            vector(player.starting_to_fall_position),
            saved.get_int_or("current_impulse_context_reset_grace_time", 0),
            vector(player.current_impulse_impact_pos())
        )
    } else {
        format!("{result}\t-\t-\t-")
    }
}
fn double(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).unwrap())
}

#[test]
fn stuck_and_reset_virtuals_match_native_flight_grace_health_and_rng() {
    let mut checked = 0;
    for row in VECTORS.lines().filter(|r| r.starts_with("stuck-reset\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let is_player = f[1] == "player";
        let (mut level, entity) = fixture(is_player);
        if is_player {
            let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
            let mut tag = CompoundTag::new();
            tag.insert(
                "current_impulse_context_reset_grace_time",
                Tag::Int(f[4].parse().unwrap()),
            );
            tag.insert(
                "current_explosion_impact_pos",
                crate::entity::persistence::vector_tag(IMPACT),
            );
            player.try_load(tag).unwrap();
            player.abilities.flying = f[3].parse().unwrap();
            player.set_health(f32::from_bits(u32::from_str_radix(f[6], 16).unwrap()));
            player.starting_to_fall_position = (f[7] == "true").then_some(START);
        }
        let base = level.entity_mut::<EntityData>(entity).unwrap();
        base.fall_distance = double(f[5]);
        base.stuck_speed_multiplier = OLD;
        base.random.set_seed(123);
        if f[2] == "stuck" {
            crate::entity::make_stuck_in_block(
                entity,
                Block::POWDER_SNOW.default_state(),
                SPEED,
                &mut level,
            )
            .unwrap();
        } else {
            crate::entity::reset_fall_distance(entity, &mut level).unwrap();
        }
        assert_eq!(state(entity, &level), f[8..13].join("\t"), "{row}");
        assert_eq!(
            format!(
                "{:x}",
                level
                    .entity_mut::<EntityData>(entity)
                    .unwrap()
                    .random
                    .next_long()
            ),
            f[13],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 42);
}

#[test]
fn fall_start_capture_is_positive_only_and_keeps_the_first_position() {
    let mut checked = 0;
    for row in VECTORS.lines().filter(|r| r.starts_with("fall-start\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity) = fixture(true);
        level.set_entity_pos(entity, START).unwrap();
        level
            .entity_mut::<EntityData>(entity)
            .unwrap()
            .fall_distance = double(f[1]);
        track_start_falling_position(entity, &mut level).unwrap();
        level
            .set_entity_pos(entity, Vec3::new(2.0, 70.0, -3.0))
            .unwrap();
        level
            .entity_mut::<EntityData>(entity)
            .unwrap()
            .fall_distance = 10.0;
        track_start_falling_position(entity, &mut level).unwrap();
        assert_eq!(
            vector(
                level
                    .entity::<ServerPlayer>(entity)
                    .unwrap()
                    .starting_to_fall_position
            ),
            f[2],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 4);
}

fn impulse_tag(scenario: &str) -> CompoundTag {
    let mut tag = CompoundTag::new();
    if scenario != "absent" {
        tag.insert(
            "current_impulse_context_reset_grace_time",
            Tag::Int(if scenario == "negative" { -1 } else { 7 }),
        );
    }
    let impact = match scenario {
        "valid" | "negative" => Some(crate::entity::persistence::codec_vector_tag(IMPACT)),
        "wrong" => {
            tag.insert(
                "current_impulse_context_reset_grace_time",
                Tag::String("bad".into()),
            );
            Some(Tag::String("bad".into()))
        }
        "short" => Some(Tag::List(ListTag(vec![Tag::Double(1.0), Tag::Double(2.0)]))),
        "bytes" => Some(Tag::ByteArray(vec![-1, 2, 3])),
        "ints" => Some(Tag::IntArray(vec![-1, 2, 3])),
        "longs" => Some(Tag::LongArray(vec![i64::MAX, 2, 3])),
        "mixed" => Some(Tag::List(ListTag(vec![
            Tag::Int(1),
            Tag::Double(2.5),
            Tag::Long(3),
        ]))),
        "absent" => None,
        _ => panic!("unknown impulse input"),
    };
    if let Some(impact) = impact {
        tag.insert("current_explosion_impact_pos", impact);
    }
    tag
}
#[test]
fn native_impulse_load_reset_and_save_preserve_negative_grace_and_optional_vectors() {
    let mut checked = 0;
    for row in VECTORS.lines().filter(|r| r.starts_with("impulse-load\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity) = fixture(true);
        let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
        player.try_load(impulse_tag(f[1])).unwrap();
        let before = player.save();
        assert_eq!(
            before
                .get_int_or("current_impulse_context_reset_grace_time", 0)
                .to_string(),
            f[2],
            "{row}"
        );
        assert_eq!(vector(player.current_impulse_impact_pos()), f[3], "{row}");
        player.try_reset_current_impulse_context();
        let saved = player.save();
        assert_eq!(
            saved
                .get_int_or("current_impulse_context_reset_grace_time", 0)
                .to_string(),
            f[4],
            "{row}"
        );
        assert_eq!(vector(player.current_impulse_impact_pos()), f[5], "{row}");
        let impact = player.current_impulse_impact_pos();
        player
            .try_load_with_recipes(
                saved,
                &crate::crafting::RecipeManager::new(Vec::new()).unwrap(),
            )
            .unwrap();
        assert_eq!(vector(player.current_impulse_impact_pos()), vector(impact));
        checked += 1;
    }
    assert_eq!(checked, 9);
}
#[test]
fn native_impulse_state_methods_and_grace_tick_keep_the_same_canonical_position() {
    let mut checked = 0;
    for row in VECTORS.lines().filter(|r| r.starts_with("impulse-state\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity) = fixture(true);
        let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
        player.set_ignore_fall_damage_from_current_impulse(true, IMPACT);
        match f[1] {
            "enable" => player.set_ignore_fall_damage_from_current_impulse(true, START),
            "disable" => player.set_ignore_fall_damage_from_current_impulse(false, START),
            "max-grace" => {
                player.apply_post_impulse_grace_time(3);
                player.apply_post_impulse_grace_time(47);
            }
            "reset" => player.reset_current_impulse_context(),
            _ => panic!("unknown impulse operation"),
        }
        let saved = player.save();
        assert_eq!(
            format!(
                "{}\t{}\t{}\t{}\t{}",
                saved.get_int_or("current_impulse_context_reset_grace_time", 0),
                vector(player.current_impulse_impact_pos()),
                player.is_in_post_impulse_grace_time(),
                player.is_ignoring_fall_damage_from_current_impulse(),
                vector(
                    saved
                        .get("current_explosion_impact_pos")
                        .and_then(crate::entity::persistence::optional_vector)
                )
            ),
            f[2..].join("\t"),
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 4);
    let (mut level, entity) = fixture(true);
    let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
    player.set_ignore_fall_damage_from_current_impulse(true, IMPACT);
    for _ in 0..40 {
        player.tick_current_impulse_grace_time();
    }
    assert!(!player.is_in_post_impulse_grace_time());
    assert_eq!(player.current_impulse_impact_pos(), Some(IMPACT));
    player.try_reset_current_impulse_context();
    assert_eq!(player.current_impulse_impact_pos(), None);
}

struct ResetProbe;
impl Entity for ResetProbe {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
    }
    fn reset_fall_distance(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        let base = level.entity_mut::<EntityData>(entity)?;
        assert_eq!(base.fall_distance, 5.25);
        assert_eq!(base.stuck_speed_multiplier, OLD);
        base.custom_data.insert("reset", Tag::Byte(1));
        crate::entity::stuck::base_reset_fall_distance(entity, level)
    }
}
#[test]
fn explicit_parent_stuck_body_keeps_the_derived_reset_callback_before_the_multiplier() {
    let (mut level, entity) = fixture(false);
    let base = level.entity_mut::<EntityData>(entity).unwrap();
    base.fall_distance = 5.25;
    base.stuck_speed_multiplier = OLD;
    ResetProbe
        .make_stuck_in_block(
            entity,
            Block::POWDER_SNOW.default_state(),
            SPEED,
            &mut level,
        )
        .unwrap();
    let base = level.entity::<EntityData>(entity).unwrap();
    assert_eq!(base.fall_distance, 0.0);
    assert_eq!(base.stuck_speed_multiplier, SPEED);
    assert_eq!(base.custom_data.get("reset"), Some(&Tag::Byte(1)));
}

#[test]
fn fall_criterion_observes_pre_reset_nbt_and_is_not_repeated_after_the_fall_is_cleared() {
    use crate::{
        block::containers::runtime::ContainerResources,
        crafting::{RecipeManager, RecipeSynchronization},
    };
    use std::sync::Arc;
    let (mut level, entity) = fixture(true);
    let id = "test:fall_before_reset".parse().unwrap();
    let manager = ServerAdvancementManager::new(vec![AdvancementHolder {
        id,
        value: Advancement::decode(&json!({
            "criteria": {"fell": {"trigger": "fall_from_height", "conditions": {
                "player": {"nbt": "{fall_distance:5.25d,current_impulse_context_reset_grace_time:0,current_explosion_impact_pos:[0.0d,70.25d,3.0d]}"},
                "distance": {"y": {"min":10,"max":10}}
            }}},
            "rewards": {"experience":1}
        })).unwrap(),
    }]);
    let recipes = Arc::new(RecipeManager::new(Vec::new()).unwrap());
    let synchronization = Arc::new(RecipeSynchronization::new(&recipes, |_| true));
    level.set_container_resources(
        Arc::new(ContainerResources {
            crafting: crate::inventory::RuntimeCraftingContext::fixture(),
            recipes,
            loot: Arc::default(),
            advancements: Arc::new(manager),
            recipe_tags: Default::default(),
            recipe_synchronization: synchronization,
        }),
        0,
    );
    let resources = level.container_resources().unwrap();
    let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
    player.advancements = PlayerAdvancements::new(&resources.advancements);
    player.fall_distance = 5.25;
    player.stuck_speed_multiplier = OLD;
    player.starting_to_fall_position = Some(START);
    player.set_ignore_fall_damage_from_current_impulse(true, IMPACT);
    player.set_ignore_fall_damage_from_current_impulse(false, IMPACT);
    crate::entity::make_stuck_in_block(
        entity,
        Block::POWDER_SNOW.default_state(),
        SPEED,
        &mut level,
    )
    .unwrap();
    let player = level.entity::<ServerPlayer>(entity).unwrap();
    assert_eq!(player.total_experience, 1);
    assert_eq!(player.fall_distance, 0.0);
    assert_eq!(player.stuck_speed_multiplier, SPEED);
    assert!(player.starting_to_fall_position.is_none());
    assert!(player.current_impulse_impact_pos().is_none());
    crate::entity::reset_fall_distance(entity, &mut level).unwrap();
    assert_eq!(
        level
            .entity::<ServerPlayer>(entity)
            .unwrap()
            .total_experience,
        1
    );
}

#[test]
fn required_fall_criterion_failure_keeps_the_fall_multiplier_and_impulse_prefix() {
    let (mut level, entity) = fixture(true);
    let manager = ServerAdvancementManager::new(vec![AdvancementHolder {
        id: "test:required_fall".parse().unwrap(),
        value: Advancement::decode(&json!({
            "criteria": {"fall": {"trigger": "fall_from_height"}}
        }))
        .unwrap(),
    }]);
    let player = level.entity_mut::<ServerPlayer>(entity).unwrap();
    player.advancements = PlayerAdvancements::new(&manager);
    player.fall_distance = 5.25;
    player.stuck_speed_multiplier = OLD;
    player.starting_to_fall_position = Some(START);
    player.set_ignore_fall_damage_from_current_impulse(true, IMPACT);
    player.set_ignore_fall_damage_from_current_impulse(false, IMPACT);
    let before = state(entity, &level);
    assert_eq!(
        crate::entity::make_stuck_in_block(
            entity,
            Block::POWDER_SNOW.default_state(),
            SPEED,
            &mut level
        ),
        Err(LevelError::Inventory("container datapack resources"))
    );
    assert_eq!(state(entity, &level), before);
}
