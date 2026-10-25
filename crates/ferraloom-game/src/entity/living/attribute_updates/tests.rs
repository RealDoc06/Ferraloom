use super::*;
use crate::{
    entity::{
        EntityState,
        ai::attributes::AttributeMap,
        player::{GameType, ServerPlayer},
    },
    level::effects::LevelEffect,
};
use ferraloom_data::EntityType;
use ferraloom_protocol::play::{WaypointOperation, clientbound as cb};
use ferraloom_util::{Uuid, Vec3};

const VECTORS: &str = include_str!("../../../../tests/java/waypoints/attribute-updates.tsv");

fn set(level: &mut Level, entity: EntityRef, attribute: Attribute, value: f64) {
    level
        .entity_mut::<LivingEntity>(entity)
        .unwrap()
        .attributes
        .get_instance(attribute)
        .unwrap()
        .set_base_value(value);
}
fn fixture() -> (Level, EntityRef, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.install_shared(Box::default());
    let mut players = Vec::new();
    for index in 1..=2 {
        let id = level.next_entity_ref();
        let mut player = ServerPlayer::new(
            Uuid::new(1, index),
            format!("observer{index}"),
            level.next_entity_id(),
            Vec3::new(8.5 + index as f64, 64.0, 8.5),
            GameType::Survival,
        );
        player.attributes = AttributeMap::new(EntityType::PLAYER);
        player.first_tick = false;
        level
            .add_entity(id, EntityState::Player(Box::new(player)))
            .unwrap();
        players.push(id);
    }
    let (player, other) = (players[0], players[1]);
    set(&mut level, player, Attribute::MAX_HEALTH, 40.0);
    level
        .entity_mut::<LivingEntity>(player)
        .unwrap()
        .set_health(35.0);
    set(&mut level, player, Attribute::MAX_ABSORPTION, 20.0);
    level
        .entity_behavior(player)
        .unwrap()
        .set_absorption_amount(player, 15.0, &mut level)
        .unwrap();
    set(&mut level, player, Attribute::WAYPOINT_RECEIVE_RANGE, 0.0);
    set(&mut level, player, Attribute::WAYPOINT_TRANSMIT_RANGE, 0.0);
    set(&mut level, other, Attribute::WAYPOINT_RECEIVE_RANGE, 128.0);
    set(&mut level, other, Attribute::WAYPOINT_TRANSMIT_RANGE, 128.0);
    level.add_waypoint_player(other).unwrap();
    refresh_dirty_attributes(player, &mut level).unwrap();
    refresh_dirty_attributes(other, &mut level).unwrap();
    level.take_effects();
    (level, player, other)
}

#[test]
fn native_refresh_clamps_health_and_absorption_refreshes_scale_and_dispatches_waypoint_ranges() {
    let mut checked = 0;
    for row in VECTORS.lines().filter(|row| row.starts_with("refresh\t")) {
        let fields: Vec<_> = row.split('\t').collect();
        let (mut level, player, other) = fixture();
        let (attribute, value) = match fields[1] {
            "health-down" => (Attribute::MAX_HEALTH, 12.0),
            "health-up" => (Attribute::MAX_HEALTH, 50.0),
            "absorption-down" => (Attribute::MAX_ABSORPTION, 6.0),
            "absorption-up" => (Attribute::MAX_ABSORPTION, 30.0),
            "scale" => (Attribute::SCALE, 2.0),
            "receive-on" => (Attribute::WAYPOINT_RECEIVE_RANGE, 128.0),
            "receive-off" => {
                set(&mut level, player, Attribute::WAYPOINT_RECEIVE_RANGE, 128.0);
                level.add_waypoint_player(player).unwrap();
                (Attribute::WAYPOINT_RECEIVE_RANGE, 0.0)
            }
            "transmit-on" => (Attribute::WAYPOINT_TRANSMIT_RANGE, 128.0),
            "transmit-off" => {
                set(
                    &mut level,
                    player,
                    Attribute::WAYPOINT_TRANSMIT_RANGE,
                    128.0,
                );
                level.track_waypoint(player).unwrap();
                (Attribute::WAYPOINT_TRANSMIT_RANGE, 0.0)
            }
            "other" => (Attribute::MOVEMENT_SPEED, 0.25),
            _ => panic!("unknown native case {row}"),
        };
        set(&mut level, player, attribute, value);
        level.take_effects();
        refresh_dirty_attributes(player, &mut level).unwrap();
        let living = level.entity::<LivingEntity>(player).unwrap();
        assert_eq!(
            format!("{:x}", living.health().to_bits()),
            fields[4],
            "{row}"
        );
        assert_eq!(
            format!(
                "{:x}",
                level
                    .entity::<ServerPlayer>(player)
                    .unwrap()
                    .absorption_amount()
                    .to_bits()
            ),
            fields[5],
            "{row}"
        );
        assert_eq!(
            living.attributes.attributes_to_update().count().to_string(),
            fields[6],
            "{row}"
        );
        // Native refresh clears only the observer set, never network dirtiness.
        assert_ne!(living.attributes.attributes_to_sync().count(), 0);
        if fields[1] == "scale" {
            assert_eq!(living.dimensions.width.to_bits(), (0.6_f32 * 2.0).to_bits());
            assert_eq!(
                living.dimensions.height.to_bits(),
                (1.8_f32 * 2.0).to_bits()
            );
        }
        let packets: Vec<_> = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| {
                if let LevelEffect::PlayerPacket { recipients, packet } = effect
                    && let cb::Packet::Waypoint(packet) = *packet
                {
                    Some((recipients, packet.operation))
                } else {
                    None
                }
            })
            .collect();
        let expected = match fields[3].split(',').next_back().unwrap() {
            "addPlayer" => vec![(vec![player], WaypointOperation::Track)],
            "removePlayer" => vec![(vec![player], WaypointOperation::Untrack)],
            "trackWaypoint" => vec![(vec![other], WaypointOperation::Track)],
            "untrackWaypoint" => vec![(vec![other], WaypointOperation::Untrack)],
            _ => Vec::new(),
        };
        assert_eq!(packets, expected, "{row}");
        refresh_dirty_attributes(player, &mut level).unwrap();
        assert!(level.take_effects().is_empty(), "{row}");
        checked += 1;
    }
    assert_eq!(checked, 10);
}

#[test]
fn native_refresh_uses_the_passed_derived_absorption_setter() {
    struct Derived;
    impl Entity for Derived {
        fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
            state
                .as_living()
                .unwrap()
                .trace_damage_entity_refs(references);
        }
        fn absorption_amount(&self, _: EntityRef, _: &Level) -> Result<f32, LevelError> {
            Ok(15.0)
        }
        fn set_absorption_amount(
            &self,
            entity: EntityRef,
            amount: f32,
            level: &mut Level,
        ) -> Result<(), LevelError> {
            // A derived receiver's setter is observable and must not be replaced
            // by another lookup of the Level's registered PlayerBehavior.
            level.entity_mut::<LivingEntity>(entity)?.last_hurt = amount;
            Ok(())
        }
    }
    let (mut level, player, _) = fixture();
    set(&mut level, player, Attribute::MAX_ABSORPTION, 6.0);
    on_attribute_updated_parent(&Derived, player, Attribute::MAX_ABSORPTION, &mut level).unwrap();
    assert_eq!(level.entity::<LivingEntity>(player).unwrap().last_hurt, 6.0);
    assert_eq!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .absorption_amount(),
        15.0
    );
}

#[test]
fn native_absorption_clamp_is_reused_by_the_attribute_observer() {
    let (mut level, player, _) = fixture();
    set(&mut level, player, Attribute::MAX_ABSORPTION, 6.0);
    let mut checked = 0;
    for row in VECTORS
        .lines()
        .filter(|row| row.starts_with("absorption\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let amount = f32::from_bits(u32::from_str_radix(fields[1], 16).unwrap());
        level
            .entity_behavior(player)
            .unwrap()
            .set_absorption_amount(player, amount, &mut level)
            .unwrap();
        assert_eq!(
            format!(
                "{:x}",
                level
                    .entity::<ServerPlayer>(player)
                    .unwrap()
                    .absorption_amount()
                    .to_bits()
            ),
            fields[2],
            "{row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 9);
}

#[test]
fn native_refresh_keeps_identity_set_order_and_clears_after_all_observers() {
    let fields: Vec<_> = VECTORS
        .lines()
        .find(|row| row.starts_with("refresh-order\t"))
        .unwrap()
        .split('\t')
        .collect();
    let (mut level, player, other) = fixture();
    for (attribute, value) in [
        (Attribute::MAX_HEALTH, 12.0),
        (Attribute::MAX_ABSORPTION, 6.0),
        (Attribute::SCALE, 2.0),
        (Attribute::WAYPOINT_RECEIVE_RANGE, 128.0),
        (Attribute::WAYPOINT_TRANSMIT_RANGE, 128.0),
    ] {
        set(&mut level, player, attribute, value);
    }
    let order: Vec<_> = level
        .entity::<LivingEntity>(player)
        .unwrap()
        .attributes
        .attributes_to_update()
        .map(|instance| instance.attribute())
        .collect();
    assert_eq!(order.len().to_string(), fields[1]);
    let receive = order
        .iter()
        .position(|attribute| *attribute == Attribute::WAYPOINT_RECEIVE_RANGE)
        .unwrap();
    let transmit = order
        .iter()
        .position(|attribute| *attribute == Attribute::WAYPOINT_TRANSMIT_RANGE)
        .unwrap();
    refresh_dirty_attributes(player, &mut level).unwrap();
    let packets: Vec<_> = level
        .take_effects()
        .into_iter()
        .filter_map(|effect| {
            if let LevelEffect::PlayerPacket { recipients, packet } = effect
                && let cb::Packet::Waypoint(packet) = *packet
            {
                assert_eq!(packet.operation, WaypointOperation::Track);
                Some(recipients)
            } else {
                None
            }
        })
        .collect();
    // Native addPlayer creates the receiver row before trackWaypoint updates
    // the transmitter column; it must finish inside the current observer.
    assert_eq!(
        packets,
        if receive < transmit {
            vec![vec![player], vec![other], vec![other]]
        } else {
            vec![vec![other], vec![player], vec![other]]
        }
    );
    let living = level.entity::<LivingEntity>(player).unwrap();
    assert_eq!(living.health(), 12.0);
    assert_eq!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .absorption_amount(),
        6.0
    );
    assert_eq!(
        living.attributes.attributes_to_update().count().to_string(),
        fields[3]
    );
}
