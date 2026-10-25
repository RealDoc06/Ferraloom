use super::*;
use crate::{
    entity::{
        EntityState,
        player::{GameType, ServerPlayer, swing_and_reset_attack_strength},
    },
    level::effects::LevelEffect,
};
use ferraloom_data::Item;
use ferraloom_protocol::{
    Wire, Writer,
    chat::Component,
    component::{DataComponent, DataComponentPatch, ItemStack, SwingAnimationType},
};
use ferraloom_util::{ChunkPos, Uuid, Vec3};
const VECTORS: &str = include_str!("../../../../tests/java/entity/vectors.tsv");
fn hand(id: &str) -> InteractionHand {
    if id == "0" {
        InteractionHand::MainHand
    } else {
        InteractionHand::OffHand
    }
}
fn animation(id: &str, duration: i32) -> SwingAnimation {
    SwingAnimation {
        animation_type: match id {
            "0" => SwingAnimationType::None,
            "1" => SwingAnimationType::Whack,
            "2" => SwingAnimationType::Stab,
            _ => panic!("native animation"),
        },
        duration: VarInt(duration),
    }
}
fn snapshot(state: &SwingState) -> String {
    let current = state.current_swing().map_or_else(
        || "none".to_owned(),
        |swing| {
            format!(
                "{},{},{},{}",
                if swing.hand == InteractionHand::MainHand {
                    0
                } else {
                    1
                },
                swing.animation.animation_type as i32,
                swing.animation.duration.0,
                swing.duration_ticks
            )
        },
    );
    format!(
        "{current}:{}:{:x}:{:x}",
        state.ticks,
        state.old_animation.to_bits(),
        state.animation.to_bits()
    )
}
#[test]
fn native_swing_state_preserves_half_duration_restart_progress_and_interpolation() {
    let mut state = SwingState::default();
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("swing-state\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        if f[2] == "0" {
            state = SwingState::default();
        }
        let result = if f[3] == "start" {
            state
                .start_if_able(
                    hand(f[4]),
                    animation(f[5], f[1].parse().unwrap()),
                    f[1].parse().unwrap(),
                )
                .to_string()
        } else {
            state.tick();
            "-".into()
        };
        let progress = [-0.25, 0.0, 0.5, 1.5]
            .map(|delta| format!("{:x}", state.get_animation(delta).to_bits()))
            .join(",");
        assert_eq!(
            format!("{result}\t{}\t{progress}", snapshot(&state)),
            f[6..].join("\t"),
            "{row}"
        );
        assert_eq!(state.is_swinging(), state.current_swing().is_some());
        count += 1;
    }
    assert_eq!(count, 128);
}
fn player(level: &mut Level, network: i32) -> EntityRef {
    let id = level.next_entity_ref();
    let state = ServerPlayer::new(
        Uuid::new(0, id.serial() as i64),
        format!("swing{network}"),
        network,
        Vec3::new(8.5, 64.0, 8.5),
        GameType::Survival,
    );
    level
        .add_entity(id, EntityState::Player(Box::new(state)))
        .unwrap();
    id
}
fn effects(entity: EntityRef, config: i32, level: &mut Level) {
    use crate::entity::effect::MobEffectInstance;
    use ferraloom_protocol::component::Persistent;
    let mut entries = Vec::new();
    if [1, 2, 3, 5].contains(&config) {
        entries.push((
            MobEffect::HASTE,
            if config == 2 {
                255
            } else if config == 5 {
                1
            } else {
                0
            },
        ));
    }
    if config == 3 || config == 6 {
        entries.push((MobEffect::CONDUIT_POWER, if config == 3 { 2 } else { 0 }));
    }
    if config >= 4 {
        entries.push((
            MobEffect::MINING_FATIGUE,
            if config == 7 {
                255
            } else if config == 5 {
                5
            } else {
                2
            },
        ));
    }
    let tag = ferraloom_nbt::Tag::List(ferraloom_nbt::ListTag(
        entries
            .into_iter()
            .map(|(effect, amplifier)| {
                MobEffectInstance::new(effect, 100, amplifier, false, true, true, None)
                    .to_component()
                    .to_nbt()
                    .unwrap()
            })
            .collect(),
    ));
    assert!(
        level
            .entity_mut::<LivingEntity>(entity)
            .unwrap()
            .active_effects
            .load(Some(&tag))
            .is_empty()
    );
}
#[test]
fn native_server_player_swing_uses_effect_duration_original_packet_and_conditional_reset() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("swing-player\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let mut level = crate::entity::tests::level(false);
        level.set_unported_behavior(crate::block::UnportedBehavior::Strict);
        let target = player(&mut level, 77);
        let viewer = player(&mut level, 88);
        let v = level.entity_mut::<ServerPlayer>(viewer).unwrap();
        v.chunk_tracking_view.insert(ChunkPos::ZERO);
        v.chunk_sender.pending.remove(&ChunkPos::ZERO);
        level.update_tracked_player(77, viewer).unwrap();
        effects(target, f[2].parse().unwrap(), &mut level);
        swing(
            target,
            InteractionHand::MainHand,
            animation("1", 6),
            false,
            &mut level,
        )
        .unwrap();
        let state = level.entity_mut::<LivingEntity>(target).unwrap();
        for _ in 0..f[3].parse::<i32>().unwrap() {
            state.swing_state.tick();
        }
        let index: i32 = f[1].parse().unwrap();
        state.attack_strength_ticker = 17 + index;
        state.item_swap_ticker = 23 + index;
        level.take_effects();
        swing_and_reset_attack_strength(
            target,
            hand(f[4]),
            animation(f[5], f[6].parse().unwrap()),
            f[7] == "true",
            &mut level,
        )
        .unwrap();
        let state = level.entity::<ServerPlayer>(target).unwrap();
        let actual_state = format!(
            "{}\t{},{}",
            snapshot(&state.swing_state),
            state.attack_strength_ticker,
            state.item_swap_ticker
        );
        let packets: Vec<_> = level
            .take_effects()
            .into_iter()
            .map(|effect| {
                let LevelEffect::PlayerPacket { recipients, packet } = effect else {
                    panic!("swing effect: {effect:?}")
                };
                let cb::Packet::SwingAnimation(packet) = *packet else {
                    panic!("swing packet")
                };
                let audience = if recipients == [viewer, target] {
                    "self"
                } else {
                    assert_eq!(recipients, [viewer]);
                    "players"
                };
                let mut bytes = Writer::new();
                packet.encode(&mut bytes).unwrap();
                format!(
                    "{audience}:{}",
                    bytes
                        .as_slice()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                )
            })
            .collect();
        assert_eq!(
            format!(
                "{actual_state}\t{}",
                if packets.is_empty() {
                    "-".into()
                } else {
                    packets.join(";")
                }
            ),
            f[8..].join("\t"),
            "{row}"
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        count += 1;
    }
    assert_eq!(count, 48);
}
#[test]
fn native_player_attack_clock_wraps_and_only_item_replacement_resets_both_tickers() {
    let mut count = 0;
    for row in VECTORS
        .lines()
        .filter(|line| line.starts_with("swing-clock\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let i: i32 = f[1].parse().unwrap();
        let mut level = crate::entity::tests::level(false);
        let id = player(&mut level, 77);
        let player = level.entity_mut::<ServerPlayer>(id).unwrap();
        player.last_item_in_main_hand = if i == 0 {
            ItemStack::default()
        } else {
            ItemStack::new(Item::STONE, 3, Default::default())
        };
        let mut current = if i < 2 {
            ItemStack::default()
        } else {
            ItemStack::new(
                if i == 2 { Item::DIRT } else { Item::STONE },
                if i == 3 { 5 } else { 3 },
                DataComponentPatch::default(),
            )
        };
        if i == 4 {
            current
                .0
                .as_mut()
                .unwrap()
                .components
                .set(DataComponent::CustomName(Box::new(Component::literal(
                    "changed",
                ))));
        }
        if i == 5 {
            current.0.as_mut().unwrap().count = 0;
        }
        if i == 6 {
            current.0.as_mut().unwrap().count = -1;
        }
        player.inventory.set_selected_item(current);
        player.attack_strength_ticker = if i == 7 { i32::MAX } else { 17 };
        player.item_swap_ticker = if i == 7 { i32::MAX } else { 23 };
        player.tick_attack_strength();
        assert_eq!(
            format!(
                "{},{}\t{},{}",
                player.attack_strength_ticker,
                player.item_swap_ticker,
                player.last_item_in_main_hand.item().name(),
                crate::entity::player::inventory::count(&player.last_item_in_main_hand)
            ),
            f[2..].join("\t"),
            "{row}"
        );
        count += 1;
    }
    assert_eq!(count, 8);
}
