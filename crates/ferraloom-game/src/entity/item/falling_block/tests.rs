use super::*;
use crate::{
    Behaviors,
    block::UnportedBehavior,
    entity::{
        DamageSource,
        animal::{Cow, CowBehavior},
        player::{GameType, PlayerBehavior, ServerPlayer},
    },
};
use ferraloom_util::Uuid;
use std::cell::RefCell;

const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
thread_local! {
    static HURT_CALLS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
struct HurtProbe;
impl Entity for HurtProbe {
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
        if matches!(state, EntityState::Player(_)) {
            PlayerBehavior.trace_entity_refs(state, references);
        } else {
            CowBehavior.trace_entity_refs(state, references);
        }
    }
    fn is_spectator(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        if matches!(level.entity_state(entity)?, EntityState::Player(_)) {
            PlayerBehavior.is_spectator(entity, level)
        } else {
            Ok(false)
        }
    }
    fn is_alive(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        if matches!(level.entity_state(entity)?, EntityState::Player(_)) {
            PlayerBehavior.is_alive(entity, level)
        } else {
            CowBehavior.is_alive(entity, level)
        }
    }
    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: DamageSource,
        amount: f32,
    ) -> Result<bool, LevelError> {
        let network = level.entity::<EntityData>(entity)?.network_id;
        let direct = source
            .direct_entity()
            .expect("falling source direct entity");
        let cause = source
            .causing_entity()
            .expect("falling source causing entity");
        let call = format!(
            "{network}:{}:{:x}:{}:{}",
            source.type_holder().name(),
            amount.to_bits(),
            level.entity::<EntityData>(direct)?.network_id,
            level.entity::<EntityData>(cause)?.network_id
        );
        HURT_CALLS.with_borrow_mut(|calls| calls.push(call));
        if network == 1
            && level
                .entity::<EntityData>(direct)?
                .custom_data
                .get_boolean_or("change_block", false)
        {
            level.entity_mut::<FallingBlockEntity>(direct)?.block_state =
                Block::STONE.default_state();
        }
        if network == 1
            && level
                .entity::<EntityData>(direct)?
                .custom_data
                .get_boolean_or("remove_captured", false)
        {
            for entity in level.all_entities() {
                level.remove_entity(entity)?;
            }
            level.finish_entity_management();
        }
        Ok(false)
    }
}

fn fixture(row: &[&str]) -> (Level, EntityRef) {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY
        .with_entity(EntityType::COW, &HurtProbe)
        .with_entity(EntityType::PLAYER, &HurtProbe);
    let mut level = crate::entity::tests::level_with_behaviors(false, &BEHAVIORS);
    level.set_unported_behavior(UnportedBehavior::Strict);
    let mut falling = FallingBlockEntity::new(77, 0);
    falling.parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
    falling.block_state = BlockState::from_id(row[1].parse().unwrap()).unwrap();
    falling.fall_damage_per_distance = f32::from_bits(u32::from_str_radix(row[3], 16).unwrap());
    falling.fall_damage_max = row[4].parse().unwrap();
    falling.parent.random.set_seed(row[5].parse().unwrap());
    falling.hurt_entities = row[6].parse().unwrap();
    falling
        .parent
        .custom_data
        .insert("change_block", Tag::Byte(i8::from(row[7] == "true")));
    let id = level.next_entity_ref();
    level
        .add_entity(id, EntityState::FallingBlock(Box::new(falling)))
        .unwrap();
    for network in 1..=9 {
        let mut state = if (2..=4).contains(&network) {
            let mode = match network {
                2 => GameType::Survival,
                3 => GameType::Creative,
                _ => GameType::Spectator,
            };
            let mut player = ServerPlayer::new(
                Uuid::new(0, network.into()),
                format!("falling{network}"),
                network,
                Vec3::ZERO,
                mode,
            );
            player.abilities.instabuild = network == 2;
            player.abilities.invulnerable = network == 2;
            EntityState::Player(Box::new(player))
        } else if network == 7 {
            EntityState::Item(Box::new(ItemEntity::new(network, i64::from(network))))
        } else {
            let cow = Cow::with_parent(EntityData::new(
                EntityType::COW,
                network,
                i64::from(network),
            ));
            EntityState::Cow(Box::new(cow))
        };
        let base = state.as_entity_mut();
        base.uuid = Uuid::new(0, i64::from(network));
        base.set_pos(Vec3::new(if network == 8 { 12.0 } else { 8.5 }, 64.0, 8.5));
        if network == 5 {
            state.as_living_mut().unwrap().set_health(0.0);
        }
        let entity = level.next_entity_ref();
        level.add_entity(entity, state).unwrap();
        if network == 6 {
            level.remove_entity(entity).unwrap();
        }
    }
    (level, id)
}

#[test]
fn native_falling_damage_rounding_selection_sources_callback_state_and_degradation_rng() {
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("falling-damage\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let (mut level, entity) = fixture(&fields);
        HURT_CALLS.with_borrow_mut(Vec::clear);
        let distance = f64::from_bits(u64::from_str_radix(fields[2], 16).unwrap());
        let ignored_source = level.damage_sources().drown();
        let result = FallingBlockBehavior
            .cause_fall_damage(entity, &mut level, distance, 9.0, &ignored_source)
            .unwrap();
        let calls = HURT_CALLS.with_borrow_mut(std::mem::take);
        let calls = if calls.is_empty() {
            "-".into()
        } else {
            calls.join(";")
        };
        let falling = level.entity_mut::<FallingBlockEntity>(entity).unwrap();
        assert_eq!(result.to_string(), fields[8], "{row}");
        assert_eq!(falling.block_state.id().to_string(), fields[9], "{row}");
        assert_eq!(falling.cancel_drop.to_string(), fields[10], "{row}");
        assert_eq!(calls, fields[11], "{row}");
        assert_eq!(
            format!("{:x}", falling.parent.random.next_long() as u64),
            fields[12],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 112);
}

#[test]
fn disabled_or_negative_height_damage_returns_before_required_world_reads() {
    let mut level = crate::entity::tests::persistence_level();
    let entity = level.next_entity_ref();
    let falling = FallingBlockEntity::new(1, 1);
    level
        .add_entity(entity, EntityState::FallingBlock(Box::new(falling)))
        .unwrap();
    assert!(level.block_tags().is_err());
    assert!(!cause_fall_damage(entity, 100.0, &mut level).unwrap());
    level
        .entity_mut::<FallingBlockEntity>(entity)
        .unwrap()
        .hurt_entities = true;
    assert!(!cause_fall_damage(entity, -1.0, &mut level).unwrap());
}

#[test]
fn hurt_callbacks_keep_the_captured_entities_alive_through_management() {
    let row = VECTORS
        .lines()
        .find(|row| {
            if !row.starts_with("falling-damage\t") {
                return false;
            }
            let fields: Vec<_> = row.split('\t').collect();
            fields[6] == "true" && fields[11] != "-" && fields[7] == "false"
        })
        .unwrap();
    let fields: Vec<_> = row.split('\t').collect();
    let (mut level, entity) = fixture(&fields);
    level
        .entity_mut::<EntityData>(entity)
        .unwrap()
        .custom_data
        .insert("remove_captured", Tag::Byte(1));
    HURT_CALLS.with_borrow_mut(Vec::clear);
    let distance = f64::from_bits(u64::from_str_radix(fields[2], 16).unwrap());
    cause_fall_damage(entity, distance, &mut level).unwrap();
    let calls = HURT_CALLS.with_borrow_mut(std::mem::take);
    assert_eq!(calls.join(";"), fields[11]);
    assert!(level.all_entities().is_empty());
    level.finish_entity_management();
    assert!(level.entity_state(entity).is_err());
}

#[test]
fn degradation_retains_a_preexisting_cancel_drop_flag() {
    let row = VECTORS
        .lines()
        .filter(|row| row.starts_with("falling-damage\t"))
        .find(|row| {
            let fields: Vec<_> = row.split('\t').collect();
            let initial = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
            initial.block() == Block::ANVIL
                && fields[1] != fields[9]
                && fields[9] != Block::STONE.default_state().id().to_string()
        })
        .unwrap();
    let fields: Vec<_> = row.split('\t').collect();
    let (mut level, entity) = fixture(&fields);
    level
        .entity_mut::<FallingBlockEntity>(entity)
        .unwrap()
        .cancel_drop = true;
    let distance = f64::from_bits(u64::from_str_radix(fields[2], 16).unwrap());
    cause_fall_damage(entity, distance, &mut level).unwrap();
    assert!(
        level
            .entity::<FallingBlockEntity>(entity)
            .unwrap()
            .cancel_drop
    );
}
