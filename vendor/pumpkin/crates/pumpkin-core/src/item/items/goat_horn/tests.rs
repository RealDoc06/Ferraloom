//! Source-authored protocol-777 contracts. These do not claim paired Java proof.
use super::*;
use crate::{
    data::player_server::test::{close_fixture, player_fixture, stop_chunk_workers},
    entity::EntityBase,
    plugin::{
        EventHandler,
        api::events::{
            EventPriority,
            player::{
                player_interact_event::PlayerInteractEvent,
                player_item_consume::PlayerItemConsumeEvent,
            },
            world::generic_game::GenericGameEvent,
        },
    },
    server::Server,
};
use futures::future::BoxFuture;
use pumpkin_data::{
    data_component::DataComponent,
    data_component_impl::{CustomDataImpl, UseCooldownImpl},
    item_stack::ItemStack,
    packet::CURRENT_MC_VERSION,
    sound::Sound,
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::{
    RawPacket, VarInt,
    java::{
        client::play::{CItemCooldown, CKeepAlive, CSoundEffect},
        packet_decoder::TCPNetworkDecoder,
        server::play::SUseItem,
    },
    packet::MultiVersionJavaPacket,
    ser::NetworkReadExt,
};
use pumpkin_util::{GameMode, math::vector3::Vector3};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::AsyncRead;

#[derive(Default)]
struct UseEvents {
    cancel_consume: AtomicBool,
    cancel_interact: AtomicBool,
    instrument_events: AtomicUsize,
}

impl EventHandler<PlayerItemConsumeEvent> for UseEvents {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut PlayerItemConsumeEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            event.cancelled = self.cancel_consume.load(Ordering::Relaxed);
        })
    }
}

impl EventHandler<PlayerInteractEvent> for UseEvents {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut PlayerInteractEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            event.cancelled = self.cancel_interact.load(Ordering::Relaxed);
        })
    }
}

impl EventHandler<GenericGameEvent> for UseEvents {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut GenericGameEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.event_key == "instrument_play" {
                self.instrument_events.fetch_add(1, Ordering::Relaxed);
            }
        })
    }
}

fn use_packet(player: &Arc<Player>, server: &Arc<Server>, hand: Hand) {
    player.client.java().unwrap().handle_use_item(
        player,
        &SUseItem {
            hand: VarInt(if hand == Hand::Right { 0 } else { 1 }),
            sequence: VarInt(1),
            yaw: 0.0,
            pitch: -90.0,
        },
        server,
    );
}

/// FIFO marker bounds absence assertions without sleeps or a server tick.
async fn observe<R: AsyncRead + Unpin>(
    reader: &mut TCPNetworkDecoder<R>,
    player: &Player,
) -> Vec<RawPacket> {
    player.try_send_client_packet(&CKeepAlive::new(739_140));
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut observed = Vec::new();
        for _ in 0..128 {
            let packet = reader.get_raw_packet().await.unwrap();
            if packet.id == CKeepAlive::to_id(CURRENT_MC_VERSION) {
                assert_eq!((&packet.payload[..]).get_i64_be().unwrap(), 739_140);
                return observed;
            }
            observed.push(packet);
        }
        panic!("horn packet marker did not arrive within the bounded batch");
    })
    .await
    .unwrap()
}

fn assert_effect_packets(packets: &[RawPacket], sound: Sound, group: &str) {
    let sounds: Vec<_> = packets
        .iter()
        .filter(|packet| packet.id == CSoundEffect::to_id(CURRENT_MC_VERSION))
        .collect();
    assert_eq!(sounds.len(), 1);
    let mut sound_packet = &sounds[0].payload[..];
    assert_eq!(sound_packet.get_var_int().unwrap().0, sound as i32 + 1);
    assert_eq!(
        sound_packet.get_var_int().unwrap().0,
        SoundCategory::Records as i32
    );
    assert_eq!(
        [
            sound_packet.get_i32_be().unwrap(),
            sound_packet.get_i32_be().unwrap(),
            sound_packet.get_i32_be().unwrap()
        ],
        [4, 520, 4]
    );
    assert_eq!(sound_packet.get_f32_be().unwrap(), 16.0);
    assert_eq!(sound_packet.get_f32_be().unwrap(), 1.0);
    sound_packet.get_i64_be().unwrap();
    assert!(sound_packet.is_empty());
    let cooldowns: Vec<_> = packets
        .iter()
        .filter(|packet| packet.id == CItemCooldown::to_id(CURRENT_MC_VERSION))
        .collect();
    assert_eq!(cooldowns.len(), 1);
    let mut cooldown = &cooldowns[0].payload[..];
    assert_eq!(cooldown.get_str().unwrap().as_ref(), group);
    assert_eq!(cooldown.get_var_int().unwrap().0, 140);
    assert!(cooldown.is_empty());
}

fn assert_no_effect_packets(packets: &[RawPacket]) {
    assert!(packets.iter().all(
        |packet| packet.id != CSoundEffect::to_id(CURRENT_MC_VERSION)
            && packet.id != CItemCooldown::to_id(CURRENT_MC_VERSION)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(
    clippy::too_many_lines,
    reason = "Exercise the actual hand/packet/stat/cooldown path in one stopped-worker fixture."
)]
async fn builtin_horns_use_actual_hand_and_gate_all_effects_on_success() {
    let mut fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    let player = fixture.player.clone();
    let server = fixture.server.clone();
    player.get_entity().set_pos(Vector3::new(0.5, 65.0, 0.5));
    player.set_client_loaded(true);
    fixture
        .client
        .java()
        .unwrap()
        .version
        .store(CURRENT_MC_VERSION);
    let events = Arc::new(UseEvents::default());
    fixture
        .server
        .plugin_manager
        .register::<PlayerItemConsumeEvent, _>(events.clone(), EventPriority::Normal, true);
    fixture
        .server
        .plugin_manager
        .register::<PlayerInteractEvent, _>(events.clone(), EventPriority::Normal, true);
    fixture
        .server
        .plugin_manager
        .register::<GenericGameEvent, _>(events.clone(), EventPriority::Normal, true);
    let used = || player.get_stat(StatisticCategory::Used, i32::from(Item::GOAT_HORN.id));
    let (remote, _write) = fixture._remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;

    // Expected sound assignments come from Instruments.bootstrap, not sound().
    for (instrument, sound) in [
        (Instrument::PonderGoatHorn, Sound::ItemGoatHornSound0),
        (Instrument::SingGoatHorn, Sound::ItemGoatHornSound1),
        (Instrument::SeekGoatHorn, Sound::ItemGoatHornSound2),
        (Instrument::FeelGoatHorn, Sound::ItemGoatHornSound3),
        (Instrument::AdmireGoatHorn, Sound::ItemGoatHornSound4),
        (Instrument::CallGoatHorn, Sound::ItemGoatHornSound5),
        (Instrument::YearnGoatHorn, Sound::ItemGoatHornSound6),
        (Instrument::DreamGoatHorn, Sound::ItemGoatHornSound7),
    ] {
        for hand in [Hand::Right, Hand::Left] {
            player.tick_counter.fetch_add(140, Ordering::Relaxed);
            let mut stack = ItemStack::new(1, &Item::GOAT_HORN);
            // Also exercise the real generated default for Ponder.
            if instrument != Instrument::PonderGoatHorn {
                stack.set_data_component(InstrumentImpl { instrument });
            }
            let mut custom = NbtCompound::new();
            custom.put_string("instrument", "minecraft:sing_goat_horn".into());
            custom.put_string("Instrument", "minecraft:ponder_goat_horn".into());
            stack.set_data_component(CustomDataImpl::new(custom));
            let other = if hand == Hand::Right {
                Hand::Left
            } else {
                Hand::Right
            };
            player
                .inventory()
                .set_stack_in_hand(other, ItemStack::new(1, &Item::STICK));
            player.inventory().set_stack_in_hand(hand, stack.clone());
            let before = used();
            let event_count = events.instrument_events.load(Ordering::Relaxed);
            use_packet(&player, &server, hand);
            assert_eq!(used(), before + 1);
            assert_eq!(
                events.instrument_events.load(Ordering::Relaxed),
                event_count + 1
            );
            assert_eq!(
                *player.living_entity.active_hand.lock().unwrap(),
                Some(hand)
            );
            assert_eq!(
                player.living_entity.item_use_time.load(Ordering::Relaxed),
                140
            );
            assert!(
                player
                    .living_entity
                    .item_in_use
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .are_items_and_components_equal(&stack)
            );
            assert!(
                player
                    .inventory()
                    .get_stack_in_hand(hand)
                    .are_items_and_components_equal(&stack)
            );
            assert_effect_packets(
                &observe(&mut reader, &player).await,
                sound,
                "minecraft:goat_horn",
            );
            player.tick_counter.fetch_add(139, Ordering::Relaxed);
            use_packet(&player, &server, hand);
            assert_eq!(used(), before + 1);
            assert_eq!(
                events.instrument_events.load(Ordering::Relaxed),
                event_count + 1
            );
            assert_no_effect_packets(&observe(&mut reader, &player).await);
        }
    }
    let mut stack = ItemStack::new(1, &Item::GOAT_HORN);
    stack.set_data_component(InstrumentImpl {
        instrument: Instrument::SingGoatHorn,
    });
    stack.set_data_component(UseCooldownImpl::new(0.05, Some("test:horns".into())));
    player
        .inventory()
        .set_stack_in_hand(Hand::Left, stack.clone());
    use_packet(&player, &server, Hand::Left);
    assert_effect_packets(
        &observe(&mut reader, &player).await,
        Sound::ItemGoatHornSound1,
        "test:horns",
    );
    player.tick_counter.fetch_add(139, Ordering::Relaxed);
    assert!(player.is_on_cooldown("test:horns"));
    player.tick_counter.fetch_add(1, Ordering::Relaxed);
    assert!(!player.is_on_cooldown("test:horns"));
    let before_reuse = used();
    use_packet(&player, &server, Hand::Left);
    assert_eq!(used(), before_reuse + 1);
    assert_effect_packets(
        &observe(&mut reader, &player).await,
        Sound::ItemGoatHornSound1,
        "test:horns",
    );

    for failure in [
        "removed",
        "spectator",
        "consume_cancelled",
        "interact_cancelled",
    ] {
        player.tick_counter.fetch_add(140, Ordering::Relaxed);
        player.living_entity.clear_active_hand();
        let mut candidate = stack.clone();
        if failure == "removed" {
            candidate.remove_data_component(DataComponent::Instrument);
        }
        player
            .inventory()
            .set_stack_in_hand(Hand::Left, candidate.clone());
        player.gamemode.store(if failure == "spectator" {
            GameMode::Spectator
        } else {
            GameMode::Survival
        });
        events
            .cancel_consume
            .store(failure == "consume_cancelled", Ordering::Relaxed);
        events
            .cancel_interact
            .store(failure == "interact_cancelled", Ordering::Relaxed);
        observe(&mut reader, &player).await;
        let before = used();
        let event_count = events.instrument_events.load(Ordering::Relaxed);
        use_packet(&player, &server, Hand::Left);
        assert_eq!(used(), before);
        assert_eq!(
            events.instrument_events.load(Ordering::Relaxed),
            event_count
        );
        assert_eq!(*player.living_entity.active_hand.lock().unwrap(), None);
        assert_eq!(
            player.living_entity.item_use_time.load(Ordering::Relaxed),
            0
        );
        assert!(!player.is_on_cooldown("test:horns"));
        assert!(
            player
                .inventory()
                .get_stack_in_hand(Hand::Left)
                .are_items_and_components_equal(&candidate)
        );
        assert_no_effect_packets(&observe(&mut reader, &player).await);
    }
    player.gamemode.store(GameMode::Survival);
    drop(reader);
    close_fixture(&fixture).await;
}
