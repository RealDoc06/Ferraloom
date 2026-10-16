//! Exercise `Server::tick` with stopped chunk
//! workers, registered Players and real menu factories; no socket tasks run.
//! Pinned 26.3 ServerLevel.tick gates runBlockEvents at offsets 354-360; its
//! runBlockEvents removes the first entry before callbacks and loops until empty.

use super::World;
use crate::{
    block::{
        BlockBehaviour, BlockMetadata, OnSyncedBlockEventArgs,
        blocks::{
            barrel::barrel_screen_handler_factory,
            chests::{ChestBlock, chest_screen_handler_factory},
        },
        entities::{BlockEntity, barrel::BarrelBlockEntity, chest::ChestBlockEntity},
        registry::default_registry,
    },
    command::{CommandSource, commands::data::tests::Fixture},
    entity::player::Player,
    net::{
        ClientPlatform, GameProfile, PacketRateLimiter, PlayerConfig,
        java::{JavaClient, pending::PendingConnection},
    },
};
use arc_swap::ArcSwap;
use pumpkin_data::{Block, BlockId, block_properties::BarrelLikeProperties};
use pumpkin_inventory::screen_handler::ScreenHandlerFactory;
use pumpkin_util::{GameMode, math::position::BlockPos};
use std::sync::{Arc, Mutex, atomic::Ordering};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

const POS: BlockPos = BlockPos::new(0, 64, 0);
const OTHER: BlockPos = BlockPos::new(1, 64, 0);
type Event = (BlockPos, u8, u8);
type EventHook = Box<dyn FnOnce(&Arc<World>) + Send + Sync>;

#[derive(Default)]
struct Observed {
    events: Mutex<Vec<Event>>,
    on_event: Mutex<Option<EventHook>>,
}

impl Observed {
    fn take_events(&self) -> Vec<Event> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }
}

struct RecordEvents(Arc<Observed>);

impl BlockMetadata for RecordEvents {
    fn ids() -> Box<[BlockId]> {
        [BlockId::CHEST].into()
    }
}

impl BlockBehaviour for RecordEvents {
    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        self.0
            .events
            .lock()
            .unwrap()
            .push((*args.position, args.r#type, args.data));
        let hook = self.0.on_event.lock().unwrap().take();
        if let Some(hook) = hook {
            // Fail before enqueueing if a future flush holds its queue lock.
            assert!(args.world.synced_block_event_queue.try_lock().is_ok());
            hook(args.world);
        }
        ChestBlock.on_synced_block_event(args)
    }
}

struct EventFixture(Fixture);

impl EventFixture {
    async fn new(observed: &Arc<Observed>) -> Self {
        let mut fixture = Fixture::new().await;
        let original = fixture.source.world();
        let mut registry = default_registry();
        Arc::get_mut(&mut registry)
            .unwrap()
            .register(RecordEvents(observed.clone()));
        let world = Arc::new(World::load(
            original.level.clone(),
            original.level_info.clone(),
            original.dimension.clone(),
            registry,
            original.server.clone(),
            original.scoreboard.clone(),
        ));
        let server = fixture.source.server().clone();
        // Bind the instrumented world to the real Server tick entry point.
        server.worlds.store(Arc::new(vec![world.clone()]));
        fixture.source = CommandSource {
            server: Some(server),
            ..CommandSource::dummy().with_world(world).with_silent()
        };
        Self(fixture)
    }

    fn freeze(&self, frozen: bool) {
        let server = self.source.server();
        server.tick_rate_manager.set_frozen(server, frozen);
    }

    fn tick(&self) {
        let server = self.source.server();
        // Ticker normally updates this before calling Server::tick.
        server.tick_rate_manager.tick();
        server.tick();
    }
}

impl std::ops::Deref for EventFixture {
    type Target = Fixture;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for EventFixture {
    fn drop(&mut self) {
        // Menus retain their World; clear player registration before teardown.
        self.source.world().players.store(Arc::new(Vec::new()));
    }
}

struct ConnectedPlayer {
    player: Arc<Player>,
    _peer: TcpStream,
}

impl ConnectedPlayer {
    async fn new(world: &Arc<World>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (peer, accepted) = tokio::join!(TcpStream::connect(address), listener.accept());
        let peer = peer.unwrap();
        let (socket, address) = accepted.unwrap();
        let profile = GameProfile {
            id: Uuid::new_v4(),
            name: "FrozenBlockEvents".into(),
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
        let client = JavaClient::from_pending(pending, profile.clone(), PlayerConfig::default());
        let player = Arc::new(Player::new(
            Arc::new(ClientPlatform::Java(client)),
            profile,
            PlayerConfig::default(),
            world,
            GameMode::Survival,
        ));
        player.living_entity.entity.set_pos(POS.to_centered_f64());
        world.players.store(Arc::new(vec![player.clone()]));
        Self {
            player,
            _peer: peer,
        }
    }
}

fn open_menu(player: &Player, factory: &dyn ScreenHandlerFactory) {
    assert!(player.open_handled_screen(factory, Some(POS)).is_some());
}

#[tokio::test]
async fn frozen_chest_cycles_flush_unique_events_on_resume_and_allow_later_recounts() {
    let observed = Arc::new(Observed::default());
    let fixture = EventFixture::new(&observed).await;
    let world = fixture.source.world();
    let chest = Arc::new(ChestBlockEntity::new(POS));
    fixture.install(&Block::CHEST, chest.clone());
    let connected = ConnectedPlayer::new(world).await;
    let player = &connected.player;
    let factory = chest_screen_handler_factory(world, &POS).unwrap();
    fixture.freeze(true);
    let before = player.tick_counter.load(Ordering::Relaxed);
    let java = player.client.java().unwrap();

    for _ in 0..2 {
        open_menu(player, factory.as_ref());
        java.packet_sequence.store(7, Ordering::Relaxed);
        fixture.tick();
        assert_eq!(java.packet_sequence.load(Ordering::Relaxed), -1);
        assert!(observed.take_events().is_empty());
        player.on_handled_screen_closed();
        fixture.tick();
        assert!(observed.take_events().is_empty());
    }
    assert_eq!(player.tick_counter.load(Ordering::Relaxed), before + 4);
    assert!(!fixture.source.server().tick_rate_manager.runs_normally());

    fixture.freeze(false);
    fixture.tick();
    assert_eq!(observed.take_events(), [(POS, 1, 1), (POS, 1, 0)]);

    open_menu(player, factory.as_ref());
    fixture.tick();
    assert_eq!(observed.take_events(), [(POS, 1, 1)]);
    // Accepted unchanged recounts must resend lids after a previous flush.
    for _ in 0..5 {
        chest.tick(world);
    }
    fixture.tick();
    assert_eq!(observed.take_events(), [(POS, 1, 1)]);
    player.on_handled_screen_closed();
}

#[tokio::test]
async fn frozen_barrel_state_changes_still_flush_while_chest_events_wait_for_step() {
    let observed = Arc::new(Observed::default());
    let fixture = EventFixture::new(&observed).await;
    let world = fixture.source.world();
    fixture.install(&Block::BARREL, Arc::new(BarrelBlockEntity::new(POS)));
    let connected = ConnectedPlayer::new(world).await;
    let factory = barrel_screen_handler_factory(world, &POS).unwrap();
    fixture.freeze(true);
    world.flush_block_updates();
    world.take_flushed_block_changes();
    fixture.set_state(OTHER, Block::CHEST.default_state.id);
    world.add_synced_block_event(OTHER, 1, 1);

    open_menu(&connected.player, factory.as_ref());
    let open_state = world.get_block_state_id(&POS);
    assert!(BarrelLikeProperties::from_state_id(open_state).open);
    fixture.tick();
    assert_eq!(world.take_flushed_block_changes(), [(POS, open_state)]);
    assert!(observed.take_events().is_empty());

    connected.player.on_handled_screen_closed();
    let closed_state = world.get_block_state_id(&POS);
    assert!(!BarrelLikeProperties::from_state_id(closed_state).open);
    fixture.tick();
    assert_eq!(world.take_flushed_block_changes(), [(POS, closed_state)]);
    assert!(observed.take_events().is_empty());

    let server = fixture.source.server();
    assert!(server.tick_rate_manager.step_game_if_paused(server, 1));
    fixture.tick();
    assert!(server.tick_rate_manager.is_frozen());
    assert!(server.tick_rate_manager.runs_normally());
    assert_eq!(observed.take_events(), [(OTHER, 1, 1)]);

    world.add_synced_block_event(OTHER, 1, 1);
    fixture.tick();
    assert!(!server.tick_rate_manager.runs_normally());
    assert!(observed.take_events().is_empty());
    assert!(server.tick_rate_manager.step_game_if_paused(server, 1));
    fixture.tick();
    assert_eq!(observed.take_events(), [(OTHER, 1, 1)]);
}

#[tokio::test]
async fn pending_events_keep_first_insertion_order_and_callback_enqueues_join_the_flush() {
    let observed = Arc::new(Observed::default());
    let fixture = EventFixture::new(&observed).await;
    let world = fixture.source.world();
    fixture.set_state(OTHER, Block::CHEST.default_state.id);
    let first = (POS, 1, 1);
    let other_pos = (OTHER, 1, 1);
    let other_type = (POS, 2, 1);
    let other_data = (POS, 1, 2);
    for (pos, kind, data) in [first, other_pos, first, other_type, other_data, other_pos] {
        world.add_synced_block_event(pos, kind, data);
    }
    *observed.on_event.lock().unwrap() = Some(Box::new(move |world| {
        // The current event has left the set; a still-pending event has not.
        // Both adds reenter the production queue through its public entry point.
        world.add_synced_block_event(first.0, first.1, first.2);
        world.add_synced_block_event(other_pos.0, other_pos.1, other_pos.2);
        world.add_synced_block_event(OTHER, 2, 3);
    }));
    fixture.freeze(true);
    fixture.tick();
    fixture.tick();
    assert!(observed.take_events().is_empty());
    assert!(observed.on_event.lock().unwrap().is_some());

    fixture.freeze(false);
    fixture.tick();
    assert_eq!(
        observed.take_events(),
        [
            first,
            other_pos,
            other_type,
            other_data,
            first,
            (OTHER, 2, 3)
        ]
    );
    fixture.tick();
    assert!(observed.take_events().is_empty());
}

#[tokio::test]
async fn clearing_pending_events_releases_their_keys_without_reordering_survivors() {
    let observed = Arc::new(Observed::default());
    let fixture = EventFixture::new(&observed).await;
    let world = fixture.source.world();
    fixture.set_state(OTHER, Block::CHEST.default_state.id);
    fixture.freeze(true);
    world.add_synced_block_event(POS, 1, 1);
    world.add_synced_block_event(OTHER, 1, 2);
    world.add_synced_block_event(OTHER, 1, 0);
    fixture.tick();
    assert!(observed.take_events().is_empty());

    // Exercise the existing structure/clone queue consumer, including its
    // exclusive maximum bound. Cleared events can be inserted again at the end.
    world.clear_synced_block_events_in_box(&POS, &BlockPos::new(1, 65, 1));
    world.add_synced_block_event(POS, 1, 1);
    world.add_synced_block_event(OTHER, 1, 2);
    fixture.freeze(false);
    fixture.tick();
    assert_eq!(
        observed.take_events(),
        [(OTHER, 1, 2), (OTHER, 1, 0), (POS, 1, 1)]
    );
}
