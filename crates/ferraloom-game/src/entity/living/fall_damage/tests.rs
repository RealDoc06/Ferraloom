use super::*;
use crate::{
    entity::{EntityState, animal::CowBehavior},
    level::{
        effects::LevelEffect,
        game_event::test_support::{self as game_events, Observation},
    },
};
use ferraloom_data::{Block, registries::game_event::Id as GameEvent};
use ferraloom_protocol::play::clientbound as cb;

struct LandingProbe;
impl Entity for LandingProbe {
    fn trace_entity_refs(&self, state: &EntityState, refs: &mut Vec<EntityRef>) {
        CowBehavior.trace_entity_refs(state, refs);
    }
    fn on_changed_block(
        &self,
        entity: EntityRef,
        _: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let position = level.entity::<EntityData>(entity)?.position;
        level.game_event(GameEvent::STEP, position, Some(entity), None)?;
        Ok(())
    }
}
fn bits(v: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        v.x.to_bits(),
        v.y.to_bits(),
        v.z.to_bits()
    )
}

#[test]
fn native_landing_particles_precede_parent_events_and_clear_climb_cache_last() {
    let mut actual = Vec::new();
    for scenario in 0..12 {
        let mut level = crate::entity::tests::level(true);
        game_events::observe(&mut level);
        let id = level.next_entity_ref();
        let mut state = EntityState::Cow(Box::default());
        let living = state.as_living_mut().unwrap();
        living.set_pos(Vec3::new(8.5, 64.25, 8.5));
        living.was_touching_water = true;
        // The landing oracle sets the same zero multiplier so this test only
        // observes particles and the parent landing events, without damage.
        living
            .attributes
            .get_instance(Attribute::FALL_DAMAGE_MULTIPLIER)
            .unwrap()
            .set_base_value(0.0);
        living.fall_distance = [
            0.0, 0.25, 3.25, 7.75, 100.0, -1.0, 0.0, 7.75, 7.75, 3.25, 100.0, 4.0,
        ][scenario];
        living.last_climbable_pos.set(Some(BlockPos::new(8, 64, 8)));
        level.add_entity(id, state).unwrap();
        game_events::take(&mut level);
        let ground = scenario % 3 != 0;
        let block = if scenario % 4 == 0 {
            Block::AIR
        } else {
            Block::STONE
        };
        let pos = BlockPos::new(
            if scenario % 2 == 0 { 8 } else { 7 },
            63,
            if scenario % 2 == 0 { 8 } else { 7 },
        );
        check_fall_damage(
            &LandingProbe,
            id,
            -0.125,
            ground,
            block.default_state(),
            pos,
            &mut level,
        )
        .unwrap();
        let calls = game_events::take(&mut level)
            .into_iter()
            .filter_map(|observation| match observation {
                Observation::GameEvent { event, .. } => Some(format!("event:{}", event.name())),
                Observation::Effect(LevelEffect::PlayerPacket { packet, .. }) => match *packet {
                    cb::Packet::LevelParticles(p) => {
                        let Particle::Block(state) = p.particle else {
                            panic!("block particle")
                        };
                        Some(format!(
                            "particles:{}:{}:{}:{}:{:x}",
                            state.id(),
                            bits(Vec3::new(p.x, p.y, p.z)),
                            p.count.0,
                            bits(Vec3::new(
                                f64::from(p.x_dist),
                                f64::from(p.y_dist),
                                f64::from(p.z_dist)
                            )),
                            f64::from(p.x_max_speed).to_bits()
                        ))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        let living = level.entity::<LivingEntity>(id).unwrap();
        actual.push(format!(
            "living-landing\t{scenario}\t{:x}\t{}\t{}",
            living.fall_distance.to_bits(),
            living.last_climbable_pos.get().is_none(),
            if calls.is_empty() {
                "-".to_owned()
            } else {
                calls.join(";")
            }
        ));
    }
    let expected: Vec<_> = include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("living-landing\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn location_enchantments_use_effective_equipment_and_the_existing_unit_policy() {
    use crate::{block::UnportedBehavior, entity::player::GameType};
    use ferraloom_data::{EntityType, Item};
    use ferraloom_protocol::{
        Identifier,
        codec::VarInt,
        component::{DataComponentPatch, Enchantment, EquipmentSlot, ItemEnchantments, ItemStack},
    };
    use ferraloom_util::Uuid;

    let mut level = crate::entity::tests::level(true);
    let id = level.next_entity_ref();
    let player = ServerPlayer::new(
        Uuid::new(3, 4),
        "landing".into(),
        level.next_entity_id(),
        Vec3::ZERO,
        GameType::Survival,
    );
    level
        .add_entity(id, EntityState::Player(Box::new(player)))
        .unwrap();
    let behavior = level.entity_behavior(id).unwrap();
    let pos = BlockPos::ZERO;
    let enchanted = |count| {
        ItemStack::new(
            Item::DIAMOND_BOOTS,
            count,
            DataComponentPatch {
                added: vec![DataComponent::Enchantments(ItemEnchantments(vec![(
                    Enchantment(Identifier::parse("minecraft:protection").unwrap()),
                    VarInt(1),
                )]))],
                removed: Vec::new(),
            },
        )
    };
    level.set_unported_behavior(UnportedBehavior::Strict);
    behavior.on_changed_block(id, pos, &mut level).unwrap();
    level.entity_mut::<ServerPlayer>(id).unwrap().equipment.set(
        EquipmentSlot::Feet,
        ItemStack::new(Item::LEATHER_BOOTS, 1, DataComponentPatch::default()),
    );
    behavior.on_changed_block(id, pos, &mut level).unwrap();
    level
        .entity_mut::<ServerPlayer>(id)
        .unwrap()
        .inventory
        .set_selected_item(enchanted(0));
    behavior.on_changed_block(id, pos, &mut level).unwrap();
    level
        .entity_mut::<ServerPlayer>(id)
        .unwrap()
        .inventory
        .set_selected_item(enchanted(1));
    assert_eq!(
        behavior.on_changed_block(id, pos, &mut level),
        Err(LevelError::UnsupportedEntityMethod(
            EntityType::PLAYER,
            EntityMethod::RunLocationChangedEffects
        ))
    );
    assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    level.set_unported_behavior(UnportedBehavior::Warn);
    behavior.on_changed_block(id, pos, &mut level).unwrap();
    assert_eq!(level.unported_behavior_stats().skipped_calls, 1);
    level.entity_mut::<ServerPlayer>(id).unwrap().game_mode = GameType::Spectator;
    behavior.on_changed_block(id, pos, &mut level).unwrap();
    assert_eq!(level.unported_behavior_stats().skipped_calls, 1);
}

fn fall_double(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).unwrap())
}
fn impulse_state(entity: EntityRef, level: &Level) -> String {
    let living = level.entity::<LivingEntity>(entity).unwrap();
    format!(
        "{},{}",
        living.current_impulse_impact_pos().map_or_else(
            || "none".to_owned(),
            |position| format!("{:x}", position.y.to_bits())
        ),
        living.current_impulse_context_reset_grace_time
    )
}
fn fall_event(entity: EntityRef, event: String, level: &mut Level) {
    use ferraloom_nbt::{ListTag, Tag};
    let data = &mut level.entity_mut::<EntityData>(entity).unwrap().custom_data;
    if !data.contains_key("fall_callbacks") {
        data.insert("fall_callbacks", Tag::List(ListTag::default()));
    }
    let Tag::List(list) = data.get_mut("fall_callbacks").unwrap() else {
        panic!("fall callback list")
    };
    list.0.push(Tag::String(event.into()));
}
fn fall_events(entity: EntityRef, level: &Level) -> String {
    use ferraloom_nbt::Tag;
    let data = &level.entity::<EntityData>(entity).unwrap().custom_data;
    let Some(Tag::List(events)) = data.get("fall_callbacks") else {
        return "-".into();
    };
    events
        .0
        .iter()
        .map(|event| event.as_string().unwrap().to_string_lossy())
        .collect::<Vec<_>>()
        .join(";")
}
/// The same native virtual boundaries as FallDamageVectors: the inherited
/// parent runs unchanged, while sound/stat/hurt calls record its exact prefix.
struct FallProbe(bool);
impl Entity for FallProbe {
    fn trace_entity_refs(&self, state: &EntityState, refs: &mut Vec<EntityRef>) {
        if self.0 {
            crate::entity::player::PlayerBehavior.trace_entity_refs(state, refs);
        } else {
            CowBehavior.trace_entity_refs(state, refs);
        }
    }
    fn fall_sounds(&self) -> crate::entity::living::FallSounds {
        if self.0 {
            crate::entity::player::PlayerBehavior.fall_sounds()
        } else {
            CowBehavior.fall_sounds()
        }
    }
    fn award_stat(
        &self,
        entity: EntityRef,
        stat: crate::stats::Stat,
        amount: i32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        fall_event(
            entity,
            format!("stat:{}:{amount}", stat.objective_name()),
            level,
        );
        Ok(())
    }
    fn play_sound(
        &self,
        entity: EntityRef,
        sound: ferraloom_data::registries::sound_event::Id,
        volume: f32,
        pitch: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        fall_event(
            entity,
            format!(
                "sound:{}:{:x}:{:x}",
                sound.name(),
                volume.to_bits(),
                pitch.to_bits()
            ),
            level,
        );
        Ok(())
    }
    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: crate::entity::DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        fall_event(
            entity,
            format!(
                "hurt:{}:{:x}:{}",
                source.type_holder().name(),
                damage.to_bits(),
                impulse_state(entity, level)
            ),
            level,
        );
        Ok(false)
    }
}
fn fall_receiver(player: bool, level: &mut Level) -> EntityRef {
    let id = level.next_entity_ref();
    let state = if player {
        EntityState::Player(Box::new(ServerPlayer::new(
            ferraloom_util::Uuid::new(0, id.serial() as i64),
            "fall".into(),
            level.next_entity_id(),
            Vec3::new(8.5, 64.0, 8.5),
            crate::entity::player::GameType::Survival,
        )))
    } else {
        let mut cow = crate::entity::animal::Cow::with_parent(
            level.create_entity_data(ferraloom_data::EntityType::COW),
        );
        cow.as_living_mut().set_pos(Vec3::new(8.5, 64.0, 8.5));
        EntityState::Cow(Box::new(cow))
    };
    level.add_entity(id, state).unwrap();
    id
}
fn fall_attributes(entity: EntityRef, safe: f64, multiplier: f64, level: &mut Level) {
    let living = level.entity_mut::<LivingEntity>(entity).unwrap();
    living
        .attributes
        .get_instance(Attribute::SAFE_FALL_DISTANCE)
        .unwrap()
        .set_base_value(safe);
    living
        .attributes
        .get_instance(Attribute::FALL_DAMAGE_MULTIPLIER)
        .unwrap()
        .set_base_value(multiplier);
}

#[test]
fn native_fall_damage_calculation_preserves_double_epsilon_modifiers_and_floor_saturation() {
    let mut level = crate::entity::tests::level(true);
    let entity = fall_receiver(false, &mut level);
    let mut count = 0;
    for row in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("fall-calculate\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        fall_attributes(entity, fall_double(f[3]), fall_double(f[4]), &mut level);
        let actual = calculate_fall_damage(
            entity,
            fall_double(f[1]),
            f32::from_bits(u32::from_str_radix(f[2], 16).unwrap()),
            &level,
        )
        .unwrap();
        assert_eq!(actual.to_string(), f[5], "{row}");
        count += 1;
    }
    assert_eq!(count, 80);
}

#[test]
fn native_living_and_player_fall_overrides_preserve_impulse_stats_sound_and_hurt_order() {
    let mut count = 0;
    for row in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("fall-apply\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let player = f[1] == "player";
        let mut level = crate::entity::tests::level(true);
        let entity = fall_receiver(player, &mut level);
        fall_attributes(entity, fall_double(f[5]), fall_double(f[6]), &mut level);
        let living = level.entity_mut::<LivingEntity>(entity).unwrap();
        if f[7] != "none" {
            living.set_ignore_fall_damage_from_current_impulse(
                true,
                Vec3::new(8.5, fall_double(f[7]), 8.5),
            );
        }
        living.current_impulse_context_reset_grace_time = f[8].parse().unwrap();
        living
            .entity_data
            .set(crate::entity::synched_data::SILENT, f[9] == "true");
        if player {
            level
                .entity_mut::<ServerPlayer>(entity)
                .unwrap()
                .abilities
                .mayfly = f[10] == "true";
        }
        if f[11] == "true" {
            level
                .set_block_state(BlockPos::new(8, 63, 8), Block::AIR.default_state())
                .unwrap();
        }
        let source = if f[2].parse::<usize>().unwrap() % 2 == 0 {
            level.damage_sources().fall()
        } else {
            level.damage_sources().drown()
        };
        let behavior = FallProbe(player);
        let distance = fall_double(f[3]);
        let modifier = f32::from_bits(u32::from_str_radix(f[4], 16).unwrap());
        let result = if player {
            player_cause_fall_damage(&behavior, entity, &mut level, distance, modifier, &source)
        } else {
            cause_fall_damage(&behavior, entity, &mut level, distance, modifier, &source)
        }
        .unwrap();
        assert_eq!(
            format!(
                "{result}\t{}\t{}",
                impulse_state(entity, &level),
                fall_events(entity, &level)
            ),
            f[12..].join("\t"),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 64);
}

#[test]
fn native_fall_parent_propagates_to_the_actual_passenger_before_own_damage() {
    let mut count = 0;
    for row in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("fall-passengers\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level(true);
        let parent = fall_receiver(false, &mut level);
        let child = fall_receiver(false, &mut level);
        fall_attributes(child, 100.0, 1.0, &mut level);
        let living = level.entity_mut::<LivingEntity>(child).unwrap();
        living.set_ignore_fall_damage_from_current_impulse(true, Vec3::new(8.5, 69.0, 8.5));
        living.current_impulse_context_reset_grace_time = f[1].parse().unwrap();
        assert!(crate::entity::start_riding(child, parent, true, &mut level).unwrap());
        let source = level.damage_sources().drown();
        let result =
            cause_fall_damage(&FallProbe(false), parent, &mut level, 6.0, 1.0, &source).unwrap();
        assert_eq!(
            format!(
                "{result}\t{}\t{}",
                impulse_state(child, &level),
                fall_events(parent, &level)
            ),
            f[2..].join("\t"),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 2);
}
