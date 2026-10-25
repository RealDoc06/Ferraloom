use super::*;
use crate::{
    block::UnportedBehavior,
    entity::{
        RemovalReason,
        animal::{Cow, CowBehavior},
        player::{GameType, PlayerBehavior},
    },
};
use ferraloom_data::registries::game_event::Id as GameEvent;
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_util::Uuid;
const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
const BOUNDARY: LevelError = LevelError::Inventory("synthetic actuallyHurt return boundary");
fn player(entity: EntityRef, level: &Level) -> bool {
    matches!(level.entity_state(entity).unwrap(), EntityState::Player(_))
}
fn absorption(entity: EntityRef, level: &Level) -> f32 {
    if player(entity, level) {
        level
            .entity::<ServerPlayer>(entity)
            .unwrap()
            .absorption_amount()
    } else {
        level
            .entity::<LivingEntity>(entity)
            .unwrap()
            .absorption_amount()
    }
}
fn snapshot(entity: EntityRef, level: &Level) -> String {
    let living = level.entity::<LivingEntity>(entity).unwrap();
    let stats = if player(entity, level) {
        let counter = &level.entity::<ServerPlayer>(entity).unwrap().stats;
        [
            custom::Id::DAMAGE_TAKEN,
            custom::Id::DAMAGE_ABSORBED,
            custom::Id::DAMAGE_RESISTED,
        ]
        .map(|stat| counter.get_value(Stat::Custom(stat)).to_string())
        .join(",")
    } else {
        "-".into()
    };
    format!(
        "{:x},{:x},{:x},{},{},{stats}",
        living.health().to_bits(),
        absorption(entity, level).to_bits(),
        living.last_hurt.to_bits(),
        living.damage_cooldown_time,
        living.hurt_time
    )
}
fn event(entity: EntityRef, text: &str, level: &mut Level) {
    let value = format!("{text}:{}", snapshot(entity, level));
    let tag = &mut level.entity_mut::<EntityData>(entity).unwrap().custom_data;
    if !tag.contains_key("hurt_events") {
        tag.insert("hurt_events", Tag::List(ListTag::default()));
    }
    let Tag::List(events) = tag.get_mut("hurt_events").unwrap() else {
        panic!("event list")
    };
    events.0.push(Tag::String(value.into()));
}
/// These native virtual callbacks record the observed prefix while invoking the
/// canonical stat, game-event and combat-packet producers underneath it.
struct Probe;
impl Entity for Probe {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        state
            .as_living()
            .unwrap()
            .trace_damage_entity_refs(references);
    }
    fn is_alive(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        level.entity_behavior(entity)?.is_alive(entity, level)
    }
    fn is_invulnerable_to(
        &self,
        entity: EntityRef,
        source: &DamageSource,
        level: &Level,
    ) -> Result<bool, LevelError> {
        if player(entity, level) {
            player_is_invulnerable_to(self, entity, source, level)
        } else {
            living_is_invulnerable_to(self, entity, source, level)
        }
    }
    fn absorption_amount(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        Ok(absorption(entity, level))
    }
    fn set_absorption_amount(
        &self,
        entity: EntityRef,
        amount: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if player(entity, level) {
            level
                .entity_mut::<ServerPlayer>(entity)?
                .set_absorption_amount(amount);
        } else {
            level
                .entity_mut::<LivingEntity>(entity)?
                .set_absorption_amount(amount);
        }
        Ok(())
    }
    fn award_stat(
        &self,
        entity: EntityRef,
        stat: Stat,
        amount: i32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        player::award_stat(entity, stat, amount, level)?;
        event(
            entity,
            &format!("stat:{}:{amount}", stat.objective_name()),
            level,
        );
        Ok(())
    }
    fn game_event(
        &self,
        entity: EntityRef,
        kind: GameEvent,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        event(entity, &format!("event:{}", kind.name()), level);
        self.game_event_with_source(entity, kind, Some(entity), level)
    }
    fn on_enter_combat(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        if player(entity, level) {
            event(entity, "packet:enter", level);
            PlayerBehavior.on_enter_combat(entity, level)?;
        }
        Ok(())
    }
    fn hurt_armor(
        &self,
        entity: EntityRef,
        source: &DamageSource,
        damage: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if player(entity, level) {
            PlayerBehavior.hurt_armor(entity, source, damage, level)
        } else {
            CowBehavior.hurt_armor(entity, source, damage, level)
        }
    }
    fn actually_hurt(
        &self,
        entity: EntityRef,
        source: &DamageSource,
        damage: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if player(entity, level) {
            player_actually_hurt(self, entity, source, damage, level)?;
        } else {
            actually_hurt(self, entity, source, damage, level)?;
        }
        if level
            .entity::<EntityData>(entity)?
            .custom_data
            .get_boolean_or("stop_after_actual", false)
        {
            return Err(BOUNDARY);
        }
        Ok(())
    }
}
fn fixture(is_player: bool) -> (Level, EntityRef, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    level.install_shared(Box::default());
    let entity = level.next_entity_ref();
    let state = if is_player {
        let mut player = ServerPlayer::new(
            Uuid::new(5, 6),
            "hurt".into(),
            level.next_entity_id(),
            Vec3::new(8.5, 64.0, 8.5),
            GameType::Survival,
        );
        player.loaded = true;
        EntityState::Player(Box::new(player))
    } else {
        let mut parent = level.create_entity_data(EntityType::COW);
        parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
        EntityState::Cow(Box::new(Cow::with_parent(parent)))
    };
    level.add_entity(entity, state).unwrap();
    let attacker = level.next_entity_ref();
    let mut parent = level.create_entity_data(EntityType::COW);
    parent.set_pos(Vec3::new(10.5, 64.0, 8.5));
    level
        .add_entity(
            attacker,
            EntityState::Cow(Box::new(Cow::with_parent(parent))),
        )
        .unwrap();
    level.take_effects();
    (level, entity, attacker)
}
fn float(value: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(value, 16).unwrap())
}
fn source(name: &str, attacker: EntityRef, level: &Level) -> DamageSource {
    let sources = level.damage_sources();
    match name {
        "fall" => sources.fall(),
        "lava" => sources.lava(),
        "freeze" => sources.freeze(),
        "in_wall" => sources.in_wall(),
        "mob_attack" => sources.mob_attack(attacker),
        "generic_kill" => sources.generic_kill(),
        "generic" => sources.generic(),
        _ => panic!("unknown native damage type"),
    }
}
fn events(entity: EntityRef, level: &Level) -> String {
    level
        .entity::<EntityData>(entity)
        .unwrap()
        .custom_data
        .get_list("hurt_events")
        .map(|list| {
            list.iter()
                .map(|tag| tag.as_string().unwrap().to_string_lossy())
                .collect::<Vec<_>>()
                .join(";")
        })
        .filter(|text| !text.is_empty())
        .unwrap_or("-".into())
}
fn native_events(text: &str) -> String {
    // getAdvancements is an instrumented native getter. The shared advancement
    // producers have their own native trigger vectors, rather than this probe.
    let events = text
        .split(';')
        .filter(|value| !value.starts_with("criterion:"))
        .collect::<Vec<_>>()
        .join(";");
    if events.is_empty() {
        "-".into()
    } else {
        events
    }
}
#[test]
fn native_damage_prefix_and_increment_replay_preserve_stats_health_absorption_and_callbacks() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("hurt-pipeline\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity, attacker) = fixture(f[1] == "player");
        let setup: Vec<_> = f[5].split(',').map(float).collect();
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        living.tick_count = 17;
        living.yaw = 37.25;
        living.y_head_rot = 37.25;
        living.on_ground = f[6].parse().unwrap();
        living.velocity = Vec3::new(0.25, -0.125, 0.5);
        living
            .attributes
            .get_instance(Attribute::ARMOR)
            .unwrap()
            .set_base_value(f64::from(setup[1]));
        living
            .attributes
            .get_instance(Attribute::ARMOR_TOUGHNESS)
            .unwrap()
            .set_base_value(f64::from(setup[2]));
        living
            .attributes
            .get_instance(Attribute::MAX_ABSORPTION)
            .unwrap()
            .set_base_value(f64::from(setup[4]));
        if f[1] == "player" {
            super::super::damage_state::player_internal_set_absorption_amount(living, setup[3]);
        } else {
            living.internal_set_absorption_amount(setup[3]);
        }
        if setup[5] >= 0.0 || setup[5] == -2.0 {
            let effect = if setup[5] == -2.0 {
                MobEffect::FIRE_RESISTANCE
            } else {
                MobEffect::RESISTANCE
            };
            let amplifier = if setup[5] < 0.0 { 0 } else { setup[5] as i32 };
            let tag = ferraloom_nbt::parse(&format!(
                "[{{id:\"{}\",duration:100,amplifier:{amplifier}}}]",
                effect.name()
            ))
            .unwrap();
            assert!(living.active_effects.load(Some(&tag)).is_empty());
        }
        living.damage_cooldown_time = setup[6] as i32;
        living.last_hurt = setup[7];
        living.random.set_seed(12345);
        living
            .custom_data
            .insert("stop_after_actual", Tag::Byte(i8::from(f[2] == "hurt")));
        let damage_source = source(f[4], attacker, &level);
        let result: Result<String, LevelError> = if f[2] == "actual" {
            if f[1] == "player" {
                player_actually_hurt(&Probe, entity, &damage_source, setup[0], &mut level)
            } else {
                actually_hurt(&Probe, entity, &damage_source, setup[0], &mut level)
            }
            .map(|()| "actual".into())
        } else {
            let result = if f[1] == "player" {
                player_hurt_server(&Probe, entity, damage_source, setup[0], &mut level)
            } else {
                hurt_server(&Probe, entity, damage_source, setup[0], &mut level)
            };
            match result {
                Err(error) if error == BOUNDARY => Ok("boundary".into()),
                result => result.map(|value| value.to_string()),
            }
        };
        assert_eq!(result.unwrap(), f[7], "{row}");
        assert_eq!(snapshot(entity, &level), f[8], "{row}");
        let living = level.entity::<LivingEntity>(entity).unwrap();
        assert_eq!(
            format!(
                "{:x},{:x},{:x}",
                living.velocity.x.to_bits(),
                living.velocity.y.to_bits(),
                living.velocity.z.to_bits()
            ),
            f[9],
            "{row}"
        );
        assert_eq!(living.needs_sync.to_string(), f[10], "{row}");
        assert_eq!(living.sync_velocity.to_string(), f[11], "{row}");
        let exhaustion = if f[1] == "player" {
            format!(
                "{:x}",
                level
                    .entity::<ServerPlayer>(entity)
                    .unwrap()
                    .food_data
                    .exhaustion_level()
                    .to_bits()
            )
        } else {
            "-".into()
        };
        assert_eq!(exhaustion, f[12], "{row}");
        assert_eq!(
            living
                .last_damage_source
                .as_ref()
                .map(|source| source.type_holder().name().to_string())
                .unwrap_or("-".into()),
            f[13],
            "{row}"
        );
        assert_eq!(
            living.last_hurt_by_mob.is_some().to_string(),
            f[14],
            "{row}"
        );
        assert_eq!(
            living.last_hurt_by_player.is_some().to_string(),
            f[15],
            "{row}"
        );
        assert_eq!(events(entity, &level), native_events(f[16]), "{row}");
        assert_eq!(
            format!(
                "{:x}",
                level
                    .entity_mut::<EntityData>(entity)
                    .unwrap()
                    .random
                    .next_long() as u64
            ),
            f[17],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 70);
}
#[test]
fn damage_call_roots_keep_removed_receivers_and_sources_alive_across_management() {
    struct Reentrant;
    impl Entity for Reentrant {
        fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
            state
                .as_living()
                .unwrap()
                .trace_damage_entity_refs(references);
        }
        fn actually_hurt(
            &self,
            entity: EntityRef,
            source: &DamageSource,
            _damage: f32,
            level: &mut Level,
        ) -> Result<(), LevelError> {
            let attacker = source.causing_entity().unwrap();
            level.remove_entity_with_reason(entity, RemovalReason::Discarded)?;
            level.remove_entity_with_reason(attacker, RemovalReason::Discarded)?;
            level.finish_entity_management();
            level.entity_mut::<LivingEntity>(entity)?.set_health(7.0);
            level.entity_mut::<LivingEntity>(attacker)?.set_health(3.0);
            assert_eq!(level.entity::<LivingEntity>(entity)?.health(), 7.0);
            assert_eq!(level.entity::<LivingEntity>(attacker)?.health(), 3.0);
            Err(BOUNDARY)
        }
    }
    let (mut level, victim, attacker) = fixture(false);
    let source = level.damage_sources().mob_attack(attacker);
    assert_eq!(
        hurt_server(&Reentrant, victim, source, 4.0, &mut level),
        Err(BOUNDARY)
    );
    level.finish_entity_management();
    assert!(level.entity_state(victim).is_err());
    assert!(level.entity_state(attacker).is_err());
}

#[test]
fn native_knockback_resistance_ground_cap_and_zero_direction_consume_the_same_random() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("hurt-knockback\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity, _) = fixture(false);
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        living
            .attributes
            .get_instance(Attribute::KNOCKBACK_RESISTANCE)
            .unwrap()
            .set_base_value(f[2].parse().unwrap());
        living.on_ground = f[3].parse().unwrap();
        living.velocity = Vec3::new(0.25, -0.125, 0.5);
        living.random.set_seed(12345);
        let (x, z) = if f[4] == "true" {
            (0.0, 0.0)
        } else {
            (2.0, -3.0)
        };
        knockback(entity, f[1].parse().unwrap(), x, z, &mut level).unwrap();
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        assert_eq!(
            format!(
                "{:x},{:x},{:x}",
                living.velocity.x.to_bits(),
                living.velocity.y.to_bits(),
                living.velocity.z.to_bits()
            ),
            f[5],
            "{row}"
        );
        assert_eq!(living.needs_sync.to_string(), f[6], "{row}");
        assert_eq!(
            format!("{:x}", living.random.next_long() as u64),
            f[7],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 36);
}
#[test]
fn native_hurt_sound_selection_baby_pitch_silence_and_separate_sound_rng_match() {
    use ferraloom_protocol::codec::Holder;
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("hurt-sound\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity, _) = fixture(f[1] == "player");
        if f[1] == "baby_cow" {
            level
                .entity_mut::<Cow>(entity)
                .unwrap()
                .parent
                .parent
                .parent
                .age = -24000;
        }
        let base = level.entity_mut::<EntityData>(entity).unwrap();
        base.entity_data
            .set(crate::entity::synched_data::SILENT, f[3].parse().unwrap());
        base.random.set_seed(12345);
        level.set_sound_seed(777);
        let source = if f[2] == "drown" {
            level.damage_sources().drown()
        } else {
            source(f[2], entity, &level)
        };
        play_hurt_sound(
            level.entity_behavior(entity).unwrap(),
            entity,
            &source,
            &mut level,
        )
        .unwrap();
        let sounds: Vec<_> = level
            .take_effects()
            .into_iter()
            .map(|effect| {
                let LevelEffect::PlayerPacket { recipients, packet } = effect else {
                    panic!("sound effect")
                };
                let cb::Packet::Sound(sound) = *packet else {
                    panic!("hurt sound packet")
                };
                if f[1] == "player" {
                    assert!(!recipients.contains(&entity));
                }
                let Holder::Reference(name) = sound.sound.0 else {
                    panic!("registered sound")
                };
                format!(
                    "sound:{name}:{}:{:x}:{:x}:{:x}",
                    sound.source as i32,
                    sound.volume.to_bits(),
                    sound.pitch.to_bits(),
                    sound.seed as u64
                )
            })
            .collect();
        assert_eq!(
            if sounds.is_empty() {
                "-".into()
            } else {
                sounds.join(";")
            },
            f[4],
            "{row}"
        );
        assert_eq!(
            format!(
                "{:x}",
                level
                    .entity_mut::<EntityData>(entity)
                    .unwrap()
                    .random
                    .next_long() as u64
            ),
            f[5],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 30);
}

#[test]
fn native_damage_memory_tick_keeps_the_player_cooldown_and_expires_at_the_native_boundaries() {
    use crate::entity::living::damage_state::{
        base_tick_damage_state, set_last_hurt_by_mob, set_last_hurt_by_player, set_last_hurt_mob,
    };
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("hurt-memory-tick\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, entity, attacker) = fixture(f[1] == "player");
        let values: Vec<i32> = f[4]
            .trim_matches(['[', ']'])
            .split(',')
            .map(|v| v.parse().unwrap())
            .collect();
        let player = level.next_entity_ref();
        let state = ServerPlayer::new(
            Uuid::new(10, 11),
            "remembered".into(),
            level.next_entity_id(),
            Vec3::ZERO,
            GameType::Survival,
        );
        level
            .add_entity(player, EntityState::Player(Box::new(state)))
            .unwrap();
        level
            .entity_mut::<LivingEntity>(attacker)
            .unwrap()
            .set_health(if f[2] == "true" { 10.0 } else { 0.0 });
        level.entity_mut::<EntityData>(entity).unwrap().tick_count = values[3];
        set_last_hurt_by_mob(entity, Some(attacker), &mut level).unwrap();
        set_last_hurt_mob(entity, Some(attacker), &mut level).unwrap();
        set_last_hurt_by_player(entity, Some(player), values[2], &mut level).unwrap();
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        living.hurt_time = values[0];
        living.damage_cooldown_time = values[1];
        living.tick_count = values[4];
        base_tick_damage_state(entity, &mut level).unwrap();
        let living = level.entity::<LivingEntity>(entity).unwrap();
        assert_eq!(
            format!(
                "{},{},{},{},{},{},{},{}",
                living.hurt_time,
                living.damage_cooldown_time,
                living.last_hurt_by_player_memory_time,
                living.last_hurt_by_player.is_some(),
                living.last_hurt_by_mob.is_some(),
                living.last_hurt_by_mob_timestamp,
                living.last_hurt_mob.is_some(),
                living.last_hurt_mob_timestamp
            ),
            f[5],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 24);
}
#[test]
fn last_hurt_mob_holds_the_actual_receiver_until_the_native_alive_check_clears_it() {
    use crate::entity::living::damage_state::{base_tick_damage_state, set_last_hurt_mob};
    let (mut level, entity, target) = fixture(false);
    level.entity_mut::<EntityData>(entity).unwrap().tick_count = 17;
    set_last_hurt_mob(entity, Some(target), &mut level).unwrap();
    level.remove_entity(target).unwrap();
    level.finish_entity_management();
    assert!(level.entity_state(target).is_ok());
    base_tick_damage_state(entity, &mut level).unwrap();
    let living = level.entity::<LivingEntity>(entity).unwrap();
    assert_eq!(living.last_hurt_mob(), None);
    assert_eq!(living.last_hurt_mob_timestamp(), 17);
    level.finish_entity_management();
    assert!(level.entity_state(target).is_err());
}
