//! Exercise the real respawn transition and player.dat writer. The shared fixture
//! stops chunk workers; only the loopback client's outgoing packet task runs.

use super::World;
use crate::data::player_server::test::{close_fixture, player_fixture, stop_chunk_workers};
use crate::entity::{EntityBase, player::Player};
use crate::net::{
    ClientPlatform, GameProfile, PacketRateLimiter, PlayerConfig,
    java::{JavaClient, pending::PendingConnection},
};
use arc_swap::ArcSwap;
use pumpkin_data::{item::Item, item_stack::ItemStack};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::{GameMode, math::vector3::Vector3};
use pumpkin_world::data::player_data::PlayerDataStorage;
use std::{future::Future, sync::Arc, sync::atomic::Ordering, task::Poll, time::Duration};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

const VANILLA_223_BITS: u32 = 0x3e67_39c4;

/// Drive admitted packet tasks without adding a gameplay tick or receiving input.
async fn drive_packet_future<F: Future>(
    server: &Arc<crate::server::Server>,
    future: F,
) -> F::Output {
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::pin!(future);
        loop {
            if let Poll::Ready(value) =
                std::future::poll_fn(|cx| Poll::Ready(future.as_mut().poll(cx))).await
            {
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
    .unwrap()
}

struct ConnectedPlayer {
    player: Arc<Player>,
    peer: TcpStream,
}

impl ConnectedPlayer {
    async fn new(world: &Arc<World>, mode: GameMode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (peer, accepted) = tokio::join!(TcpStream::connect(address), listener.accept());
        let peer = peer.unwrap();
        let (socket, address) = accepted.unwrap();
        let profile = GameProfile {
            id: Uuid::new_v4(),
            name: "RespawnScore".into(),
            properties: ArcSwap::from_pointee(Vec::new()),
            profile_actions: None,
        };
        let pending = PendingConnection::new(
            socket,
            address,
            0,
            PacketRateLimiter::new(false, 0.0, 0.0),
            world.server.clone(),
        );
        let mut client =
            JavaClient::from_pending(pending, profile.clone(), PlayerConfig::default());
        // Respawn waits for a high-priority packet's writer completion.
        client.start_outgoing_packet_task();
        let player = Arc::new(Player::new(
            Arc::new(ClientPlatform::Java(client)),
            profile,
            PlayerConfig::default(),
            world,
            mode,
        ));
        world.players.store(Arc::new(vec![player.clone()]));
        Self { player, peer }
    }
}

impl Drop for ConnectedPlayer {
    fn drop(&mut self) {
        let world = self.player.world();
        world.players.store(Arc::new(Vec::new()));
        world
            .entity_tracker
            .remove_entity(self.player.as_ref(), &world);
        if let ClientPlatform::Java(client) = self.player.client.as_ref() {
            client.close();
        }
    }
}

async fn respawned_player_data(mode: GameMode, keep_inventory: bool, alive: bool) -> NbtCompound {
    let fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    let world = &fixture.player.world();
    world.level_info.rcu(|current| {
        let mut info = (**current).clone();
        info.spawn_x = 8;
        info.spawn_y = 64;
        info.spawn_z = 8;
        info.game_rules.keep_inventory = keep_inventory;
        info
    });
    let connected = ConnectedPlayer::new(world, mode).await;
    let player = &connected.player;
    player.add_experience_points(223);
    assert_eq!(player.score.load(Ordering::Relaxed), 223);
    assert_eq!(player.get_experience_level(), 12);
    assert_eq!(player.get_experience_progress().to_bits(), VANILLA_223_BITS);
    assert_eq!(player.get_total_experience(), 223);
    player
        .inventory
        .set_slot(0, ItemStack::new(3, &Item::STONE));
    if !alive {
        player.living_entity.health.store(0.0);
    }

    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), world.respawn_player(player, alive))
            .await
            .expect("real respawn transition must complete with stopped chunk workers"),
        super::RespawnOutcome::Committed,
    );
    assert!(player.living_entity.health.load() > 0.0);
    assert_eq!(player.get_entity().pos.load(), Vector3::new(8.5, 64.0, 8.5));

    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path(), true);
    let snapshot = storage.snapshot_player_data(player.gameprofile.id, |nbt| {
        EntityBase::write_nbt(player.as_ref() as &dyn EntityBase, nbt);
    });
    storage.save_player_data(snapshot).unwrap();
    let (found, saved) = storage.load_player_data(&player.gameprofile.id).unwrap();
    assert!(found);
    drop(connected);
    world.players.store(Arc::new(vec![fixture.player.clone()]));
    close_fixture(&fixture).await;
    saved
}

#[tokio::test]
async fn normal_death_resets_score_and_experience_in_player_dat() {
    let saved = respawned_player_data(GameMode::Survival, false, false).await;
    assert_eq!(saved.get_int("Score"), Some(0));
    assert_eq!(saved.get_int("XpLevel"), Some(0));
    assert_eq!(saved.get_int("XpTotal"), Some(0));
    assert_eq!(saved.get_float("XpP").map(f32::to_bits), Some(0));
    assert!(saved.get_list("Inventory").unwrap().is_empty());
}

#[tokio::test]
async fn respawn_preserves_score_and_experience_for_keep_inventory_spectator_and_keep_all() {
    // Official 26.3 restoreFrom calls transferInventoryXpAndScore for each case.
    for (mode, keep_inventory, alive) in [
        (GameMode::Survival, true, false),
        (GameMode::Spectator, false, false),
        (GameMode::Survival, false, true),
    ] {
        let saved = respawned_player_data(mode, keep_inventory, alive).await;
        assert_eq!(saved.get_int("Score"), Some(223));
        assert_eq!(saved.get_int("XpLevel"), Some(12));
        assert_eq!(saved.get_int("XpTotal"), Some(223));
        assert_eq!(
            saved.get_float("XpP").map(f32::to_bits),
            Some(VANILLA_223_BITS)
        );
        let inventory = saved.get_list("Inventory").unwrap();
        assert_eq!(inventory.len(), 1);
        let item = inventory[0].extract_compound().unwrap();
        assert_eq!(item.get_string("id"), Some("minecraft:stone"));
        assert_eq!(item.get_int("count"), Some(3));
    }
}

#[tokio::test]
async fn world_info_refresh_preserves_live_health() {
    let fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    let world = &fixture.player.world();
    let connected = ConnectedPlayer::new(world, GameMode::Survival).await;
    let player = &connected.player;
    player.set_client_loaded(true);
    player.set_health(7.0);
    player.hunger_manager.level.store(13);
    player.hunger_manager.saturation.store(2.5);

    world.send_world_info(player);

    assert_eq!(player.living_entity.health.load(), 7.0);
    assert_eq!(player.hunger_manager.level.load(), 13);
    assert_eq!(player.hunger_manager.saturation.load(), 2.5);
    let mut saved = NbtCompound::new();
    EntityBase::write_nbt(player.as_ref() as &dyn EntityBase, &mut saved);
    assert_eq!(saved.get_float("Health"), Some(7.0));
    drop(connected);
    world.players.store(Arc::new(vec![fixture.player.clone()]));
    close_fixture(&fixture).await;
}

/// Cancel the bed world's readiness wait while the accepted source remains usable.
/// This must not turn cancellation into an invalid-bed notification or saved bed deletion.
#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep the real two-world wait, wire barrier and persisted-state assertions together."
)]
async fn cancelled_bed_world_wait_preserves_respawn_data_and_sends_no_invalid_bed_event() {
    use crate::entity::player::RespawnPoint;
    use pumpkin_data::{dimension::Dimension, packet::CURRENT_MC_VERSION};
    use pumpkin_protocol::{
        MultiVersionJavaPacket,
        java::{
            client::play::{CGameEvent, CKeepAlive, GameEvent},
            packet_decoder::TCPNetworkDecoder,
        },
    };
    use pumpkin_util::math::{position::BlockPos, vector2::Vector2};
    use pumpkin_world::{chunk::ChunkData, chunk_system::ChunkLoading};

    let fixture = player_fixture().await;
    let bed_fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    stop_chunk_workers(&bed_fixture).await;
    let world = fixture.player.world();
    let bed_level = bed_fixture.player.world().level.clone();
    let bed_world = Arc::new(World::load(
        bed_level.clone(),
        bed_fixture.player.world().level_info.clone(),
        Dimension::THE_NETHER,
        world.block_registry.clone(),
        Arc::downgrade(&fixture.server),
        world.scoreboard.clone(),
    ));
    fixture
        .server
        .worlds
        .store(Arc::new(vec![world.clone(), bed_world]));
    let source = Vector2::new(0, 0);
    let source_chunk = ChunkData::empty_sync(0, 0);
    source_chunk
        .post_processing_ready
        .store(true, Ordering::Release);
    world.level.loaded_chunks.insert(source, source_chunk);
    {
        let mut loading = world.level.chunk_loading.lock().unwrap();
        loading.add_ticket(source, ChunkLoading::FULL_CHUNK_LEVEL);
        loading.send_change();
    };
    let bed_chunk = Vector2::new(4, 4);
    bed_level
        .loaded_chunks
        .insert(bed_chunk, ChunkData::empty_sync(4, 4));
    let mut connected = ConnectedPlayer::new(&world, GameMode::Survival).await;
    let player = &connected.player;
    player.get_entity().set_pos(Vector3::new(0.5, 65.0, 0.5));
    player.get_entity().last_pos.store(player.position());
    player.living_entity.health.store(0.0);
    player.hunger_manager.level.store(13);
    player.hunger_manager.saturation.store(2.5);
    let bed = RespawnPoint {
        dimension: Dimension::THE_NETHER,
        position: BlockPos::new(72, 65, 72),
        yaw: 37.0,
        force: false,
    };
    *player.respawn_point.lock().unwrap() = Some(bed.clone());
    let mut before = NbtCompound::new();
    EntityBase::write_nbt(player.as_ref() as &dyn EntityBase, &mut before);
    let java = player.client.java().unwrap();
    java.set_player(player);
    assert!(world.enter_packet_callbacks(Some(source)).is_some());
    let source_tickets = world.level.chunk_loading.lock().unwrap().ticket.clone();
    let bed_tickets = bed_level.chunk_loading.lock().unwrap().ticket.clone();
    let activity = player.packet_activity.enter_mutation().unwrap();
    {
        let respawn = activity.scope(world.respawn_player(player, false));
        tokio::pin!(respawn);
        drive_packet_future(
            &fixture.server,
            std::future::poll_fn(|cx| {
                assert!(respawn.as_mut().poll(cx).is_pending());
                if bed_level.chunk_loading.lock().unwrap().ticket != bed_tickets {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }),
        )
        .await;
        assert!(!bed_level.is_chunk_ready(&bed_chunk));
        assert_ne!(bed_level.chunk_loading.lock().unwrap().ticket, bed_tickets);
        assert_eq!(*player.respawn_point.lock().unwrap(), Some(bed.clone()));
        assert!(!java.is_closed());
        bed_level.cancel_token.cancel();
        assert_eq!(
            drive_packet_future(&fixture.server, respawn).await,
            super::RespawnOutcome::Aborted,
        );
    };
    assert!(!java.is_closed());
    assert!(world.enter_packet_callbacks(Some(source)).is_some());
    assert_eq!(
        world.level.chunk_loading.lock().unwrap().ticket,
        source_tickets
    );
    assert_eq!(bed_level.chunk_loading.lock().unwrap().ticket, bed_tickets);
    assert_eq!(*player.respawn_point.lock().unwrap(), Some(bed));
    assert_eq!(player.gamemode.load(), GameMode::Survival);
    assert_eq!(player.living_entity.health.load(), 0.0);
    assert_eq!(player.hunger_manager.level.load(), 13);
    assert_eq!(player.hunger_manager.saturation.load(), 2.5);
    assert_eq!(player.position(), Vector3::new(0.5, 65.0, 0.5));

    // A FIFO wire barrier proves no earlier packet was hidden by writer scheduling.
    tokio::time::timeout(
        Duration::from_secs(10),
        java.send_packet(&CKeepAlive::new(26)),
    )
    .await
    .unwrap();
    java.flush_channel();
    let mut decoder = TCPNetworkDecoder::new(&mut connected.peer);
    let packet = tokio::time::timeout(Duration::from_secs(10), decoder.get_raw_packet())
        .await
        .unwrap()
        .unwrap();
    assert!(
        packet.id != CGameEvent::to_id(CURRENT_MC_VERSION)
            || packet.payload.first() != Some(&(GameEvent::NoRespawnBlockAvailable as u8))
    );
    assert_eq!(
        packet.id,
        CKeepAlive::to_id(CURRENT_MC_VERSION),
        "cancelled respawn must send no packets before the barrier"
    );
    assert_eq!(packet.payload.as_ref(), &26i64.to_be_bytes());
    drop(decoder);

    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path(), true);
    let snapshot = storage.snapshot_player_data(player.gameprofile.id, |nbt| {
        EntityBase::write_nbt(player.as_ref() as &dyn EntityBase, nbt);
    });
    storage.save_player_data(snapshot).unwrap();
    let (found, saved) = storage.load_player_data(&player.gameprofile.id).unwrap();
    assert!(found);
    for key in ["respawn", "abilities"] {
        assert_eq!(saved.get_compound(key), before.get_compound(key));
    }
    assert!(saved.get_compound("respawn").is_some());
    assert_eq!(
        saved.get_int("playerGameType"),
        before.get_int("playerGameType")
    );
    assert_eq!(saved.get_float("Health"), Some(0.0));
    drop(connected);
    world.players.store(Arc::new(vec![fixture.player.clone()]));
    fixture.server.worlds.store(Arc::new(vec![world]));
    close_fixture(&fixture).await;
    close_fixture(&bed_fixture).await;
}
