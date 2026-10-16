//! Exercise the real respawn transition and player.dat writer. The shared fixture
//! stops chunk workers; only the loopback client's outgoing packet task runs.

use super::World;
use crate::command::commands::data::tests::Fixture;
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
use std::{sync::Arc, sync::atomic::Ordering, time::Duration};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

const VANILLA_223_BITS: u32 = 0x3e67_39c4;

struct ConnectedPlayer {
    player: Arc<Player>,
    _peer: TcpStream,
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
        Self {
            player,
            _peer: peer,
        }
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
    let fixture = Fixture::new().await;
    let world = fixture.source.world();
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
    let fixture = Fixture::new().await;
    let world = fixture.source.world();
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
}
