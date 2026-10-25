use super::*;
use crate::entity::{
    EntityMethod, EntityState, RemovalReason,
    effects::EntityEffect,
    experience_orb::ExperienceOrb,
    item::{ItemEntity, PrimedTnt},
    player::GameType,
};
use ferraloom_data::Item;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{Uuid, Vec3};

fn vector(value: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits()
    )
}
#[test]
fn native_contact_bounds_section_order_health_guards_and_orb_rng() {
    let mut actual = Vec::new();
    for scenario in 0..6 {
        let mut level = crate::entity::tests::level(false);
        let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(1, 2),
                    "contact".into(),
                    1,
                    Vec3::new(15.8, 64.0, -0.2),
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let vehicle = EntityRef::fixture(crate::level::RegionId::FIXTURE, 9);
        let mut vehicle_data = PrimedTnt::new(9, 9);
        vehicle_data.parent.set_pos(Vec3::new(18.5, 64.0, 0.5));
        level
            .add_entity(vehicle, EntityState::PrimedTnt(Box::new(vehicle_data)))
            .unwrap();
        if scenario == 1 || scenario == 2 {
            crate::entity::start_riding(player, vehicle, true, &mut level).unwrap();
            if scenario == 2 {
                level
                    .remove_entity_with_reason(vehicle, RemovalReason::Discarded)
                    .unwrap();
            }
        }
        for (index, position) in [
            Vec3::new(16.6, 64.0, -0.2),
            Vec3::new(15.1, 64.0, -0.2),
            Vec3::new(16.6, 64.3, 0.5),
            Vec3::new(17.5, 64.0, -0.2),
            Vec3::new(15.8, 66.25, -0.2),
            Vec3::new(18.4, 64.0, 1.2),
        ]
        .into_iter()
        .enumerate()
        {
            let id = index as i32 + 2;
            let mut item = ItemEntity::new(id, i64::from(id));
            item.parent.set_pos(position);
            item.set_item(ItemStack::new(Item::STONE, 1, Default::default()));
            level
                .add_entity(
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, id as u64),
                    EntityState::Item(Box::new(item)),
                )
                .unwrap();
        }
        for index in 0..3 {
            let mut orb = ExperienceOrb::new(10 + index, i64::from(10 + index));
            orb.parent
                .set_pos(Vec3::new(15.5 + f64::from(index) * 0.25, 64.0, 0.0));
            level
                .add_entity(
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, (10 + index) as u64),
                    EntityState::ExperienceOrb(Box::new(orb)),
                )
                .unwrap();
        }
        let player_data = level.entity_mut::<ServerPlayer>(player).unwrap();
        match scenario {
            3 => player_data.set_health(0.0),
            4 => player_data.game_mode = GameType::Spectator,
            5 => player_data.set_health(f32::NAN),
            _ => {}
        }
        player_data.random.set_seed(99);
        let area = pickup_area(player, &level).unwrap();
        touch_entities(player, &mut level).unwrap();
        let calls: Vec<_> = level
            .take_entity_effects()
            .into_iter()
            .filter_map(|effect| {
                if let EntityEffect::TakeItem { entity, .. } = effect {
                    Some(entity.serial().to_string())
                } else {
                    None
                }
            })
            .collect();
        let bounds = area
            .map(|bounds| {
                format!(
                    "{};{}",
                    vector(Vec3::new(bounds.min_x, bounds.min_y, bounds.min_z)),
                    vector(Vec3::new(bounds.max_x, bounds.max_y, bounds.max_z)),
                )
            })
            .unwrap_or_else(|| "none".into());
        let next = level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .random
            .next_long();
        assert_eq!(
            level.unported_behavior_stats().skipped_calls,
            u64::from(scenario < 3)
        );
        actual.push(format!(
            "player-contact\t{scenario}\t{}\t{bounds}\t{:x}",
            if calls.is_empty() {
                "-".into()
            } else {
                calls.join(",")
            },
            next as u64
        ));
    }
    let expected: Vec<_> = include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("player-contact\t"))
        .collect();
    assert_eq!(actual, expected);
}
#[test]
fn orb_contact_is_explicit_until_xp_and_mending_are_ported() {
    let mut level = crate::entity::tests::level(false);
    let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "orb".into(),
                1,
                Vec3::new(8.5, 64.0, 8.5),
                GameType::Survival,
            ))),
        )
        .unwrap();
    let mut orb = ExperienceOrb::new(2, 2);
    orb.parent.set_pos(Vec3::new(8.5, 64.0, 8.5));
    level
        .add_entity(
            EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
            EntityState::ExperienceOrb(Box::new(orb)),
        )
        .unwrap();
    level.set_unported_behavior(crate::block::UnportedBehavior::Strict);
    assert_eq!(
        touch_entities(player, &mut level),
        Err(LevelError::UnsupportedEntityMethod(
            EntityType::EXPERIENCE_ORB,
            EntityMethod::PlayerTouch
        ))
    );
}
