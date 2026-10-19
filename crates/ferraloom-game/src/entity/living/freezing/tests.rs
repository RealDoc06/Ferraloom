use super::*;
use crate::entity::{
    EntityState,
    player::{GameType, ServerPlayer},
};
use ferraloom_data::{Block, Item};
use ferraloom_protocol::component::{ItemStack, Persistent};
use ferraloom_util::{BlockPos, Uuid, Vec3};

const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn add_player(level: &mut Level, mode: GameType) -> EntityRef {
    let entity = level.next_entity_ref();
    let player = ServerPlayer::new(
        Uuid::new(0, 1),
        "freeze".into(),
        level.next_entity_id(),
        Vec3::new(8.5, 64.0, 8.5),
        mode,
    );
    level
        .add_entity(entity, EntityState::Player(Box::new(player)))
        .unwrap();
    entity
}
#[test]
fn frozen_counters_and_armor_eligibility_match_native_float_bits_and_slot_scope() {
    let mut counters = 0;
    let mut armor = 0;
    for row in VECTORS.lines() {
        let fields: Vec<_> = row.split('\t').collect();
        if fields[0] == "freeze-counter" {
            let mut base = EntityData::new(ferraloom_data::EntityType::TNT, 1, 0);
            base.set_ticks_frozen(fields[1].parse().unwrap());
            assert_eq!(format!("{:x}", base.percent_frozen().to_bits()), fields[2]);
            assert_eq!(base.is_fully_frozen(), fields[3] == "true");
            assert_eq!(
                base.ticks_required_to_freeze(),
                fields[4].parse::<i32>().unwrap()
            );
            counters += 1;
        } else if fields[0] == "freeze-armor" {
            let mut level = crate::entity::tests::level(false);
            let entity = add_player(
                &mut level,
                if fields[1] == "true" {
                    GameType::Spectator
                } else {
                    GameType::Survival
                },
            );
            let slot =
                EquipmentSlot::from_json(&serde_json::Value::String(fields[2].into())).unwrap();
            let mut stack = ItemStack::new(Item::LEATHER_BOOTS, 1, Default::default());
            stack.0.as_mut().unwrap().count = fields[3].parse().unwrap();
            level
                .entity_mut::<LivingEntity>(entity)
                .unwrap()
                .equipment
                .set(slot, stack);
            assert_eq!(
                level
                    .entity_behavior(entity)
                    .unwrap()
                    .can_freeze(entity, &level)
                    .unwrap(),
                fields[4] == "true",
                "{row}"
            );
            armor += 1;
        }
    }
    assert_eq!((counters, armor), (10, 48));
}

struct FreezeReceiver(bool);
impl Entity for FreezeReceiver {
    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        references: &mut Vec<EntityRef>,
    ) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
    }

    fn can_freeze(&self, _entity: EntityRef, _level: &Level) -> Result<bool, LevelError> {
        Ok(self.0)
    }
    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: crate::entity::damage::DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        let living = level.entity::<LivingEntity>(entity)?;
        let value = format!(
            "{}:{:x}:{}:{:x}",
            source.damage_type().message_id,
            damage.to_bits(),
            living.ticks_frozen(),
            living
                .attributes
                .value(Attribute::MOVEMENT_SPEED)
                .unwrap()
                .to_bits()
        );
        level
            .entity_mut::<EntityData>(entity)?
            .saved
            .insert("freeze_hurt", ferraloom_nbt::Tag::String(value.into()));
        Ok(false)
    }
}

#[test]
fn native_freezing_phase_decays_before_frost_and_dispatches_periodic_damage_after_it() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("freeze-phase\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level(true);
        let entity = add_player(&mut level, GameType::Survival);
        let base = level.entity_mut::<EntityData>(entity).unwrap();
        base.set_ticks_frozen(f[1].parse().unwrap());
        base.is_in_powder_snow = f[2].parse().unwrap();
        base.tick_count = f[4].parse().unwrap();
        tick_freezing(&FreezeReceiver(f[3].parse().unwrap()), entity, &mut level).unwrap();
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        assert_eq!(
            living.ticks_frozen(),
            f[5].parse::<i32>().unwrap(),
            "counter {row}"
        );
        let modifier = living
            .attributes
            .get_instance(Attribute::MOVEMENT_SPEED)
            .unwrap()
            .modifier(&modifier_id())
            .map(|value| format!("{:x}", value.amount.0.to_bits()))
            .unwrap_or_else(|| "none".into());
        assert_eq!(modifier, f[6], "frost {row}");
        assert_eq!(
            format!(
                "{:x}",
                living
                    .attributes
                    .value(Attribute::MOVEMENT_SPEED)
                    .unwrap()
                    .to_bits()
            ),
            f[7],
            "speed {row}"
        );
        assert_eq!(
            living
                .saved
                .get_string("freeze_hurt")
                .map(|v| v.to_string_lossy())
                .unwrap_or_else(|| "-".to_owned()),
            f[8],
            "damage {row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 64);
}
#[test]
fn powder_snow_modifiers_match_native_float_promotion_and_support_conditions() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("freeze-frost\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level(false);
        let entity = add_player(&mut level, GameType::Survival);
        level
            .set_block_state(
                BlockPos::new(8, 63, 8),
                if fields[1] == "true" {
                    Block::AIR
                } else {
                    Block::STONE
                }
                .default_state(),
            )
            .unwrap();
        level
            .entity_mut::<EntityData>(entity)
            .unwrap()
            .set_ticks_frozen(fields[2].parse().unwrap());
        remove_frost(entity, &mut level).unwrap();
        try_add_frost(entity, &mut level).unwrap();
        assert_eq!(
            fields[3], "1",
            "native support read precedes every counter check"
        );
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        let speed = living
            .attributes
            .get_instance(Attribute::MOVEMENT_SPEED)
            .unwrap();
        assert_eq!(
            speed
                .modifier(&modifier_id())
                .map_or_else(|| "none".into(), |m| format!("{:x}", m.amount.0.to_bits())),
            fields[4]
        );
        assert_eq!(format!("{:x}", speed.value().to_bits()), fields[5]);
        remove_frost(entity, &mut level).unwrap();
        assert!(
            level
                .entity_mut::<LivingEntity>(entity)
                .unwrap()
                .attributes
                .get_instance(Attribute::MOVEMENT_SPEED)
                .unwrap()
                .modifier(&modifier_id())
                .is_none()
        );
        count += 1;
    }
    assert_eq!(count, 20);
}
#[test]
fn zero_frozen_still_requires_the_supporting_block_and_spectators_skip_tag_reads() {
    static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
    let mut level = Level::new(
        crate::level::Region::new(&BEHAVIORS),
        crate::level::LevelContext::fixture(0, 63),
    );
    let entity = add_player(&mut level, GameType::Spectator);
    let behavior = level.entity_behavior(entity).unwrap();
    assert!(!behavior.can_freeze(entity, &level).unwrap());
    level.entity_mut::<ServerPlayer>(entity).unwrap().game_mode = GameType::Survival;
    assert_eq!(
        behavior.can_freeze(entity, &level),
        Err(LevelError::MissingItemTags)
    );
    assert_eq!(
        try_add_frost(entity, &mut level),
        Err(LevelError::MissingChunk(ferraloom_util::ChunkPos::ZERO))
    );
}
