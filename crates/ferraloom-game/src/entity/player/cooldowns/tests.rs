use super::*;
use crate::entity::{EntityState, player::GameType};
use ferraloom_protocol::component::{DataComponentPatch, UseCooldown};
use ferraloom_util::{Uuid, Vec3};

fn stacks() -> Vec<ItemStack> {
    let shared = DataComponentPatch {
        added: vec![DataComponent::UseCooldown(UseCooldown {
            seconds: 1.0.into(),
            cooldown_group: Some(Identifier::parse("minecraft:shared").unwrap()),
        })],
        removed: Vec::new(),
    };
    let stone = ItemStack::new(Item::STONE, 1, Default::default());
    let shield = ItemStack::new(Item::SHIELD, 1, shared.clone());
    let dirt = ItemStack::new(Item::DIRT, 1, shared);
    let mut empty = shield.clone();
    inventory::set_count(&mut empty, 0);
    vec![stone, shield, dirt, empty]
}
fn snapshot(name: &str, player: EntityRef, stacks: &[ItemStack], level: &mut Level) -> String {
    let player_data = level.entity::<ServerPlayer>(player).unwrap();
    let values: Vec<_> = stacks
        .iter()
        .flat_map(|stack| {
            [0.0, 0.5, -0.0, f32::NAN, f32::INFINITY].map(|partial| {
                let value = player_data.cooldowns.get_cooldown_percent(stack, partial);
                if value.is_nan() {
                    "nan".into()
                } else {
                    format!("{:x}", value.to_bits())
                }
            })
        })
        .collect();
    let calls: Vec<_> = level
        .take_effects()
        .into_iter()
        .filter_map(|effect| {
            let LevelEffect::PlayerPacket { recipients, packet } = effect else {
                return None;
            };
            assert_eq!(recipients, [player]);
            let cb::Packet::Cooldown(packet) = *packet else {
                panic!("unexpected packet")
            };
            Some(format!(
                "{}:{}",
                packet.cooldown_group.as_str(),
                packet.duration.0
            ))
        })
        .collect();
    format!(
        "cooldown\t{name}\t{}\t{}",
        values.join(","),
        if calls.is_empty() {
            "-".into()
        } else {
            calls.join("|")
        }
    )
}
#[test]
fn native_cooldown_groups_float_percent_wrapping_and_ordered_expiry_packets() {
    let mut level = crate::entity::tests::level(false);
    let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 91);
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "cooldowns".into(),
                1,
                Vec3::new(8.5, 64.0, 8.5),
                GameType::Survival,
            ))),
        )
        .unwrap();
    level.take_effects();
    let stacks = stacks();
    let mut actual: Vec<_> = stacks
        .iter()
        .enumerate()
        .map(|(i, stack)| {
            format!(
                "cooldown-group\t{i}\t{}",
                ItemCooldowns::get_cooldown_group(stack).as_str()
            )
        })
        .collect();
    actual.push(snapshot("fresh", player, &stacks, &mut level));
    add_cooldown(player, &stacks[0], 3, &mut level).unwrap();
    add_cooldown(player, &stacks[1], 2, &mut level).unwrap();
    actual.push(snapshot("added", player, &stacks, &mut level));
    tick(player, &mut level).unwrap();
    actual.push(snapshot("tick1", player, &stacks, &mut level));
    add_cooldown(player, &stacks[2], 4, &mut level).unwrap();
    actual.push(snapshot("replace", player, &stacks, &mut level));
    tick(player, &mut level).unwrap();
    actual.push(snapshot("tick2", player, &stacks, &mut level));
    tick(player, &mut level).unwrap();
    actual.push(snapshot("tick3", player, &stacks, &mut level));
    let group = Identifier::parse("minecraft:shared").unwrap();
    remove_cooldown(player, group.clone(), &mut level).unwrap();
    remove_cooldown(player, group, &mut level).unwrap();
    actual.push(snapshot("remove-absent", player, &stacks, &mut level));
    add_cooldown(player, &stacks[0], 0, &mut level).unwrap();
    add_cooldown(player, &stacks[1], -1, &mut level).unwrap();
    actual.push(snapshot("nonpositive", player, &stacks, &mut level));
    tick(player, &mut level).unwrap();
    actual.push(snapshot("nonpositive-expire", player, &stacks, &mut level));
    for i in 0..24 {
        add_cooldown_group(
            player,
            Identifier::parse(&format!("minecraft:group_{i}")).unwrap(),
            1,
            &mut level,
        )
        .unwrap();
    }
    actual.push(snapshot("rehash", player, &stacks, &mut level));
    tick(player, &mut level).unwrap();
    actual.push(snapshot("iterator-expire", player, &stacks, &mut level));
    level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .cooldowns
        .parent
        .tick_count = i32::MAX - 1;
    add_cooldown(player, &stacks[0], 2, &mut level).unwrap();
    actual.push(snapshot("overflow-add", player, &stacks, &mut level));
    tick(player, &mut level).unwrap();
    actual.push(snapshot("overflow-end", player, &stacks, &mut level));
    add_cooldown(player, &stacks[1], 2, &mut level).unwrap();
    tick(player, &mut level).unwrap();
    actual.push(snapshot("wrap-tick", player, &stacks, &mut level));
    let expected: Vec<_> = include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|line| line.starts_with("cooldown\t") || line.starts_with("cooldown-group\t"))
        .map(str::to_owned)
        .collect();
    // The Java generator prints group rows before its first snapshot.
    assert_eq!(actual, expected);
}
#[test]
fn player_tick_expires_cooldowns_and_runtime_state_is_not_persisted() {
    let mut level = crate::entity::tests::level(false);
    level.install_shared(Box::default());
    let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 92);
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "tick".into(),
                1,
                Vec3::new(8.5, 64.0, 8.5),
                GameType::Creative,
            ))),
        )
        .unwrap();
    let stack = &stacks()[0];
    add_cooldown(player, stack, 1, &mut level).unwrap();
    assert!(
        level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .cooldowns
            .is_on_cooldown(stack)
    );
    level.take_effects();
    // ServerPlayer's entity-phase tick owns attributes, while the connection's
    // doTick owns Player's cooldown tick. Keep both native slots explicit.
    level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .update_interaction_range_attributes();
    super::super::player_do_tick_with_tick_rate(
        player,
        &mut level,
        &crate::level::tick_rate::TickRateManager::default(),
    )
    .unwrap();
    let player_data = level.entity::<ServerPlayer>(player).unwrap();
    assert!(!player_data.cooldowns.is_on_cooldown(stack));
    assert_eq!(player_data.block_interaction_range(), 5.0);
    assert_eq!(player_data.entity_interaction_range(), 5.0);
    let saved = player_data.save();
    assert!(!saved.contains_key("cooldowns"));
    assert!(!saved.contains_key("lastClientInput"));
    assert!(!saved.contains_key("textFilteringEnabled"));
    assert!(level.take_effects().iter().any(|effect| matches!(effect,
        LevelEffect::PlayerPacket { recipients, packet } if recipients == &[player] && matches!(packet.as_ref(), cb::Packet::Cooldown(p) if p.duration.0 == 0)
    )));
}
