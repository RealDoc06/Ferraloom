use super::*;

fn rows(prefix: &str) -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(move |line| line.starts_with(prefix))
        .map(|line| line.split('\t').collect())
}
fn instance(duration: i32, amplifier: i32, flags: i32) -> MobEffectInstance {
    MobEffectInstance::new(
        MobEffect::SPEED,
        duration,
        amplifier,
        flags & 1 != 0,
        flags & 2 != 0,
        flags & 4 != 0,
        None,
    )
}
fn input(row: &[&str], start: usize) -> MobEffectInstance {
    instance(
        row[start].parse().unwrap(),
        row[start + 1].parse().unwrap(),
        row[start + 2].parse().unwrap(),
    )
}
fn state(value: &MobEffectInstance) -> String {
    format!(
        "{}:{}:{}:{}:{}/{}",
        value.duration(),
        value.amplifier(),
        value.is_ambient(),
        value.is_visible(),
        value.show_icon(),
        value.hidden_effect().map_or_else(|| "-".into(), state)
    )
}

#[test]
fn native_effect_instance_clamps_amplifiers_and_preserves_infinite_durations() {
    let mut checked = 0;
    for row in rows("mob-effect-instance\t") {
        let effect = input(&row, 1);
        assert_eq!(state(&effect), row[4], "{row:?}");
        assert_eq!(effect.ends_within(0).to_string(), row[5], "{row:?}");
        assert_eq!(
            effect.has_remaining_duration().to_string(),
            row[6],
            "{row:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 32);
}

#[test]
fn native_effect_updates_keep_hidden_only_changes_out_of_the_update_callback() {
    let mut checked = 0;
    for row in rows("mob-effect-update\t") {
        let mut effect = input(&row, 1);
        let changed = effect.update(&input(&row, 4));
        assert_eq!(changed.to_string(), row[7], "{row:?}");
        assert_eq!(state(&effect), row[8], "{row:?}");
        checked += 1;
    }
    assert_eq!(checked, 32);
}

#[test]
fn native_hidden_chain_copies_scaling_and_duration_promotion_match() {
    let mut effect = instance(100, 0, 6);
    let mut checked = 0;
    for row in rows("mob-effect-chain\t") {
        let changed = effect.update(&input(&row, 2));
        assert_eq!(changed.to_string(), row[5], "{row:?}");
        assert_eq!(state(&effect), row[6], "{row:?}");
        checked += 1;
    }
    assert_eq!(checked, 5);
    let restored = MobEffectInstance::from_component(effect.to_component()).unwrap();
    assert_eq!(state(&restored), state(&effect));
    let copy = effect.clone();
    let row = rows("mob-effect-copy\t").next().unwrap();
    assert_eq!(state(&copy), row[1]);
    assert_eq!((effect == copy).to_string(), row[2]);
    assert!(copy.hidden_effect().is_none());
    let mut checked = 0;
    for row in rows("mob-effect-scale\t") {
        let scale = f32::from_bits(u32::from_str_radix(row[1], 16).unwrap());
        assert_eq!(
            state(&effect.with_scaled_duration(scale)),
            row[2],
            "{row:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 6);
    let mut ticks = 0;
    for row in rows("mob-effect-duration\t") {
        let target = row[1].parse().unwrap();
        let mut changed = false;
        while ticks < target {
            effect.tick_down_duration();
            changed = effect.downgrade_to_hidden_effect();
            ticks += 1;
        }
        assert_eq!(changed.to_string(), row[2], "{row:?}");
        assert_eq!(state(&effect), row[3], "{row:?}");
    }
    assert_eq!(ticks, 42);
    let mut checked = 0;
    for row in rows("mob-effect-negative\t") {
        let mut effect = instance(row[1].parse().unwrap(), 0, 6);
        effect.tick_down_duration();
        assert_eq!(state(&effect), row[2], "{row:?}");
        assert_eq!(state(&effect.with_scaled_duration(0.5)), row[3], "{row:?}");
        checked += 1;
    }
    assert_eq!(checked, 4);
}

#[test]
fn native_living_load_keeps_valid_partial_effects_and_saves_the_full_hidden_chain() {
    use crate::entity::player::{GameType, ServerPlayer};
    use ferraloom_nbt::{ListTag, Tag};
    use ferraloom_util::{Uuid, Vec3};

    let mut player = ServerPlayer::new(
        Uuid::new(1, 2),
        "effects".into(),
        1,
        Vec3::ZERO,
        GameType::Survival,
    );
    let mut checked = 0;
    for row in rows("mob-effect-load\t") {
        player
            .try_load(ferraloom_nbt::parse_compound(row[2]).unwrap())
            .unwrap();
        let mut effects: Vec<_> = player.active_effects.values().collect();
        // Observations sort registry keys; native holder iteration is not parity.
        effects.sort_by_key(|effect| effect.effect().name());
        let actual = effects
            .iter()
            .map(|effect| format!("{}={}", effect.effect().name(), state(effect)))
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(actual, row[3], "{row:?}");
        for effect in &effects {
            assert!(player.has_effect(effect.effect()));
            assert_eq!(player.get_effect(effect.effect()).unwrap(), *effect);
        }
        let saved = player.save_without_id();
        let mut saved_effects = saved
            .get_list("active_effects")
            .cloned()
            .unwrap_or_default()
            .0;
        saved_effects.sort_by_key(|effect| {
            effect
                .as_compound()
                .unwrap()
                .get_string("id")
                .unwrap()
                .to_string_lossy()
        });
        assert_eq!(
            Tag::List(ListTag(saved_effects)),
            ferraloom_nbt::parse(row[4]).unwrap(),
            "{row:?}"
        );
        assert_eq!(saved.contains_key("active_effects"), !effects.is_empty());
        checked += 1;
    }
    assert_eq!(checked, 8);
}
