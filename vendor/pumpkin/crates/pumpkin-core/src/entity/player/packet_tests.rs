//! Packet ordering, transport liveness and session-drain acceptance tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::entity::EntityBase;
use crate::plugin::server::packet::PacketReceivedEvent;
use crate::{
    data::VanillaData,
    entity::player::Player,
    net::{
        ClientPlatform, GameProfile, PacketRateLimiter, PlayerConfig,
        java::{JavaClient, pending::PendingConnection},
    },
    plugin::{
        EventHandler, EventPriority,
        player::{
            item_held::PlayerItemHeldEvent, player_change_world::PlayerChangeWorldEvent,
            player_command_send::PlayerCommandSendEvent,
        },
    },
    server::Server,
};
use arc_swap::ArcSwap;
use bytes::Bytes;
use futures::future::BoxFuture;
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_protocol::{
    ClientPacket, ConnectionState,
    java::{
        packet_decoder::TCPNetworkDecoder,
        packet_encoder::TCPNetworkEncoder,
        server::play::{SChatAck, SChatCommand, SClientCommand, SKeepAlive, SSetHeldItem},
    },
    packet::MultiVersionJavaPacket,
};
use pumpkin_util::{GameMode, PermissionLvl, world_seed::Seed};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use uuid::Uuid;

async fn bounded<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(15), future)
        .await
        .expect("activity must finish without sleeps or another tick")
}

#[derive(Default)]
struct Barrier {
    entered: Notify,
    release: Notify,
}

impl Barrier {
    async fn hold(&self) {
        self.entered.notify_one();
        self.release.notified().await;
    }
}

struct CommandBarrier {
    player: Uuid,
    barrier: Arc<Barrier>,
    descendant: Option<Arc<Barrier>>,
}

impl EventHandler<PlayerCommandSendEvent> for CommandBarrier {
    fn handle_blocking<'a>(
        &'a self,
        server: &'a Arc<Server>,
        event: &'a mut PlayerCommandSendEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.player.gameprofile.id != self.player {
                return;
            }
            self.barrier.hold().await;
            if let Some(barrier) = self.descendant.clone() {
                let player = event.player.clone();
                // The real Server::spawn_task path must inherit before spawning,
                // including when the accepted parent resumes after close.
                server.spawn_task(async move {
                    barrier.hold().await;
                    player.add_experience_levels(7);
                });
            }
        })
    }
}

struct HeldBarrier(Arc<Barrier>);

impl EventHandler<PlayerItemHeldEvent> for HeldBarrier {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a mut PlayerItemHeldEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(self.0.hold())
    }
}

struct TransferBarrier(Arc<Barrier>);

impl EventHandler<PlayerChangeWorldEvent> for TransferBarrier {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        _event: &'a mut PlayerChangeWorldEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(self.0.hold())
    }
}

struct Fixture {
    directory: tempfile::TempDir,
    server: Arc<Server>,
    players: Vec<Arc<Player>>,
    peers: Vec<tokio::net::TcpStream>,
}

impl Fixture {
    async fn new(count: usize) -> Self {
        Self::with_nether(count, false).await
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Keep server, terrain, transport and session admission setup in one fixture."
    )]
    async fn with_nether(count: usize, allow_nether: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let basic = BasicConfiguration {
            default_level_name: directory.path().to_string_lossy().into_owned(),
            seed: Seed(0),
            allow_nether,
            allow_end: false,
            allow_chat_reports: false,
            use_favicon: false,
            ..BasicConfiguration::default()
        };
        let mut advanced = AdvancedConfiguration::default();
        advanced.chat.anti_spam.chat_spam_threshold_seconds = Some(0);
        advanced.chat.anti_spam.command_spam_threshold_seconds = Some(0);
        advanced.networking.java.simulation_distance =
            serde_json::from_value(serde_json::json!(2)).unwrap();
        advanced.networking.java.view_distance =
            serde_json::from_value(serde_json::json!(2)).unwrap();
        advanced.networking.java.online_mode = false;
        advanced.networking.bedrock.online_mode = false;
        let data = VanillaData {
            banned_ip_list: std::sync::RwLock::default(),
            banned_player_list: std::sync::RwLock::default(),
            operator_config: std::sync::RwLock::default(),
            user_cache: std::sync::RwLock::default(),
            whitelist_config: std::sync::RwLock::default(),
        };
        let server = Server::new(
            basic,
            advanced,
            TelemetryConfig::default(),
            data,
            Vec::new(),
        )
        .await
        .unwrap();
        for world in server.worlds.load().iter() {
            world.level_info.rcu(|current| {
                let mut info = (**current).clone();
                info.spawn_x = 0;
                info.spawn_y = 65;
                info.spawn_z = 0;
                info
            });
            let chunk = pumpkin_world::chunk::ChunkData::empty_sync(0, 0);
            chunk.post_processing_ready.store(true, Ordering::Release);
            let center = pumpkin_util::math::vector2::Vector2::new(0, 0);
            world.level.loaded_chunks.insert(center, chunk);
            let mut loading = world.level.chunk_loading.lock().unwrap();
            loading.add_ticket(
                center,
                pumpkin_world::chunk_system::ChunkLoading::FULL_CHUNK_LEVEL,
            );
            loading.send_change();
        }
        let mut players = Vec::new();
        let mut peers = Vec::new();
        for index in 0..count {
            let profile = GameProfile {
                id: Uuid::from_u128(800 + index as u128),
                name: format!("PacketPlayer{index}"),
                properties: ArcSwap::from_pointee(Vec::new()),
                profile_actions: None,
            };
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (peer, accepted) =
                tokio::join!(tokio::net::TcpStream::connect(address), listener.accept());
            peers.push(peer.unwrap());
            let (stream, address) = accepted.unwrap();
            let pending = PendingConnection::new(
                stream,
                address,
                index as u64,
                PacketRateLimiter::new(false, 0.0, 0.0),
                Arc::downgrade(&server),
            );
            pending.connection_state.store(ConnectionState::Play);
            let mut java =
                JavaClient::from_pending(pending, profile.clone(), PlayerConfig::default());
            java.start_outgoing_packet_task();
            let client = Arc::new(ClientPlatform::Java(java));
            let world = server.worlds.load().first().cloned().unwrap();
            let player = Arc::new(Player::new(
                client.clone(),
                profile,
                PlayerConfig::default(),
                &world,
                GameMode::Survival,
            ));
            player.permission_lvl.store(PermissionLvl::Four);
            player.advancements.lock().unwrap().load().unwrap();
            server.statistics_manager.load_player(&player).unwrap();
            player
                .get_entity()
                .set_pos(pumpkin_util::math::vector3::Vector3::new(0.5, 65.0, 0.5));
            player
                .screen_handler_sync_handler
                .store_player(player.clone());
            client.java().unwrap().set_player(&player);
            let session = server
                .player_data_storage
                .reserve_player_session(player.gameprofile.id)
                .await;
            session.publish(&player);
            *player.session.lock().unwrap() = Some(session);
            world.add_player(&player).unwrap();
            player.packet_activity.begin_epoch();
            players.push(player);
        }
        Self {
            directory,
            server,
            players,
            peers,
        }
    }

    async fn finish(&self) {
        for player in &self.players {
            if self
                .server
                .get_player_by_uuid(player.gameprofile.id)
                .is_some()
            {
                drive_until(
                    &self.server,
                    crate::finish_player_session(&self.server, player),
                )
                .await
                .unwrap();
            }
        }
        self.server.packet_runner.finish_drain();
        bounded(self.server.shutdown()).await;
    }
}

fn enqueue<P: ClientPacket + MultiVersionJavaPacket>(player: &Player, packet: &P) {
    let mut payload = Vec::new();
    packet
        .write_packet_data(&mut payload, &CURRENT_MC_VERSION)
        .unwrap();
    player
        .client
        .java()
        .unwrap()
        .receive_play_packet(
            player,
            RawPacket {
                id: P::to_id(CURRENT_MC_VERSION),
                payload: Bytes::from(payload),
            },
        )
        .unwrap();
}

// Use the real encoder, decoder, close-aware read, ingress admission and timeout
// helper. Duplex transport keeps paused-clock assertions independent of
// operating-system socket readiness.
async fn receive_wire(player: &Player, bytes: Bytes) -> Result<(), AdmissionError> {
    let client = player.client.java().unwrap();
    let (write, read) = tokio::io::duplex(8192);
    let mut encoder = TCPNetworkEncoder::new(write);
    let mut decoder = TCPNetworkDecoder::new(read);
    encoder.set_compression((0, 6));
    decoder.set_compression(0);
    encoder.set_encryption(&[9; 16]).unwrap();
    decoder.set_encryption(&[9; 16]).unwrap();
    encoder.write_packet(bytes).await.unwrap();
    encoder.flush().await.unwrap();
    // A closed socket read returns None, so closed-admission assertions use
    // the decoded frame directly, as if close raced frame completion.
    let packet = bounded(async {
        if client.is_closed() {
            decoder.get_raw_packet().await.unwrap()
        } else {
            client.get_packet_with_reader(&mut decoder).await.unwrap()
        }
    })
    .await;
    client.receive_play_packet(player, packet)
}

async fn receive<P: ClientPacket>(player: &Player, packet: &P) -> Result<(), AdmissionError> {
    receive_wire(
        player,
        JavaClient::serialize_packet_for_version(packet, CURRENT_MC_VERSION).unwrap(),
    )
    .await
}

/// Exercise exactly the runner called by `wait_until_next_tick`, with no
/// `Player::tick` or inbound dequeue in the harness.
async fn dispatch(player: &Arc<Player>) {
    let server = player.world().server.upgrade().unwrap();
    bounded(tokio::task::spawn_blocking(move || {
        for _ in 0..256 {
            if !server.packet_runner.run_ready(&server, None) {
                return;
            }
        }
        panic!("ready lane did not become idle within a finite test wave");
    }))
    .await
    .unwrap();
}

async fn drive_until<F: Future>(server: &Arc<Server>, future: F) -> F::Output {
    bounded(async {
        tokio::pin!(future);
        loop {
            if let std::task::Poll::Ready(value) = futures::poll!(&mut future) {
                return value;
            }
            let server = server.clone();
            tokio::task::spawn_blocking(move || server.packet_runner.run_ready(&server, None))
                .await
                .unwrap();
            tokio::task::yield_now().await;
        }
    })
    .await
}

async fn settled(player: &Arc<Player>) {
    let server = player.world().server.upgrade().unwrap();
    drive_until(
        &server,
        std::future::poll_fn(|_| {
            let state = player.packet_activity.state();
            if state.active == 0
                && (state.sealed || state.remaining == 0 || player.inbound_packets.is_empty())
            {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        }),
    )
    .await;
}

struct TransportObserver {
    player: Uuid,
    seen: Mutex<Vec<i32>>,
    rewrite_gameplay: AtomicBool,
}

impl EventHandler<PacketReceivedEvent> for TransportObserver {
    fn handle_blocking<'a>(
        &'a self,
        _server: &'a Arc<Server>,
        event: &'a mut PacketReceivedEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.player.gameprofile.id != self.player {
                return;
            }
            let keep_alive_id = SKeepAlive::to_id(CURRENT_MC_VERSION);
            let keep_alive_count = {
                let mut seen = self.seen.lock().unwrap();
                seen.push(event.packet_id);
                seen.iter().filter(|&&id| id == keep_alive_id).count()
            };
            if event.packet_id == keep_alive_id {
                // Exercise both rewrites and cancellation after transport
                // accounting. Neither may turn this event into gameplay.
                event.packet_id = SSetHeldItem::to_id(CURRENT_MC_VERSION);
                event.payload = Bytes::copy_from_slice(&8i16.to_be_bytes());
                event.cancelled = keep_alive_count == 2;
            } else if self.rewrite_gameplay.load(Ordering::Relaxed) {
                event.packet_id = keep_alive_id;
                event.payload = Bytes::copy_from_slice(
                    &event
                        .player
                        .client
                        .java()
                        .unwrap()
                        .keep_alive_id
                        .load()
                        .to_be_bytes(),
                );
            }
        })
    }
}

#[tokio::test]
async fn received_keep_alive_stays_live_behind_real_command_and_ordered_hooks() {
    let fixture = bounded(Fixture::new(2)).await;
    let player = &fixture.players[0];
    let other = &fixture.players[1];
    let client = player.client.java().unwrap();
    let barrier = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(CommandBarrier {
            player: player.gameprofile.id,
            barrier: barrier.clone(),
            descendant: None,
        }),
        EventPriority::Normal,
        true,
    );
    enqueue(
        player,
        &SChatCommand {
            command: "gamemode creative @s",
        },
    );
    dispatch(player).await;
    bounded(barrier.entered.notified()).await;
    let observer = Arc::new(TransportObserver {
        player: player.gameprofile.id,
        seen: Mutex::new(Vec::new()),
        rewrite_gameplay: AtomicBool::new(false),
    });
    fixture
        .server
        .plugin_manager
        .register(observer.clone(), EventPriority::Normal, true);
    receive(player, &SSetHeldItem { slot: 5 }).await.unwrap();
    tokio::time::pause();
    let interval = Duration::from_secs(
        fixture
            .server
            .advanced_config
            .networking
            .java
            .keep_alive_time,
    );
    let timeout = interval * 2;
    // Stay busy past the first challenge's real deadline, answering every
    // challenge before the next interval. No timeout multiplier or sleeps.
    for _ in 0..3 {
        assert!(client.send_keep_alive_or_timeout(timeout).await);
        let id = client.keep_alive_id.load();
        tokio::time::advance(interval.checked_sub(Duration::from_secs(1)).unwrap()).await;
        receive(player, &SKeepAlive { keep_alive_id: id })
            .await
            .unwrap();
        assert!(client.pending_keep_alives.lock().unwrap().is_empty());
        assert!(!client.wait_for_keep_alive.load(Ordering::Relaxed));
        tokio::time::advance(Duration::from_secs(1)).await;
    }
    assert!(client.send_keep_alive_or_timeout(timeout).await);
    let unanswered = client.keep_alive_id.load();
    dispatch(player).await;
    assert_eq!(player.inventory.get_selected_slot(), 0);
    assert!(
        observer.seen.lock().unwrap().is_empty(),
        "hooks remain ordered"
    );
    receive(other, &SSetHeldItem { slot: 3 }).await.unwrap();
    dispatch(other).await;
    assert_eq!(other.inventory.get_selected_slot(), 3);
    assert!(!client.is_closed());
    tokio::time::resume();
    barrier.release.notify_one();
    settled(player).await;
    assert_eq!(player.gamemode.load(), GameMode::Creative);
    dispatch(player).await;
    assert_eq!(player.inventory.get_selected_slot(), 5);
    assert_eq!(
        *observer.seen.lock().unwrap(),
        vec![
            SSetHeldItem::to_id(CURRENT_MC_VERSION),
            SKeepAlive::to_id(CURRENT_MC_VERSION),
            SKeepAlive::to_id(CURRENT_MC_VERSION),
            SKeepAlive::to_id(CURRENT_MC_VERSION),
        ]
    );
    // Delayed transport events cannot replay an old reply or clear the new
    // challenge. An ordinary event rewritten to keepalive cannot do so either.
    observer.rewrite_gameplay.store(true, Ordering::Relaxed);
    receive(player, &SSetHeldItem { slot: 7 }).await.unwrap();
    dispatch(player).await;
    assert_eq!(player.inventory.get_selected_slot(), 5);
    assert_eq!(client.pending_keep_alives.lock().unwrap()[0].0, unanswered);
    assert!(client.wait_for_keep_alive.load(Ordering::Relaxed));
    fixture.finish().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep exact-ID, duplicate, rewrite, close and capacity controls in one transport scenario."
)]
async fn transport_reply_requires_exact_outstanding_id_and_open_bounded_admission() {
    let fixture = bounded(Fixture::new(2)).await;
    let player = &fixture.players[0];
    let client = player.client.java().unwrap();
    let timeout = Duration::from_secs(30);
    receive(player, &SKeepAlive { keep_alive_id: 0 })
        .await
        .unwrap();
    assert_eq!(player.ping.load(Ordering::Relaxed), 0);
    assert!(client.send_keep_alive_or_timeout(timeout).await);
    let first = client.keep_alive_id.load();
    assert!(client.send_keep_alive_or_timeout(timeout).await);
    let second = client.keep_alive_id.load();
    assert_ne!(
        first, second,
        "same-millisecond challenges must not collide"
    );
    for id in [0, second.wrapping_add(1)] {
        receive(player, &SKeepAlive { keep_alive_id: id })
            .await
            .unwrap();
    }
    let valid = JavaClient::serialize_packet_for_version(
        &SKeepAlive {
            keep_alive_id: first,
        },
        CURRENT_MC_VERSION,
    )
    .unwrap();
    let mut trailing = valid.to_vec();
    trailing.push(0);
    receive_wire(player, Bytes::from(trailing)).await.unwrap();
    receive_wire(player, valid.slice(..valid.len() - 1))
        .await
        .unwrap();
    // A different packet with the same bytes is not a transport response.
    let mut chat_ack = Vec::new();
    pumpkin_protocol::ser::NetworkWriteExt::write_var_int(
        &mut chat_ack,
        &pumpkin_protocol::codec::var_int::VarInt(SChatAck::to_id(CURRENT_MC_VERSION)),
    )
    .unwrap();
    chat_ack.extend_from_slice(&first.to_be_bytes());
    receive_wire(player, Bytes::from(chat_ack)).await.unwrap();
    assert_eq!(client.pending_keep_alives.lock().unwrap().len(), 2);
    receive(
        player,
        &SKeepAlive {
            keep_alive_id: first,
        },
    )
    .await
    .unwrap();
    assert_eq!(client.pending_keep_alives.lock().unwrap()[0].0, second);
    assert!(client.wait_for_keep_alive.load(Ordering::Relaxed));
    let ping = player.ping.load(Ordering::Relaxed);
    receive(
        player,
        &SKeepAlive {
            keep_alive_id: first,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        player.ping.load(Ordering::Relaxed),
        ping,
        "duplicates cannot update latency"
    );
    assert_eq!(client.pending_keep_alives.lock().unwrap().len(), 1);
    receive(
        player,
        &SKeepAlive {
            keep_alive_id: second,
        },
    )
    .await
    .unwrap();
    assert!(client.pending_keep_alives.lock().unwrap().is_empty());
    assert!(!client.wait_for_keep_alive.load(Ordering::Relaxed));
    assert!(client.send_keep_alive_or_timeout(timeout).await);
    let third = client.keep_alive_id.load();
    client.close();
    assert_eq!(
        receive(
            player,
            &SKeepAlive {
                keep_alive_id: third
            }
        )
        .await,
        Err(AdmissionError::Closed)
    );
    assert_eq!(client.pending_keep_alives.lock().unwrap()[0].0, third);

    let full = &fixture.players[1];
    let full_client = full.client.java().unwrap();
    assert!(full_client.send_keep_alive_or_timeout(timeout).await);
    let id = full_client.keep_alive_id.load();
    for _ in 0..MAX_QUEUED_PACKETS {
        enqueue(full, &SSetHeldItem { slot: 1 });
    }
    assert_eq!(
        receive(full, &SKeepAlive { keep_alive_id: id }).await,
        Err(AdmissionError::Full)
    );
    assert_eq!(full.inbound_packets.len(), MAX_QUEUED_PACKETS);
    assert_eq!(full_client.pending_keep_alives.lock().unwrap()[0].0, id);
    assert_eq!(full.ping.load(Ordering::Relaxed), 0);
    fixture.finish().await;
}

#[tokio::test]
async fn busy_command_does_not_hide_an_unanswered_transport_challenge() {
    let fixture = bounded(Fixture::new(1)).await;
    let player = &fixture.players[0];
    let client = player.client.java().unwrap();
    let barrier = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(CommandBarrier {
            player: player.gameprofile.id,
            barrier: barrier.clone(),
            descendant: None,
        }),
        EventPriority::Normal,
        true,
    );
    enqueue(
        player,
        &SChatCommand {
            command: "gamemode creative @s",
        },
    );
    dispatch(player).await;
    bounded(barrier.entered.notified()).await;
    tokio::time::pause();
    let interval = Duration::from_secs(
        fixture
            .server
            .advanced_config
            .networking
            .java
            .keep_alive_time,
    );
    let timeout = interval * 2;
    assert!(client.send_keep_alive_or_timeout(timeout).await);
    tokio::time::advance(interval).await;
    // Gameplay traffic is live, but it cannot satisfy a transport challenge.
    receive(player, &SSetHeldItem { slot: 4 }).await.unwrap();
    assert!(client.send_keep_alive_or_timeout(timeout).await);
    tokio::time::advance(interval + Duration::from_secs(1)).await;
    receive(player, &SKeepAlive { keep_alive_id: -1 })
        .await
        .unwrap();
    assert!(!client.send_keep_alive_or_timeout(timeout).await);
    assert!(client.is_closed());
    assert_eq!(player.inventory.get_selected_slot(), 0);
    tokio::time::resume();
    barrier.release.notify_one();
    settled(player).await;
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_session_save_and_uuid_admission_wait_for_late_server_descendant() {
    let fixture = bounded(Fixture::new(1)).await;
    let player = &fixture.players[0];
    let barrier = Arc::new(Barrier::default());
    let child = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(CommandBarrier {
            player: player.gameprofile.id,
            barrier: barrier.clone(),
            descendant: Some(child.clone()),
        }),
        EventPriority::Normal,
        true,
    );
    enqueue(
        player,
        &SChatCommand {
            command: "gamemode creative @s",
        },
    );
    enqueue(player, &SSetHeldItem { slot: 4 });
    dispatch(player).await;
    bounded(barrier.entered.notified()).await;
    let mut finish = Box::pin(crate::finish_player_session(&fixture.server, player));
    assert!(futures::poll!(&mut finish).is_pending());
    assert!(player.packet_activity.enter_mutation().is_none());
    let mut replacement = Box::pin(
        fixture
            .server
            .player_data_storage
            .reserve_player_session(player.gameprofile.id),
    );
    assert!(futures::poll!(&mut replacement).is_pending());
    barrier.release.notify_one();
    drive_until(&fixture.server, child.entered.notified()).await;
    assert!(futures::poll!(&mut finish).is_pending());
    assert!(futures::poll!(&mut replacement).is_pending());
    child.release.notify_one();
    drive_until(&fixture.server, finish).await.unwrap();
    let data = fixture
        .server
        .player_data_storage
        .load_data(&player.gameprofile.id)
        .unwrap()
        .unwrap();
    assert_eq!(data.get_int("playerGameType"), Some(1));
    assert_eq!(data.get_int("XpLevel"), Some(7));
    assert_eq!(
        player.inventory.get_selected_slot(),
        0,
        "queued input discarded on close"
    );
    assert!(player.inbound_packets.is_empty());
    drop(bounded(replacement).await);
    fixture.finish().await;
}

// Uses the same finish implementation as both connection lifecycles.
// Only its exit-status destination is local to avoid process-global test races.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[expect(
    clippy::too_many_lines,
    reason = "Keep all three writer failure combinations with persistence and sticky-status assertions."
)]
async fn final_session_write_failures_survive_empty_shutdown_snapshots() {
    use crate::PlayerSessionSaveError;
    use std::sync::atomic::AtomicI32;

    let fixture = bounded(Fixture::new(9)).await;
    let exit_status = AtomicI32::new(0);
    let failures = [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (true, true, false),
        (true, false, true),
        (false, true, true),
        (true, true, true),
        (false, false, false),
    ];
    for (index, (player, (fail_player, fail_advancements, fail_statistics))) in
        fixture.players.iter().zip(failures).enumerate()
    {
        let storage = &fixture.server.player_data_storage;
        player.experience_level.store(1, Ordering::Relaxed);
        storage.extract_data_and_save_player(player).unwrap();
        fixture
            .server
            .advancement_manager
            .save_player(player)
            .await
            .unwrap();
        let player_path = fixture
            .directory
            .path()
            .join("players/data")
            .join(format!("{}.dat", player.gameprofile.id));
        let advancement_path = player.advancements.lock().unwrap().path.clone();
        let prior_player = std::fs::read(&player_path).unwrap();
        let prior_advancements = std::fs::read(&advancement_path).unwrap();
        fixture
            .server
            .statistics_manager
            .save_player(player)
            .await
            .unwrap();
        let statistics_path = fixture
            .server
            .statistics_manager
            .player_path(player.gameprofile.id);
        let prior_statistics = std::fs::read(&statistics_path).unwrap();
        player.increment_custom_stat(
            crate::entity::player::statistics::CustomStatistic::OpenBarrel,
            11,
        );
        let statistics_dir = &fixture.server.statistics_manager.statistics_path;
        let statistics_backup = fixture.directory.path().join("statistics-before-failure");
        if fail_statistics {
            std::fs::rename(statistics_dir, &statistics_backup).unwrap();
            std::fs::write(statistics_dir, b"not a directory").unwrap();
        }
        player.experience_level.store(2, Ordering::Relaxed);
        {
            let mut advancements = player.advancements.lock().unwrap();
            assert!(
                advancements
                    .progress
                    .get_mut_or_start_progress(pumpkin_data::Advancement::STORY_ROOT)
                    .grant_progress("crafting_table")
            );
            if fail_advancements {
                // A regular file cannot be a parent directory. This makes the
                // real advancement writer fail while preserving its prior JSON.
                advancements.path = advancement_path.join("cannot-create.json");
            }
        }
        if fail_player {
            // Backup replacement fails before replacing the valid primary.
            std::fs::create_dir(player_path.with_extension("dat_old")).unwrap();
        }
        let active = player.packet_activity.enter_non_packet().unwrap();
        let mut finishing = Box::pin(crate::finish_player_session_with_status(
            &fixture.server,
            player,
            &exit_status,
        ));
        assert!(futures::poll!(&mut finishing).is_pending());
        assert_eq!(std::fs::read(&player_path).unwrap(), prior_player);
        drop(active);
        let result = bounded(finishing).await;
        let previous = match (fail_statistics, result) {
            (false, result) => result,
            (true, Err(PlayerSessionSaveError::Statistics(_)))
                if !fail_player && !fail_advancements =>
            {
                Ok(())
            }
            (true, Err(PlayerSessionSaveError::WithStatistics { previous, .. })) => Err(*previous),
            (_, result) => panic!("wrong statistics outcome: {result:?}"),
        };
        match (fail_player, fail_advancements, previous) {
            (false, false, Ok(()))
            | (true, false, Err(PlayerSessionSaveError::Player(_)))
            | (false, true, Err(PlayerSessionSaveError::Advancements(_)))
            | (true, true, Err(PlayerSessionSaveError::Both { .. })) => {}
            (_, _, result) => panic!("wrong player/advancement outcome: {result:?}"),
        }
        if fail_statistics {
            std::fs::remove_file(statistics_dir).unwrap();
            std::fs::rename(&statistics_backup, statistics_dir).unwrap();
            assert_eq!(std::fs::read(&statistics_path).unwrap(), prior_statistics);
        } else {
            let saved: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&statistics_path).unwrap()).unwrap();
            assert_eq!(
                saved["stats"]["minecraft:custom"]["minecraft:open_barrel"],
                11
            );
        }
        assert!(
            fixture
                .server
                .get_player_by_uuid(player.gameprofile.id)
                .is_none()
        );
        if fail_player {
            assert_eq!(std::fs::read(&player_path).unwrap(), prior_player);
            assert_eq!(
                storage
                    .load_data(&player.gameprofile.id)
                    .unwrap()
                    .unwrap()
                    .get_int("XpLevel"),
                Some(1)
            );
            std::fs::remove_dir(player_path.with_extension("dat_old")).unwrap();
        } else {
            assert_eq!(
                storage
                    .load_data(&player.gameprofile.id)
                    .unwrap()
                    .unwrap()
                    .get_int("XpLevel"),
                Some(2)
            );
        }
        let saved_advancements = std::fs::read(&advancement_path).unwrap();
        assert!(
            serde_json::from_slice::<serde_json::Value>(&saved_advancements)
                .unwrap()
                .is_object()
        );
        if fail_advancements {
            assert_eq!(saved_advancements, prior_advancements);
        } else {
            assert_ne!(
                saved_advancements, prior_advancements,
                "advancements still write if player.dat fails"
            );
        }
        assert_eq!(exit_status.load(Ordering::Acquire), i32::from(index != 0));
    }
    assert!(fixture.server.get_all_players().is_empty());
    // The precise late-shutdown retry from the regression now succeeds, but
    // must not clear the sticky failures from the real session writes.
    fixture
        .server
        .player_data_storage
        .save_all_players(&fixture.server)
        .unwrap();
    fixture
        .server
        .advancement_manager
        .save_all_players(&fixture.server.get_all_players())
        .await
        .unwrap();
    fixture
        .server
        .statistics_manager
        .save_all_players(&fixture.server.get_all_players())
        .await
        .unwrap();
    assert_eq!(exit_status.load(Ordering::Acquire), 1);
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_inline_callback_finishes_before_disconnect_snapshot() {
    let fixture = bounded(Fixture::new(1)).await;
    let player = &fixture.players[0];
    let barrier = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(HeldBarrier(barrier.clone())),
        EventPriority::Normal,
        true,
    );
    enqueue(player, &SSetHeldItem { slot: 7 });
    let server = fixture.server.clone();
    let dispatch =
        tokio::task::spawn_blocking(move || server.packet_runner.run_ready(&server, None));
    bounded(barrier.entered.notified()).await;
    let mut finish = Box::pin(crate::finish_player_session(&fixture.server, player));
    assert!(futures::poll!(&mut finish).is_pending());
    assert_eq!(player.inventory.get_selected_slot(), 0);
    barrier.release.notify_one();
    bounded(dispatch).await.unwrap();
    drive_until(&fixture.server, finish).await.unwrap();
    let data = fixture
        .server
        .player_data_storage
        .load_data(&player.gameprofile.id)
        .unwrap()
        .unwrap();
    assert_eq!(data.get_int("SelectedItemSlot"), Some(7));
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_cross_world_teleport_child_holds_input_and_drains_on_close() {
    let fixture = bounded(Fixture::with_nether(1, true)).await;
    let player = &fixture.players[0];
    let origin = player.world();
    let barrier = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(TransferBarrier(barrier.clone())),
        EventPriority::Normal,
        true,
    );
    enqueue(
        player,
        &SChatCommand {
            command: "execute in minecraft:the_nether run tp @s 2 90 2",
        },
    );
    enqueue(player, &SSetHeldItem { slot: 6 });
    dispatch(player).await;
    bounded(barrier.entered.notified()).await;
    // EntityBase::teleport has returned to the command dispatcher while its
    // client-owned transfer is still paused inside the actual world callback.
    dispatch(player).await;
    assert_eq!(player.inventory.get_selected_slot(), 0);
    let mut finish = Box::pin(crate::finish_player_session(&fixture.server, player));
    assert!(futures::poll!(&mut finish).is_pending());
    barrier.release.notify_one();
    drive_until(&fixture.server, finish).await.unwrap();
    // Exercise teleport_world's existing post-callback close check. A complete
    // connected dimension transfer still needs paired client acceptance.
    assert!(Arc::ptr_eq(&origin, &player.world()));
    fixture.finish().await;
}

#[tokio::test]
async fn cancellation_does_not_release_a_blocking_mutators_owned_lease() {
    let activity = Arc::new(PacketActivity::default());
    let mutation = activity.enter_mutation().unwrap();
    let worker_lease = mutation.clone();
    let changed = Arc::new(AtomicBool::new(false));
    let worker_changed = changed.clone();
    let (release, receive) = std::sync::mpsc::channel();
    let worker = tokio::task::spawn_blocking(move || {
        worker_lease.sync_scope(|| {
            receive.recv_timeout(Duration::from_secs(15)).unwrap();
            worker_changed.store(true, Ordering::Release);
        });
    });
    let (entered, started) = tokio::sync::oneshot::channel();
    let waiter = tokio::spawn(mutation.scope(async move {
        entered.send(()).unwrap();
        worker.await.unwrap();
    }));
    bounded(started).await.unwrap();
    waiter.abort();
    assert!(bounded(waiter).await.unwrap_err().is_cancelled());
    let mut drained = Box::pin(activity.drain());
    assert!(futures::poll!(&mut drained).is_pending());
    assert!(!changed.load(Ordering::Acquire));
    release.send(()).unwrap();
    bounded(drained).await;
    assert!(changed.load(Ordering::Acquire));
    assert!(activity.enter_mutation().is_none());
}

#[tokio::test]
async fn aborting_result_waiter_cannot_release_an_owned_mutator() {
    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    let barrier = Arc::new(Barrier::default());
    let owned_barrier = barrier.clone();
    let owned = player.clone();
    let task = player.client.java().unwrap().spawn_activity_task(
        player.packet_activity.enter_mutation().unwrap(),
        async move {
            owned_barrier.hold().await;
            owned.add_experience_levels(7);
        },
    );
    dispatch(player).await;
    bounded(barrier.entered.notified()).await;
    task.abort();
    assert!(bounded(task).await.unwrap_err().is_cancelled());
    let mut finish = Box::pin(crate::finish_player_session(&fixture.server, player));
    assert!(futures::poll!(&mut finish).is_pending());
    barrier.release.notify_one();
    drive_until(&fixture.server, finish).await.unwrap();
    assert_eq!(
        fixture
            .server
            .player_data_storage
            .load_data(&player.gameprofile.id)
            .unwrap()
            .unwrap()
            .get_int("XpLevel"),
        Some(7)
    );
    fixture.finish().await;
}

#[tokio::test]
async fn readiness_cancellation_before_waiter_registration_keeps_child_ownership() {
    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    let lease = player.packet_activity.enter_mutation().unwrap();
    let child = lease.clone();
    let completion = lease.submit(std::future::pending::<()>());
    let world = player.world();
    world.withdraw_packet_admission().await;
    player.client.java().unwrap().close();
    dispatch(player).await;
    let tracker = tokio_util::task::TaskTracker::new();
    let waiter = completion.spawn_on(&tracker, &fixture.server.runtime);
    assert!(bounded(waiter).await.unwrap_err().is_cancelled());
    let mut finish = Box::pin(crate::finish_player_session(&fixture.server, player));
    assert!(futures::poll!(&mut finish).is_pending());
    drop(child);
    bounded(finish).await.unwrap();
    fixture.finish().await;
}

#[tokio::test]
async fn error_and_cancelled_future_release_dispatch_and_waiters_without_a_tick() {
    let activity = Arc::new(PacketActivity::default());
    let lease = activity.enter_mutation().unwrap();
    assert_eq!(
        lease.scope(async { Err::<(), _>("handler failure") }).await,
        Err("handler failure")
    );
    let lease = activity.enter_mutation().unwrap();
    let future = Box::pin(lease.scope(std::future::pending::<()>()));
    let synchronous_tick = activity.enter_non_packet().unwrap();
    let mut first = Box::pin(activity.drain());
    let mut second = Box::pin(activity.drain());
    assert!(futures::poll!(&mut first).is_pending());
    assert!(futures::poll!(&mut second).is_pending());
    drop(future);
    assert!(futures::poll!(&mut first).is_pending());
    drop(synchronous_tick);
    bounded(first).await;
    bounded(second).await;
}

#[tokio::test]
async fn same_player_child_after_seal_is_owned_but_unrelated_roots_are_rejected() {
    let source = Arc::new(PacketActivity::default());
    let target = Arc::new(PacketActivity::default());
    let parent = source.enter_mutation().unwrap();
    let child = parent.sync_scope(|| target.enter_mutation().unwrap());
    source.seal();
    target.seal();
    assert!(target.enter_mutation().is_none());
    let grandchild = child.sync_scope(|| source.enter_mutation().unwrap());
    let mut source_idle = Box::pin(source.drain());
    let mut target_idle = Box::pin(target.drain());
    assert!(futures::poll!(&mut source_idle).is_pending());
    assert!(futures::poll!(&mut target_idle).is_pending());
    drop(grandchild);
    bounded(source_idle).await;
    bounded(target_idle).await;
}

#[test]
fn backlog_bounds_count_and_bytes_and_reclaims_dispatched_payload() {
    let activity = Arc::new(PacketActivity::default());
    let queue = SegQueue::new();
    let packet = || RawPacket {
        id: 1,
        payload: Bytes::new(),
    };
    for _ in 0..MAX_QUEUED_PACKETS {
        activity.enqueue(&queue, packet()).unwrap();
    }
    assert_eq!(
        activity.enqueue(&queue, packet()),
        Err(AdmissionError::Full)
    );
    for index in 0..MAX_QUEUED_PACKETS {
        if index % PACKETS_PER_EPOCH == 0 {
            activity.begin_epoch();
        }
        drop(activity.next_packet(&queue).unwrap());
    }
    activity.begin_epoch();
    assert!(activity.next_packet(&queue).is_none());
    let large = || RawPacket {
        id: 2,
        payload: Bytes::from(vec![0; MAX_QUEUED_BYTES]),
    };
    activity.enqueue(&queue, large()).unwrap();
    assert_eq!(
        activity.enqueue(
            &queue,
            RawPacket {
                id: 3,
                payload: Bytes::from_static(b"x")
            }
        ),
        Err(AdmissionError::Full)
    );
    drop(activity.next_packet(&queue).unwrap());
    activity.enqueue(&queue, large()).unwrap();
    activity.seal();
    assert_eq!(
        activity.enqueue(&queue, packet()),
        Err(AdmissionError::Closed)
    );
    assert!(activity.next_packet(&queue).is_none());
}

fn spam_debt(player: &Player, kind: crate::entity::player::SpamType) -> u32 {
    let config = pumpkin_config::AntiSpamConfig {
        message_cost: 0,
        spam_threshold: 1,
        ops_bypass: false,
        ..Default::default()
    };
    player
        .spam_counters
        .record(&config, kind, || false)
        .map_or(0, |violation| violation.count)
}

async fn real_tick(server: &Arc<Server>, frozen: bool) {
    let server = server.clone();
    bounded(tokio::task::spawn_blocking(move || {
        server.tick_rate_manager.set_frozen(&server, frozen);
        server.tick_rate_manager.tick();
        server.tick();
    }))
    .await
    .unwrap();
}

#[tokio::test]
async fn thirty_two_real_commands_and_chats_finish_without_a_player_tick() {
    use crate::entity::player::SpamType;
    use pumpkin_protocol::java::server::play::SChatMessage;
    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    let ticks = player.tick_counter.load(Ordering::Relaxed);
    // One received batch contains 32 actual async handlers, alternating lanes.
    for _ in 0..16 {
        enqueue(
            player,
            &SChatCommand {
                command: "experience add @s 1 levels",
            },
        );
        enqueue(
            player,
            &SChatMessage {
                message: "burst",
                timestamp: 0,
                salt: 0,
                signature: None,
                message_count: 0.into(),
                acknowledged: &[0, 0, 0],
                checksum: 0,
            },
        );
    }
    settled(player).await;
    assert_eq!(player.experience_level.load(Ordering::Relaxed), 16);
    assert_eq!(player.tick_counter.load(Ordering::Relaxed), ticks);
    assert_eq!(spam_debt(player, SpamType::Command), 320);
    assert_eq!(spam_debt(player, SpamType::Chat), 320);
    assert!(player.inbound_packets.is_empty());
    assert!(!player.client.closed());
    fixture.finish().await;
}

#[tokio::test]
async fn sixty_fifth_command_waits_for_the_next_epoch_not_one_tick_per_command() {
    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    for _ in 0..65 {
        enqueue(
            player,
            &SChatCommand {
                command: "experience add @s 1 levels",
            },
        );
    }
    settled(player).await;
    assert_eq!(player.experience_level.load(Ordering::Relaxed), 64);
    assert_eq!(player.inbound_packets.len(), 1);
    assert_eq!(player.packet_activity.state().remaining, 0);
    // Only packet 65 needs another scheduling epoch. The production real tick
    // replenishes it; the first 64 already completed with no gameplay tick.
    real_tick(&fixture.server, true).await;
    settled(player).await;
    assert_eq!(player.experience_level.load(Ordering::Relaxed), 65);
    assert!(player.inbound_packets.is_empty());
    fixture.finish().await;
}

#[tokio::test]
async fn held_command_orders_a_real_menu_close_while_another_player_progresses() {
    use pumpkin_protocol::java::server::play::SCloseContainer;
    let fixture = Fixture::new(2).await;
    let player = &fixture.players[0];
    let menu_player = player.clone();
    let window = bounded(tokio::task::spawn_blocking(move || {
        menu_player.open_ender_chest().unwrap()
    }))
    .await
    .unwrap();
    let barrier = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(CommandBarrier {
            player: player.gameprofile.id,
            barrier: barrier.clone(),
            descendant: None,
        }),
        EventPriority::Normal,
        true,
    );
    enqueue(
        player,
        &SChatCommand {
            command: "gamemode creative @s",
        },
    );
    enqueue(
        player,
        &SCloseContainer {
            window_id: i32::from(window).into(),
        },
    );
    dispatch(player).await;
    bounded(barrier.entered.notified()).await;
    let normal: Arc<Mutex<dyn pumpkin_inventory::screen_handler::ScreenHandler>> =
        player.player_screen_handler.clone();
    assert!(!Arc::ptr_eq(
        &player.current_screen_handler.lock().unwrap(),
        &normal
    ));
    let other = &fixture.players[1];
    enqueue(
        other,
        &SChatCommand {
            command: "experience add @s 2 levels",
        },
    );
    settled(other).await;
    assert_eq!(other.experience_level.load(Ordering::Relaxed), 2);
    assert_eq!(player.gamemode.load(), GameMode::Survival);
    barrier.release.notify_one();
    settled(player).await;
    assert_eq!(player.gamemode.load(), GameMode::Creative);
    let normal: Arc<Mutex<dyn pumpkin_inventory::screen_handler::ScreenHandler>> =
        player.player_screen_handler.clone();
    assert!(Arc::ptr_eq(
        &player.current_screen_handler.lock().unwrap(),
        &normal
    ));
    fixture.finish().await;
}

#[tokio::test]
async fn normal_and_frozen_ticks_decay_each_budget_once_while_work_is_pending() {
    use crate::entity::player::SpamType;
    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    for _ in 0..5 {
        player.check_chat_spam(&fixture.server, SpamType::Chat);
        player.check_chat_spam(&fixture.server, SpamType::Command);
    }
    let barrier = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(CommandBarrier {
            player: player.gameprofile.id,
            barrier: barrier.clone(),
            descendant: None,
        }),
        EventPriority::Normal,
        true,
    );
    enqueue(
        player,
        &SChatCommand {
            command: "experience add @s 1 levels",
        },
    );
    dispatch(player).await;
    bounded(barrier.entered.notified()).await;
    let ticks = player.tick_counter.load(Ordering::Relaxed);
    for (index, frozen) in [false, true].into_iter().enumerate() {
        real_tick(&fixture.server, frozen).await;
        assert_eq!(
            player.tick_counter.load(Ordering::Relaxed),
            ticks + index as i32 + 1
        );
        assert_eq!(spam_debt(player, SpamType::Command), 99 - index as u32);
        assert_eq!(spam_debt(player, SpamType::Chat), 99 - index as u32);
    }
    barrier.release.notify_one();
    settled(player).await;
    assert_eq!(player.tick_counter.load(Ordering::Relaxed), ticks + 2);
    assert_eq!(spam_debt(player, SpamType::Command), 118);
    assert_eq!(spam_debt(player, SpamType::Chat), 98);
    fixture.finish().await;
}

#[tokio::test]
async fn stopped_runner_still_completes_a_late_native_child_before_finals() {
    struct StopRunner(Arc<PacketRunner>);
    impl Drop for StopRunner {
        fn drop(&mut self) {
            self.0.finish_drain();
        }
    }

    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    let barrier = Arc::new(Barrier::default());
    let child = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(CommandBarrier {
            player: player.gameprofile.id,
            barrier: barrier.clone(),
            descendant: Some(child.clone()),
        }),
        EventPriority::Normal,
        true,
    );
    enqueue(
        player,
        &SChatCommand {
            command: "gamemode creative @s",
        },
    );
    dispatch(player).await;
    bounded(barrier.entered.notified()).await;
    let ticks = player.tick_counter.load(Ordering::Relaxed);
    fixture.server.packet_runner.begin_drain();
    let _stop_runner = StopRunner(fixture.server.packet_runner.clone());
    let server = fixture.server.clone();
    let drain_loop =
        tokio::task::spawn_blocking(move || crate::server::ticker::Ticker::drain_sessions(&server));
    // begin_drain also fences a direct normal/frozen tick request.
    real_tick(&fixture.server, true).await;
    assert_eq!(player.tick_counter.load(Ordering::Relaxed), ticks);
    let mut finish = Box::pin(crate::finish_player_session(&fixture.server, player));
    assert!(futures::poll!(&mut finish).is_pending());
    barrier.release.notify_one();
    bounded(child.entered.notified()).await;
    assert!(futures::poll!(&mut finish).is_pending());
    child.release.notify_one();
    bounded(finish).await.unwrap();
    assert_eq!(player.tick_counter.load(Ordering::Relaxed), ticks);
    assert_eq!(
        fixture
            .server
            .player_data_storage
            .load_data(&player.gameprofile.id)
            .unwrap()
            .unwrap()
            .get_int("XpLevel"),
        Some(7)
    );
    fixture.server.packet_runner.finish_drain();
    bounded(drain_loop).await.unwrap();
    assert!(fixture.server.packet_runner.is_finished());
    fixture.finish().await;
}

#[tokio::test]
async fn connected_transfer_and_respawn_finish_before_following_input() {
    let fixture = Fixture::with_nether(1, true).await;
    let player = &fixture.players[0];
    player.set_client_loaded(true);
    let source = player.world();
    enqueue(
        player,
        &SChatCommand {
            command: "execute in minecraft:the_nether run tp @s 2 90 2",
        },
    );
    enqueue(player, &SSetHeldItem { slot: 6 });
    settled(player).await;
    assert!(!Arc::ptr_eq(&source, &player.world()));
    assert_eq!(
        player.world().dimension,
        pumpkin_data::dimension::Dimension::THE_NETHER
    );
    assert_eq!(player.inventory.get_selected_slot(), 6);
    assert!(!player.client.closed());
    player.living_entity.health.store(0.0);
    enqueue(
        player,
        &SClientCommand::new(SClientCommand::PERFORM_RESPAWN.into()),
    );
    enqueue(player, &SSetHeldItem { slot: 3 });
    settled(player).await;
    assert!(Arc::ptr_eq(&source, &player.world()));
    assert!(player.living_entity.health.load() > 0.0);
    assert_eq!(player.inventory.get_selected_slot(), 3);
    assert!(!player.client.closed());
    fixture.finish().await;
}

#[tokio::test]
async fn readiness_withdrawal_parks_input_and_callback_retirement_waits() {
    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    let world = player.world();
    let position = player.get_entity().chunk_pos.load();
    let callback = world.enter_packet_callbacks(Some(position)).unwrap();
    let mut withdraw = Box::pin(world.withdraw_packet_admission());
    assert!(futures::poll!(&mut withdraw).is_pending());
    assert!(world.enter_packet_callbacks(Some(position)).is_none());
    drop(callback);
    bounded(withdraw).await;
    enqueue(player, &SSetHeldItem { slot: 5 });
    dispatch(player).await;
    assert_eq!(player.inventory.get_selected_slot(), 0);
    let chunk = world
        .level
        .loaded_chunks
        .get(&position)
        .unwrap()
        .value()
        .clone();
    chunk.post_processing_ready.store(false, Ordering::Release);
    world.restore_packet_admission();
    dispatch(player).await;
    assert_eq!(player.inventory.get_selected_slot(), 0);
    chunk.post_processing_ready.store(true, Ordering::Release);
    fixture.server.packet_runner.retry_worlds();
    settled(player).await;
    assert_eq!(player.inventory.get_selected_slot(), 5);
    fixture.finish().await;
}

#[tokio::test]
async fn parked_command_does_not_hide_a_ready_world_reconciliation() {
    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    let barrier = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(CommandBarrier {
            player: player.gameprofile.id,
            barrier: barrier.clone(),
            descendant: None,
        }),
        EventPriority::Normal,
        true,
    );
    enqueue(
        player,
        &SChatCommand {
            command: "experience add @s 1 levels",
        },
    );
    enqueue(player, &SSetHeldItem { slot: 5 });
    dispatch(player).await;
    bounded(barrier.entered.notified()).await;
    let world = player.world();
    world.withdraw_packet_admission().await;
    barrier.release.notify_one();
    let activity = player.packet_activity.enter_mutation().unwrap();
    player
        .client
        .java()
        .unwrap()
        .spawn_world_activity_task(activity, async move {
            world.restore_packet_admission();
        });
    let ticks = player.tick_counter.load(Ordering::Relaxed);
    settled(player).await;
    assert_eq!(player.experience_level.load(Ordering::Relaxed), 1);
    assert_eq!(player.inventory.get_selected_slot(), 5);
    assert_eq!(player.tick_counter.load(Ordering::Relaxed), ticks);
    fixture.finish().await;
}

#[tokio::test]
async fn close_discards_unadmitted_input_when_chunk_readiness_never_returns() {
    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    let world = player.world();
    let chunk = world
        .level
        .loaded_chunks
        .get(&player.get_entity().chunk_pos.load())
        .unwrap()
        .value()
        .clone();
    chunk.post_processing_ready.store(false, Ordering::Release);
    enqueue(player, &SSetHeldItem { slot: 5 });
    dispatch(player).await;
    assert_eq!(player.inventory.get_selected_slot(), 0);
    assert_eq!(player.packet_activity.state().active, 0);
    assert_eq!(player.inbound_packets.len(), 1);

    drive_until(
        &fixture.server,
        crate::finish_player_session(&fixture.server, player),
    )
    .await
    .unwrap();
    assert!(player.inbound_packets.is_empty());
    assert_eq!(player.inventory.get_selected_slot(), 0);
    assert_eq!(player.packet_activity.state().active, 0);
    assert!(!chunk.post_processing_ready.load(Ordering::Acquire));
    assert!(
        fixture
            .server
            .player_data_storage
            .load_data(&player.gameprofile.id)
            .unwrap()
            .is_some()
    );
    fixture.finish().await;
}

#[tokio::test]
async fn actual_socket_ingress_accepts_a_command_burst_and_exact_keepalive() {
    let mut fixture = Fixture::new(1).await;
    let player = fixture.players[0].clone();
    let peer = fixture.peers.remove(0);
    let (_reader, writer) = peer.into_split();
    let mut encoder = TCPNetworkEncoder::new(writer);
    let server = fixture.server.clone();
    let receiving = player.clone();
    let transport = tokio::spawn(async move {
        receiving
            .client
            .java()
            .unwrap()
            .progress_player_packets(&receiving, &server)
            .await;
    });
    for _ in 0..32 {
        encoder
            .write_packet(
                JavaClient::serialize_packet_for_version(
                    &SChatCommand {
                        command: "experience add @s 1 levels",
                    },
                    CURRENT_MC_VERSION,
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }
    let client = player.client.java().unwrap();
    assert!(
        client
            .send_keep_alive_or_timeout(Duration::from_secs(30))
            .await
    );
    let id = client.keep_alive_id.load();
    encoder
        .write_packet(
            JavaClient::serialize_packet_for_version(
                &SKeepAlive { keep_alive_id: id },
                CURRENT_MC_VERSION,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    encoder.flush().await.unwrap();
    let ticks = player.tick_counter.load(Ordering::Relaxed);
    drive_until(
        &fixture.server,
        std::future::poll_fn(|_| {
            if player.experience_level.load(Ordering::Relaxed) == 32
                && client.pending_keep_alives.lock().unwrap().is_empty()
            {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        }),
    )
    .await;
    assert_eq!(player.tick_counter.load(Ordering::Relaxed), ticks);
    client.close();
    bounded(transport).await.unwrap();
    fixture.finish().await;
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep the real block mutation, racing tick and outgoing ACK ordering assertions together."
)]
async fn actual_block_update_precedes_covering_ack_when_completion_races_a_tick() {
    use pumpkin_protocol::{
        ServerPacket,
        java::{
            client::play::{CAcknowledgeBlockChange, CBlockUpdate, CMultiBlockUpdate},
            server::play::SUseItemOn,
        },
        ser::NetworkReadExt,
    };
    use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
    let mut fixture = Fixture::new(1).await;
    let player = fixture.players[0].clone();
    let world = player.world();
    let position = BlockPos::new(1, 65, 1);
    world
        .level
        .read_chunk_sync(&position.chunk_position(), |chunk| {
            chunk.set_block_absolute_y(1, 64, 1, pumpkin_data::Block::STONE.default_state.id);
            chunk.set_block_absolute_y(1, 65, 1, pumpkin_data::Block::LEVER.default_state.id);
        })
        .unwrap();
    player.set_client_loaded(true);
    let before = world.get_block_state_id(&position);
    let barrier = Arc::new(Barrier::default());
    fixture.server.plugin_manager.register(
        Arc::new(CommandBarrier {
            player: player.gameprofile.id,
            barrier: barrier.clone(),
            descendant: None,
        }),
        EventPriority::Normal,
        true,
    );
    enqueue(
        &player,
        &SChatCommand {
            command: "experience add @s 1 levels",
        },
    );
    enqueue(
        &player,
        &SUseItemOn {
            hand: 0.into(),
            position,
            face: 1.into(),
            cursor_pos: Vector3::new(0.5, 0.5, 0.5),
            inside_block: false,
            is_against_world_border: false,
            sequence: 77.into(),
        },
    );
    dispatch(&player).await;
    bounded(barrier.entered.notified()).await;
    let server = fixture.server.clone();
    let racing_tick = tokio::task::spawn_blocking(move || {
        server.tick_rate_manager.set_frozen(&server, true);
        server.tick_rate_manager.tick();
        server.tick();
    });
    barrier.release.notify_one();
    settled(&player).await;
    bounded(racing_tick).await.unwrap();
    assert_ne!(
        world.get_block_state_id(&position),
        before,
        "the actual use-item handler toggled the lever"
    );
    // If the race tick preceded the resumed mutation, the following real tick
    // owns its covering ACK. Packet completion itself must never send that ACK.
    real_tick(&fixture.server, true).await;
    let (reader, _writer) = fixture.peers.remove(0).into_split();
    let mut decoder = TCPNetworkDecoder::new(reader);
    let mut observed_update = false;
    bounded(async {
        loop {
            let packet = decoder.get_raw_packet().await.unwrap();
            if packet.id == CBlockUpdate::to_id(CURRENT_MC_VERSION) {
                let location = (&packet.payload[..])
                    .get_block_pos(&CURRENT_MC_VERSION)
                    .unwrap();
                if location == position {
                    observed_update = true;
                }
            } else if packet.id == CMultiBlockUpdate::to_id(CURRENT_MC_VERSION) {
                let update =
                    CMultiBlockUpdate::read(&mut &packet.payload[..], &CURRENT_MC_VERSION).unwrap();
                observed_update |= update
                    .updates
                    .iter()
                    .any(|(updated, _)| *updated == position);
            } else if packet.id == CAcknowledgeBlockChange::to_id(CURRENT_MC_VERSION) {
                let sequence = (&packet.payload[..]).get_var_int().unwrap().0;
                if sequence == 77 {
                    assert!(
                        observed_update,
                        "outgoing authoritative block update must precede its covering ACK"
                    );
                    break;
                }
            }
        }
    })
    .await;
    fixture.finish().await;
}

#[tokio::test]
async fn respawn_is_owned_before_first_poll_and_close_discards_later_input() {
    let fixture = Fixture::new(1).await;
    let player = &fixture.players[0];
    player.living_entity.health.store(0.0);
    enqueue(
        player,
        &SClientCommand::new(SClientCommand::PERFORM_RESPAWN.into()),
    );
    enqueue(player, &SSetHeldItem { slot: 6 });
    let server = fixture.server.clone();
    // A single production wave accepts the deferred root. Its future is queued
    // for the next wave, but has not been polled or spawned its respawn child.
    bounded(tokio::task::spawn_blocking(move || {
        server.packet_runner.run_ready(&server, None)
    }))
    .await
    .unwrap();
    assert_eq!(player.inbound_packets.len(), 1);
    assert_eq!(player.inventory.get_selected_slot(), 0);
    let mut finish = Box::pin(crate::finish_player_session(&fixture.server, player));
    assert!(futures::poll!(&mut finish).is_pending());
    drive_until(&fixture.server, finish).await.unwrap();
    assert_eq!(player.living_entity.health.load(), 0.0);
    assert!(player.inbound_packets.is_empty());
    fixture.finish().await;
}

/// A real command hook that stays runnable across several ticker epochs. A
/// self-wake must not turn the mandatory opportunity into an unbounded drain.
struct TickerCommandPolls {
    player: Uuid,
    polls: std::sync::atomic::AtomicUsize,
    finish_on: usize,
}

impl EventHandler<PlayerCommandSendEvent> for TickerCommandPolls {
    fn handle_blocking<'a>(
        &'a self,
        server: &'a Arc<Server>,
        event: &'a mut PlayerCommandSendEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.player.gameprofile.id != self.player {
                return;
            }
            std::future::poll_fn(|cx| {
                assert!(server.packet_runner.phase.try_lock().is_err());
                let poll = self.polls.fetch_add(1, Ordering::Relaxed) + 1;
                assert_eq!(
                    server.tick_count.load(Ordering::Relaxed),
                    poll as i32,
                    "the ready poll follows the completed gameplay epoch"
                );
                if poll < self.finish_on {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
        })
    }
}

const fn overdue_schedule() -> crate::server::ticker::TickSchedule {
    // The deadline stays negative throughout these short controls. The recent
    // warning prevents overload skip from granting slack, independent of speed.
    crate::server::ticker::TickSchedule {
        next_tick_nanos: -60_000_000_000,
        last_overload_warning_nanos: 0,
    }
}

#[tokio::test]
async fn ticker_sprint_completes_command_chat_and_menu_before_sprint_ends() {
    use crate::{
        entity::player::SpamType,
        server::ticker::{TickSchedule, Ticker},
    };
    use pumpkin_gametest::GameTestRunner;
    use pumpkin_protocol::java::server::play::{SChatMessage, SCloseContainer};

    let fixture = Fixture::new(1).await;
    let player = fixture.players[0].clone();
    let server = fixture.server.clone();
    bounded(tokio::task::spawn_blocking(move || {
        let _runtime = server.runtime.enter();
        server.packet_runner.attach();
        let window = player.open_ender_chest().unwrap();
        let normal: Arc<Mutex<dyn pumpkin_inventory::screen_handler::ScreenHandler>> =
            player.player_screen_handler.clone();
        assert!(!Arc::ptr_eq(
            &player.current_screen_handler.lock().unwrap(),
            &normal
        ));
        for _ in 0..5 {
            player.check_chat_spam(&server, SpamType::Command);
            player.check_chat_spam(&server, SpamType::Chat);
        }
        let hook = Arc::new(TickerCommandPolls {
            player: player.gameprofile.id,
            polls: std::sync::atomic::AtomicUsize::new(0),
            finish_on: 3,
        });
        server
            .plugin_manager
            .register(hook.clone(), EventPriority::Normal, true);
        enqueue(
            &player,
            &SChatCommand {
                command: "experience add @s 1 levels",
            },
        );
        enqueue(
            &player,
            &SChatMessage {
                message: "sprint continuation",
                timestamp: 0,
                salt: 0,
                signature: None,
                message_count: 0.into(),
                acknowledged: &[0, 0, 0],
                checksum: 0,
            },
        );
        enqueue(
            &player,
            &SCloseContainer {
                window_id: i32::from(window).into(),
            },
        );
        server
            .tick_rate_manager
            .request_game_to_sprint(&server, 1_000);
        let base = Instant::now();
        let mut schedule = TickSchedule::new();
        let mut game_tests = GameTestRunner::new();
        let ticks = player.tick_counter.load(Ordering::Relaxed);
        let server_ticks = server.tick_count.load(Ordering::Relaxed);
        let mut chat_debt = 100;
        for epoch in 1..=16 {
            // Exactly the production loop body, including its real deadline and
            // ready phase. No direct pump or extra Player::tick in this harness.
            Ticker::run_epoch(&server, base, &mut schedule, &mut game_tests);
            assert!(server.tick_rate_manager.is_sprinting());
            assert_eq!(player.tick_counter.load(Ordering::Relaxed), ticks + epoch);
            assert_eq!(
                server.tick_count.load(Ordering::Relaxed),
                server_ticks + epoch
            );
            assert!(schedule.next_tick_nanos <= base.elapsed().as_nanos() as i64);
            assert_eq!(hook.polls.load(Ordering::Relaxed), (epoch as usize).min(3));
            let command_charge = if epoch >= 3 { 20 } else { 0 };
            assert_eq!(
                spam_debt(&player, SpamType::Command),
                100 + command_charge - epoch as u32
            );
            let next_chat_debt = spam_debt(&player, SpamType::Chat);
            assert!(next_chat_debt == chat_debt - 1 || next_chat_debt == chat_debt + 19);
            chat_debt = next_chat_debt;
            let menu_closed = Arc::ptr_eq(&player.current_screen_handler.lock().unwrap(), &normal);
            if menu_closed {
                assert_eq!(player.experience_level.load(Ordering::Relaxed), 1);
                assert_eq!(chat_debt, 120 - epoch as u32);
                assert!(player.inbound_packets.is_empty());
                assert_eq!(player.packet_activity.state().active, 0);
                assert!(!player.client.closed());
                return;
            }
        }
        panic!("command, chat and menu must complete while the long sprint is still active");
    }))
    .await
    .unwrap();
    fixture.finish().await;
}

#[tokio::test]
async fn ticker_continuously_overdue_epochs_poll_once_and_other_player_progresses() {
    use crate::{entity::player::SpamType, server::ticker::Ticker};
    use pumpkin_gametest::GameTestRunner;

    let fixture = Fixture::new(2).await;
    let player = fixture.players[0].clone();
    let other = fixture.players[1].clone();
    let server = fixture.server.clone();
    bounded(tokio::task::spawn_blocking(move || {
        let _runtime = server.runtime.enter();
        server.packet_runner.attach();
        let hook = Arc::new(TickerCommandPolls {
            player: player.gameprofile.id,
            polls: std::sync::atomic::AtomicUsize::new(0),
            finish_on: 4,
        });
        server
            .plugin_manager
            .register(hook.clone(), EventPriority::Normal, true);
        for _ in 0..5 {
            player.check_chat_spam(&server, SpamType::Command);
            player.check_chat_spam(&server, SpamType::Chat);
        }
        enqueue(
            &player,
            &SChatCommand {
                command: "experience add @s 1 levels",
            },
        );
        enqueue(&player, &SSetHeldItem { slot: 6 });
        enqueue(
            &other,
            &SChatCommand {
                command: "experience add @s 2 levels",
            },
        );
        let base = Instant::now();
        let mut schedule = overdue_schedule();
        let mut game_tests = GameTestRunner::new();
        let ticks = player.tick_counter.load(Ordering::Relaxed);
        let other_ticks = other.tick_counter.load(Ordering::Relaxed);
        let server_ticks = server.tick_count.load(Ordering::Relaxed);
        for epoch in 1..=5 {
            Ticker::run_epoch(&server, base, &mut schedule, &mut game_tests);
            assert!(schedule.next_tick_nanos < 0, "there was no slack wave");
            assert_eq!(schedule.last_overload_warning_nanos, 0);
            assert_eq!(hook.polls.load(Ordering::Relaxed), (epoch as usize).min(4));
            assert_eq!(player.tick_counter.load(Ordering::Relaxed), ticks + epoch);
            assert_eq!(
                other.tick_counter.load(Ordering::Relaxed),
                other_ticks + epoch
            );
            assert_eq!(
                server.tick_count.load(Ordering::Relaxed),
                server_ticks + epoch
            );
            assert_eq!(other.experience_level.load(Ordering::Relaxed), 2);
            assert_eq!(spam_debt(&player, SpamType::Chat), 100 - epoch as u32);
            let completed = epoch >= 4;
            assert_eq!(
                player.experience_level.load(Ordering::Relaxed),
                i32::from(completed)
            );
            assert_eq!(
                spam_debt(&player, SpamType::Command),
                100 + if completed { 20 } else { 0 } - epoch as u32
            );
            assert_eq!(
                player.inventory.get_selected_slot(),
                if epoch == 5 { 6 } else { 0 }
            );
            if epoch < 4 {
                assert_eq!(player.packet_activity.state().remaining, 63);
                assert_eq!(player.inbound_packets.len(), 1);
            }
        }
        assert!(player.inbound_packets.is_empty());
        assert_eq!(player.packet_activity.state().active, 0);
        assert!(!player.client.closed());
    }))
    .await
    .unwrap();
    fixture.finish().await;
}

#[tokio::test]
async fn ticker_overdue_without_ready_work_ticks_and_decays_once_normal_or_frozen() {
    use crate::{entity::player::SpamType, server::ticker::Ticker};
    use pumpkin_gametest::GameTestRunner;

    let fixture = Fixture::new(1).await;
    let player = fixture.players[0].clone();
    let server = fixture.server.clone();
    bounded(tokio::task::spawn_blocking(move || {
        let _runtime = server.runtime.enter();
        server.packet_runner.attach();
        player.check_chat_spam(&server, SpamType::Command);
        player.check_chat_spam(&server, SpamType::Chat);
        assert!(server.packet_runner.ready.is_empty());
        let base = Instant::now();
        let mut schedule = overdue_schedule();
        let mut game_tests = GameTestRunner::new();
        let ticks = player.tick_counter.load(Ordering::Relaxed);
        let server_ticks = server.tick_count.load(Ordering::Relaxed);
        for epoch in 1..=4 {
            server.tick_rate_manager.set_frozen(&server, epoch % 2 == 0);
            Ticker::run_epoch(&server, base, &mut schedule, &mut game_tests);
            assert!(schedule.next_tick_nanos < 0);
            assert_eq!(schedule.last_overload_warning_nanos, 0);
            assert_eq!(player.tick_counter.load(Ordering::Relaxed), ticks + epoch);
            assert_eq!(
                server.tick_count.load(Ordering::Relaxed),
                server_ticks + epoch
            );
            assert_eq!(spam_debt(&player, SpamType::Command), 20 - epoch as u32);
            assert_eq!(spam_debt(&player, SpamType::Chat), 20 - epoch as u32);
            assert!(server.packet_runner.ready.is_empty());
            assert!(player.inbound_packets.is_empty());
            assert_eq!(player.packet_activity.state().active, 0);
        }
    }))
    .await
    .unwrap();
    fixture.finish().await;
}
