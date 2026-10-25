//! Port of Entity's bubble-column hooks. Block contact queries live in block/.
use super::{Entity, EntityData, EntityRef};
use crate::level::{Level, LevelError};
use ferraloom_protocol::entity_data::Particle;
use ferraloom_util::{BlockPos, Vec3, random::RandomSource};

/// Port of Entity#handleOnAboveBubbleColumn. Above-column movement does not
/// reset fall distance, and particles are emitted even without local authority.
pub fn on_above_bubble_column<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    drag_down: bool,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    if behavior.can_simulate_movement(entity, level)? {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.velocity.y = if drag_down {
            (base.velocity.y - 0.03).max(-0.9)
        } else {
            (base.velocity.y + 0.1).min(1.8)
        };
    }
    for _ in 0..2 {
        send_particles(level, pos, Particle::Splash, 0.0, 1.0)?;
        send_particles(level, pos, Particle::Bubble, 0.01, 0.2)?;
    }
    Ok(())
}

/// Port of Entity#handleOnInsideBubbleColumn. Fall distance resets even when
/// this instance cannot simulate movement.
pub fn on_inside_bubble_column<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    drag_down: bool,
    level: &mut Level,
) -> Result<(), LevelError> {
    if behavior.can_simulate_movement(entity, level)? {
        let base = level.entity_mut::<EntityData>(entity)?;
        base.velocity.y = if drag_down {
            (base.velocity.y - 0.03).max(-0.3)
        } else {
            (base.velocity.y + 0.06).min(0.7)
        };
    }
    level.entity_mut::<EntityData>(entity)?.fall_distance = 0.0;
    Ok(())
}

fn send_particles(
    level: &mut Level,
    pos: BlockPos,
    particle: Particle,
    y_dist: f64,
    speed: f64,
) -> Result<(), LevelError> {
    let x = f64::from(pos.x) + level.random().next_double();
    let y = f64::from(pos.y.wrapping_add(1));
    let z = f64::from(pos.z) + level.random().next_double();
    level.send_particles(
        particle,
        Vec3::new(x, y, z),
        1,
        Vec3::new(0.0, y_dist, 0.0),
        speed,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::{
            EntityState,
            item::PrimedTnt,
            player::{GameType, ServerPlayer},
        },
        level::Region,
    };
    use ferraloom_util::{Uuid, Vec3, random::LegacyRandomSource};

    #[test]
    fn vanilla_bubble_column_velocity_and_fall_distance_bits() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut rows = 0;
        for row in include_str!("../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|row| row.starts_with("bubble\t"))
        {
            let fields: Vec<_> = row.split('\t').collect();
            let mut level = {
                let mut fixture_level = Level::new(
                    Region::new(&BEHAVIORS),
                    crate::level::LevelContext::fixture(0, 63),
                );
                fixture_level.install_shared(Box::default());
                fixture_level
            };
            let id = level.next_entity_ref();
            let network = level.next_entity_id().unwrap();
            let state = if fields[1] != "tnt" {
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(1, 2),
                    "bubble".into(),
                    network,
                    Vec3::ZERO,
                    GameType::Survival,
                )))
            } else {
                EntityState::PrimedTnt(Box::new(PrimedTnt::new(network, 123)))
            };
            level.add_entity(id, state).unwrap();
            if fields[1] == "flying" {
                level
                    .entity_mut::<ServerPlayer>(id)
                    .unwrap()
                    .abilities
                    .flying = true;
            }
            level.random().set_seed(123);
            let base = level.entity_mut::<EntityData>(id).unwrap();
            base.velocity = Vec3::new(0.25, fields[4].parse().unwrap(), -0.125);
            base.fall_distance = 8.0;
            let behavior = level.entity_behavior(id).unwrap();
            let drag = fields[3].parse().unwrap();
            if fields[2] == "above" {
                behavior
                    .on_above_bubble_column(id, drag, BlockPos::ZERO, &mut level)
                    .unwrap();
            } else {
                behavior
                    .on_inside_bubble_column(id, drag, &mut level)
                    .unwrap();
            }
            let base = level.entity::<EntityData>(id).unwrap();
            let actual = format!(
                "{:x},{:x},{:x}\t{:x}",
                base.velocity.x.to_bits(),
                base.velocity.y.to_bits(),
                base.velocity.z.to_bits(),
                base.fall_distance.to_bits()
            );
            assert_eq!(actual, format!("{}\t{}", fields[5], fields[6]), "{row}");
            if fields[1] == "flying" {
                assert!(level.take_entity_effects().is_empty(), "{row}");
                assert_eq!(
                    level.random().next_long(),
                    LegacyRandomSource::new(123).next_long(),
                    "{row}"
                );
            }
            rows += 1;
        }
        assert_eq!(rows, 48);
    }

    #[test]
    fn above_column_particles_consume_level_random_in_native_packet_order() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = {
            let mut fixture_level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        let entity = level.next_entity_ref();
        let tnt = PrimedTnt::new(level.next_entity_id().unwrap(), 123);
        level
            .add_entity(entity, EntityState::PrimedTnt(Box::new(tnt)))
            .unwrap();
        let mut expected_random = LegacyRandomSource::new(123);
        level.random().set_seed(123);
        let pos = BlockPos::new(3, 64, -7);
        level
            .entity_behavior(entity)
            .unwrap()
            .on_above_bubble_column(entity, false, pos, &mut level)
            .unwrap();
        let effects = level.take_effects();
        assert_eq!(effects.len(), 4);
        for (index, effect) in effects.iter().enumerate() {
            let crate::level::effects::LevelEffect::PlayerPacket { packet, .. } = effect else {
                panic!("expected particle output");
            };
            let ferraloom_protocol::play::clientbound::Packet::LevelParticles(packet) =
                packet.as_ref()
            else {
                panic!("expected particle packet");
            };
            let (particle, spread, speed) = if index % 2 == 0 {
                (Particle::Splash, 0.0, 1.0)
            } else {
                (Particle::Bubble, 0.01, 0.2)
            };
            assert_eq!(packet.particle, particle);
            assert_eq!(
                packet.x.to_bits(),
                (3.0 + expected_random.next_double()).to_bits()
            );
            assert_eq!(packet.y, 65.0);
            assert_eq!(
                packet.z.to_bits(),
                (-7.0 + expected_random.next_double()).to_bits()
            );
            assert_eq!(
                [packet.x_dist, packet.y_dist, packet.z_dist],
                [0.0, spread, 0.0]
            );
            assert_eq!(
                [packet.x_max_speed, packet.y_max_speed, packet.z_max_speed],
                [speed; 3]
            );
            assert_eq!(packet.count.0, 1);
        }
        assert_eq!(level.random().next_long(), expected_random.next_long());
    }
}
