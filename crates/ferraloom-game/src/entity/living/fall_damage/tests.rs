use super::*;
use crate::{
    entity::{EntityState, animal::CowBehavior},
    level::effects::LevelEffect,
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
        level.game_event(GameEvent::STEP, position, Some(entity), None);
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
        let id = level.next_entity_ref();
        let mut state = EntityState::Cow(Box::default());
        let living = state.as_living_mut().unwrap();
        living.set_pos(Vec3::new(8.5, 64.25, 8.5));
        living.was_touching_water = true;
        living.fall_distance = [
            0.0, 0.25, 3.25, 7.75, 100.0, -1.0, 0.0, 7.75, 7.75, 3.25, 100.0, 4.0,
        ][scenario];
        living.last_climbable_pos.set(Some(BlockPos::new(8, 64, 8)));
        level.add_entity(id, state).unwrap();
        level.take_effects();
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
        let calls = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                LevelEffect::GameEvent { event, .. } => Some(format!("event:{}", event.name())),
                LevelEffect::PlayerPacket { packet, .. } => match *packet {
                    cb::Packet::LevelParticles(p) => {
                        let Particle::Block(state) = p.particle else {
                            panic!("block particle")
                        };
                        Some(format!(
                            "particles:{}:{}:{}:{}:{:x}",
                            state.id(),
                            bits(Vec3::new(p.x, p.y, p.z)),
                            p.count,
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
