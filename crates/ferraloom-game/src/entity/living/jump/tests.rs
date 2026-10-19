use super::*;
use crate::entity::{
    EntityState,
    effect::MobEffectInstance,
    player::{GameType, ServerPlayer},
};
use ferraloom_data::BlockState;
use ferraloom_nbt::{ListTag, Tag};
use ferraloom_protocol::component::Persistent;
use ferraloom_util::{BlockPos, Uuid};
fn double(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).unwrap())
}
fn float(value: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(value, 16).unwrap())
}
fn vector(value: &str) -> Vec3 {
    let mut v = value.split(',').map(double);
    Vec3::new(v.next().unwrap(), v.next().unwrap(), v.next().unwrap())
}
fn bits(value: Vec3) -> String {
    format!(
        "{:x},{:x},{:x}",
        value.x.to_bits(),
        value.y.to_bits(),
        value.z.to_bits()
    )
}
#[test]
fn native_jump_power_ground_sync_and_liquid_impulse_preserve_bits() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("living-jump\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let mut level = crate::entity::tests::level(true);
        level.install_shared(Box::default());
        level
            .set_block_state(
                BlockPos::new(8, 63, 8),
                BlockState::from_id(row[3].parse().unwrap()).unwrap(),
            )
            .unwrap();
        let entity = level.next_entity_ref();
        let mut state = if row[1] == "player" {
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(3, 4),
                "jump".into(),
                level.next_entity_id(),
                Vec3::ZERO,
                GameType::Survival,
            )))
        } else {
            EntityState::Cow(Box::default())
        };
        let living = state.as_living_mut().unwrap();
        living.set_pos(Vec3::new(8.5, 64.0, 8.5));
        living.yaw = float(row[7]);
        living.set_shared_flag(3, row[8].parse().unwrap());
        living.velocity = vector(row[9]);
        living.needs_sync = false;
        living
            .attributes
            .get_instance(Attribute::JUMP_STRENGTH)
            .unwrap()
            .set_base_value(double(row[4]));
        if let Ok(amplifier) = row[6].parse::<u8>() {
            let effect = MobEffectInstance::new(
                MobEffect::JUMP_BOOST,
                100,
                i32::from(amplifier),
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
        assert_eq!(
            format!(
                "{:x}",
                behavior
                    .jump_power(entity, float(row[5]), &level)
                    .unwrap()
                    .to_bits()
            ),
            row[10],
            "scaled {row:?}"
        );
        assert_eq!(
            format!(
                "{:x}",
                behavior.jump_power(entity, 1.0, &level).unwrap().to_bits()
            ),
            row[11],
            "ordinary {row:?}"
        );
        behavior.jump_from_ground(entity, &mut level).unwrap();
        let base = level.entity::<EntityData>(entity).unwrap();
        assert_eq!(bits(base.velocity), row[12], "ground {row:?}");
        assert_eq!(base.needs_sync.to_string(), row[13], "sync {row:?}");
        level.entity_mut::<EntityData>(entity).unwrap().velocity = vector(row[9]);
        if row[1] == "cow" {
            crate::entity::mob::jump_in_liquid(entity, row[15].parse().unwrap(), &mut level)
                .unwrap();
        } else {
            behavior.jump_in_liquid(entity, &mut level).unwrap();
        }
        assert_eq!(
            bits(level.entity::<EntityData>(entity).unwrap().velocity),
            row[14],
            "liquid {row:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 48);
}

#[test]
fn mob_liquid_jump_uses_both_native_navigation_capabilities() {
    let mut checked = 0;
    for line in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("mob-liquid-jump\t"))
    {
        let row: Vec<_> = line.split('\t').collect();
        let mut level = crate::entity::tests::level(false);
        let id = level.next_entity_ref();
        let mut state = EntityState::Cow(Box::default());
        state.as_entity_mut().set_delta_movement(vector(row[2]));
        level.add_entity(id, state).unwrap();
        crate::entity::mob::jump_in_liquid(id, row[1].parse().unwrap(), &mut level).unwrap();
        assert_eq!(
            bits(level.entity::<EntityData>(id).unwrap().velocity),
            row[3],
            "{row:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 6);
}
