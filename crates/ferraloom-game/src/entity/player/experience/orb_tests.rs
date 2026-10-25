use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    entity::{
        Entity, EntityState, LivingEntity,
        experience_orb::{ExperienceOrb, ExperienceOrbBehavior},
        player::{GameType, PlayerBehavior},
    },
    inventory::{ContainerHandle, ContainerSlot},
    level::effects::LevelEffect,
};
use ferraloom_data::{EntityType, Item};
use ferraloom_protocol::play::clientbound as cb;
use ferraloom_util::{Uuid, Vec3, random::RandomSource};
use std::cell::RefCell;

const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
thread_local! { static CALLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) }; }
struct TakeProbe;
fn damages(player: EntityRef, level: &Level) -> String {
    crate::entity::player::equipment::SLOTS
        .into_iter()
        .map(|slot| {
            let stack = PlayerBehavior
                .item_by_slot(player, slot, level)
                .unwrap()
                .unwrap();
            crate::item::durability::damage_value(stack).to_string()
        })
        .collect::<Vec<_>>()
        .join(",")
}
fn callback(phase: &str, player: EntityRef, orb: EntityRef, level: &Level) {
    let state = level.entity::<ServerPlayer>(player).unwrap();
    let count = level.entity::<ExperienceOrb>(orb).unwrap().count;
    CALLS.with_borrow_mut(|calls| {
        calls.push(format!(
            "{phase}:{}:{count}:{}:{}",
            state.take_xp_delay,
            state.total_experience,
            damages(player, level)
        ))
    });
}
impl Entity for TakeProbe {
    fn trace_entity_refs(&self, state: &EntityState, refs: &mut Vec<EntityRef>) {
        PlayerBehavior.trace_entity_refs(state, refs);
    }
    fn item_by_slot<'a>(
        &self,
        entity: EntityRef,
        slot: ferraloom_protocol::component::EquipmentSlot,
        level: &'a Level,
    ) -> Result<Option<&'a ferraloom_protocol::component::ItemStack>, LevelError> {
        PlayerBehavior.item_by_slot(entity, slot, level)
    }
    fn take(
        &self,
        player: EntityRef,
        entity: EntityRef,
        amount: i32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        callback("before", player, entity, level);
        PlayerBehavior.take(player, entity, amount, level)?;
        callback("after", player, entity, level);
        Ok(())
    }
}
fn fixture(fields: &[&str]) -> (Level, EntityRef, EntityRef) {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY.with_entity(EntityType::PLAYER, &TakeProbe);
    let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
    level.set_unported_behavior(UnportedBehavior::Strict);
    crate::level::drop_fixtures::install(&mut level);
    let player = level.next_entity_ref();
    let mut state = ServerPlayer::new(
        Uuid::new(0, 1),
        "orb".into(),
        1,
        Vec3::new(8.5, 64.0, 8.5),
        GameType::Survival,
    );
    crate::entity::tests::orb_equipment(&mut state, fields[1].parse().unwrap());
    state.tick_count = 37;
    state.take_xp_delay = fields[3].parse().unwrap();
    state.last_sent_experience = 0;
    state.random.set_seed(fields[5].parse().unwrap());
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    let orb = level.next_entity_ref();
    let mut state = ExperienceOrb::new(2, 777);
    state.parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
    state.set_value(fields[2].parse().unwrap());
    state.count = fields[4].parse().unwrap();
    level
        .add_entity(orb, EntityState::ExperienceOrb(Box::new(state)))
        .unwrap();
    level.take_effects();
    (level, player, orb)
}

#[test]
fn native_orb_pickup_mending_recursion_delay_counts_xp_and_receiver_order() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("orb-pickup\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let (mut level, player, orb) = fixture(&fields);
        CALLS.with_borrow_mut(Vec::clear);
        ExperienceOrbBehavior
            .player_touch(orb, player, &mut level)
            .unwrap();
        let state = level.entity::<ServerPlayer>(player).unwrap();
        assert_eq!(state.take_xp_delay.to_string(), fields[6], "{row}");
        assert_eq!(
            format!(
                "{}:{:x}:{}:{}:{}:{}",
                state.experience_level,
                state.experience_progress.to_bits(),
                state.total_experience,
                state.score(),
                state.last_level_up_time,
                state.last_sent_experience
            ),
            fields[10],
            "{row}"
        );
        let state = level.entity::<ExperienceOrb>(orb).unwrap();
        assert_eq!(state.count.to_string(), fields[7], "{row}");
        assert_eq!(state.parent.removed.to_string(), fields[8], "{row}");
        assert_eq!(damages(player, &level), fields[9], "{row}");
        let calls = CALLS.with_borrow_mut(std::mem::take);
        assert_eq!(
            if calls.is_empty() {
                "-".into()
            } else {
                calls.join(";")
            },
            fields[11],
            "{row}"
        );
        assert_eq!(
            format!(
                "{:x}",
                level
                    .entity_mut::<ServerPlayer>(player)
                    .unwrap()
                    .random
                    .next_long() as u64
            ),
            fields[12],
            "{row}"
        );
        assert!(!level.take_effects().iter().any(|effect| matches!(effect, LevelEffect::PlayerPacket { packet, .. } if matches!(packet.as_ref(), cb::Packet::SetExperience(_)))), "XP sync stays in doTick");
        checked += 1;
    }
    assert_eq!(checked, 70);
}

#[test]
fn living_equipment_roots_keep_removed_owner_and_replaced_receivers_addressable() {
    let mut level = crate::entity::tests::level(false);
    let player = level.next_entity_ref();
    let mut state = ServerPlayer::new(
        Uuid::new(0, 1),
        "roots".into(),
        1,
        Vec3::ZERO,
        GameType::Survival,
    );
    crate::entity::tests::orb_equipment(&mut state, 5);
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    let slot = ContainerSlot {
        container: ContainerHandle::LivingEquipment(player),
        slot: 1,
    };
    let reference = level.stack_ref(slot).unwrap();
    level.retain_stack(reference).unwrap();
    level
        .entity_mut::<LivingEntity>(player)
        .unwrap()
        .equipment
        .set(
            ferraloom_protocol::component::EquipmentSlot::Offhand,
            Default::default(),
        );
    level.remove_entity(player).unwrap();
    level.finish_entity_management();
    assert_eq!(
        level.stack(reference).unwrap().item(),
        Item::DIAMOND_PICKAXE
    );
    crate::item::durability::set_damage_value(level.stack_mut(reference).unwrap(), 1);
    assert_eq!(
        crate::item::durability::damage_value(level.stack(reference).unwrap()),
        1
    );
    level.release_stack(reference);
    level.finish_entity_management();
    assert!(level.entity_state(player).is_err());
}
