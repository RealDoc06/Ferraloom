//! Source-authored protocol-777 contracts. These do not claim paired Java proof.
use super::*;
use crate::{
    data::player_server::test::{close_fixture, java_client, player_fixture, stop_chunk_workers},
    entity::EntityBase,
    net::{GameProfile, PlayerConfig},
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
    world::World,
};
use arc_swap::ArcSwap;
use futures::future::BoxFuture;
use pumpkin_data::{
    Block,
    advancement::Advancement,
    data_component::DataComponent,
    data_component_impl::{
        ConsumableImpl, ConsumeEffect, CustomDataImpl, FoodImpl, StatusEffectInstance,
        UseCooldownImpl,
    },
    item_stack::ItemStack,
    packet::CURRENT_MC_VERSION,
    sound::{Sound, SoundCategory},
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::{
    RawPacket, VarInt,
    java::{
        client::play::{CEntitySoundEffect, CItemCooldown, CKeepAlive, CSoundEffect},
        packet_decoder::TCPNetworkDecoder,
        server::play::{SUseItem, SUseItemOn},
    },
    packet::MultiVersionJavaPacket,
    ser::NetworkReadExt,
};
use pumpkin_util::{
    GameMode,
    math::{position::BlockPos, vector2::Vector2, vector3::Vector3},
};
use std::{
    borrow::Cow,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::AsyncRead;
use tokio::net::TcpStream;
use uuid::Uuid;

struct SoundObserver {
    player: Arc<Player>,
    reader: TCPNetworkDecoder<TcpStream>,
}

impl SoundObserver {
    async fn new(server: &Arc<Server>, world: &Arc<World>) -> Self {
        let profile = GameProfile {
            id: Uuid::new_v4(),
            name: "HornObserver".into(),
            properties: ArcSwap::from_pointee(Vec::new()),
            profile_actions: None,
        };
        let (client, remote) = java_client(server, &profile).await;
        client.java().unwrap().version.store(CURRENT_MC_VERSION);
        let player = Arc::new(Player::new(
            client,
            profile,
            PlayerConfig::default(),
            world,
            GameMode::Survival,
        ));
        player.get_entity().set_pos(Vector3::new(8.5, 65.0, 0.5));
        player.set_client_loaded(true);
        world.add_player(&player).unwrap();
        Self {
            player,
            reader: TCPNetworkDecoder::new(remote),
        }
    }

    async fn close(self) {
        let client = self.player.client.clone();
        drop(self);
        tokio::time::timeout(Duration::from_secs(5), client.java().unwrap().await_tasks())
            .await
            .unwrap();
    }
}

impl Drop for SoundObserver {
    fn drop(&mut self) {
        let world = self.player.world();
        world.players.rcu(|players| {
            players
                .iter()
                .filter(|player| !Arc::ptr_eq(player, &self.player))
                .cloned()
                .collect::<Vec<_>>()
        });
        world
            .entity_tracker
            .remove_entity(self.player.as_ref(), &world);
        self.player.client.java().unwrap().close();
    }
}

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
            hand: VarInt(i32::from(hand != Hand::Right)),
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

fn assert_entity_sound_packets(packets: &[RawPacket], sound: Sound, source_entity_id: i32) {
    assert_eq!(CEntitySoundEffect::to_id(CURRENT_MC_VERSION), 118);
    assert_eq!(CSoundEffect::to_id(CURRENT_MC_VERSION), 119);
    assert!(packets.iter().all(|packet| {
        packet.id != CSoundEffect::to_id(CURRENT_MC_VERSION)
            && packet.id != CItemCooldown::to_id(CURRENT_MC_VERSION)
    }));
    let sounds: Vec<_> = packets
        .iter()
        .filter(|packet| packet.id == CEntitySoundEffect::to_id(CURRENT_MC_VERSION))
        .collect();
    assert_eq!(sounds.len(), 1);
    let mut sound_packet = &sounds[0].payload[..];
    assert_eq!(sound_packet.get_var_int().unwrap().0, sound as i32 + 1);
    assert_eq!(
        sound_packet.get_var_int().unwrap().0,
        SoundCategory::Records as i32
    );
    assert_eq!(sound_packet.get_var_int().unwrap().0, source_entity_id);
    assert_eq!(sound_packet.get_f32_be().unwrap(), 16.0);
    assert_eq!(sound_packet.get_f32_be().unwrap(), 1.0);
    sound_packet.get_i64_be().unwrap();
    assert!(sound_packet.is_empty());
}

fn assert_cooldown_packets(packets: &[RawPacket], group: &str, ticks: i32) {
    assert_eq!(CItemCooldown::to_id(CURRENT_MC_VERSION), 22);
    assert!(packets.iter().all(|packet| {
        packet.id != CEntitySoundEffect::to_id(CURRENT_MC_VERSION)
            && packet.id != CSoundEffect::to_id(CURRENT_MC_VERSION)
    }));
    let cooldowns: Vec<_> = packets
        .iter()
        .filter(|packet| packet.id == CItemCooldown::to_id(CURRENT_MC_VERSION))
        .collect();
    assert_eq!(cooldowns.len(), 1);
    let mut cooldown = &cooldowns[0].payload[..];
    assert_eq!(cooldown.get_str().unwrap().as_ref(), group);
    assert_eq!(cooldown.get_var_int().unwrap().0, ticks);
    assert!(cooldown.is_empty());
}

fn assert_effect_packets(
    performer_packets: &[RawPacket],
    observer_packets: &[RawPacket],
    source_entity_id: i32,
    sound: Sound,
    group: &str,
) {
    assert_cooldown_packets(performer_packets, group, 140);
    assert_entity_sound_packets(observer_packets, sound, source_entity_id);
}

fn assert_no_effect_packets(packets: &[RawPacket]) {
    assert!(packets.iter().all(|packet| packet.id
        != CEntitySoundEffect::to_id(CURRENT_MC_VERSION)
        && packet.id != CSoundEffect::to_id(CURRENT_MC_VERSION)
        && packet.id != CItemCooldown::to_id(CURRENT_MC_VERSION)));
}

fn consumable_with_speed_effect() -> ConsumableImpl {
    let mut consumable = ItemStack::new(1, &Item::APPLE)
        .get_data_component::<ConsumableImpl>()
        .unwrap()
        .clone();
    consumable.effects = Cow::Owned(vec![ConsumeEffect::ApplyEffects((
        Cow::Owned(vec![StatusEffectInstance {
            effect_id: Cow::Borrowed("minecraft:speed"),
            amplifier: 0,
            duration: 200,
            ambient: false,
            show_particles: false,
            show_icon: false,
        }]),
        1.0,
    ))]);
    consumable
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
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    let mut observer = SoundObserver::new(&server, &player.world()).await;
    observe(&mut reader, &player).await;
    observe(&mut observer.reader, &observer.player).await;

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
                &observe(&mut observer.reader, &observer.player).await,
                player.entity_id(),
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
            assert_no_effect_packets(&observe(&mut observer.reader, &observer.player).await);
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
        &observe(&mut observer.reader, &observer.player).await,
        player.entity_id(),
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
        &observe(&mut observer.reader, &observer.player).await,
        player.entity_id(),
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
        observe(&mut observer.reader, &observer.player).await;
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
        assert_no_effect_packets(&observe(&mut observer.reader, &observer.player).await);
    }
    player.gamemode.store(GameMode::Survival);
    events.cancel_consume.store(false, Ordering::Relaxed);
    events.cancel_interact.store(false, Ordering::Relaxed);
    let world = player.world();
    world
        .level
        .loaded_chunks
        .get(&Vector2::new(0, 0))
        .unwrap()
        .set_block_absolute_y(0, 64, 0, Block::STONE.default_state.id);
    for hand in [Hand::Right, Hand::Left] {
        for on_block in [true, false] {
            player.tick_counter.fetch_add(140, Ordering::Relaxed);
            player.living_entity.clear_active_hand();
            player.inventory().set_stack_in_hand(hand, stack.clone());
            observe(&mut reader, &player).await;
            observe(&mut observer.reader, &observer.player).await;
            let before = used();
            let event_count = events.instrument_events.load(Ordering::Relaxed);
            if on_block {
                player
                    .client
                    .java()
                    .unwrap()
                    .handle_use_item_on(
                        &player,
                        &SUseItemOn {
                            hand: VarInt(i32::from(hand != Hand::Right)),
                            position: BlockPos::new(0, 64, 0),
                            face: VarInt(1),
                            cursor_pos: Vector3::new(0.5, 1.0, 0.5),
                            inside_block: false,
                            is_against_world_border: false,
                            sequence: VarInt(2),
                        },
                        &server,
                    )
                    .unwrap();
            } else {
                let mut candidate = player.inventory().get_stack_in_hand(hand);
                server
                    .item_registry
                    .use_on_entity(&mut candidate, &player, player.clone());
                assert!(candidate.are_items_and_components_equal(&stack));
            }
            assert_eq!(used(), before);
            assert_eq!(
                events.instrument_events.load(Ordering::Relaxed),
                event_count
            );
            assert!(!player.is_on_cooldown("test:horns"));
            assert_eq!(*player.living_entity.active_hand.lock().unwrap(), None);
            assert_no_effect_packets(&observe(&mut reader, &player).await);
            assert_no_effect_packets(&observe(&mut observer.reader, &observer.player).await);
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
            assert_effect_packets(
                &observe(&mut reader, &player).await,
                &observe(&mut observer.reader, &observer.player).await,
                player.entity_id(),
                Sound::ItemGoatHornSound1,
                "test:horns",
            );
        }
    }
    drop(reader);
    observer.close().await;
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(
    clippy::too_many_lines,
    reason = "Follow real Player/LivingEntity ticks through horn expiry and a food positive control."
)]
async fn horn_expiry_ticks_apply_only_explicit_finish_cooldown() {
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
    player.world().level_info.rcu(|current| {
        let mut info = (**current).clone();
        info.game_rules.natural_health_regeneration = false;
        info
    });
    player.hunger_manager.level.store(10);
    player.hunger_manager.saturation.store(0.0);
    let events = Arc::new(UseEvents::default());
    server.plugin_manager.register::<GenericGameEvent, _>(
        events.clone(),
        EventPriority::Normal,
        true,
    );
    let used = || player.get_stat(StatisticCategory::Used, i32::from(Item::GOAT_HORN.id));
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    let mut observer = SoundObserver::new(&server, &player.world()).await;
    observe(&mut reader, &player).await;
    observe(&mut observer.reader, &observer.player).await;

    // Food alone does not make a horn consumable. The apple remains a positive
    // control for the generic food/effect/ConsumeItem completion path below.
    let apple = ItemStack::new(2, &Item::APPLE);
    let food = apple.get_data_component::<FoodImpl>().unwrap().clone();
    let consumable = consumable_with_speed_effect();

    for hand in [Hand::Right, Hand::Left] {
        for (finish_cooldown, food_only) in [
            (None, false),
            (
                Some(UseCooldownImpl::new(1.0, Some("test:expiry_horns".into()))),
                false,
            ),
            (None, true),
            (Some(UseCooldownImpl::new(1.0, None)), true),
        ] {
            let group = finish_cooldown
                .as_ref()
                .and_then(|cooldown| cooldown.cooldown_group.as_deref())
                .unwrap_or("minecraft:goat_horn");
            let mut horn = ItemStack::new(1, &Item::GOAT_HORN);
            horn.set_data_component(InstrumentImpl {
                instrument: Instrument::SingGoatHorn,
            });
            if let Some(cooldown) = finish_cooldown.as_ref() {
                // Use begins with 140 instrument ticks. ItemStack reapplies
                // this explicit component as 20 ticks only after completion.
                horn.set_data_component(cooldown.clone());
            }
            if food_only {
                horn.set_data_component(food.clone());
                if finish_cooldown.is_some() {
                    horn.set_data_component(consumable.clone());
                    horn.remove_data_component(DataComponent::Consumable);
                }
            }
            // Equal stacks catch the old completion branch consuming the main
            // hand even when the off hand was active.
            for held_hand in [Hand::Right, Hand::Left] {
                player
                    .inventory()
                    .set_stack_in_hand(held_hand, horn.clone());
            }
            observe(&mut reader, &player).await;
            observe(&mut observer.reader, &observer.player).await;

            // Reuse waits for both use and any explicit finish cooldown. Neither
            // timer nor active-hand state is advanced manually.
            for _ in 0..2 {
                let before = used();
                let event_count = events.instrument_events.load(Ordering::Relaxed);
                let started = player.tick_counter.load(Ordering::Relaxed);
                use_packet(&player, &server, hand);
                assert_eq!(used(), before + 1);
                assert_eq!(
                    *player.living_entity.active_hand.lock().unwrap(),
                    Some(hand)
                );
                assert_eq!(
                    player.living_entity.item_use_time.load(Ordering::Relaxed),
                    140
                );
                assert_effect_packets(
                    &observe(&mut reader, &player).await,
                    &observe(&mut observer.reader, &observer.player).await,
                    player.entity_id(),
                    Sound::ItemGoatHornSound1,
                    group,
                );

                for elapsed in 1..=140 {
                    if elapsed == 140 {
                        // This decrement follows expiry in LivingEntity::tick.
                        // A whole-tick early return would leave it at two.
                        player
                            .living_entity
                            .hurt_cooldown
                            .store(2, Ordering::Relaxed);
                    }
                    player.tick(&server);
                    assert_eq!(
                        player.tick_counter.load(Ordering::Relaxed),
                        started + elapsed
                    );
                    assert_eq!(
                        player.living_entity.item_use_time.load(Ordering::Relaxed),
                        140 - elapsed
                    );
                    assert_eq!(
                        *player.living_entity.active_hand.lock().unwrap(),
                        (elapsed < 140).then_some(hand)
                    );
                    {
                        let active = player.living_entity.item_in_use.lock().unwrap();
                        if elapsed < 140 {
                            assert!(active.as_ref().unwrap().are_equal(&horn));
                        } else {
                            assert!(active.is_none());
                        }
                    }
                    for held_hand in [Hand::Right, Hand::Left] {
                        assert!(
                            player
                                .inventory()
                                .get_stack_in_hand(held_hand)
                                .are_equal(&horn)
                        );
                    }
                    {
                        let cooldowns = player.item_cooldowns.lock().unwrap();
                        let cooldown = cooldowns.get(group).unwrap();
                        let finished_with_cooldown = elapsed == 140 && finish_cooldown.is_some();
                        assert_eq!(
                            cooldown.start_tick,
                            started + if finished_with_cooldown { 140 } else { 0 }
                        );
                        assert_eq!(
                            cooldown.duration,
                            if finished_with_cooldown { 20 } else { 140 }
                        );
                    };
                    assert_eq!(used(), before + 1);
                    assert_eq!(
                        events.instrument_events.load(Ordering::Relaxed),
                        event_count + 1
                    );
                    assert!(!player.has_advancement(Advancement::HUSBANDRY_ROOT));
                    assert_eq!(player.hunger_manager.level.load(), 10);
                    assert_eq!(player.hunger_manager.saturation.load(), 0.0);
                    assert!(
                        player
                            .living_entity
                            .active_effects
                            .lock()
                            .unwrap()
                            .is_empty()
                    );
                    if elapsed == 139 {
                        assert!(player.is_on_cooldown(group));
                        use_packet(&player, &server, hand);
                        assert_eq!(used(), before + 1);
                        assert_eq!(
                            player.living_entity.item_use_time.load(Ordering::Relaxed),
                            1
                        );
                    }
                    // Drain each production tick, including the expiry tick,
                    // so unrelated inventory updates cannot fill the queue.
                    let packets = observe(&mut reader, &player).await;
                    if elapsed == 140 && finish_cooldown.is_some() {
                        assert_cooldown_packets(&packets, group, 20);
                    } else {
                        assert_no_effect_packets(&packets);
                    }
                    assert_no_effect_packets(
                        &observe(&mut observer.reader, &observer.player).await,
                    );
                }
                assert_eq!(
                    player.living_entity.hurt_cooldown.load(Ordering::Relaxed),
                    1
                );
                if finish_cooldown.is_some() {
                    assert!(player.is_on_cooldown(group));
                    use_packet(&player, &server, hand);
                    assert_eq!(used(), before + 1);
                    assert_eq!(*player.living_entity.active_hand.lock().unwrap(), None);
                    assert_no_effect_packets(&observe(&mut reader, &player).await);
                    assert_no_effect_packets(
                        &observe(&mut observer.reader, &observer.player).await,
                    );
                    for elapsed in 1..=20 {
                        player.tick(&server);
                        assert_eq!(
                            player.tick_counter.load(Ordering::Relaxed),
                            started + 140 + elapsed
                        );
                        assert_eq!(player.is_on_cooldown(group), elapsed < 20);
                        assert_eq!(used(), before + 1);
                        assert_eq!(
                            events.instrument_events.load(Ordering::Relaxed),
                            event_count + 1
                        );
                        assert_eq!(
                            player.living_entity.item_use_time.load(Ordering::Relaxed),
                            0
                        );
                        assert_eq!(*player.living_entity.active_hand.lock().unwrap(), None);
                        assert!(player.living_entity.item_in_use.lock().unwrap().is_none());
                        assert!(!player.has_advancement(Advancement::HUSBANDRY_ROOT));
                        for held_hand in [Hand::Right, Hand::Left] {
                            assert!(
                                player
                                    .inventory()
                                    .get_stack_in_hand(held_hand)
                                    .are_equal(&horn)
                            );
                        }
                        assert_no_effect_packets(&observe(&mut reader, &player).await);
                        assert_no_effect_packets(
                            &observe(&mut observer.reader, &observer.player).await,
                        );
                    }
                }
                assert!(!player.is_on_cooldown(group));
            }
        }
    }

    // The same production expiry path still consumes non-horns, applies their
    // food/effect payload, and grants the ConsumeItem advancement.
    let before_food = used();
    let mut apple = apple;
    apple.set_data_component(consumable);
    player
        .inventory()
        .set_stack_in_hand(Hand::Left, apple.clone());
    use_packet(&player, &server, Hand::Left);
    assert_eq!(
        player.living_entity.item_use_time.load(Ordering::Relaxed),
        32
    );
    assert_eq!(
        *player.living_entity.active_hand.lock().unwrap(),
        Some(Hand::Left)
    );
    observe(&mut reader, &player).await;
    observe(&mut observer.reader, &observer.player).await;
    for elapsed in 1..=32 {
        player.tick(&server);
        if elapsed < 32 {
            assert!(
                player
                    .inventory()
                    .get_stack_in_hand(Hand::Left)
                    .are_equal(&apple)
            );
            assert!(!player.has_advancement(Advancement::HUSBANDRY_ROOT));
            assert!(
                player
                    .living_entity
                    .active_effects
                    .lock()
                    .unwrap()
                    .is_empty()
            );
        }
        observe(&mut reader, &player).await;
        observe(&mut observer.reader, &observer.player).await;
    }
    apple.decrement(1);
    assert!(
        player
            .inventory()
            .get_stack_in_hand(Hand::Left)
            .are_equal(&apple)
    );
    assert_eq!(player.hunger_manager.level.load(), 14);
    assert_eq!(player.hunger_manager.saturation.load(), 2.4);
    assert!(player.has_advancement(Advancement::HUSBANDRY_ROOT));
    assert!(
        player
            .living_entity
            .active_effects
            .lock()
            .unwrap()
            .contains_key(&pumpkin_data::effect::StatusEffect::SPEED)
    );
    assert_eq!(*player.living_entity.active_hand.lock().unwrap(), None);
    assert!(player.living_entity.item_in_use.lock().unwrap().is_none());
    assert_eq!(
        player.living_entity.item_use_time.load(Ordering::Relaxed),
        0
    );
    assert_eq!(used(), before_food);
    drop(reader);
    observer.close().await;
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(
    clippy::too_many_lines,
    reason = "Exercise component-gated horn consumption and replaced active stacks through real ticks."
)]
async fn consumable_horn_expiry_consumes_only_the_active_hand() {
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
    player.world().level_info.rcu(|current| {
        let mut info = (**current).clone();
        info.game_rules.natural_health_regeneration = false;
        info
    });
    let events = Arc::new(UseEvents::default());
    server.plugin_manager.register::<GenericGameEvent, _>(
        events.clone(),
        EventPriority::Normal,
        true,
    );
    let (remote, _write) = fixture._remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    let mut observer = SoundObserver::new(&server, &player.world()).await;
    observe(&mut reader, &player).await;
    observe(&mut observer.reader, &observer.player).await;
    let used = || player.get_stat(StatisticCategory::Used, i32::from(Item::GOAT_HORN.id));
    let food = ItemStack::new(1, &Item::APPLE)
        .get_data_component::<FoodImpl>()
        .unwrap()
        .clone();

    for hand in [Hand::Right, Hand::Left] {
        for (with_food, replace_active) in [(false, false), (true, false), (true, true)] {
            let other_hand = if hand == Hand::Right {
                Hand::Left
            } else {
                Hand::Right
            };
            let group = if with_food {
                "test:consume_horns"
            } else {
                "minecraft:goat_horn"
            };
            player
                .advancements
                .lock()
                .unwrap()
                .revoke(Advancement::HUSBANDRY_ROOT, "consumed_item");
            player
                .living_entity
                .remove_effect(&pumpkin_data::effect::StatusEffect::SPEED);
            player.hunger_manager.level.store(10);
            player.hunger_manager.saturation.store(0.0);
            let mut horn = ItemStack::new(1, &Item::GOAT_HORN);
            horn.set_data_component(consumable_with_speed_effect());
            horn.set_data_component(UseCooldownImpl::new(1.0, with_food.then(|| group.into())));
            if with_food {
                horn.set_data_component(food.clone());
            }
            // Identical main/offhand components must not redirect completion to
            // the main hand when the offhand was used.
            player.inventory().set_stack_in_hand(hand, horn.clone());
            player
                .inventory()
                .set_stack_in_hand(other_hand, horn.clone());
            observe(&mut reader, &player).await;
            observe(&mut observer.reader, &observer.player).await;
            let before = used();
            let event_count = events.instrument_events.load(Ordering::Relaxed);
            let started = player.tick_counter.load(Ordering::Relaxed);
            use_packet(&player, &server, hand);
            assert_eq!(used(), before + 1);
            assert_eq!(
                player.living_entity.item_use_time.load(Ordering::Relaxed),
                140
            );
            assert_effect_packets(
                &observe(&mut reader, &player).await,
                &observe(&mut observer.reader, &observer.player).await,
                player.entity_id(),
                Sound::ItemGoatHornSound0,
                group,
            );
            let replacement = ItemStack::new(1, &Item::STICK);
            if replace_active {
                player
                    .inventory()
                    .set_stack_in_hand(hand, replacement.clone());
            }
            for elapsed in 1..=140 {
                player.tick(&server);
                let consumed = elapsed == 140 && !replace_active;
                assert_eq!(
                    player.tick_counter.load(Ordering::Relaxed),
                    started + elapsed
                );
                assert_eq!(
                    player.living_entity.item_use_time.load(Ordering::Relaxed),
                    140 - elapsed
                );
                assert_eq!(
                    *player.living_entity.active_hand.lock().unwrap(),
                    (elapsed < 140).then_some(hand)
                );
                assert_eq!(used(), before + if consumed { 2 } else { 1 });
                assert_eq!(
                    events.instrument_events.load(Ordering::Relaxed),
                    event_count + 1
                );
                assert_eq!(
                    player.has_advancement(Advancement::HUSBANDRY_ROOT),
                    consumed
                );
                assert_eq!(
                    player.hunger_manager.level.load(),
                    if consumed && with_food { 14 } else { 10 }
                );
                assert_eq!(
                    player.hunger_manager.saturation.load(),
                    if consumed && with_food { 2.4 } else { 0.0 }
                );
                assert_eq!(
                    player
                        .living_entity
                        .active_effects
                        .lock()
                        .unwrap()
                        .contains_key(&pumpkin_data::effect::StatusEffect::SPEED),
                    consumed
                );
                let active_stack = player.inventory().get_stack_in_hand(hand);
                if consumed {
                    assert!(active_stack.is_empty());
                } else {
                    assert!(active_stack.are_equal(if replace_active {
                        &replacement
                    } else {
                        &horn
                    }));
                }
                assert!(
                    player
                        .inventory()
                        .get_stack_in_hand(other_hand)
                        .are_equal(&horn)
                );
                {
                    let cooldowns = player.item_cooldowns.lock().unwrap();
                    let cooldown = cooldowns.get(group).unwrap();
                    assert_eq!(
                        cooldown.start_tick,
                        started + if consumed { 140 } else { 0 }
                    );
                    assert_eq!(cooldown.duration, if consumed { 20 } else { 140 });
                }
                let packets = observe(&mut reader, &player).await;
                let cooldowns: Vec<_> = packets
                    .iter()
                    .filter(|packet| packet.id == CItemCooldown::to_id(CURRENT_MC_VERSION))
                    .collect();
                assert_eq!(cooldowns.len(), usize::from(consumed));
                if consumed {
                    let mut payload = &cooldowns[0].payload[..];
                    assert_eq!(payload.get_str().unwrap().as_ref(), group);
                    assert_eq!(payload.get_var_int().unwrap().0, 20);
                    assert!(payload.is_empty());
                }
                // Consumable particles, sounds and EAT/DRINK event parity are
                // separate gaps; do not assert their absence as a Java contract.
                observe(&mut observer.reader, &observer.player).await;
            }
            assert!(player.living_entity.item_in_use.lock().unwrap().is_none());
            if !replace_active {
                assert!(player.is_on_cooldown(group));
                let finished_used = used();
                // The still-full identical horn cannot be used in the other hand
                // until the explicit completion cooldown elapses.
                use_packet(&player, &server, other_hand);
                assert_eq!(used(), finished_used);
                assert_eq!(*player.living_entity.active_hand.lock().unwrap(), None);
                assert_no_effect_packets(&observe(&mut reader, &player).await);
                assert_no_effect_packets(&observe(&mut observer.reader, &observer.player).await);
                for elapsed in 1..=20 {
                    player.tick(&server);
                    assert_eq!(
                        player.tick_counter.load(Ordering::Relaxed),
                        started + 140 + elapsed
                    );
                    assert_eq!(player.is_on_cooldown(group), elapsed < 20);
                    assert_eq!(used(), finished_used);
                    observe(&mut reader, &player).await;
                    observe(&mut observer.reader, &observer.player).await;
                }
            }
            assert!(!player.is_on_cooldown(group));
        }
    }
    drop(reader);
    observer.close().await;
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn horn_entity_sound_recipients_exclude_performer_and_strict_range_boundary() {
    let mut fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    let player = fixture.player.clone();
    let server = fixture.server.clone();
    let origin = Vector3::new(0.5, 65.0, 0.5);
    player.get_entity().set_pos(origin);
    player.set_client_loaded(true);
    fixture
        .client
        .java()
        .unwrap()
        .version
        .store(CURRENT_MC_VERSION);
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::new(1, &Item::GOAT_HORN));
    let mut observer = SoundObserver::new(&server, &player.world()).await;
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    observe(&mut observer.reader, &observer.player).await;

    // Exact primary PlayerList.broadcast sums all three squared coordinate
    // differences and sends only when distance squared is strictly below range².
    for (offset, audible) in [
        (Vector3::new(255.5, 0.0, 0.0), true),
        (Vector3::new(-255.5, 0.0, 0.0), true),
        (Vector3::new(0.0, 255.5, 0.0), true),
        (Vector3::new(0.0, 0.0, 255.5), true),
        (Vector3::new(256.0, 0.0, 0.0), false),
        (Vector3::new(256.5, 0.0, 0.0), false),
        (Vector3::new(0.0, 256.0, 0.0), false),
        (Vector3::new(0.0, 0.0, 256.0), false),
        (Vector3::new(200.0, 0.0, 200.0), false),
    ] {
        observer.player.get_entity().set_pos(origin + offset);
        // This scenario isolates recipients. The separate expiry test advances
        // both use time and cooldown through production Player ticks.
        player.tick_counter.fetch_add(140, Ordering::Relaxed);
        let before = player.get_stat(StatisticCategory::Used, i32::from(Item::GOAT_HORN.id));
        use_packet(&player, &server, Hand::Right);
        assert_eq!(
            player.get_stat(StatisticCategory::Used, i32::from(Item::GOAT_HORN.id)),
            before + 1
        );
        assert_cooldown_packets(
            &observe(&mut reader, &player).await,
            "minecraft:goat_horn",
            140,
        );
        let packets = observe(&mut observer.reader, &observer.player).await;
        if audible {
            assert_entity_sound_packets(&packets, Sound::ItemGoatHornSound0, player.entity_id());
        } else {
            assert_no_effect_packets(&packets);
        }
    }

    drop(reader);
    observer.close().await;
    close_fixture(&fixture).await;
}
