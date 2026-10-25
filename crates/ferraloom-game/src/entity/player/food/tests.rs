use super::*;
use crate::{
    entity::{EntityData, EntityState, player::GameType},
    gamerules::{GameRule, GameRules},
};
use ferraloom_util::{Uuid, Vec3};
fn f(value: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(value, 16).unwrap())
}
fn state(food: &FoodData) -> String {
    format!(
        "{},{:x},{:x},{}",
        food.food_level,
        food.saturation_level.to_bits(),
        food.exhaustion_level.to_bits(),
        food.tick_timer
    )
}
fn read(value: &str) -> FoodData {
    let v: Vec<_> = value.split(',').collect();
    FoodData {
        food_level: v[0].parse().unwrap(),
        saturation_level: f(v[1]),
        exhaustion_level: f(v[2]),
        tick_timer: v[3].parse().unwrap(),
    }
}
fn difficulty(id: &str) -> Difficulty {
    match id {
        "0" => Difficulty::Peaceful,
        "1" => Difficulty::Easy,
        "2" => Difficulty::Normal,
        "3" => Difficulty::Hard,
        _ => panic!("difficulty"),
    }
}
fn fixture(
    difficulty: Difficulty,
    natural: bool,
    food: FoodData,
    health: f32,
) -> (Level, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_difficulty(difficulty);
    level.rules_mut().natural_health_regeneration = natural;
    let entity = level.next_entity_ref();
    let mut player = ServerPlayer::new(
        Uuid::new(3, 4),
        "food".into(),
        level.next_entity_id(),
        Vec3::ZERO,
        GameType::Survival,
    );
    player.food_data = food;
    player.set_health(health);
    level
        .add_entity(entity, EntityState::Player(Box::new(player)))
        .unwrap();
    (level, entity)
}
struct StarveProbe;
impl Entity for StarveProbe {
    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        references: &mut Vec<EntityRef>,
    ) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
    }

    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: crate::entity::damage::DamageSource,
        amount: f32,
    ) -> Result<bool, LevelError> {
        let event = format!(
            "hurt:{}:{:x}:{}",
            source.damage_type().message_id,
            amount.to_bits(),
            state(&level.entity::<ServerPlayer>(entity)?.food_data)
        );
        level
            .entity_mut::<EntityData>(entity)?
            .custom_data
            .insert("food_hurt", Tag::String(event.into()));
        Ok(false)
    }
}
#[test]
fn native_food_save_canonicalizes_signed_zero_without_changing_live_fields() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("food-save\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let mut food = FoodData::default();
        food.set_saturation(f(row[1]));
        food.exhaustion_level = f(row[1]);
        let mut saved = CompoundTag::new();
        food.save(&mut saved);
        assert_eq!(format!("{:x}", food.saturation_level().to_bits()), row[2]);
        assert_eq!(food.exhaustion_level().to_bits(), f(row[1]).to_bits());
        let mut restored = FoodData::default();
        restored.load(&saved);
        assert_eq!(state(&restored), row[3]);
        checked += 1;
    }
    assert_eq!(checked, 5);
}

#[test]
fn native_food_state_numeric_load_defaults_roundtrip_and_float_arithmetic() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv").lines() {
        let row: Vec<_> = line.split('\t').collect();
        match row[0] {
            "food-load" => {
                let mut tag = CompoundTag::new();
                tag.insert("foodLevel", Tag::Int(7));
                tag.insert("foodSaturationLevel", Tag::Float(1.25));
                tag.insert("foodExhaustionLevel", Tag::Float(3.5));
                tag.insert("foodTickTimer", Tag::Int(17));
                match row[1] {
                    "default" => tag = CompoundTag::new(),
                    "negative" => {
                        tag.insert("foodLevel", Tag::Int(-8));
                        // CompoundTag#putFloat uses FloatTag.valueOf, whose ZERO is positive.
                        tag.insert("foodSaturationLevel", Tag::Float(0.0));
                        tag.insert("foodExhaustionLevel", Tag::Float(-7.5));
                        tag.insert("foodTickTimer", Tag::Int(-9));
                    }
                    "wrong" => {
                        for key in [
                            "foodLevel",
                            "foodTickTimer",
                            "foodSaturationLevel",
                            "foodExhaustionLevel",
                        ] {
                            tag.insert(key, Tag::String("x".into()));
                        }
                    }
                    "numeric" => {
                        tag.insert("foodLevel", Tag::Double(17.8));
                        tag.insert("foodSaturationLevel", Tag::Double(2.7));
                        tag.insert("foodTickTimer", Tag::Long(i64::from(i32::MAX) + 1));
                    }
                    "nan" => {
                        tag.insert("foodLevel", Tag::Int(99));
                        tag.insert("foodSaturationLevel", Tag::Float(f32::NAN));
                        tag.insert("foodExhaustionLevel", Tag::Float(f32::INFINITY));
                        tag.insert("foodTickTimer", Tag::Int(i32::MAX));
                    }
                    _ => {}
                }
                let mut food = FoodData::default();
                food.load(&tag);
                assert_eq!(state(&food), row[2], "{row:?}");
                assert_eq!(food.needs_food().to_string(), row[3]);
                assert_eq!(food.has_enough_food().to_string(), row[4]);
                let mut tag = CompoundTag::new();
                food.save(&mut tag);
                let mut restored = FoodData::default();
                restored.load(&tag);
                assert_eq!(state(&restored), row[2]);
                checked += 1;
            }
            "food-eat" => {
                let mut food = FoodData::default();
                let modifier = f(row[2]);
                food.eat(row[1].parse().unwrap(), modifier);
                food.add_exhaustion(modifier * 100.0);
                assert_eq!(state(&food), row[3], "{row:?}");
                checked += 1;
            }
            _ => {}
        }
    }
    assert_eq!(checked, 30);
}
#[test]
fn native_food_tick_preserves_thresholds_difficulty_damage_order_and_timer_wrap() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("food-tick\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let before = read(row[4]);
        let initial_health = f(row[5]);
        let (mut level, entity) = fixture(
            difficulty(row[2]),
            row[3].parse().unwrap(),
            before,
            initial_health,
        );
        tick(&StarveProbe, entity, &mut level).unwrap();
        let player = level.entity::<ServerPlayer>(entity).unwrap();
        assert_eq!(state(&player.food_data), row[6], "{row:?}");
        assert_eq!(
            format!("{:x}", player.health().to_bits()),
            row[7],
            "health {row:?}"
        );
        let damage = player
            .custom_data
            .get_string("food_hurt")
            .map(|v| v.to_string_lossy());
        if row[8].starts_with("hurt:") {
            assert_eq!(damage.as_deref(), Some(row[8]), "callback {row:?}");
        } else {
            assert!(damage.is_none());
        }
        let mut saved = player.save(&level).unwrap();
        assert_eq!(
            saved.get_int_or("foodTickTimer", -999),
            player.food_data.tick_timer
        );
        saved.remove("recipeBook");
        let mut restored = ServerPlayer::new(
            Uuid::new(3, 4),
            "food".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        );
        restored.try_load(saved).unwrap();
        assert_eq!(state(&restored.food_data), row[6]);
        checked += 1;
    }
    assert_eq!(checked, 112);
}
#[test]
fn native_peaceful_regeneration_respects_rule_cadence_and_fractional_saturation() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("food-peaceful\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let food = FoodData {
            food_level: 19,
            saturation_level: 19.5,
            exhaustion_level: 3.5,
            tick_timer: 7,
        };
        let (mut level, entity) = fixture(difficulty(row[1]), row[2].parse().unwrap(), food, 18.75);
        level.entity_mut::<ServerPlayer>(entity).unwrap().tick_count = row[3].parse().unwrap();
        tick_regeneration(entity, &mut level).unwrap();
        let player = level.entity::<ServerPlayer>(entity).unwrap();
        assert_eq!(state(&player.food_data), row[4], "{row:?}");
        assert_eq!(
            format!("{:x}", player.health().to_bits()),
            row[5],
            "{row:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 40);
    let rules = GameRules::default();
    let mut published = crate::level::LevelRules::default();
    rules.update_level_rules(&mut published);
    assert_eq!(
        published.natural_health_regeneration,
        rules.boolean(GameRule::NaturalHealthRegeneration)
    );
}
