//! Geometry and flow vectors captured from vanilla's real Entity.move/getFlow.
use super::*;
#[cfg(test)]
use crate::level::game_event::test_support::{self as game_events, Observation};
use crate::{
    Behaviors,
    block::{UnportedBehavior, fluid::FlowingFluid},
    level::{Level, Region, chunk::LevelChunk},
};
use ferraloom_data::{
    Block, BlockState,
    datapack::{TagSet, Tags},
};
use ferraloom_util::{BlockPos, ChunkPos, Uuid, Vec3};
use std::sync::Arc;

const VECTORS: &str = include_str!("../../tests/java/entity/vectors.tsv");
static BEHAVIORS: Behaviors = Behaviors::EMPTY;
fn double(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).unwrap())
}
fn vector(value: &str) -> Vec3 {
    let values: Vec<_> = value.split(',').map(double).collect();
    Vec3::new(values[0], values[1], values[2])
}
fn assert_vector(actual: Vec3, expected: Vec3, label: &str) {
    assert_eq!(
        [actual.x.to_bits(), actual.y.to_bits(), actual.z.to_bits()],
        [
            expected.x.to_bits(),
            expected.y.to_bits(),
            expected.z.to_bits()
        ],
        "{label}: {actual:?} != {expected:?}"
    );
}

#[test]
fn no_collision_stops_at_first_shape_and_retains_later_required_reads() {
    let mut actual = Vec::new();
    for block in [Block::STONE, Block::AIR, Block::WATER] {
        for liquids in [false, true] {
            let mut level = level(false);
            level.remove_chunk(ChunkPos::new(0, 1)).unwrap();
            level
                .set_block_state(BlockPos::new(0, 64, 0), block.default_state())
                .unwrap();
            let result = match level.no_collision(
                None,
                ferraloom_util::Aabb::new(0.1, 64.1, 0.1, 0.9, 64.9, 16.2),
                liquids,
            ) {
                Ok(clear) => clear.to_string(),
                Err(crate::level::LevelError::MissingChunk(pos)) => {
                    assert_eq!(pos, ChunkPos::new(0, 1));
                    "required-read".to_owned()
                }
                Err(error) => panic!("unexpected collision query error: {error:?}"),
            };
            actual.push(format!(
                "no-collision-order\t{}\t{liquids}\t{result}",
                block.name()
            ));
        }
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|line| line.starts_with("no-collision-order\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn loaded_living_head_and_body_yaw_precede_base_yaw_wrapping() {
    use ferraloom_nbt::{CompoundTag, ListTag, Tag};
    let mut actual = Vec::new();
    for (index, rotation) in [721.25, -721.25, f32::NAN, 37.25].into_iter().enumerate() {
        let mut player = player::ServerPlayer::new(
            Uuid::new(1, 2),
            "rotation".into(),
            1,
            Vec3::ZERO,
            player::GameType::Survival,
        );
        player.set_y_rot(11.25);
        let mut tag = CompoundTag::new();
        tag.insert(
            "Rotation",
            Tag::List(ListTag(vec![Tag::Float(rotation), Tag::Float(0.0)])),
        );
        player.try_load(tag).unwrap();
        actual.push(format!(
            "living-load-rotation\t{index}\t{:x},{:x},{:x},{:x},{:x}",
            player.yaw.to_bits(),
            player.y_head_rot.to_bits(),
            player.y_body_rot.to_bits(),
            player.old_y_head_rot.to_bits(),
            player.old_y_body_rot.to_bits()
        ));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|line| line.starts_with("living-load-rotation\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn lava_fall_distance_starts_after_the_first_base_tick() {
    let mut level = level(false);
    level
        .set_block_state(BlockPos::new(8, 64, 8), Block::LAVA.default_state())
        .unwrap();
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let mut tnt = item::PrimedTnt::new(1, 123);
    tnt.parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
    tnt.parent.fall_distance = 8.0;
    level
        .add_entity(id, EntityState::PrimedTnt(Box::new(tnt)))
        .unwrap();
    let mut actual = Vec::new();
    for index in 0..3 {
        let before = level
            .entity::<EntityData>(id)
            .unwrap()
            .is_in_lava(&level)
            .unwrap();
        base_tick(&item::PrimedTntBehavior, id, &mut level).unwrap();
        let base = level.entity::<EntityData>(id).unwrap();
        actual.push(format!(
            "lava-base-tick\t{index}\t{before},{:x},{}",
            base.fall_distance.to_bits(),
            base.is_in_lava(&level).unwrap()
        ));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|line| line.starts_with("lava-base-tick\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn entity_sound_packets_rng_and_events_match_real_native_call_sites() {
    use crate::level::effects::LevelEffect;
    use ferraloom_protocol::{codec::Holder, play::clientbound::Packet};
    use ferraloom_util::random::RandomSource;
    fn bits(position: Vec3) -> String {
        format!(
            "{:x},{:x},{:x}",
            position.x.to_bits(),
            position.y.to_bits(),
            position.z.to_bits()
        )
    }
    let mut actual = Vec::new();
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("entity-sound\t"))
    {
        let name = row.split('\t').nth(1).unwrap();
        let mut level = level(false);
        game_events::observe(&mut level);
        let pos = BlockPos::new(8, 63, 8);
        level
            .set_block_state(pos, Block::STONE.default_state())
            .unwrap();
        level.set_sound_seed(321);
        let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let is_player = name.starts_with("player-")
            || name == "sprint-player-spectator"
            || name.starts_with("extinguish-player");
        let state = if is_player {
            EntityState::Player(Box::new(player::ServerPlayer::new(
                Uuid::new(1, 2),
                "sound".into(),
                1,
                Vec3::new(8.5, 64.0, 8.5),
                player::GameType::Survival,
            )))
        } else if name.starts_with("orb-") {
            EntityState::ExperienceOrb(Box::new(experience_orb::ExperienceOrb::new(1, 123)))
        } else if name == "item-swim" {
            EntityState::Item(Box::new(item::ItemEntity::new(1, 123)))
        } else if name == "controller-splash" {
            EntityState::Cow(Box::new(animal::Cow::with_parent(EntityData::new(
                ferraloom_data::EntityType::COW,
                1,
                123,
            ))))
        } else {
            EntityState::PrimedTnt(Box::new(item::PrimedTnt::new(1, 123)))
        };
        level.add_entity(id, state).unwrap();
        let observer = EntityRef::fixture(crate::level::RegionId::FIXTURE, 99);
        level
            .add_entity(
                observer,
                EntityState::Player(Box::new(player::ServerPlayer::new(
                    Uuid::new(9, 9),
                    "listener".into(),
                    99,
                    Vec3::new(8.5, 64.0, 8.5),
                    player::GameType::Survival,
                ))),
            )
            .unwrap();
        level.set_entity_pos(id, Vec3::new(8.5, 64.0, 8.5)).unwrap();
        {
            let base = level.entity_mut::<EntityData>(id).unwrap();
            base.velocity = Vec3::new(0.25, -0.5, 0.125);
            base.random.set_seed(123);
            base.entity_data
                .set(synched_data::SILENT, name.contains("silent"));
        }
        if name == "player-snow" {
            level
                .set_block_state(pos.above(1), Block::SNOW.default_state())
                .unwrap();
        }
        if name == "player-lichen" {
            level
                .set_block_state(pos.above(1), Block::GLOW_LICHEN.default_state())
                .unwrap();
        }
        if name == "player-water-step" {
            level
                .entity_mut::<EntityData>(id)
                .unwrap()
                .was_touching_water = true;
        }
        if name == "player-splash-high" {
            level.entity_mut::<EntityData>(id).unwrap().velocity = Vec3::new(2.0, -3.0, 1.0);
        }
        if name.ends_with("spectator") {
            level
                .entity_mut::<player::ServerPlayer>(id)
                .unwrap()
                .game_mode = player::GameType::Spectator;
        }
        if name == "controller-splash" {
            let mut rider =
                animal::Cow::with_parent(EntityData::new(ferraloom_data::EntityType::COW, 2, 124));
            rider.as_living_mut().velocity = Vec3::new(2.0, -3.0, 1.0);
            level
                .add_entity(
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
                    EntityState::Cow(Box::new(rider)),
                )
                .unwrap();
            start_riding(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
                id,
                true,
                &mut level,
            )
            .unwrap();
        }
        game_events::take(&mut level);
        let behavior = level.entity_behavior(id).unwrap();
        if name == "crystal" {
            level
                .set_block_state(pos, Block::AMETHYST_BLOCK.default_state())
                .unwrap();
            game_events::take(&mut level);
            for tick in [0, 20, 21, 40, 100] {
                level.entity_mut::<EntityData>(id).unwrap().tick_count = tick;
                sounds::walking_step_sound(
                    behavior,
                    id,
                    pos,
                    Block::AMETHYST_BLOCK.default_state(),
                    &mut level,
                )
                .unwrap();
            }
        } else if name == "player-move-step" {
            let base = level.entity_mut::<EntityData>(id).unwrap();
            base.move_dist = 2.0;
            base.on_ground = true;
            behavior
                .move_by(
                    id,
                    &mut level,
                    movement::MoverType::SelfMovement,
                    Vec3::new(0.4, -0.01, 0.0),
                )
                .unwrap();
        } else if name.contains("step") || matches!(name, "player-snow" | "player-lichen") {
            sounds::walking_step_sound(behavior, id, pos, Block::STONE.default_state(), &mut level)
                .unwrap();
        } else if name.ends_with("swim") {
            sounds::water_swim_sound(behavior, id, &mut level).unwrap();
        } else if name.contains("splash") {
            behavior.do_water_splash_effect(id, &mut level).unwrap();
        } else if name.starts_with("sprint-") {
            if name == "sprint-barrier" {
                level
                    .set_block_state(pos, Block::BARRIER.default_state())
                    .unwrap();
            }
            if name == "sprint-water" {
                level
                    .set_block_state(pos, Block::WATER.default_state())
                    .unwrap();
            }
            if name == "sprint-crouch" {
                dimensions::set_pose(
                    id,
                    ferraloom_protocol::entity_data::Pose::Crouching,
                    &mut level,
                )
                .unwrap();
            }
            level
                .entity_mut::<EntityData>(id)
                .unwrap()
                .set_shared_flag(3, true);
            game_events::take(&mut level);
            sounds::maybe_spawn_sprint_particle(behavior, id, &mut level).unwrap();
        } else if name.starts_with("extinguish") {
            behavior
                .play_fire_extinguished_sound(id, &mut level)
                .unwrap();
        } else {
            panic!("unknown native sound scenario {name}");
        }
        let random = level
            .entity_mut::<EntityData>(id)
            .unwrap()
            .random
            .next_int() as u32;
        let mut packets = Vec::new();
        let mut events = Vec::new();
        for effect in game_events::take(&mut level) {
            match effect {
                Observation::Effect(LevelEffect::PlayerPacket { recipients, packet }) => {
                    assert!(
                        recipients.contains(&observer),
                        "{name}: listener should be in range"
                    );
                    let Packet::Sound(packet) = *packet else {
                        panic!("unexpected particle or entity sound output in {name}");
                    };
                    let Holder::Reference(sound) = packet.sound.0 else {
                        panic!("registered sound");
                    };
                    let except = if is_player && !recipients.contains(&id) {
                        "1"
                    } else {
                        "none"
                    };
                    packets.push(format!(
                        "{except}:{sound}:{}:{},{},{}:{:x}:{:x}:{:x}",
                        packet.source as i32,
                        packet.x,
                        packet.y,
                        packet.z,
                        packet.volume.to_bits(),
                        packet.pitch.to_bits(),
                        packet.seed as u64
                    ));
                }
                Observation::GameEvent {
                    event,
                    position,
                    source,
                    affected_state,
                } => {
                    events.push(format!(
                        "{}:{}:{}:{}",
                        event.name(),
                        source.map_or_else(|| "none".into(), |id| id.serial().to_string()),
                        affected_state
                            .map_or_else(|| "none".into(), |state| state.id().to_string()),
                        bits(position)
                    ));
                }
                other => panic!("unexpected effect {other:?} in {name}"),
            }
        }
        actual.push(format!(
            "entity-sound\t{name}\t{}\t{random:x}\t{}",
            packets.join(";"),
            if events.is_empty() {
                "-".into()
            } else {
                events.join(";")
            }
        ));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|line| line.starts_with("entity-sound\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn living_movement_and_ignition_use_live_attributes_with_native_float_steps() {
    use ferraloom_data::registries::attribute::Id as Attribute;
    use ferraloom_protocol::codec::VarInt;
    let mut level = level(true);
    level
        .set_block_state(BlockPos::new(8, 63, 8), Block::SOUL_SAND.default_state())
        .unwrap();
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let player = player::ServerPlayer::new(
        Uuid::new(1, 2),
        "movement".into(),
        1,
        Vec3::new(8.5, 64.0, 8.5),
        player::GameType::Survival,
    );
    level
        .add_entity(id, EntityState::Player(Box::new(player)))
        .unwrap();
    let attributes = [
        Attribute::STEP_HEIGHT,
        Attribute::GRAVITY,
        Attribute::AIR_DRAG_MODIFIER,
        Attribute::BOUNCINESS,
        Attribute::MOVEMENT_EFFICIENCY,
        Attribute::BURNING_TIME,
    ];
    let values = [
        [0.6, 0.08, 1.0, 0.0, 0.0, 1.0],
        [1.25, 0.015, 2.0, 0.75, 0.5, 0.125],
        [0.0, -0.1, 0.0, 1.25, 1.0, 2.0],
        [0.6, 0.08, 100.0, 0.0, 0.25, 1024.0],
    ];
    let mut actual = Vec::new();
    for (index, values) in values.into_iter().enumerate() {
        for (attribute, value) in attributes.into_iter().zip(values) {
            level
                .entity_mut::<LivingEntity>(id)
                .unwrap()
                .attributes
                .get_instance(attribute)
                .unwrap()
                .set_base_value(value);
        }
        {
            let base = level.entity_mut::<EntityData>(id).unwrap();
            base.entity_data.set(synched_data::NO_GRAVITY, index == 2);
            base.remaining_fire_ticks = -20;
            base.entity_data
                .set(synched_data::TICKS_FROZEN, VarInt(100));
        }
        inside_block::ignite_for_ticks(id, if index == 3 { i32::MAX } else { 123 }, &mut level)
            .unwrap();
        let behavior = level.entity_behavior(id).unwrap();
        let base = level.entity::<EntityData>(id).unwrap();
        actual.push(format!(
            "living-movement\t{index}\t{:x},{:x},{:x},{:x},{:x},{},{}",
            behavior.max_up_step(id, &level).unwrap().to_bits(),
            behavior.effective_gravity(id, &level).unwrap().to_bits(),
            behavior.air_drag(id, &level).unwrap().to_bits(),
            behavior.bounciness(id, &level).unwrap().to_bits(),
            behavior.block_speed_factor(id, &level).unwrap().to_bits(),
            base.remaining_fire_ticks,
            base.ticks_frozen(),
        ));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|line| line.starts_with("living-movement\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn native_dimensions_pose_scales_and_attachment_rotations_keep_float_and_double_steps() {
    use dimensions::EntityAttachment;
    use ferraloom_protocol::entity_data::Pose;
    fn vector_bits(point: Vec3) -> String {
        format!(
            "{:x},{:x},{:x}",
            point.x.to_bits(),
            point.y.to_bits(),
            point.z.to_bits()
        )
    }
    fn row(name: &str, dimensions: &EntityDimensions) -> String {
        let mut row = format!(
            "dimensions\t{name}\t{:x},{:x},{:x},{}",
            dimensions.width.to_bits(),
            dimensions.height.to_bits(),
            dimensions.eye_height.to_bits(),
            dimensions.fixed
        );
        for attachment in [
            EntityAttachment::Passenger,
            EntityAttachment::Vehicle,
            EntityAttachment::NameTag,
            EntityAttachment::WardenChest,
        ] {
            let points = (0..dimensions.attachments.points(attachment).len())
                .map(|index| vector_bits(dimensions.attachments.get(attachment, index as i32, 0.0)))
                .collect::<Vec<_>>()
                .join(";");
            row.push_str(&format!(
                "\t{points}:{}",
                vector_bits(dimensions.attachments.get_average(attachment))
            ));
        }
        row
    }
    let cow = EntityDimensions::for_type(ferraloom_data::EntityType::COW);
    let ghast = EntityDimensions::for_type(ferraloom_data::EntityType::HAPPY_GHAST);
    let mut actual = vec![
        row("cow", &cow),
        row(
            "baby-cow",
            &cow.scale(0.5, 0.5).with_eye_height(0.665).scale(1.7, 1.7),
        ),
        row("happy-ghast", &ghast),
    ];
    let mut level = {
        let mut fixture_level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        fixture_level.install_shared(Box::default());
        fixture_level
    };
    let player = player::ServerPlayer::new(
        Uuid::new(1, 2),
        "dimensions".into(),
        1,
        Vec3::ZERO,
        player::GameType::Survival,
    );
    level
        .add_entity(
            EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
            EntityState::Player(Box::new(player)),
        )
        .unwrap();
    level
        .entity_mut::<LivingEntity>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1))
        .unwrap()
        .attributes
        .get_instance(ferraloom_data::registries::attribute::Id::SCALE)
        .unwrap()
        .set_base_value(1.7);
    for (index, pose) in [
        Pose::Standing,
        Pose::FallFlying,
        Pose::Sleeping,
        Pose::Swimming,
        Pose::SpinAttack,
        Pose::Crouching,
        Pose::LongJumping,
        Pose::Dying,
        Pose::Croaking,
        Pose::UsingTongue,
        Pose::Sitting,
        Pose::Roaring,
        Pose::Sniffing,
        Pose::Emerging,
        Pose::Digging,
        Pose::Sliding,
        Pose::Shooting,
        Pose::Inhaling,
    ]
    .into_iter()
    .enumerate()
    {
        let dimensions = player::PlayerBehavior
            .dimensions(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
                pose,
                &level,
            )
            .unwrap();
        actual.push(row(&format!("player-{index}"), &dimensions));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|line| line.starts_with("dimensions\t"))
        .collect();
    assert_eq!(actual, expected);
    let points = ghast.attachments.scale(0.7, 1.3, 0.7);
    let mut actual = Vec::new();
    for rotation in [-180.0_f32, -90.0, 0.0, 37.3, 90.0, 180.0] {
        for index in [-1, 0, 3, 5] {
            actual.push(format!(
                "attachment\t{:x}\t{index}\t{}",
                rotation.to_bits(),
                vector_bits(points.get_clamped(EntityAttachment::Passenger, index, rotation))
            ));
        }
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|line| line.starts_with("attachment\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn native_size_refresh_moves_nonplayers_out_of_blocks_and_preserves_player_positions() {
    fn check(label: &str, id: EntityRef, level: &Level) {
        let prefix = format!("size-refresh\t{label}\t");
        let expected: Vec<_> = VECTORS
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .unwrap()
            .split('\t')
            .collect();
        let base = level.entity::<EntityData>(id).unwrap();
        assert_vector(base.position, vector(expected[0]), label);
        assert_vector(
            Vec3::new(
                base.bounding_box.min_x,
                base.bounding_box.min_y,
                base.bounding_box.min_z,
            ),
            vector(expected[1]),
            label,
        );
        assert_vector(
            Vec3::new(
                base.bounding_box.max_x,
                base.bounding_box.max_y,
                base.bounding_box.max_z,
            ),
            vector(expected[2]),
            label,
        );
    }
    struct ResizeBehavior;
    impl Entity for ResizeBehavior {
        fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
            trace_base_entity_refs(state.as_entity(), references);
        }
        fn dimensions(
            &self,
            entity: EntityRef,
            _pose: ferraloom_protocol::entity_data::Pose,
            level: &Level,
        ) -> Result<EntityDimensions, crate::level::LevelError> {
            let saved = &level.entity::<EntityData>(entity)?.saved;
            Ok(EntityDimensions::scalable(
                saved.get_float_or("width", 0.5),
                saved.get_float_or("height", 0.5),
            ))
        }
    }
    let mut level = level(true);
    for y in 64..=66 {
        level
            .set_block_state(BlockPos::new(9, y, 8), Block::STONE.default_state())
            .unwrap();
    }
    let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let mut tnt = item::PrimedTnt::new(1, 4);
    tnt.parent.set_pos(Vec3::new(8.7, 64.0, 8.5));
    level
        .add_entity(entity, EntityState::PrimedTnt(Box::new(tnt)))
        .unwrap();
    dimensions::refresh_dimensions(&ResizeBehavior, entity, &mut level).unwrap();
    let base = level.entity_mut::<EntityData>(entity).unwrap();
    base.first_tick = false;
    base.saved.insert("width", ferraloom_nbt::Tag::Float(1.0));
    base.saved.insert("height", ferraloom_nbt::Tag::Float(2.0));
    dimensions::refresh_dimensions(&ResizeBehavior, entity, &mut level).unwrap();
    check("growth", entity, &level);
    let base = level.entity_mut::<EntityData>(entity).unwrap();
    base.saved.insert("width", ferraloom_nbt::Tag::Float(0.5));
    base.saved.insert("height", ferraloom_nbt::Tag::Float(0.5));
    dimensions::refresh_dimensions(&ResizeBehavior, entity, &mut level).unwrap();
    level
        .set_entity_pos(entity, Vec3::new(8.7, 64.0, 8.5))
        .unwrap();
    for x in 5..=12 {
        for z in 5..=12 {
            level
                .set_block_state(BlockPos::new(x, 65, z), Block::STONE.default_state())
                .unwrap();
        }
    }
    let base = level.entity_mut::<EntityData>(entity).unwrap();
    base.saved.insert("width", ferraloom_nbt::Tag::Float(1.0));
    base.saved.insert("height", ferraloom_nbt::Tag::Float(2.0));
    dimensions::refresh_dimensions(&ResizeBehavior, entity, &mut level).unwrap();
    check("growth-low-ceiling", entity, &level);
    let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 2);
    let mut state = player::ServerPlayer::new(
        Uuid::new(1, 2),
        "resize".into(),
        2,
        Vec3::new(8.7, 64.0, 8.5),
        player::GameType::Survival,
    );
    state.first_tick = false;
    state
        .attributes
        .get_instance(ferraloom_data::registries::attribute::Id::SCALE)
        .unwrap()
        .set_base_value(1.7);
    level
        .add_entity(player, EntityState::Player(Box::new(state)))
        .unwrap();
    dimensions::set_pose(
        player,
        ferraloom_protocol::entity_data::Pose::Crouching,
        &mut level,
    )
    .unwrap();
    dimensions::set_pose(
        player,
        ferraloom_protocol::entity_data::Pose::Standing,
        &mut level,
    )
    .unwrap();
    check("player-growth", player, &level);
}
pub(super) fn tags(level: &mut Level) {
    level.set_item_tags(level.context().generator.access().data().item_tags.clone());
    let entity_values = VECTORS
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            if fields.next()? != "entity-tag" {
                return None;
            }
            let name = fields.next()?.parse().unwrap();
            let values = fields
                .next()?
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| ferraloom_data::EntityType::from_name(s).unwrap())
                .collect();
            Some((name, values))
        })
        .collect();
    level.set_entity_type_tags(Arc::new(
        TagSet::from_tags(
            Tags {
                values: entity_values,
                diagnostics: Vec::new(),
            },
            ferraloom_data::ENTITY_TYPES.len(),
        )
        .unwrap(),
    ));

    let block_values = VECTORS
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            if fields.next()? != "block-tag" {
                return None;
            }
            let name = fields.next()?.parse().unwrap();
            let values = fields
                .next()?
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| Block::from_name(s).unwrap())
                .collect();
            Some((name, values))
        })
        .collect();
    level.set_block_tags(Arc::new(
        TagSet::from_tags(
            Tags {
                values: block_values,
                diagnostics: Vec::new(),
            },
            ferraloom_data::BLOCKS.len(),
        )
        .unwrap(),
    ));
    let fluid_values = VECTORS
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            if fields.next()? != "fluid-tag" {
                return None;
            }
            let name = fields.next()?.parse().unwrap();
            let values = fields
                .next()?
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| ferraloom_data::registries::fluid::Id::from_name(s).unwrap())
                .collect();
            Some((name, values))
        })
        .collect();
    level.set_fluid_tags(Arc::new(
        TagSet::from_tags(
            Tags {
                values: fluid_values,
                diagnostics: Vec::new(),
            },
            ferraloom_data::registries::fluid::ALL.len(),
        )
        .unwrap(),
    ));
}
pub(crate) fn level(floor: bool) -> Level {
    level_with_behaviors(floor, &BEHAVIORS)
}
/// Cold entity-NBT tests use explicit world services without loaded terrain.
pub(crate) fn persistence_level() -> Level {
    let mut level = Level::new(
        Region::new(&BEHAVIORS),
        crate::level::LevelContext::fixture(0, 63),
    );
    level.install_shared(Box::default());
    install_test_environment(&mut level, false, "minecraft:overworld");
    level
}
pub(crate) fn install_test_environment(level: &mut Level, fast_lava: bool, dimension_key: &str) {
    use crate::level::environment::{ClockSnapshot, EnvironmentDefinitions};
    let resources = ferraloom_data::datapack::ResourceManager::new(Vec::new());
    let definitions = Arc::new(EnvironmentDefinitions::load(&resources).unwrap());
    let dimension = definitions
        .dimension(&serde_json::json!({
            "attributes": {"gameplay/fast_lava": fast_lava}, "timelines": [],
        }))
        .unwrap();
    level
        .set_environment_attributes(
            definitions,
            &dimension,
            dimension_key.parse().unwrap(),
            &[],
            ClockSnapshot::default(),
        )
        .unwrap();
}

fn level_with_behaviors(floor: bool, behaviors: &'static Behaviors) -> Level {
    use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
    let template = LevelChunkSection::new(
        Strategy::block_states(),
        Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
        BiomeId(0),
    );
    let mut level = {
        let mut fixture_level = Level::new(
            Region::new(behaviors),
            crate::level::LevelContext::fixture(0, 63),
        );
        fixture_level.install_shared(Box::default());
        fixture_level
    };
    // Explicit controlled dimension with native constants and no timelines.
    install_test_environment(&mut level, false, "test");
    for x in -2..=2 {
        for z in -2..=2 {
            level
                .add_chunk(LevelChunk::new(
                    ChunkPos::new(x, z),
                    -4..20,
                    template.clone(),
                ))
                .unwrap();
            level
                .update_entity_chunk_status(
                    ChunkPos::new(x, z),
                    crate::level::entity::Visibility::Ticking,
                )
                .unwrap();
        }
    }
    tags(&mut level);
    // These vectors cover Entity geometry. Unsupported block side effects remain
    // counted at their hooks; no skipped shape query can fabricate a collision.
    level.set_unported_behavior(UnportedBehavior::Warn);
    if floor {
        for x in -4..24 {
            for z in -4..24 {
                level
                    .set_block_state(BlockPos::new(x, 63, z), Block::STONE.default_state())
                    .unwrap();
            }
        }
    }
    level
}
fn flags(base: &EntityData) -> String {
    [
        base.on_ground,
        base.horizontal_collision,
        base.vertical_collision,
        base.vertical_collision_below,
        base.minor_horizontal_collision,
    ]
    .map(|v| if v { '1' } else { '0' })
    .iter()
    .collect()
}

#[test]
fn vanilla_entity_move_positions_flags_fall_distance_and_velocity_bits() {
    let mut count = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("move\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = level(true);
        let obstacle: i32 = f[3].parse().unwrap();
        if obstacle >= 0 {
            level
                .set_block_state(
                    BlockPos::new(4, 64, 4),
                    BlockState::from_id(obstacle as u16).unwrap(),
                )
                .unwrap();
        }
        let mut id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let position = vector(f[4]);
        let state = if f[2] == "player" {
            EntityState::Player(Box::new(player::ServerPlayer::new(
                Uuid::new(1, 2),
                "fixture".into(),
                1,
                position,
                player::GameType::Survival,
            )))
        } else {
            let mut cow = animal::Cow::default();
            cow.as_living_mut().parent.set_pos(position);
            EntityState::Cow(Box::new(cow))
        };
        level.add_entity(id, state).unwrap();
        movement::set_on_ground(id, f[6] == "true", None, &mut level).unwrap();
        let delta = vector(f[5]);
        {
            let base = level.entity_mut::<EntityData>(id).unwrap();
            base.set_shared_flag(1, f[7] == "true");
            base.fall_distance = double(f[8]);
            base.velocity = delta;
        }
        id.move_by(&mut level, movement::MoverType::SelfMovement, delta)
            .unwrap();
        let base = level.entity::<EntityData>(id).unwrap();
        assert_vector(base.position, vector(f[9]), f[1]);
        assert_eq!(flags(base), f[10], "{}", f[1]);
        assert_eq!(
            base.fall_distance.to_bits(),
            double(f[11]).to_bits(),
            "{} fall distance",
            f[1]
        );
        assert_vector(base.velocity, vector(f[12]), f[1]);
        count += 1;
    }
    assert_eq!(count, 56);
}
#[test]
fn vanilla_player_sneak_edge_backoff_bits() {
    let row = VECTORS.lines().find(|s| s.starts_with("edge\t")).unwrap();
    let f: Vec<_> = row.split('\t').collect();
    let mut level = level(false);
    level
        .set_block_state(BlockPos::new(3, 63, 4), Block::STONE.default_state())
        .unwrap();
    let mut id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    level
        .add_entity(
            id,
            EntityState::Player(Box::new(player::ServerPlayer::new(
                Uuid::new(1, 2),
                "fixture".into(),
                1,
                Vec3::new(3.5, 64.0, 4.5),
                player::GameType::Survival,
            ))),
        )
        .unwrap();
    movement::set_on_ground(id, true, None, &mut level).unwrap();
    let delta = Vec3::new(1.1, 0.0, 0.7);
    {
        let base = level.entity_mut::<EntityData>(id).unwrap();
        base.set_shared_flag(1, true);
        base.velocity = delta;
    }
    id.move_by(&mut level, movement::MoverType::SelfMovement, delta)
        .unwrap();
    let base = level.entity::<EntityData>(id).unwrap();
    assert_vector(base.position, vector(f[1]), "sneak edge");
    assert_eq!(flags(base), f[2]);
    assert_vector(base.velocity, vector(f[3]), "sneak velocity");
}
#[test]
fn vanilla_fluid_current_vectors_preserve_float_height_arithmetic() {
    let mut count = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("flow\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = level(false);
        let kind = if f[1] == "minecraft:water" {
            FlowingFluid::Water
        } else {
            FlowingFluid::Lava
        };
        let case: i32 = f[2].parse().unwrap();
        let pos = BlockPos::new(8, 65, 8);
        let fluid = kind.source();
        let mut fluid = fluid;
        fluid.falling = case == 4;
        level
            .set_block_state(pos, crate::block::fluid::legacy_block(fluid))
            .unwrap();
        match case {
            0 => {
                level
                    .set_block_state(
                        pos.east(1),
                        crate::block::fluid::legacy_block(kind.flowing(4, false)),
                    )
                    .unwrap();
            }
            1 => {
                level
                    .set_block_state(
                        pos.west(1).below(1),
                        crate::block::fluid::legacy_block(kind.source()),
                    )
                    .unwrap();
            }
            2 => {
                level
                    .set_block_state(
                        pos.south(1),
                        crate::block::fluid::legacy_block(kind.flowing(2, false)),
                    )
                    .unwrap();
            }
            3 => {
                let other = if kind == FlowingFluid::Water {
                    FlowingFluid::Lava
                } else {
                    FlowingFluid::Water
                };
                level
                    .set_block_state(
                        pos.north(1),
                        crate::block::fluid::legacy_block(other.source()),
                    )
                    .unwrap();
            }
            4 => {
                level
                    .set_block_state(pos.east(1), Block::STONE.default_state())
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert_vector(
            kind.get_flow(fluid, &level, pos).unwrap(),
            vector(f[3]),
            row,
        );
        count += 1;
    }
    assert_eq!(count, 10);
}
#[test]
fn piston_budget_resets_per_tick_and_uses_first_nonzero_axis() {
    let mut base = EntityData::default();
    assert_eq!(
        movement::limit_piston_movement(&mut base, Vec3::new(0.4, 0.3, 0.2), 1),
        Vec3::new(0.4, 0.0, 0.0)
    );
    assert_eq!(
        movement::limit_piston_movement(&mut base, Vec3::new(0.4, 0.0, 0.0), 1)
            .x
            .to_bits(),
        (0.51_f64 - 0.4).to_bits()
    );
    assert_eq!(
        movement::limit_piston_movement(&mut base, Vec3::new(0.4, 0.0, 0.0), 1),
        Vec3::ZERO
    );
    assert_eq!(
        movement::limit_piston_movement(&mut base, Vec3::new(0.4, 0.0, 0.0), 2),
        Vec3::new(0.4, 0.0, 0.0)
    );
}

#[test]
fn vanilla_fluid_interaction_heights_eyes_and_current_bits() {
    use ferraloom_data::tags::FluidTags;
    let mut count = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("interaction\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = level(false);
        let kind = if f[1] == "minecraft:water" {
            FlowingFluid::Water
        } else {
            FlowingFluid::Lava
        };
        let case: i32 = f[2].parse().unwrap();
        let pos = BlockPos::new(8, 64, 8);
        level
            .set_block_state(pos, crate::block::fluid::legacy_block(kind.source()))
            .unwrap();
        if case == 1 {
            level
                .set_block_state(
                    pos.above(1),
                    crate::block::fluid::legacy_block(kind.source()),
                )
                .unwrap();
        }
        if case == 2 {
            level
                .set_block_state(
                    pos.east(1),
                    crate::block::fluid::legacy_block(kind.flowing(4, false)),
                )
                .unwrap();
        }
        if case == 4 {
            level
                .set_block_state(pos.above(1), Block::STONE.default_state())
                .unwrap();
        }
        let mut cow = animal::Cow::default();
        cow.as_living_mut().parent.set_pos(vector(f[3]));
        cow.as_living_mut().parent.fall_distance = 12.5;
        let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        level
            .add_entity(id, EntityState::Cow(Box::new(cow)))
            .unwrap();
        let any = fluid::update_fluid_interaction(&animal::CowBehavior, id, &mut level).unwrap();
        let base = level.entity::<EntityData>(id).unwrap();
        assert_eq!(any, f[4] == "true", "{row}");
        assert_vector(base.velocity, vector(f[5]), row);
        for (tag, height, eyes) in [(FluidTags::WATER, 6, 8), (FluidTags::LAVA, 7, 9)] {
            assert_eq!(
                base.fluid_interaction
                    .height(tag, &level)
                    .unwrap()
                    .to_bits(),
                double(f[height]).to_bits(),
                "{row}"
            );
            assert_eq!(
                base.fluid_interaction.eyes_inside(tag, &level).unwrap(),
                f[eyes] == "true",
                "{row}"
            );
        }
        assert_eq!(
            base.fall_distance.to_bits(),
            double(f[10]).to_bits(),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 10);
}

#[test]
fn vanilla_movement_path_block_contact_order_and_step_indices() {
    let mut count = 0;
    let dimensions = EntityDimensions::for_type(ferraloom_data::EntityType::COW);
    for row in VECTORS.lines().filter(|s| s.starts_with("trace\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let from = vector(f[2]);
        let to = vector(f[3]);
        let epsilon = f64::from(1e-5_f32);
        let bb = dimensions
            .make_bounding_box(to)
            .deflate(epsilon, epsilon, epsilon);
        let mut contacts = Vec::new();
        trace::for_each_intersected_block(from, to, bb, |pos, step| {
            contacts.push(format!("{},{},{},{}", pos.x, pos.y, pos.z, step));
            Ok(true)
        })
        .unwrap();
        assert_eq!(contacts.join(";"), f[4], "trace {}", f[1]);
        count += 1;
    }
    assert_eq!(count, 14);
}

#[test]
fn vanilla_primed_tnt_tick_motion_fuse_and_removal_bits() {
    let mut level = level(true);
    let mut tnt = item::PrimedTnt::new(level.next_entity_id().unwrap(), 123);
    tnt.parent.set_pos(Vec3::new(8.5, 66.0, 8.5));
    tnt.parent.velocity = Vec3::new(0.125, 0.2, -0.03);
    tnt.set_fuse(4);
    let mut entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    level
        .add_entity(entity, EntityState::PrimedTnt(Box::new(tnt)))
        .unwrap();
    let mut n = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("tnt\t")) {
        let f: Vec<_> = row.split('\t').collect();
        common_tick(entity, &mut level).unwrap();
        entity.tick(&mut level).unwrap();
        let tnt = level.entity::<item::PrimedTnt>(entity).unwrap();
        assert_vector(tnt.parent.position, vector(f[2]), row);
        assert_vector(tnt.parent.velocity, vector(f[3]), row);
        assert_eq!(flags(&tnt.parent), f[4], "{row}");
        assert_eq!(tnt.fuse(), f[5].parse::<i32>().unwrap());
        assert_eq!(tnt.parent.removed, f[6] == "true");
        assert_eq!(tnt.parent.requires_precise_position, f[7] == "true");
        n += 1;
    }
    assert_eq!(n, 4);
    let effects = level.take_entity_effects();
    assert!(matches!(
        effects.as_slice(),
        [effects::EntityEffect::Explosion { power: 4.0, .. }]
    ));
}

#[test]
fn vanilla_item_ticks_preserve_friction_gravity_flags_and_pickup_delay() {
    use ferraloom_protocol::component::ItemStack;
    let mut level = level(true);
    let mut item = item::ItemEntity::new(2, 123);
    item.set_item(ItemStack::new(
        ferraloom_data::Item::STONE,
        64,
        Default::default(),
    ));
    item.parent.set_pos(Vec3::new(8.5, 64.7, 8.5));
    item.parent.velocity = Vec3::new(0.15, 0.0, 0.03);
    item.pickup_delay = 7;
    let mut entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    level
        .add_entity(entity, EntityState::Item(Box::new(item)))
        .unwrap();
    let mut n = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("item-tick\t")) {
        let f: Vec<_> = row.split('\t').collect();
        common_tick(entity, &mut level).unwrap();
        entity.tick(&mut level).unwrap();
        let item = level.entity::<item::ItemEntity>(entity).unwrap();
        assert_vector(item.parent.position, vector(f[2]), row);
        assert_vector(item.parent.velocity, vector(f[3]), row);
        assert_eq!(flags(&item.parent), f[4], "{row}");
        assert_eq!(item.age, f[5].parse::<i32>().unwrap(), "{row}");
        assert_eq!(item.pickup_delay > 0, f[6] == "true", "{row}");
        assert_eq!(
            item.parent.fall_distance.to_bits(),
            double(f[7]).to_bits(),
            "{row}"
        );
        n += 1;
    }
    assert_eq!(n, 12);
}

#[test]
fn item_pickup_uses_mutable_instabuild_when_inventory_is_full() {
    use ferraloom_data::Item;
    use ferraloom_protocol::component::ItemStack;
    for mode in [player::GameType::Survival, player::GameType::Creative] {
        for instabuild in [false, true] {
            let mut level = level(false);
            let mut player =
                player::ServerPlayer::new(Uuid::new(1, 2), "pickup".into(), 1, Vec3::ZERO, mode);
            player.abilities.instabuild = instabuild;
            player
                .inventory
                .items
                .fill_with(|| ItemStack::new(Item::STONE, 64, Default::default()));
            level
                .add_entity(
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
                    EntityState::Player(Box::new(player)),
                )
                .unwrap();
            let mut item = item::ItemEntity::new(2, 2);
            item.set_item(ItemStack::new(Item::STONE, 2, Default::default()));
            level
                .add_entity(
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
                    EntityState::Item(Box::new(item)),
                )
                .unwrap();
            item::item_entity::player_touch(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
                &mut level,
            )
            .unwrap();
            assert_eq!(
                level
                    .entity::<EntityData>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 2))
                    .unwrap()
                    .removed,
                instabuild
            );
            assert_eq!(level.take_entity_effects().is_empty(), !instabuild);
        }
    }
}

#[test]
fn item_merge_equal_counts_choose_other_and_partial_pickup_preserves_native_return() {
    use ferraloom_protocol::component::ItemStack;
    use player::inventory::{count, set_count};
    let mut level = level(false);
    for n in 1..=3 {
        let mut item = item::ItemEntity::new(n as i32, n);
        item.parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
        item.set_item(ItemStack::new(
            ferraloom_data::Item::STONE,
            10,
            Default::default(),
        ));
        level
            .add_entity(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, n as u64),
                EntityState::Item(Box::new(item)),
            )
            .unwrap();
    }
    item::item_entity::merge_with_neighbours(
        EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
        &mut level,
    )
    .unwrap();
    assert!(
        level
            .entity::<EntityData>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1))
            .unwrap()
            .removed
    );
    assert_eq!(
        count(
            level
                .entity::<item::ItemEntity>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 2))
                .unwrap()
                .item()
        ),
        20
    );
    assert_eq!(
        count(
            level
                .entity::<item::ItemEntity>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 3))
                .unwrap()
                .item()
        ),
        10
    );
    let player_id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 4);
    let mut player = player::ServerPlayer::new(
        Uuid::new(1, 2),
        "fixture".into(),
        4,
        Vec3::ZERO,
        player::GameType::Survival,
    );
    player
        .inventory
        .items
        .fill_with(|| ItemStack::new(ferraloom_data::Item::STONE, 64, Default::default()));
    set_count(&mut player.inventory.items[0], 63);
    level
        .add_entity(player_id, EntityState::Player(Box::new(player)))
        .unwrap();
    let pickup = EntityRef::fixture(crate::level::RegionId::FIXTURE, 3);
    set_count(
        level
            .entity_mut::<item::ItemEntity>(pickup)
            .unwrap()
            .item_mut(),
        2,
    );
    item::item_entity::player_touch(pickup, player_id, &mut level).unwrap();
    assert_eq!(
        count(level.entity::<item::ItemEntity>(pickup).unwrap().item()),
        1
    );
    assert!(level.take_entity_effects().is_empty());
    level
        .entity_mut::<player::ServerPlayer>(player_id)
        .unwrap()
        .inventory
        .items[1] = ItemStack::default();
    item::item_entity::player_touch(pickup, player_id, &mut level).unwrap();
    let item = level.entity::<item::ItemEntity>(pickup).unwrap();
    assert!(item.parent.removed);
    assert_eq!(count(item.item()), 1);
    assert!(matches!(
        level.take_entity_effects().first(),
        Some(effects::EntityEffect::ItemPickup { count: 1, .. })
    ));
}

#[test]
fn pick_result_preserves_null_and_cow_spawn_egg() {
    use ferraloom_protocol::component::ItemStack;
    let mut level = level(false);
    let cow = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    level
        .add_entity(cow, EntityState::Cow(Box::default()))
        .unwrap();
    let result = level
        .entity_behavior(cow)
        .unwrap()
        .get_pick_result(cow, &level)
        .unwrap();
    assert_eq!(
        result,
        Some(ItemStack::new(
            ferraloom_data::Item::COW_SPAWN_EGG,
            1,
            Default::default()
        ))
    );
    let item = EntityRef::fixture(crate::level::RegionId::FIXTURE, 2);
    level
        .add_entity(
            item,
            EntityState::Item(Box::new(item::ItemEntity::new(2, 123))),
        )
        .unwrap();
    assert_eq!(
        level
            .entity_behavior(item)
            .unwrap()
            .get_pick_result(item, &level)
            .unwrap(),
        None
    );
}

#[test]
fn vanilla_player_dropped_item_position_and_velocity_bits() {
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::random::RandomSource;
    let mut n = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("drop\t")) {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = level(false);
        if level.shared().is_err() {
            level.install_shared(Box::default());
        }
        let player_id = level.next_entity_ref();
        let mut player = player::ServerPlayer::new(
            Uuid::new(1, 2),
            "fixture".into(),
            level.next_entity_id().unwrap(),
            Vec3::new(8.5, 64.0, 8.5),
            player::GameType::Survival,
        );
        player.set_y_rot(f[1].parse().unwrap());
        player.set_x_rot(f[2].parse().unwrap());
        player.random.set_seed(123);
        level
            .add_entity(player_id, EntityState::Player(Box::new(player)))
            .unwrap();
        let id = player::drop_item(
            player_id,
            ItemStack::new(ferraloom_data::Item::STONE, 3, Default::default()),
            true,
            &mut level,
        )
        .unwrap()
        .unwrap();
        let dropped = level.entity::<item::ItemEntity>(id).unwrap();
        assert_vector(dropped.parent.position, vector(f[3]), row);
        assert_vector(dropped.parent.velocity, vector(f[4]), row);
        assert_eq!(dropped.parent.network_id, 2);
        assert_eq!(dropped.pickup_delay, 40);
        assert_eq!(
            dropped.thrower.unwrap().uuid(&level).unwrap(),
            Uuid::new(1, 2)
        );
        n += 1;
    }
    assert_eq!(n, 3);
}

#[test]
fn vanilla_falling_block_airborne_motion_flags_and_fall_distance_bits() {
    let mut level = level(true);
    let mut falling = item::FallingBlockEntity::new(3, 123);
    falling.parent.set_pos(Vec3::new(8.5, 68.0, 8.5));
    let mut id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    level
        .add_entity(id, EntityState::FallingBlock(Box::new(falling)))
        .unwrap();
    let mut n = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("falling-tick\t")) {
        let f: Vec<_> = row.split('\t').collect();
        common_tick(id, &mut level).unwrap();
        id.tick(&mut level).unwrap();
        let falling = level.entity::<item::FallingBlockEntity>(id).unwrap();
        assert_vector(falling.parent.position, vector(f[2]), row);
        assert_vector(falling.parent.velocity, vector(f[3]), row);
        assert_eq!(flags(&falling.parent), f[4], "{row}");
        assert_eq!(falling.time, f[5].parse::<i32>().unwrap());
        assert_eq!(
            falling.parent.fall_distance.to_bits(),
            double(f[6]).to_bits(),
            "{row}"
        );
        n += 1;
    }
    assert_eq!(n, 4);
}

#[test]
fn falling_sand_is_constructed_before_source_replacement_then_lands() {
    struct OrderingProbe;
    impl crate::block::BlockBehavior for OrderingProbe {
        fn affect_neighbors_after_removal(
            &self,
            _state: BlockState,
            pos: BlockPos,
            _moved: bool,
            level: &mut Level,
        ) -> Result<(), crate::level::LevelError> {
            if pos.y == 66 {
                let tnt = item::PrimedTnt::new(level.next_entity_id().unwrap(), 123);
                let id = level.next_entity_ref();
                level.add_entity(id, EntityState::PrimedTnt(Box::new(tnt)))?;
            }
            Ok(())
        }
    }
    static ORDERING: Behaviors = Behaviors::EMPTY.with_block(Block::SAND, &OrderingProbe);
    let mut level = level_with_behaviors(true, &ORDERING);
    let source = BlockPos::new(8, 66, 8);
    level
        .set_block_state(source, Block::SAND.default_state())
        .unwrap();
    let mut id =
        item::falling_block::fall(&mut level, source, Block::SAND.default_state()).unwrap();
    assert!(level.get_block_state(source).unwrap().is_air());
    assert_eq!(
        level
            .entity::<item::FallingBlockEntity>(id)
            .unwrap()
            .parent
            .network_id,
        1
    );
    let probe = level
        .entity_by_network_id(2)
        .expect("neighbor callback allocated second");
    assert_eq!(level.all_entities(), [probe, id]);
    assert_eq!(
        level
            .entity::<item::FallingBlockEntity>(id)
            .unwrap()
            .start_pos(),
        source
    );
    for _ in 0..30 {
        if level.entity::<EntityData>(id).unwrap().removed {
            break;
        }
        common_tick(id, &mut level).unwrap();
        id.tick(&mut level).unwrap();
    }
    assert!(level.entity::<EntityData>(id).unwrap().removed);
    assert_eq!(
        level.get_block_state(BlockPos::new(8, 64, 8)).unwrap(),
        Block::SAND.default_state()
    );
    assert!(matches!(
        level.take_entity_effects().last(),
        Some(effects::EntityEffect::FallingBlockLanded {
            pos: BlockPos { x: 8, y: 64, z: 8 },
            ..
        })
    ));
}

#[test]
fn vanilla_experience_orb_motion_friction_and_bounce_bits() {
    use experience_orb::ExperienceOrb;
    let mut level = level(true);
    let mut orb = ExperienceOrb::new(4, 123);
    orb.parent.set_pos(Vec3::new(8.5, 64.7, 8.5));
    orb.parent.velocity = Vec3::new(0.1, 0.0, 0.01);
    let mut id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    level
        .add_entity(id, EntityState::ExperienceOrb(Box::new(orb)))
        .unwrap();
    let mut n = 0;
    for row in VECTORS.lines().filter(|s| s.starts_with("orb-tick\t")) {
        let f: Vec<_> = row.split('\t').collect();
        common_tick(id, &mut level).unwrap();
        id.tick(&mut level).unwrap();
        let orb = level.entity::<ExperienceOrb>(id).unwrap();
        assert_vector(orb.parent.position, vector(f[2]), row);
        assert_vector(orb.parent.velocity, vector(f[3]), row);
        assert_eq!(flags(&orb.parent), f[4], "{row}");
        assert_eq!(
            orb.parent.fall_distance.to_bits(),
            double(f[5]).to_bits(),
            "{row}"
        );
        n += 1;
    }
    assert_eq!(n, 12);
}

#[test]
fn orb_retains_disconnected_player_until_its_native_follow_cache_clears() {
    use experience_orb::{ExperienceOrb, follow_nearby_player};
    let mut level = level(false);
    let player_id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let uuid = Uuid::new(1, 2);
    let player = player::ServerPlayer::new(
        uuid,
        "fixture".into(),
        1,
        Vec3::new(4.5, 64.0, 1.5),
        player::GameType::Survival,
    );
    level
        .add_entity(player_id, EntityState::Player(Box::new(player)))
        .unwrap();
    let mut orb = ExperienceOrb::new(2, 123);
    orb.parent.set_pos(Vec3::new(1.5, 64.0, 1.5));
    let orb_id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 2);
    level
        .add_entity(orb_id, EntityState::ExperienceOrb(Box::new(orb)))
        .unwrap();
    follow_nearby_player(orb_id, &mut level).unwrap();
    assert_eq!(
        level
            .entity::<ExperienceOrb>(orb_id)
            .unwrap()
            .following_player,
        Some(player_id)
    );
    level
        .entity_mut::<ExperienceOrb>(orb_id)
        .unwrap()
        .parent
        .velocity = Vec3::ZERO;
    level.remove_entity(player_id).unwrap();
    level.finish_entity_management();
    assert_eq!(level.entity_by_uuid(uuid), None);
    assert!(
        level
            .entity::<player::ServerPlayer>(player_id)
            .unwrap()
            .removed
    );
    follow_nearby_player(orb_id, &mut level).unwrap();
    let row = VECTORS
        .lines()
        .find(|s| s.starts_with("orb-removed-follow\t"))
        .unwrap();
    let f: Vec<_> = row.split('\t').collect();
    assert_vector(
        level
            .entity::<ExperienceOrb>(orb_id)
            .unwrap()
            .parent
            .velocity,
        vector(f[1]),
        row,
    );
    assert_eq!(f[2], "true");
    level
        .set_entity_pos(orb_id, Vec3::new(30.0, 64.0, 1.5))
        .unwrap();
    follow_nearby_player(orb_id, &mut level).unwrap();
    assert_eq!(
        level
            .entity::<ExperienceOrb>(orb_id)
            .unwrap()
            .following_player,
        None
    );
    level.finish_entity_management();
    assert!(level.entity_state(player_id).is_err());
}

#[test]
fn orb_merges_only_matching_value_and_modulo_forty_group() {
    use experience_orb::ExperienceOrb;
    let mut level = level(false);
    for (n, network, age, count) in [(1, 1, 30, 1), (2, 41, 5, 2), (3, 2, 10, 3)] {
        let mut orb = ExperienceOrb::new(network, n);
        orb.set_value(7);
        orb.age = age;
        orb.count = count;
        orb.parent.set_pos(Vec3::new(8.5, 65.0, 8.5));
        level
            .add_entity(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, n as u64),
                EntityState::ExperienceOrb(Box::new(orb)),
            )
            .unwrap();
    }
    let mut id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    common_tick(id, &mut level).unwrap();
    id.tick(&mut level).unwrap();
    let orb = level.entity::<ExperienceOrb>(id).unwrap();
    assert_eq!(orb.count, 3);
    assert_eq!(orb.age, 6);
    assert!(
        level
            .entity::<ExperienceOrb>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 2))
            .unwrap()
            .parent
            .removed
    );
    assert_eq!(
        level
            .entity::<ExperienceOrb>(EntityRef::fixture(crate::level::RegionId::FIXTURE, 3))
            .unwrap()
            .count,
        3
    );
}

#[test]
fn relative_teleports_match_native_transform_state_and_packet_flags() {
    use super::teleport::{calculate_absolute, position_move_rotation, teleport_player};
    use crate::level::effects::LevelEffect;
    use ferraloom_protocol::play::clientbound as cb;
    let source = position_move_rotation(
        Vec3::new(8.5, 64.25, -12.75),
        Vec3::new(0.25, -0.125, 0.5),
        721.25,
        -28.125,
    );
    let change = position_move_rotation(
        Vec3::new(-0.0, 0.0625, 14.0),
        Vec3::new(-0.0, 0.2, -0.3),
        -91.5,
        120.25,
    );
    let mut actual = Vec::new();
    for flags in [
        0,
        1,
        2,
        4,
        8,
        16,
        32,
        64,
        128,
        256,
        511,
        504,
        31,
        255,
        257,
        288,
        384,
        i32::MAX,
    ] {
        let absolute = calculate_absolute(source.clone(), change.clone(), flags);
        let text = |value: &ferraloom_protocol::play::PositionMoveRotation| {
            format!(
                "{:x},{:x},{:x}\t{:x},{:x},{:x}\t{:x},{:x}",
                value.position.x.to_bits(),
                value.position.y.to_bits(),
                value.position.z.to_bits(),
                value.delta_movement.x.to_bits(),
                value.delta_movement.y.to_bits(),
                value.delta_movement.z.to_bits(),
                value.y_rot.to_bits(),
                value.x_rot.to_bits()
            )
        };
        actual.push(format!("teleport-transform\t{flags}\t{}", text(&absolute)));
        let mut level = level(true);
        let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let mut player = player::ServerPlayer::new(
            Uuid::new(1, 2),
            "teleport".into(),
            1,
            Vec3::new(8.5, 64.25, -12.75),
            player::GameType::Survival,
        );
        player.set_y_rot(721.25);
        player.set_x_rot(-28.125);
        player.velocity = Vec3::new(0.25, -0.125, 0.5);
        player.known_movement = player.velocity;
        player.connection_ticks = 42;
        let position = player.position;
        player.movement_this_tick.push_back(Movement {
            from: Vec3::ZERO,
            to: position,
            axis_dependent_original_movement: None,
        });
        level
            .add_entity(id, EntityState::Player(Box::new(player)))
            .unwrap();
        level.take_effects(); // Initial join packets precede the teleport under test.
        teleport_player(id, change.clone(), flags, &mut level).unwrap();
        let player = level.entity::<player::ServerPlayer>(id).unwrap();
        let state =
            position_move_rotation(player.position, player.velocity, player.yaw, player.pitch);
        actual.push(format!(
            "teleport-set\t{flags}\t{}\t{:x}\t{:x},{:x},{:x}",
            text(&state),
            player.y_head_rot.to_bits(),
            player.old_position.x.to_bits(),
            player.old_position.y.to_bits(),
            player.old_position.z.to_bits()
        ));
        assert!(player.movement_this_tick.is_empty());
        assert_eq!(player.teleport.awaiting_teleport, 1);
        assert_eq!(player.teleport.awaiting_teleport_time, 42);
        assert_eq!(player.teleport.awaiting_position, Some(player.position));
        level.take_player_section_changes();
        let packets = level.take_effects();
        let [LevelEffect::PlayerPacket { recipients, packet }] = packets.as_slice() else {
            panic!("teleport must enqueue its real packet at the call site")
        };
        assert_eq!(recipients, &[id]);
        let cb::Packet::PlayerPosition(packet) = packet.as_ref() else {
            panic!("wrong teleport packet")
        };
        assert_eq!(packet.id.0, 1);
        assert_eq!(packet.change, change);
        assert_eq!(packet.relatives, flags & teleport::Relative::ALL);
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|row| row.starts_with("teleport-transform\t") || row.starts_with("teleport-set\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn player_teleport_counter_confirmation_and_mounted_known_movement_are_canonical() {
    use super::teleport::{
        accept_teleport, position_move_rotation, teleport_player, teleport_relative,
    };
    let mut level = level(true);
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let vehicle = EntityRef::fixture(crate::level::RegionId::FIXTURE, 2);
    let mut player = player::ServerPlayer::new(
        Uuid::new(1, 2),
        "teleport".into(),
        1,
        Vec3::new(8.5, 64.0, 8.5),
        player::GameType::Survival,
    );
    player.teleport.awaiting_teleport = i32::MAX - 1;
    player.connection_ticks = 100;
    player.known_movement = Vec3::new(9.0, 9.0, 9.0);
    player.velocity = Vec3::new(1.0, 1.0, 1.0);
    let mut tnt = item::PrimedTnt::new(2, 123);
    tnt.parent.velocity = Vec3::new(0.25, -0.125, 0.5);
    level
        .add_entity(id, EntityState::Player(Box::new(player)))
        .unwrap();
    level
        .add_entity(vehicle, EntityState::PrimedTnt(Box::new(tnt)))
        .unwrap();
    start_riding(id, vehicle, true, &mut level).unwrap();
    teleport_relative(id, Vec3::new(0.0, 0.0625, 0.0), &mut level).unwrap();
    let player = level.entity::<player::ServerPlayer>(id).unwrap();
    assert_eq!(player.teleport.awaiting_teleport, 0);
    assert_eq!(player.position, Vec3::new(8.5, 64.0625, 8.5));
    assert_vector(
        player.velocity,
        Vec3::new(0.25, -0.125, 0.5),
        "mounted teleport uses vehicle known movement",
    );
    assert!(!player.teleport.resend_due(120));
    assert!(player.teleport.resend_due(121));
    assert!(!accept_teleport(id, 99, &mut level).unwrap());
    assert!(accept_teleport(id, 0, &mut level).unwrap());
    assert!(
        level
            .entity::<player::ServerPlayer>(id)
            .unwrap()
            .teleport
            .awaiting_position
            .is_none()
    );
    teleport_player(
        id,
        position_move_rotation(Vec3::new(9.0, 65.0, 9.0), Vec3::ZERO, 37.25, -28.125),
        0,
        &mut level,
    )
    .unwrap();
    assert_eq!(
        level
            .entity::<player::ServerPlayer>(id)
            .unwrap()
            .teleport
            .awaiting_teleport,
        1
    );
}

#[test]
fn push_entities_up_matches_native_shape_difference_query_order_and_spectators() {
    struct PushRecorder;
    impl Entity for PushRecorder {
        fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>) {
            trace_base_entity_refs(state.as_entity(), references);
        }
        fn teleport_relative(
            &self,
            entity: EntityRef,
            delta: Vec3,
            level: &mut Level,
        ) -> Result<(), LevelError> {
            level.game_event(
                ferraloom_data::registries::game_event::Id::STEP,
                delta,
                Some(entity),
                None,
            )?;
            teleport::base_teleport_relative(entity, delta, level)
        }
    }
    static PUSH: PushRecorder = PushRecorder;
    static DISPATCH: Behaviors =
        Behaviors::EMPTY.with_entity(ferraloom_data::EntityType::TNT, &PUSH);
    let pairs = [
        (Block::DIRT_PATH, Block::DIRT),
        (Block::FARMLAND, Block::DIRT),
        (Block::AIR, Block::STONE),
        (Block::STONE, Block::AIR),
        (Block::STONE_SLAB, Block::STONE),
        (Block::STONE, Block::STONE),
    ];
    let mut actual = Vec::new();
    for (index, (old, new)) in pairs.into_iter().enumerate() {
        let mut level = level_with_behaviors(false, &DISPATCH);
        game_events::observe(&mut level);
        for (id, position) in [
            (2, Vec3::new(8.75, 63.5, 8.5)),
            (1, Vec3::new(8.5, 63.9375, 8.5)),
        ] {
            let mut tnt = item::PrimedTnt::new(id as i32, 123);
            tnt.parent.uuid = Uuid::new(0, id as i64);
            tnt.parent.set_pos(position);
            level
                .add_entity(
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, id),
                    EntityState::PrimedTnt(Box::new(tnt)),
                )
                .unwrap();
        }
        let spectator = player::ServerPlayer::new(
            Uuid::new(1, 2),
            "spectator".into(),
            3,
            Vec3::new(8.5, 63.9375, 8.5),
            player::GameType::Spectator,
        );
        level
            .add_entity(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, 3),
                EntityState::Player(Box::new(spectator)),
            )
            .unwrap();
        game_events::take(&mut level); // Initial join packets precede the push under test.
        let state = level
            .push_entities_up(
                old.default_state(),
                new.default_state(),
                BlockPos::new(8, 63, 8),
            )
            .unwrap();
        let calls: Vec<_> = game_events::take(&mut level)
            .into_iter()
            .map(|effect| {
                let Observation::GameEvent {
                    position,
                    source: Some(entity),
                    ..
                } = effect
                else {
                    panic!("spectator must not receive a teleport")
                };
                format!(
                    "{}:{:x},{:x},{:x}",
                    entity.serial(),
                    position.x.to_bits(),
                    position.y.to_bits(),
                    position.z.to_bits()
                )
            })
            .collect();
        let vec = |id| {
            let p = level
                .entity::<EntityData>(EntityRef::fixture(crate::level::RegionId::FIXTURE, id))
                .unwrap()
                .position;
            format!(
                "{:x},{:x},{:x}",
                p.x.to_bits(),
                p.y.to_bits(),
                p.z.to_bits()
            )
        };
        actual.push(format!(
            "push-entities\t{index}\t{}\t{}\t{}\t{}\t{}",
            state.id(),
            if calls.is_empty() {
                "-".into()
            } else {
                calls.join(";")
            },
            vec(1),
            vec(2),
            vec(3)
        ));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|row| row.starts_with("push-entities\t"))
        .map(|row| {
            row.split('\t')
                .enumerate()
                .filter_map(|(index, value)| (index != 3).then_some(value))
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn nonplayer_teleport_snaps_recursive_passengers_without_changing_motion_or_rotation() {
    let mut level = level(false);
    for index in 0..3 {
        let mut entity = item::PrimedTnt::new(index + 1, 123);
        entity.parent.uuid = Uuid::new(0, i64::from(index) + 1);
        entity
            .parent
            .set_pos(Vec3::new(8.5, 64.0 + f64::from(index) * 5.0, 8.5));
        entity.parent.set_y_rot(index as f32 * 37.25);
        entity.parent.set_x_rot(index as f32 * -14.5);
        entity.parent.velocity = Vec3::new(0.25, -0.125, 0.5);
        level
            .add_entity(
                EntityRef::fixture(crate::level::RegionId::FIXTURE, index as u64 + 1),
                EntityState::PrimedTnt(Box::new(entity)),
            )
            .unwrap();
    }
    start_riding(
        EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
        EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
        true,
        &mut level,
    )
    .unwrap();
    start_riding(
        EntityRef::fixture(crate::level::RegionId::FIXTURE, 3),
        EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
        true,
        &mut level,
    )
    .unwrap();
    EntityRef::fixture(crate::level::RegionId::FIXTURE, 1)
        .teleport_relative(Vec3::new(2.0, 2.0, 4.0), &mut level)
        .unwrap();
    let mut actual = Vec::new();
    for id in 1..=3 {
        let entity = level
            .entity::<EntityData>(EntityRef::fixture(crate::level::RegionId::FIXTURE, id))
            .unwrap();
        let text = |p: Vec3| {
            format!(
                "{:x},{:x},{:x}",
                p.x.to_bits(),
                p.y.to_bits(),
                p.z.to_bits()
            )
        };
        actual.push(format!(
            "teleport-passengers\t{id}\t{}\t{}\t{}\t{:x},{:x}",
            text(entity.position),
            text(entity.old_position),
            text(entity.velocity),
            entity.yaw.to_bits(),
            entity.pitch.to_bits()
        ));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|row| row.starts_with("teleport-passengers\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn accepted_player_section_changes_capture_a_to_b_to_a_before_any_drain() {
    use ferraloom_util::SectionPos;
    let mut level = level(false);
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    let player = player::ServerPlayer::new(
        Uuid::new(1, 2),
        "sections".into(),
        1,
        Vec3::new(8.5, 64.0, 8.5),
        player::GameType::Survival,
    );
    level
        .add_entity(id, EntityState::Player(Box::new(player)))
        .unwrap();
    for position in [Vec3::new(24.5, 64.0, 8.5), Vec3::new(8.5, 64.0, 8.5)] {
        level.set_entity_pos(id, position).unwrap();
    }
    assert_eq!(
        level.take_player_section_changes(),
        vec![
            (id, SectionPos::new(1, 4, 0)),
            (id, SectionPos::new(0, 4, 0))
        ]
    );
}

#[test]
fn piston_effects_use_only_the_explicit_path_and_keep_recorded_movements() {
    let mut level = level(false);
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    level
        .set_block_state(BlockPos::new(8, 64, 8), Block::WATER.default_state())
        .unwrap();
    let mut entity = item::PrimedTnt::new(1, 123);
    entity.parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
    entity.parent.remaining_fire_ticks = 100;
    let recording = Movement {
        from: Vec3::new(20.5, 64.0, 8.5),
        to: Vec3::new(21.5, 64.0, 8.5),
        axis_dependent_original_movement: None,
    };
    entity.parent.movement_this_tick.push_back(recording);
    level
        .add_entity(id, EntityState::PrimedTnt(Box::new(entity)))
        .unwrap();
    let behavior = level.entity_behavior(id).unwrap();
    assert_eq!(
        behavior.piston_push_reaction(id, &level).unwrap(),
        ferraloom_data::PushReaction::PushPull
    );
    assert!(!behavior.is_ignoring_block_triggers(id, &level).unwrap());
    behavior
        .apply_effects_from_blocks_between(
            id,
            Vec3::new(8.5, 64.0, 8.5),
            Vec3::new(9.5, 64.0, 8.5),
            &mut level,
        )
        .unwrap();
    let entity = level.entity::<EntityData>(id).unwrap();
    assert!(entity.remaining_fire_ticks <= 0);
    assert_eq!(entity.movement_this_tick.len(), 1);
    assert_eq!(entity.movement_this_tick[0].from, recording.from);
    assert_eq!(entity.movement_this_tick[0].to, recording.to);
    assert!(entity.final_movements_this_tick.is_empty());
    let entity = level.entity_mut::<EntityData>(id).unwrap();
    entity.remove_latest_movement_recording();
    assert!(entity.movement_this_tick.is_empty());
    entity.remove_latest_movement_recording();
    assert!(entity.movement_this_tick.is_empty());
}

#[test]
fn pending_teleport_rotation_matches_native_absolute_snap_without_moving_history() {
    let mut actual = Vec::new();
    for (yaw, pitch) in [(721.25_f32, 270_f32), (-721.25, -360.0), (37.25, -28.125)] {
        let mut entity = EntityData::new(ferraloom_data::EntityType::PLAYER, 1, 123);
        entity.set_pos(Vec3::new(8.5, 64.25, -12.75));
        entity.set_old_pos_and_rot();
        entity.abs_snap_rotation_to(
            ferraloom_util::mth::wrap_degrees_f32(yaw),
            ferraloom_util::mth::wrap_degrees_f32(pitch),
        );
        actual.push(format!(
            "teleport-pending-rotation\t{:x},{:x}\t{:x},{:x},{:x},{:x}\t{:x},{:x},{:x}",
            yaw.to_bits(),
            pitch.to_bits(),
            entity.yaw.to_bits(),
            entity.pitch.to_bits(),
            entity.old_yaw.to_bits(),
            entity.old_pitch.to_bits(),
            entity.old_position.x.to_bits(),
            entity.old_position.y.to_bits(),
            entity.old_position.z.to_bits()
        ));
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|row| row.starts_with("teleport-pending-rotation\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn teleport_confirmation_uses_native_clamp_and_preserves_interpolation_history() {
    let mut actual = Vec::new();
    for destination in [
        Vec3::new(8.5, 64.0, 8.5),
        Vec3::new(3.0e7 + 1.0, 65.0, -3.0e7 - 1.0),
    ] {
        let mut level = level(false);
        let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        level
            .add_entity(
                id,
                EntityState::Player(Box::new(player::ServerPlayer::new(
                    Uuid::new(1, 2),
                    "confirm".into(),
                    1,
                    Vec3::ZERO,
                    player::GameType::Survival,
                ))),
            )
            .unwrap();
        teleport::teleport_player(
            id,
            teleport::position_move_rotation(destination, Vec3::ZERO, 721.25, -28.125),
            0,
            &mut level,
        )
        .unwrap();
        assert!(teleport::accept_teleport(id, 1, &mut level).unwrap());
        let player = level.entity::<player::ServerPlayer>(id).unwrap();
        let text = |p: Vec3| {
            format!(
                "{:x},{:x},{:x}",
                p.x.to_bits(),
                p.y.to_bits(),
                p.z.to_bits()
            )
        };
        actual.push(format!(
            "teleport-confirm-snap\t{}\t{}\t{}\t{}\t{:x},{:x},{:x}",
            text(destination),
            text(player.position),
            text(player.old_position),
            text(player.interpolation_old_position),
            player.yaw.to_bits(),
            player.y_head_rot.to_bits(),
            player.old_yaw.to_bits()
        ));
        assert_eq!(player.last_good, destination);
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|row| row.starts_with("teleport-confirm-snap\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn may_build_is_independent_and_game_mode_updates_match_native_abilities() {
    let mut actual = Vec::new();
    for mode in [
        player::GameType::Survival,
        player::GameType::Creative,
        player::GameType::Adventure,
        player::GameType::Spectator,
    ] {
        for flying in [false, true] {
            let mut player = player::ServerPlayer::new(
                Uuid::new(1, 2),
                "ability".into(),
                1,
                Vec3::ZERO,
                player::GameType::Survival,
            );
            player.game_mode = mode;
            player.abilities.flying = flying;
            player.update_game_mode_abilities();
            let may_build = player.may_build();
            let result_flying = player.abilities.flying;
            player.abilities.may_build = !may_build;
            let saved = player
                .save(&crate::entity::tests::persistence_level())
                .unwrap();
            let saved_may_build = saved
                .get_compound("abilities")
                .unwrap()
                .get_boolean("mayBuild")
                .unwrap();
            actual.push(format!(
                "player-may-build\t{}\t{flying}\t{may_build},{result_flying},{},{}",
                mode.id(),
                player.may_build(),
                saved_may_build
            ));
            assert_eq!(player.game_mode, mode);
        }
    }
    let expected: Vec<_> = VECTORS
        .lines()
        .filter(|row| row.starts_with("player-may-build\t"))
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn lava_entity_force_reads_the_live_dimension_instead_of_the_legacy_rule_input() {
    let mut level = level(false);
    let pos = BlockPos::new(8, 64, 8);
    level
        .set_block_state(pos, Block::LAVA.default_state())
        .unwrap();
    level
        .set_block_state(
            pos.east(1),
            crate::block::fluid::legacy_block(FlowingFluid::Lava.flowing(4, false)),
        )
        .unwrap();
    let mut entity = item::PrimedTnt::new(1, 123);
    entity.parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
    let id = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    level
        .add_entity(id, EntityState::PrimedTnt(Box::new(entity)))
        .unwrap();
    for fast in [false, true, false] {
        install_test_environment(&mut level, fast, "test");
        level.rules_mut().fast_lava = !fast;
        level.entity_mut::<EntityData>(id).unwrap().velocity = Vec3::new(0.1, 0.0, 0.0);
        fluid::update_fluid_interaction(&item::PrimedTntBehavior, id, &mut level).unwrap();
        let scale = if fast {
            0.007
        } else {
            0.002_333_333_333_333_333_5
        };
        assert_vector(
            level.entity::<EntityData>(id).unwrap().velocity,
            Vec3::new(0.1 + scale, 0.0, 0.0),
            "live FAST_LAVA",
        );
    }
}
