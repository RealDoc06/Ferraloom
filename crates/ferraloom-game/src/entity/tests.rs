//! Geometry and flow vectors captured from vanilla's real Entity.move/getFlow.
use super::*;
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
                Err(error) => panic!("unexpected collision query error: {error}"),
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
    let id = EntityRef(1);
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
fn living_movement_and_ignition_use_live_attributes_with_native_float_steps() {
    use ferraloom_data::registries::attribute::Id as Attribute;
    use ferraloom_protocol::codec::VarInt;
    let mut level = level(true);
    level
        .set_block_state(BlockPos::new(8, 63, 8), Block::SOUL_SAND.default_state())
        .unwrap();
    let id = EntityRef(1);
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
    let mut level = Level::new(Region::new(&BEHAVIORS));
    let player = player::ServerPlayer::new(
        Uuid::new(1, 2),
        "dimensions".into(),
        1,
        Vec3::ZERO,
        player::GameType::Survival,
    );
    level
        .add_entity(EntityRef(1), EntityState::Player(Box::new(player)))
        .unwrap();
    level
        .entity_mut::<LivingEntity>(EntityRef(1))
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
            .dimensions(EntityRef(1), pose, &level)
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
    let entity = EntityRef(1);
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
    let player = EntityRef(2);
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
fn level(floor: bool) -> Level {
    level_with_behaviors(floor, &BEHAVIORS)
}
fn level_with_behaviors(floor: bool, behaviors: &'static Behaviors) -> Level {
    use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
    let template = LevelChunkSection::new(
        Strategy::block_states(),
        Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
        BiomeId(0),
    );
    let mut level = Level::new(Region::new(behaviors));
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
        let id = EntityRef(1);
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
        EntityRef(id.0)
            .move_by(&mut level, movement::MoverType::SelfMovement, delta)
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
    let mut id = EntityRef(1);
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
        let id = EntityRef(1);
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
    let mut tnt = item::PrimedTnt::new(level.next_entity_id(), 123);
    tnt.parent.set_pos(Vec3::new(8.5, 66.0, 8.5));
    tnt.parent.velocity = Vec3::new(0.125, 0.2, -0.03);
    tnt.set_fuse(4);
    let mut entity = EntityRef(1);
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
    let mut entity = EntityRef(1);
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
            .add_entity(EntityRef(n as u64), EntityState::Item(Box::new(item)))
            .unwrap();
    }
    item::item_entity::merge_with_neighbours(EntityRef(1), &mut level).unwrap();
    assert!(level.entity::<EntityData>(EntityRef(1)).unwrap().removed);
    assert_eq!(
        count(
            level
                .entity::<item::ItemEntity>(EntityRef(2))
                .unwrap()
                .item()
        ),
        20
    );
    assert_eq!(
        count(
            level
                .entity::<item::ItemEntity>(EntityRef(3))
                .unwrap()
                .item()
        ),
        10
    );
    let player_id = EntityRef(4);
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
    let pickup = EntityRef(3);
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
        Some(effects::EntityEffect::TakeItem { count: 1, .. })
    ));
}

#[test]
fn pick_result_preserves_null_and_cow_spawn_egg() {
    use ferraloom_protocol::component::ItemStack;
    let mut level = level(false);
    let cow = EntityRef(1);
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
    let item = EntityRef(2);
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
        let player_id = level.next_entity_ref();
        let mut player = player::ServerPlayer::new(
            Uuid::new(1, 2),
            "fixture".into(),
            level.next_entity_id(),
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
    let mut id = EntityRef(1);
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
                let tnt = item::PrimedTnt::new(level.next_entity_id(), 123);
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
    let id = item::falling_block::fall(&mut level, source, Block::SAND.default_state()).unwrap();
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
        EntityRef(id.0).tick(&mut level).unwrap();
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
    let mut id = EntityRef(1);
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
    let player_id = EntityRef(1);
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
    let orb_id = EntityRef(2);
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
                EntityRef(n as u64),
                EntityState::ExperienceOrb(Box::new(orb)),
            )
            .unwrap();
    }
    let mut id = EntityRef(1);
    common_tick(id, &mut level).unwrap();
    id.tick(&mut level).unwrap();
    let orb = level.entity::<ExperienceOrb>(id).unwrap();
    assert_eq!(orb.count, 3);
    assert_eq!(orb.age, 6);
    assert!(
        level
            .entity::<ExperienceOrb>(EntityRef(2))
            .unwrap()
            .parent
            .removed
    );
    assert_eq!(
        level.entity::<ExperienceOrb>(EntityRef(3)).unwrap().count,
        3
    );
}

#[test]
fn piston_effects_use_only_the_explicit_path_and_keep_recorded_movements() {
    let mut level = level(false);
    let id = EntityRef(1);
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
    entity
        .parent
        .movement_this_tick
        .push_back(recording.clone());
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
