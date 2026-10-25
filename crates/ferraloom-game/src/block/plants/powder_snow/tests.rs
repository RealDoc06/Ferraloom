use super::*;
use crate::{
    block::{BlockMethod, UnportedBehavior},
    entity::{
        EntityData, EntityState,
        animal::Cow,
        inside_block,
        item::item_entity::ItemEntity,
        player::{GameType, ServerPlayer},
        synched_data,
    },
    level::{border::WorldBorder, interaction::SpawnProtectionSnapshot},
};
use ferraloom_protocol::{codec::Holder, play::clientbound as cb};
use ferraloom_util::{
    Uuid,
    random::{LegacyRandomSource, Random},
};
use std::io::Read;

const POS: BlockPos = BlockPos::new(8, 100, 8);
fn vectors() -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/plants/powder/powder-snow.tsv.gz").as_slice(),
    )
    .read_to_string(&mut text)
    .unwrap();
    text
}
fn fixture(kind: u8, permissions: bool) -> (Level, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    if permissions {
        level.install_shared(Box::default());
        level.set_spawn_protection(SpawnProtectionSnapshot::new(
            "minecraft:overworld".parse().unwrap(),
            "minecraft:overworld".parse().unwrap(),
            POS,
            0,
        ));
    }
    let id = level.next_entity_ref();
    let network = level.next_entity_id();
    let state = match kind {
        0 => EntityState::Item(Box::new(ItemEntity::new(network, 0))),
        1 => {
            let mut cow = Cow::with_parent(EntityData::new(EntityType::COW, network, 0));
            cow.as_living_mut().set_health(10.0);
            EntityState::Cow(Box::new(cow))
        }
        2 => EntityState::Player(Box::new(ServerPlayer::new(
            Uuid::new(0, 1),
            "powder".into(),
            network,
            Vec3::new(8.5, 100.0, 8.5),
            GameType::Survival,
        ))),
        _ => unreachable!(),
    };
    level.add_entity(id, state).unwrap();
    level
        .set_entity_pos(id, Vec3::new(8.5, 100.0, 8.5))
        .unwrap();
    level.take_effects();
    (level, id)
}
fn vector(v: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        v.x.to_bits(),
        v.y.to_bits(),
        v.z.to_bits()
    )
}
fn counters(entity: EntityRef, level: &Level) -> String {
    let base = level.entity::<EntityData>(entity).unwrap();
    format!(
        "{},{},{}",
        base.is_in_powder_snow,
        base.ticks_frozen(),
        base.remaining_fire_ticks
    )
}
fn emissions(level: &mut Level) -> String {
    level
        .take_effects()
        .into_iter()
        .filter_map(|effect| {
            let LevelEffect::PlayerPacket { packet, .. } = effect else {
                return None;
            };
            match *packet {
                cb::Packet::Sound(p) => {
                    let Holder::Reference(sound) = p.sound.0 else {
                        panic!("registered fall sound");
                    };
                    Some(format!(
                        "sound:{sound}:{}:{:x}:{:x}",
                        p.source as i32,
                        p.volume.to_bits(),
                        p.pitch.to_bits()
                    ))
                }
                cb::Packet::LevelParticles(p) if p.particle == Particle::Snowflake => {
                    assert!(!p.override_limiter && !p.always_show);
                    assert_eq!([p.x_dist, p.y_dist, p.z_dist], [0.0; 3]);
                    Some(format!(
                        "particle:{}:{}:{:x},{:x},{:x}:{}",
                        vector(Vec3::new(p.x, p.y, p.z)),
                        p.count.0,
                        p.x_max_speed.to_bits(),
                        p.y_max_speed.to_bits(),
                        p.z_max_speed.to_bits(),
                        p.randomization as i32
                    ))
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn native_inside_callbacks_rng_and_deferred_freeze_match_real_server_level() {
    let mut cases = 0;
    for line in vectors()
        .lines()
        .filter(|line| line.starts_with("inside\t"))
    {
        let f: Vec<_> = line.split('\t').collect();
        let kind = f[1].parse::<u8>().unwrap();
        let in_snow = f[2] == "true";
        let fire = f[3].parse().unwrap();
        let moving = f[5] == "true";
        let seed = f[6].parse::<i64>().unwrap();
        let (mut level, entity) = fixture(kind, true);
        level.rules_mut().mob_griefing = f[4] == "true";
        let mut border = WorldBorder::default();
        border.set_size(if kind == 2 && seed == 1234 {
            16.0
        } else {
            59999968.0
        });
        level.set_entity_world_border(border);
        level
            .set_block_state(POS, Block::POWDER_SNOW.default_state())
            .unwrap();
        level
            .set_entity_pos(
                entity,
                Vec3::new(if in_snow { 8.5 } else { 10.5 }, 100.0, 8.5),
            )
            .unwrap();
        let motion = if moving {
            Vec3::new(0.5, 0.0, -0.25)
        } else {
            Vec3::ZERO
        };
        let base = level.entity_mut::<EntityData>(entity).unwrap();
        base.velocity = motion;
        base.remaining_fire_ticks = fire;
        base.set_ticks_frozen(if seed == 1 { 140 } else { 0 });
        base.inside_block_is_precise = seed != 0;
        base.inside_effect_collector.advance_step(0);
        if kind == 2 {
            level
                .entity_mut::<ServerPlayer>(entity)
                .unwrap()
                .known_movement = motion;
        }
        *level.random() = Random::Legacy(LegacyRandomSource::new(seed));
        let mut stuck = None;
        entity_inside_with_stuck(
            Block::POWDER_SNOW.default_state(),
            POS,
            entity,
            &mut level,
            |_, state, multiplier, _| {
                assert_eq!(state.block(), Block::POWDER_SNOW);
                stuck = Some(multiplier);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(counters(entity, &level), f[7], "queued: {line}");
        if seed == 1 {
            level
                .set_block_state(POS, Block::STONE.default_state())
                .unwrap();
        }
        let behavior = level.entity_behavior(entity).unwrap();
        inside_block::apply_collected(behavior, entity, &mut level).unwrap();
        assert_eq!(
            stuck.map_or_else(|| "-".into(), vector),
            f[8],
            "stuck: {line}"
        );
        assert_eq!(emissions(&mut level), f[9], "emissions: {line}");
        assert_eq!(
            level.get_block_state(POS).unwrap().id(),
            f[10].parse::<u16>().unwrap(),
            "captured position: {line}"
        );
        assert_eq!(counters(entity, &level), f[11], "applied: {line}");
        assert_eq!(
            level.random().next_long(),
            f[12].parse::<i64>().unwrap(),
            "RNG: {line}"
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        cases += 1;
    }
    assert_eq!(cases, 216);
}

#[test]
fn native_fall_on_uses_virtual_sounds_without_applying_fall_damage() {
    let mut cases = 0;
    for line in vectors().lines().filter(|line| line.starts_with("fall\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let (mut level, entity) = fixture(f[1].parse().unwrap(), true);
        level
            .entity_mut::<EntityData>(entity)
            .unwrap()
            .entity_data
            .set(synched_data::SILENT, f[3] == "true");
        let health = level
            .entity_state(entity)
            .unwrap()
            .as_living()
            .map_or(-1.0, |living| living.health())
            .to_bits();
        assert_eq!(health, u32::from_str_radix(f[4], 16).unwrap(), "{line}");
        level
            .call_fall_on(
                Block::POWDER_SNOW.default_state(),
                POS,
                entity,
                f64::from_bits(u64::from_str_radix(f[2], 16).unwrap()),
            )
            .unwrap();
        let health = level
            .entity_state(entity)
            .unwrap()
            .as_living()
            .map_or(-1.0, |living| living.health())
            .to_bits();
        assert_eq!(
            health,
            u32::from_str_radix(f[5], 16).unwrap(),
            "health: {line}"
        );
        assert_eq!(emissions(&mut level), f[6], "{line}");
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        cases += 1;
    }
    assert_eq!(cases, 54);
}

#[test]
fn freeze_precedes_required_live_extinguish_permission_and_errors_preserve_fire() {
    for policy in [UnportedBehavior::Strict, UnportedBehavior::Warn] {
        let (mut level, entity) = fixture(2, false);
        level.set_unported_behavior(policy);
        level
            .set_block_state(POS, Block::POWDER_SNOW.default_state())
            .unwrap();
        level
            .entity_mut::<EntityData>(entity)
            .unwrap()
            .remaining_fire_ticks = 40;
        entity_inside_with_stuck(
            Block::POWDER_SNOW.default_state(),
            POS,
            entity,
            &mut level,
            |_, _, _, _| Ok(()),
        )
        .unwrap();
        let behavior = level.entity_behavior(entity).unwrap();
        assert_eq!(
            inside_block::apply_collected(behavior, entity, &mut level),
            Err(LevelError::MissingSpawnProtectionContext)
        );
        assert_eq!(counters(entity, &level), "true,1,40");
        assert_eq!(
            level.get_block_state(POS).unwrap().block(),
            Block::POWDER_SNOW
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
    }
    for method in [BlockMethod::EntityInside, BlockMethod::FallOn] {
        assert!(super::super::ported_behavior(Block::POWDER_SNOW, method).is_some());
    }
}
