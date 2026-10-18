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
    use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
    let template = LevelChunkSection::new(
        Strategy::block_states(),
        Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
        BiomeId(0),
    );
    let mut level = Level::new(Region::new(&BEHAVIORS));
    for x in -2..=2 {
        for z in -2..=2 {
            level
                .add_chunk(LevelChunk::new(
                    ChunkPos::new(x, z),
                    -4..20,
                    template.clone(),
                ))
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
            EntityState::Cow(cow)
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
        level.add_entity(id, EntityState::Cow(cow)).unwrap();
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
