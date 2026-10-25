use crate::{
    entity::{
        EntityData, EntityState, LivingEntity,
        item::PrimedTnt,
        player::{GameType, ServerPlayer},
    },
    level::Level,
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{BlockPos, Uuid, Vec3};

fn double(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).unwrap())
}
fn float(value: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(value, 16).unwrap())
}
fn vector(value: &str) -> Vec3 {
    let mut fields = value.split(',').map(double);
    Vec3::new(
        fields.next().unwrap(),
        fields.next().unwrap(),
        fields.next().unwrap(),
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
fn prepare(level: &mut Level, case: i32, surface: BlockState) {
    let fluid = if (3..=9).contains(&case) || case == 11 || case == 13 {
        Block::WATER
    } else if (16..=17).contains(&case) {
        Block::LAVA
    } else {
        Block::AIR
    };
    let top = if matches!(case, 6 | 7 | 17) { 64 } else { 66 };
    level
        .set_block_state(BlockPos::new(8, 63, 8), surface)
        .unwrap();
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
    if case == 15 {
        level
            .set_block_state(BlockPos::new(8, 64, 8), Block::LADDER.default_state())
            .unwrap();
    }
}

#[test]
fn native_virtual_travel_preserves_player_swim_flight_and_passenger_parent_calls() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("travel-dispatch\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let mut level = crate::entity::tests::level(true);
        prepare(
            &mut level,
            row[2].parse().unwrap(),
            BlockState::from_id(row[3].parse().unwrap()).unwrap(),
        );
        let entity = level.next_entity_ref();
        let mut state = if row[1] == "player" {
            let mut player = ServerPlayer::new(
                Uuid::new(3, 4),
                "travel".into(),
                level.next_entity_id().unwrap(),
                vector(row[4]),
                GameType::Survival,
            );
            player.abilities.flying = row[11].parse().unwrap();
            player.set_ability_flying_speed(float(row[12]));
            EntityState::Player(Box::new(player))
        } else {
            EntityState::Cow(Box::default())
        };
        let living = state.as_living_mut().unwrap();
        living.set_pos(vector(row[4]));
        living.velocity = vector(row[5]);
        living.yaw = float(row[7]);
        living.pitch = float(row[8]);
        living.on_ground = row[9].parse().unwrap();
        living.set_shared_flag(4, row[10].parse().unwrap());
        living.set_shared_flag(7, row[14].parse().unwrap());
        living.jumping = row[15].parse().unwrap();
        living.speed = 0.17;
        living
            .attributes
            .get_instance(ferraloom_data::registries::attribute::Id::MOVEMENT_SPEED)
            .unwrap()
            .set_base_value(0.17);
        level.add_entity(entity, state).unwrap();
        if row[13] == "true" {
            let mount = level.next_entity_ref();
            let tnt = PrimedTnt::new(level.next_entity_id().unwrap(), 0);
            level
                .add_entity(mount, EntityState::PrimedTnt(Box::new(tnt)))
                .unwrap();
            assert!(crate::entity::start_riding(entity, mount, true, &mut level).unwrap());
        }
        let behavior = level.entity_behavior(entity).unwrap();
        crate::entity::fluid::update_fluid_interaction(behavior, entity, &mut level).unwrap();
        level.entity_mut::<EntityData>(entity).unwrap().first_tick = false;
        behavior.travel(entity, vector(row[6]), &mut level).unwrap();
        let base = level.entity::<EntityData>(entity).unwrap();
        assert_eq!(bits(base.position), row[16], "position {row:?}");
        assert_eq!(bits(base.velocity), row[17], "velocity {row:?}");
        let flags: String = [
            base.on_ground,
            base.horizontal_collision,
            base.vertical_collision,
            base.vertical_collision_below,
            base.minor_horizontal_collision,
        ]
        .map(|value| if value { '1' } else { '0' })
        .into_iter()
        .collect();
        assert_eq!(flags, row[18], "flags {row:?}");
        assert_eq!(
            format!("{:x}", base.fall_distance.to_bits()),
            row[19],
            "fall {row:?}"
        );
        assert_eq!(base.shared_flag(7).to_string(), row[20], "flight {row:?}");
        assert_eq!(
            level.entity::<LivingEntity>(entity).unwrap().jumping,
            row[15].parse::<bool>().unwrap()
        );
        checked += 1;
    }
    assert_eq!(checked, 46);
}
