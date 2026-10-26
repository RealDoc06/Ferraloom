use super::*;
use crate::entity::{
    EntityState,
    effect::MobEffectInstance,
    player::{GameType, ServerPlayer},
    synched_data,
};
use ferraloom_data::{Block, properties};
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_protocol::component::{AttributeModifier, AttributeOperation, Persistent};
use ferraloom_util::{BlockPos, Uuid};
use std::sync::Arc;

fn double(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).unwrap())
}
fn float(value: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(value, 16).unwrap())
}
fn vector(value: &str) -> Vec3 {
    let mut values = value.split(',').map(double);
    Vec3::new(
        values.next().unwrap(),
        values.next().unwrap(),
        values.next().unwrap(),
    )
}
fn bits(value: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits()
    )
}
fn rows(prefix: &str) -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(move |line| line.starts_with(prefix))
        .map(|line| line.split('\t').collect())
}
fn prepare(level: &mut Level, kind: &str, case: i32, player: bool) {
    let fluid = match kind {
        "water" => Block::WATER,
        "lava" => Block::LAVA,
        "air" => Block::AIR,
        _ => unreachable!(),
    };
    let top = if matches!(case, 1 | 5 | 6) { 64 } else { 66 };
    for x in 6..=11 {
        for y in 64..=68 {
            for z in 6..=11 {
                level
                    .set_block_state(
                        BlockPos::new(x, y, z),
                        if y <= top { fluid } else { Block::AIR }.default_state(),
                    )
                    .unwrap();
            }
        }
    }
    if matches!(case, 5..=8) {
        for y in 64..=if case == 6 { 67 } else { 64 } {
            for z in 6..=11 {
                level
                    .set_block_state(BlockPos::new(9, y, z), Block::STONE.default_state())
                    .unwrap();
            }
        }
        if case == 7 {
            level
                .set_block_state(BlockPos::new(8, 64, 8), Block::LADDER.default_state())
                .unwrap();
        }
        if case == 6 {
            let ceiling = if player {
                Block::STONE_SLAB
                    .default_state()
                    .with(properties::TYPE_, properties::Type::Top)
                    .unwrap()
            } else {
                Block::STONE.default_state()
            };
            level
                .set_block_state(BlockPos::new(8, 66, 8), ceiling)
                .unwrap();
        }
    }
}

#[test]
fn native_complete_water_lava_and_flying_bodies_preserve_bits_and_exit_probes() {
    let mut checked = 0;
    for row in rows("travel-fluid\t") {
        let mut level = crate::entity::tests::level(true);
        let case = row[3].parse().unwrap();
        let player = row[1] == "player";
        prepare(&mut level, row[2], case, player);
        let entity = level.next_entity_ref();
        let mut state = if player {
            let mut player = ServerPlayer::new(
                Uuid::new(3, 4),
                "travel".into(),
                level.next_entity_id().unwrap(),
                vector(row[4]),
                GameType::Survival,
            );
            player.abilities.flying = row[14].parse().unwrap();
            EntityState::Player(Box::new(player))
        } else {
            EntityState::Cow(Box::default())
        };
        let living = state.as_living_mut().unwrap();
        living.set_pos(vector(row[4]));
        living.yaw = 0.0;
        living.velocity = vector(row[5]);
        living.on_ground = row[8].parse().unwrap();
        living
            .entity_data
            .set(synched_data::NO_GRAVITY, row[9].parse().unwrap());
        living.fall_distance = 4.0;
        living.speed = 0.17;
        for (attribute, value) in [
            (Attribute::MOVEMENT_SPEED, 0.17),
            (Attribute::WATER_MOVEMENT_EFFICIENCY, double(row[10])),
            (Attribute::GRAVITY, double(row[11])),
        ] {
            living
                .attributes
                .get_instance(attribute)
                .unwrap()
                .set_base_value(value);
        }
        let sprint = row[13].parse().unwrap();
        living.set_shared_flag(3, sprint);
        if sprint {
            living
                .attributes
                .get_instance(Attribute::MOVEMENT_SPEED)
                .unwrap()
                .add_transient_modifier(Arc::new(AttributeModifier {
                    id: "minecraft:sprinting".parse().unwrap(),
                    amount: 0.30000001192092896.into(),
                    operation: AttributeOperation::AddMultipliedTotal,
                }))
                .unwrap();
        }
        if let Ok(effect) = row[15].parse::<u16>() {
            // These two effects have no attribute modifiers. Hydration supplies
            // canonical active state while the full travel body runs unchanged.
            let effect = MobEffectInstance::new(
                MobEffect::from_protocol_id(effect).unwrap(),
                100,
                0,
                false,
                true,
                true,
                None,
            );
            let tag = Tag::List(ListTag(vec![effect.to_component().to_nbt().unwrap()]));
            assert!(living.active_effects.load(Some(&tag)).is_empty());
        }
        level.add_entity(entity, state).unwrap();
        let behavior = level.entity_behavior(entity).unwrap();
        crate::entity::fluid::update_fluid_interaction(behavior, entity, &mut level).unwrap();
        level.entity_mut::<EntityData>(entity).unwrap().first_tick = false;
        let base = level.entity::<EntityData>(entity).unwrap();
        assert_eq!(bits(base.velocity), row[6], "fluid update {row:?}");
        assert_eq!(
            format!(
                "{:x}",
                base.fluid_interaction
                    .height(FluidTags::LAVA, &level)
                    .unwrap()
                    .to_bits()
            ),
            row[16],
            "height {row:?}"
        );
        assert_eq!(
            format!("{:x}", base.fluid_jump_threshold().to_bits()),
            row[17],
            "threshold {row:?}"
        );
        assert_eq!(
            format!(
                "{:x}",
                behavior.get_speed(entity, &level).unwrap().to_bits()
            ),
            row[12],
            "speed {row:?}"
        );
        let fluid = level
            .get_block_state(base.block_position)
            .unwrap()
            .data()
            .fluid;
        assert_eq!(
            should_travel_in_fluid(behavior, entity, fluid, &level)
                .unwrap()
                .to_string(),
            row[18],
            "dispatch {row:?}"
        );
        if row[19] == "true" {
            travel_flying(
                behavior,
                entity,
                vector(row[7]),
                [0.03125, 0.0625, 0.125],
                &mut level,
            )
            .unwrap();
        } else {
            travel_in_fluid(behavior, entity, vector(row[7]), &mut level).unwrap();
        }
        let base = level.entity::<EntityData>(entity).unwrap();
        assert_eq!(bits(base.position), row[20], "position {row:?}");
        assert_eq!(bits(base.velocity), row[21], "velocity {row:?}");
        let flags: String = [
            base.on_ground,
            base.horizontal_collision,
            base.vertical_collision,
            base.vertical_collision_below,
            base.minor_horizontal_collision,
        ]
        .map(|v| if v { '1' } else { '0' })
        .into_iter()
        .collect();
        assert_eq!(flags, row[22], "flags {row:?}");
        assert_eq!(
            format!("{:x}", base.fall_distance.to_bits()),
            row[23],
            "fall {row:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 70);
}

#[test]
fn native_fluid_jump_threshold_compares_widened_float_height() {
    let mut base = EntityData::new(ferraloom_data::EntityType::COW, 1, 0);
    let mut checked = 0;
    for row in rows("fluid-jump-threshold\t") {
        base.eye_height = float(row[1]);
        assert_eq!(
            format!("{:x}", base.fluid_jump_threshold().to_bits()),
            row[2]
        );
        checked += 1;
    }
    assert_eq!(checked, 4);
}
