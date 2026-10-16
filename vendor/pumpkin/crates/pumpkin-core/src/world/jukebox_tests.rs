//! ALL tests in this module are UNRUN (source-only R26 implementation).
//! Exercise production handlers, inventories, World ticks and guarded persistence.
//! No paired Java acceptance, advancement criterion or vibration claim.
#![allow(
    clippy::too_many_lines,
    reason = "Keep each real handler/lifecycle scenario in one stopped-worker fixture"
)]

use super::*;
use crate::{
    block::{
        BlockBehaviour, GetComparatorOutputArgs,
        blocks::jukebox::JukeboxBlock,
        entities::{hopper::HopperBlockEntity, jukebox::JukeboxBlockEntity},
    },
    data::player_server::test::{
        PlayerFixture, close_fixture, java_client, player_fixture, stop_chunk_workers,
    },
    entity::EntityBase,
    net::{GameProfile, PlayerConfig},
    plugin::{
        EventHandler,
        api::events::{
            EventPriority, block::block_physics::BlockPhysicsEvent,
            player::player_interact_event::PlayerInteractEvent,
            world::generic_game::GenericGameEvent,
        },
    },
};
use futures::future::BoxFuture;
use pumpkin_data::{
    block_properties::{FacingHopper, HopperLikeProperties, JukeboxLikeProperties},
    data_component::DataComponent,
    data_component_impl::JukeboxPlayableImpl,
    item::Item,
    jukebox_song::JukeboxSong,
    statistic::{CustomStatistic, StatisticCategory},
};
use pumpkin_inventory::{Clearable, Inventory};
use pumpkin_protocol::{
    RawPacket, VarInt,
    java::{
        client::play::{CEntitySoundEffect, CKeepAlive, CSetPlayerInventory, CSoundEffect},
        packet_decoder::TCPNetworkDecoder,
        server::play::SUseItemOn,
    },
    packet::MultiVersionJavaPacket,
    ser::NetworkReadExt,
};
use pumpkin_util::{GameMode, Hand};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU8},
        mpsc,
    },
    time::Duration,
};
use tokio::{io::AsyncRead, net::TcpStream};

const POS: BlockPos = BlockPos::new(0, 64, 0);
const ELAPSED: &str = "ticks_since_song_started";

async fn fixture() -> PlayerFixture {
    let fixture = player_fixture().await;
    stop_chunk_workers(&fixture).await;
    fixture
        .player
        .get_entity()
        .set_pos(Vector3::new(0.5, 65.0, 0.5));
    fixture.player.set_client_loaded(true);
    fixture
        .client
        .java()
        .unwrap()
        .version
        .store(CURRENT_MC_VERSION);
    assert!(Weak::ptr_eq(
        fixture.player.world().weak_self(),
        &Arc::downgrade(&fixture.player.world())
    ));
    fixture
}

fn set_state(world: &World, position: BlockPos, state: pumpkin_data::BlockStateId) {
    let local = position.chunk_relative_position();
    world
        .level
        .read_chunk_sync(&position.chunk_position(), |chunk| {
            chunk.set_block_absolute_y(local.x as usize, local.y, local.z as usize, state);
        })
        .unwrap();
}

fn install(world: &World) -> Arc<JukeboxBlockEntity> {
    world.remove_block_entity(&POS);
    set_state(world, POS, Block::JUKEBOX.default_state.id);
    let live = Arc::new(JukeboxBlockEntity::new(POS));
    world.add_block_entity(live.clone());
    live
}

fn persisted(live: &dyn BlockEntity) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    live.write_internal(&mut nbt);
    nbt
}

fn record_nbt(item: &ItemStack, elapsed: Option<i64>) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    let mut record = NbtCompound::new();
    item.write_item_stack(&mut record);
    nbt.put_compound("RecordItem", record);
    if let Some(elapsed) = elapsed {
        nbt.put_long(ELAPSED, elapsed);
    }
    nbt.put_string("id", JukeboxBlockEntity::ID.into());
    nbt.put_int("x", POS.0.x);
    nbt.put_int("y", POS.0.y);
    nbt.put_int("z", POS.0.z);
    nbt
}

fn disc(song: JukeboxSong, count: u8) -> ItemStack {
    let mut item = ItemStack::new(count, &Item::MUSIC_DISC_11);
    item.set_data_component(JukeboxPlayableImpl { song });
    item
}

fn use_on(player: &Arc<Player>, server: &Arc<Server>, hand: Hand) {
    player
        .client
        .java()
        .unwrap()
        .handle_use_item_on(
            player,
            &SUseItemOn {
                hand: VarInt(i32::from(hand != Hand::Right)),
                position: POS,
                face: VarInt(1),
                cursor_pos: Vector3::new(0.5, 1.0, 0.5),
                inside_block: false,
                is_against_world_border: false,
                sequence: VarInt(41),
            },
            server,
        )
        .unwrap();
}

/// A FIFO marker asserts absence without sleeping or advancing unrelated ticks.
async fn observe<R: AsyncRead + Unpin>(
    reader: &mut TCPNetworkDecoder<R>,
    player: &Player,
) -> Vec<RawPacket> {
    player.try_send_client_packet(&CKeepAlive::new(918_441));
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut packets = Vec::new();
        for _ in 0..512 {
            let packet = reader.get_raw_packet().await.unwrap();
            if packet.id == CKeepAlive::to_id(CURRENT_MC_VERSION) {
                assert_eq!((&packet.payload[..]).get_i64_be().unwrap(), 918_441);
                return packets;
            }
            packets.push(packet);
        }
        panic!("jukebox FIFO marker missing");
    })
    .await
    .unwrap()
}

fn level_events(packets: &[RawPacket]) -> Vec<(i32, i32)> {
    assert!(packets.iter().all(|packet| packet.id
        != CEntitySoundEffect::to_id(CURRENT_MC_VERSION)
        && packet.id != CSoundEffect::to_id(CURRENT_MC_VERSION)));
    packets
        .iter()
        .filter(|packet| packet.id == CWorldEvent::to_id(CURRENT_MC_VERSION))
        .map(|packet| {
            let mut reader = &packet.payload[..];
            let event = reader.get_i32_be().unwrap();
            assert_eq!(reader.get_block_pos(&CURRENT_MC_VERSION).unwrap(), POS);
            let data = reader.get_i32_be().unwrap();
            assert!(!reader.get_bool().unwrap());
            assert!(reader.is_empty());
            (event, data)
        })
        .collect()
}

fn synced_slots(packets: &[RawPacket]) -> Vec<(i32, u16, u8)> {
    packets.iter().filter(|packet| packet.id == CSetPlayerInventory::to_id(CURRENT_MC_VERSION))
        .map(|packet| {
            let mut reader = &packet.payload[..];
            let slot = reader.get_var_int().unwrap().0;
            let item = pumpkin_protocol::codec::item_stack_seralizer::ItemStackSerializer::read_with_version(&mut reader, &CURRENT_MC_VERSION).unwrap();
            assert!(reader.is_empty());
            (slot, item.0.item.id, item.0.item_count)
        }).collect()
}

fn notes(packets: &[RawPacket]) -> usize {
    let mut count = 0;
    for packet in packets
        .iter()
        .filter(|packet| packet.id == CParticle::to_id(CURRENT_MC_VERSION))
    {
        let mut reader = &packet.payload[..];
        assert_eq!(
            reader.get_var_int().unwrap().0,
            pumpkin_data::particle::Particle::Note.to_id() as i32
        );
        assert!(!reader.get_bool().unwrap());
        assert!(!reader.get_bool().unwrap());
        assert_eq!(reader.get_f64_be().unwrap(), 0.5);
        assert_eq!(reader.get_f64_be().unwrap(), 65.200_000_047_683_72);
        assert_eq!(reader.get_f64_be().unwrap(), 0.5);
        let color = reader.get_f32_be().unwrap();
        assert!((0..4).any(|value| color == value as f32 / 24.0));
        assert_eq!(reader.get_f32_be().unwrap(), 0.0);
        assert_eq!(reader.get_f32_be().unwrap(), 0.0);
        assert_eq!(reader.get_f32_be().unwrap(), 1.0);
        assert_eq!(reader.get_f32_be().unwrap(), 1.0);
        assert_eq!(reader.get_f32_be().unwrap(), 1.0);
        assert_eq!(reader.get_var_int().unwrap().0, 0);
        assert_eq!(reader.get_var_int().unwrap().0, 0);
        assert!(reader.is_empty());
        count += 1;
    }
    count
}

struct Observer {
    player: Arc<Player>,
    reader: TCPNetworkDecoder<TcpStream>,
}
impl Observer {
    async fn new(server: &Arc<Server>, world: &Arc<World>) -> Self {
        let profile = GameProfile {
            id: Uuid::new_v4(),
            name: "RecordObserver".into(),
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
        player.set_client_loaded(true);
        player.get_entity().set_pos(Vector3::new(8.5, 65.0, 0.5));
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
impl Drop for Observer {
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

struct Hooks {
    world: Weak<World>,
    player: Weak<Player>,
    cancel: AtomicBool,
    mode: AtomicU8,
    events: Mutex<Vec<(String, Option<i64>)>>,
    neighbors: Mutex<Vec<bool>>,
    pause: Mutex<Option<(mpsc::SyncSender<()>, mpsc::Receiver<()>)>>,
    replacement: Mutex<Option<Arc<dyn BlockEntity>>>,
}
impl Hooks {
    fn register(fixture: &PlayerFixture) -> Arc<Self> {
        let hooks = Arc::new(Self {
            world: Arc::downgrade(&fixture.player.world()),
            player: Arc::downgrade(&fixture.player),
            cancel: AtomicBool::new(false),
            mode: AtomicU8::new(0),
            events: Mutex::new(Vec::new()),
            neighbors: Mutex::new(Vec::new()),
            pause: Mutex::new(None),
            replacement: Mutex::new(None),
        });
        fixture
            .server
            .plugin_manager
            .register::<PlayerInteractEvent, _>(hooks.clone(), EventPriority::Normal, true);
        fixture
            .server
            .plugin_manager
            .register::<GenericGameEvent, _>(hooks.clone(), EventPriority::Normal, true);
        fixture
            .server
            .plugin_manager
            .register::<BlockPhysicsEvent, _>(hooks.clone(), EventPriority::Normal, true);
        hooks
    }
}
impl EventHandler<PlayerInteractEvent> for Hooks {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut PlayerInteractEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            event.cancelled = self.cancel.load(Relaxed);
        })
    }
}
impl EventHandler<GenericGameEvent> for Hooks {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut GenericGameEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.position != POS.to_centered_f64() {
                return;
            }
            let world = self.world.upgrade().unwrap();
            assert!(
                world.entity_lifecycle.try_lock().is_ok(),
                "gameplay callback under lifecycle guard"
            );
            assert!(
                !world
                    .block_entities
                    .try_get(&POS.chunk_position())
                    .is_locked()
            );
            world.level.read_chunk_sync(&POS.chunk_position(), |chunk| {
                assert!(chunk.pending_block_entities.try_lock().is_ok());
            });
            // These reads would deadlock if the owner mutex crossed the hook.
            let current = world.published_block_entity(&POS);
            let elapsed = current
                .as_ref()
                .and_then(|live| persisted(live.as_ref()).get_long(ELAPSED));
            self.events
                .lock()
                .unwrap()
                .push((event.event_key.clone(), elapsed));
            if event.event_key == "block_change"
                && self.mode.compare_exchange(1, 0, Relaxed, Relaxed).is_ok()
            {
                current
                    .unwrap()
                    .get_inventory()
                    .unwrap()
                    .set_stack(0, disc(JukeboxSong::Cat, 1));
            } else if event.event_key == "jukebox_stop_play"
                && self.mode.compare_exchange(2, 0, Relaxed, Relaxed).is_ok()
            {
                let replacement = Arc::new(JukeboxBlockEntity::new(POS));
                world.add_block_entity(replacement.clone());
                replacement.set_record(disc(JukeboxSong::Cat, 1));
                *self.replacement.lock().unwrap() = Some(replacement);
            } else if event.event_key == "block_change"
                && self.mode.compare_exchange(3, 0, Relaxed, Relaxed).is_ok()
            {
                self.player
                    .upgrade()
                    .unwrap()
                    .inventory()
                    .set_stack_in_hand(Hand::Right, ItemStack::new(7, &Item::DIAMOND));
            } else if event.event_key == "block_change"
                && self.mode.compare_exchange(4, 0, Relaxed, Relaxed).is_ok()
            {
                let player = self.player.upgrade().unwrap();
                assert_eq!(
                    player.inventory().get_stack(0).item_count,
                    2,
                    "consume precedes first gameplay hook"
                );
                player.inventory().set_selected_slot(1);
            } else if event.event_key == "jukebox_stop_play"
                && self.mode.compare_exchange(5, 0, Relaxed, Relaxed).is_ok()
            {
                set_state(&world, POS, Block::JUKEBOX.default_state.id);
                let replacement = Arc::new(JukeboxBlockEntity::new(POS));
                world.add_block_entity(replacement.clone());
                replacement.set_record(disc(JukeboxSong::Cat, 1));
            } else if event.event_key == "block_change"
                && self.mode.compare_exchange(6, 0, Relaxed, Relaxed).is_ok()
            {
                assert!(
                    world
                        .edit_block_entity_nbt(&current.unwrap(), &mut |_| NbtEditDecision::Commit)
                        .unwrap()
                );
            } else if event.event_key == "block_change"
                && self.mode.compare_exchange(7, 0, Relaxed, Relaxed).is_ok()
            {
                let (entered, release) = self.pause.lock().unwrap().take().unwrap();
                entered.send(()).unwrap();
                release.recv_timeout(Duration::from_secs(5)).unwrap();
            } else if event.event_key == "jukebox_stop_play"
                && self.mode.compare_exchange(8, 0, Relaxed, Relaxed).is_ok()
            {
                let replacement =
                    Arc::new(crate::block::entities::barrel::BarrelBlockEntity::new(POS));
                replacement.set_stack(0, ItemStack::new(7, &Item::DIAMOND));
                world.add_block_entity(replacement.clone());
                *self.replacement.lock().unwrap() = Some(replacement);
            }
        })
    }
}
impl EventHandler<BlockPhysicsEvent> for Hooks {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut BlockPhysicsEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.changed_pos == POS {
                let world = self.world.upgrade().unwrap();
                if let Some(live) = world.published_block_entity(&POS) {
                    self.neighbors.lock().unwrap().push(live.is_dirty());
                    let _ = persisted(live.as_ref());
                }
            }
        })
    }
}

async fn bounded(work: impl FnOnce() + Send + 'static) {
    tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(work))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handler_actual_hands_creative_missing_entity_cancel_and_fallback_accounting() {
    let mut fixture = fixture().await;
    let hooks = Hooks::register(&fixture);
    let player = fixture.player.clone();
    let server = fixture.server.clone();
    let world = player.world();
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    let mut observer = Observer::new(&server, &world).await;
    for hand in [Hand::Right, Hand::Left] {
        for mode in [GameMode::Survival, GameMode::Creative] {
            for missing in [false, true] {
                for cancelled in [false, true] {
                    let live = install(&world);
                    if missing {
                        world.remove_block_entity(&POS);
                    }
                    let other = if hand == Hand::Right {
                        Hand::Left
                    } else {
                        Hand::Right
                    };
                    player.gamemode.store(mode);
                    player
                        .inventory()
                        .set_stack_in_hand(hand, ItemStack::new(3, &Item::MUSIC_DISC_11));
                    player
                        .inventory()
                        .set_stack_in_hand(other, ItemStack::new(7, &Item::DIAMOND));
                    hooks.cancel.store(cancelled, Relaxed);
                    let before_play = player.get_stat(
                        StatisticCategory::Custom,
                        CustomStatistic::PlayRecord as i32,
                    );
                    let before_used =
                        player.get_stat(StatisticCategory::Used, Item::MUSIC_DISC_11.id as i32);
                    observe(&mut reader, &player).await;
                    observe(&mut observer.reader, &observer.player).await;
                    use_on(&player, &server, hand);
                    assert_eq!(
                        player.inventory().get_stack_in_hand(hand).item_count,
                        if !cancelled && mode == GameMode::Survival {
                            2
                        } else {
                            3
                        }
                    );
                    assert_eq!(player.inventory().get_stack_in_hand(other).item_count, 7);
                    assert_eq!(
                        player.get_stat(StatisticCategory::Used, Item::MUSIC_DISC_11.id as i32),
                        before_used
                    );
                    assert_eq!(
                        player.get_stat(
                            StatisticCategory::Custom,
                            CustomStatistic::PlayRecord as i32
                        ),
                        before_play + i32::from(!cancelled)
                    );
                    let accepted = !cancelled && !missing;
                    assert_eq!(live.is_playing(), accepted);
                    assert_eq!(
                        JukeboxLikeProperties::from_state_id(world.get_block_state_id(&POS))
                            .has_record,
                        accepted
                    );
                    let expected = if accepted {
                        vec![(1010, 0)]
                    } else {
                        Vec::new()
                    };
                    let packets = observe(&mut reader, &player).await;
                    assert_eq!(level_events(&packets), expected);
                    assert_eq!(
                        synced_slots(&packets),
                        if !cancelled && mode == GameMode::Survival {
                            vec![(
                                if hand == Hand::Right { 0 } else { 40 },
                                Item::MUSIC_DISC_11.id,
                                2,
                            )]
                        } else {
                            vec![]
                        }
                    );
                    assert_eq!(
                        level_events(&observe(&mut observer.reader, &observer.player).await),
                        expected
                    );
                    assert!(world.block_entity_update_packets.lock().unwrap().is_empty());
                }
            }
        }
    }
    hooks.cancel.store(false, Relaxed);
    player.gamemode.store(GameMode::Survival);
    let live = install(&world);
    live.set_record(ItemStack::new(1, &Item::MUSIC_DISC_11));
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::new(1, &Item::MUSIC_DISC_CAT));
    player
        .inventory()
        .set_stack_in_hand(Hand::Left, ItemStack::new(1, &Item::MUSIC_DISC_CAT));
    let before_play = player.get_stat(
        StatisticCategory::Custom,
        CustomStatistic::PlayRecord as i32,
    );
    let before_used = player.get_stat(StatisticCategory::Used, Item::MUSIC_DISC_CAT.id as i32);
    observe(&mut reader, &player).await;
    use_on(&player, &server, Hand::Left);
    assert!(live.is_playing(), "offhand TryEmptyHand must not eject");
    assert!(level_events(&observe(&mut reader, &player).await).is_empty());
    let drops = world.entities.load().len();
    use_on(&player, &server, Hand::Right);
    assert!(live.get_record().is_empty());
    assert_eq!(world.entities.load().len(), drops + 1);
    for entity in world.entities.load().iter() {
        if let Some(item) = entity.get_item_entity() {
            assert_eq!(item.get_pickup_delay(), 10);
            let position = entity.get_entity().pos.load();
            assert!((0.15..0.85).contains(&position.x));
            assert_eq!(position.y, 65.01);
            assert!((0.15..0.85).contains(&position.z));
        }
    }
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)]
    );
    assert_eq!(
        player.inventory().get_stack_in_hand(Hand::Right).item_count,
        1
    );
    assert_eq!(
        player.get_stat(StatisticCategory::Used, Item::MUSIC_DISC_CAT.id as i32),
        before_used
    );
    assert_eq!(
        player.get_stat(
            StatisticCategory::Custom,
            CustomStatistic::PlayRecord as i32
        ),
        before_play
    );
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::EMPTY.clone());
    use_on(&player, &server, Hand::Right);
    assert!(level_events(&observe(&mut reader, &player).await).is_empty());
    // Reentrant callbacks may replace the hand; bounded writeback must preserve it.
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::new(3, &Item::MUSIC_DISC_11));
    hooks.mode.store(3, Relaxed);
    use_on(&player, &server, Hand::Right);
    assert!(
        player
            .inventory()
            .get_stack_in_hand(Hand::Right)
            .are_equal(&ItemStack::new(7, &Item::DIAMOND))
    );
    // Builtin disc fallback on a nonjukebox still awards neither statistic.
    world.remove_block_entity(&POS);
    set_state(&world, POS, Block::STONE.default_state.id);
    for hand in [Hand::Right, Hand::Left] {
        player
            .inventory()
            .set_stack_in_hand(hand, ItemStack::new(2, &Item::MUSIC_DISC_11));
        let used = player.get_stat(StatisticCategory::Used, Item::MUSIC_DISC_11.id as i32);
        let played = player.get_stat(
            StatisticCategory::Custom,
            CustomStatistic::PlayRecord as i32,
        );
        use_on(&player, &server, hand);
        assert_eq!(
            player.get_stat(StatisticCategory::Used, Item::MUSIC_DISC_11.id as i32),
            used
        );
        assert_eq!(
            player.get_stat(
                StatisticCategory::Custom,
                CustomStatistic::PlayRecord as i32
            ),
            played
        );
        assert_eq!(player.inventory().get_stack_in_hand(hand).item_count, 2);
    }
    observer.close().await;
    drop(reader);
    close_fixture(&fixture).await;
}

fn power(world: &World) -> u8 {
    world.block_registry.get_weak_redstone_power(
        &Block::JUKEBOX,
        world,
        &POS,
        world.get_block_state(&POS),
        BlockDirection::North,
    )
}

fn comparator(world: &World) -> u8 {
    JukeboxBlock
        .get_comparator_output(GetComparatorOutputArgs {
            world,
            block: &Block::JUKEBOX,
            state: world.get_block_state(&POS),
            position: &POS,
            direction: BlockDirection::North,
        })
        .unwrap()
}

async fn enable_tick_phase(world: &Arc<World>) {
    let position = POS.chunk_position();
    world
        .level
        .read_chunk_sync(&position, |chunk| {
            assert!(
                chunk.process_post_processing(|_| panic!("empty fixture has no postprocessing"))
            );
        })
        .unwrap();
    let entities = tokio::time::timeout(
        Duration::from_secs(5),
        world
            .level
            .get_or_fetch_entity_chunk(position, Clone::clone),
    )
    .await
    .unwrap()
    .unwrap();
    entities.live.store(true, Relaxed);
    world.block_ticking_chunks.write().unwrap().insert(position);
    assert!(world.level.is_chunk_ready(&position));
}

/// Reproduces the differential circuit through neighbor admission, the real
/// scheduled-block phase and the attached comparator BE.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn world_jukebox_comparator_disc11_insert_and_eject_complete_after_two_ticks() {
    use crate::block::entities::comparator::ComparatorBlockEntity;
    use pumpkin_data::block_properties::{
        ComparatorLikeProperties, HorizontalFacing, RedstoneWireLikeProperties,
    };
    use pumpkin_world::chunk::ChunkData;

    const JUKEBOX: BlockPos = BlockPos::new(4100, 200, 1);
    const COMPARATOR: BlockPos = BlockPos::new(4100, 200, 2);
    const WIRE: BlockPos = BlockPos::new(4101, 200, 1);

    let fixture = fixture().await;
    let player = fixture.player.clone();
    let world = player.world();
    let chunk_pos = JUKEBOX.chunk_position();
    assert!(
        world
            .level
            .loaded_chunks
            .insert(chunk_pos, ChunkData::empty_sync(chunk_pos.x, chunk_pos.y))
            .is_none()
    );
    world
        .level
        .read_chunk_sync(&chunk_pos, |chunk| {
            assert!(
                chunk.process_post_processing(|_| panic!("empty fixture has no postprocessing"))
            );
        })
        .unwrap();
    let entities = tokio::time::timeout(
        Duration::from_secs(5),
        world
            .level
            .get_or_fetch_entity_chunk(chunk_pos, Clone::clone),
    )
    .await
    .unwrap()
    .unwrap();
    entities.live.store(true, Relaxed);
    world.block_ticking_chunks.write().unwrap().insert(chunk_pos);
    assert!(world.level.is_chunk_ready(&chunk_pos));
    player
        .get_entity()
        .set_pos(Vector3::new(4098.5, 200.0, 3.5));
    for x in 4096..=4111 {
        for z in 0..=15 {
            set_state(&world, BlockPos::new(x, 199, z), Block::STONE.default_state.id);
        }
    }
    world.set_block_state(
        &JUKEBOX,
        Block::JUKEBOX.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );
    let mut comparator_props = ComparatorLikeProperties::default(&Block::COMPARATOR);
    comparator_props.facing = HorizontalFacing::North;
    world.set_block_state(
        &COMPARATOR,
        comparator_props.to_state_id(&Block::COMPARATOR),
        BlockFlags::NOTIFY_ALL,
    );
    world.set_block_state(
        &WIRE,
        Block::REDSTONE_WIRE.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );

    let jukebox_entity = world.get_block_entity(&JUKEBOX).unwrap();
    let jukebox = jukebox_entity
        .as_any()
        .downcast_ref::<JukeboxBlockEntity>()
        .unwrap();
    let comparator_entity = world.get_block_entity(&COMPARATOR).unwrap();
    assert!(comparator_entity.as_any().is::<ComparatorBlockEntity>());
    let assert_comparator = |expected| {
        let attached = world.get_block_entity(&COMPARATOR).unwrap();
        assert!(Arc::ptr_eq(&attached, &comparator_entity));
        assert_eq!(
            persisted(attached.as_ref()).get_int("OutputSignal"),
            Some(expected)
        );
        assert_eq!(
            ComparatorLikeProperties::from_state_id(world.get_block_state_id(&COMPARATOR))
                .powered,
            expected != 0
        );
    };
    let wire_power = || {
        assert_eq!(world.get_block(&WIRE), &Block::REDSTONE_WIRE);
        RedstoneWireLikeProperties::from_state_id(world.get_block_state_id(&WIRE)).power
    };
    let use_jukebox = || {
        player
            .client
            .java()
            .unwrap()
            .handle_use_item_on(
                &player,
                &SUseItemOn {
                    hand: VarInt(0),
                    position: JUKEBOX,
                    face: VarInt(1),
                    cursor_pos: Vector3::new(0.5, 1.0, 0.5),
                    inside_block: false,
                    is_against_world_border: false,
                    sequence: VarInt(41),
                },
                &fixture.server,
            )
            .unwrap();
    };

    assert_comparator(0);
    assert_eq!(wire_power(), 0);
    assert!(!world.is_block_tick_scheduled(&COMPARATOR, &Block::COMPARATOR));
    assert!(
        world
            .entities
            .load()
            .iter()
            .all(|entity| entity.get_item_entity().is_none())
    );
    let record = ItemStack::new(1, &Item::MUSIC_DISC_11);
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, record.clone());
    use_jukebox();
    assert!(player.inventory().get_stack_in_hand(Hand::Right).is_empty());
    assert!(jukebox.get_record().are_equal(&record));
    assert!(JukeboxLikeProperties::from_state_id(world.get_block_state_id(&JUKEBOX)).has_record);
    assert_eq!(persisted(jukebox).get_long(ELAPSED), Some(0));
    assert_eq!(wire_power(), 15);
    assert_comparator(0);
    assert!(world.is_block_tick_scheduled(&COMPARATOR, &Block::COMPARATOR));

    for (elapsed, output) in [(1, 0), (2, 11)] {
        world.tick_chunks(&fixture.server);
        assert_eq!(
            world.tick_block_entities(&tokio::runtime::Handle::current()),
            2
        );
        assert_comparator(output);
        assert_eq!(persisted(jukebox).get_long(ELAPSED), Some(elapsed));
        assert_eq!(wire_power(), 15);
    }

    use_jukebox();
    assert!(jukebox.get_record().is_empty());
    assert!(!jukebox.is_playing());
    assert!(!JukeboxLikeProperties::from_state_id(world.get_block_state_id(&JUKEBOX)).has_record);
    assert_eq!(persisted(jukebox).get_long(ELAPSED), None);
    assert_eq!(wire_power(), 0);
    assert_comparator(11);
    assert!(world.is_block_tick_scheduled(&COMPARATOR, &Block::COMPARATOR));
    let drops: Vec<_> = world
        .entities
        .load()
        .iter()
        .filter(|entity| entity.get_item_entity().is_some())
        .cloned()
        .collect();
    assert_eq!(drops.len(), 1);
    let mut drop_nbt = NbtCompound::new();
    drops[0].write_nbt(&mut drop_nbt);
    let mut record_nbt = NbtCompound::new();
    record.write_item_stack(&mut record_nbt);
    assert_eq!(drop_nbt.get_compound("Item"), Some(&record_nbt));
    assert_eq!(drops[0].get_item_entity().unwrap().get_pickup_delay(), 10);

    for output in [11, 0] {
        world.tick_chunks(&fixture.server);
        assert_eq!(
            world.tick_block_entities(&tokio::runtime::Handle::current()),
            2
        );
        assert_comparator(output);
        assert_eq!(wire_power(), 0);
        assert!(jukebox.get_record().is_empty());
    }
    assert!(!world.is_block_tick_scheduled(&COMPARATOR, &Block::COMPARATOR));
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn world_tick_disc11_full_padded_lifetime_neighbors_and_scoped_wire() {
    let mut fixture = fixture().await;
    let player = fixture.player.clone();
    let world = player.world();
    let hooks = Hooks::register(&fixture);
    enable_tick_phase(&world).await;
    let live = install(&world);
    let mut observer = Observer::new(&fixture.server, &world).await;
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    observe(&mut observer.reader, &observer.player).await;
    live.set_record(ItemStack::new(1, &Item::MUSIC_DISC_11));
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1010, 0)]
    );
    assert_eq!(
        level_events(&observe(&mut observer.reader, &observer.player).await),
        vec![(1010, 0)]
    );
    assert_eq!(persisted(live.as_ref()).get_long(ELAPSED), Some(0));
    assert_eq!(power(&world), 15);
    assert_eq!(comparator(&world), 11);
    assert!(hooks.neighbors.lock().unwrap().iter().any(|dirty| !dirty));
    hooks.events.lock().unwrap().clear();
    hooks.neighbors.lock().unwrap().clear();
    live.clear_dirty();
    for tick in 1..=1440 {
        assert_eq!(
            world.tick_block_entities(&tokio::runtime::Handle::current()),
            1
        );
        assert!(live.is_playing(), "disc11 stopped on call {tick}");
        assert_eq!(persisted(live.as_ref()).get_long(ELAPSED), Some(tick));
    }
    assert_eq!(power(&world), 15);
    assert_eq!(comparator(&world), 11);
    let events = hooks.events.lock().unwrap().clone();
    assert_eq!(events.len(), 72);
    for (index, (event, elapsed)) in events.iter().enumerate() {
        assert_eq!(event, "jukebox_play");
        // ROOT's documented notification boundary observes committed elapsed+1.
        assert_eq!(*elapsed, Some(index as i64 * 20 + 1));
    }
    let owner_packets = observe(&mut reader, &player).await;
    let observer_packets = observe(&mut observer.reader, &observer.player).await;
    assert!(level_events(&owner_packets).is_empty());
    assert!(level_events(&observer_packets).is_empty());
    assert_eq!(notes(&owner_packets), 72);
    assert_eq!(notes(&observer_packets), 72);
    assert_eq!(
        world.tick_block_entities(&tokio::runtime::Handle::current()),
        1
    );
    assert!(!live.is_playing());
    assert_eq!(power(&world), 0);
    assert_eq!(comparator(&world), 11);
    assert!(JukeboxLikeProperties::from_state_id(world.get_block_state_id(&POS)).has_record);
    assert_eq!(live.get_record().item.id, Item::MUSIC_DISC_11.id);
    assert_eq!(persisted(live.as_ref()).get_long(ELAPSED), None);
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)]
    );
    assert_eq!(
        level_events(&observe(&mut observer.reader, &observer.player).await),
        vec![(1011, 0)]
    );
    assert_eq!(
        hooks.events.lock().unwrap().last().unwrap().0,
        "jukebox_stop_play"
    );
    assert!(!hooks.neighbors.lock().unwrap().is_empty());
    assert!(
        hooks.neighbors.lock().unwrap().iter().all(|dirty| !dirty),
        "neighbors precede dirty"
    );
    assert!(live.is_dirty());
    // Strict XYZ64 for level events, distinct strict32 block-center NOTE scope.
    for (position, receive_event, receive_note) in [
        (Vector3::new(64.0, 64.0, 0.0), false, false),
        (Vector3::new(0.0, 128.0, 0.0), false, false),
        (Vector3::new(63.5, 64.0, 0.0), true, false),
        (Vector3::new(32.5, 65.0, 0.5), true, false),
        (Vector3::new(31.5, 65.0, 0.5), true, true),
    ] {
        observer.player.get_entity().set_pos(position);
        live.set_record(ItemStack::new(1, &Item::MUSIC_DISC_11));
        world.tick_block_entities(&tokio::runtime::Handle::current());
        let packets = observe(&mut observer.reader, &observer.player).await;
        assert_eq!(
            level_events(&packets),
            if receive_event {
                vec![(1010, 0)]
            } else {
                vec![]
            }
        );
        assert_eq!(notes(&packets), usize::from(receive_note));
    }
    assert!(world.block_entity_update_packets.lock().unwrap().is_empty());
    observer.close().await;
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inventory_and_real_hopper_share_synchronous_transitions_and_predicates() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let hooks = Hooks::register(&fixture);
    let live = install(&world);
    let inventory = live.clone().get_inventory().unwrap();
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    let mut removed_component = ItemStack::new(1, &Item::MUSIC_DISC_11);
    removed_component.remove_data_component(DataComponent::JukeboxPlayable);
    assert!(!inventory.is_valid_slot_for(0, &removed_component));
    assert!(!inventory.is_valid_slot_for(1, &disc(JukeboxSong::Cat, 1)));
    assert!(inventory.is_valid_slot_for(0, &disc(JukeboxSong::Cat, 1)));
    inventory.set_stack(1, disc(JukeboxSong::Cat, 1));
    assert!(inventory.get_stack(1).is_empty());
    assert!(inventory.remove_stack(1).is_empty());
    assert!(level_events(&observe(&mut reader, &player).await).is_empty());
    assert_eq!(inventory.get_max_count_per_stack(), 1);
    inventory.set_stack(0, disc(JukeboxSong::Cat, 3));
    assert_eq!(
        inventory.get_stack(0).item_count,
        3,
        "setter does not clamp to insertion max"
    );
    assert!(!inventory.is_valid_slot_for(0, &disc(JukeboxSong::Cat, 1)));
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1010, 5)]
    );
    assert_eq!(
        inventory.remove_stack_specific(0, 0).item_count,
        3,
        "split ignores requested amount"
    );
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)]
    );
    inventory.set_stack(0, disc(JukeboxSong::Cat, 1));
    observe(&mut reader, &player).await;
    inventory.clear();
    assert!(inventory.is_empty());
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)]
    );
    // Destination with merge room but no empty slot cannot extract a record.
    let below = POS.down();
    let destination = Arc::new(HopperBlockEntity::new(below, FacingHopper::Down));
    let mut stackable = ItemStack::new(1, &Item::PAPER);
    stackable.set_data_component(JukeboxPlayableImpl {
        song: JukeboxSong::Cat,
    });
    for slot in 0..destination.size() {
        destination.set_stack(slot, stackable.clone());
    }
    inventory.set_stack(0, stackable);
    assert!(!inventory.can_transfer_to(destination.as_ref(), 0, &inventory.get_stack(0)));
    let mut hopper_state = HopperLikeProperties::default(&Block::HOPPER);
    hopper_state.enabled = true;
    hopper_state.facing = FacingHopper::Down;
    set_state(&world, below, hopper_state.to_state_id(&Block::HOPPER));
    world.add_block_entity(destination.clone());
    observe(&mut reader, &player).await;
    assert!(world.is_current_block_entity(destination.as_ref()));
    assert!(HopperLikeProperties::from_state_id(world.get_block_state_id(&below)).enabled);
    destination.tick(&world);
    assert_eq!(
        persisted(destination.as_ref()).get_int("TransferCooldown"),
        Some(0)
    );
    assert_eq!(inventory.get_stack(0).item.id, Item::PAPER.id);
    assert!(level_events(&observe(&mut reader, &player).await).is_empty());
    for slot in 0..4 {
        destination.set_stack(slot, ItemStack::new(64, &Item::STONE));
    }
    destination.remove_stack(4);
    inventory.set_stack(0, disc(JukeboxSong::Cat, 1));
    assert!(inventory.can_transfer_to(destination.as_ref(), 0, &inventory.get_stack(0)));
    observe(&mut reader, &player).await;
    // Playback's neighbor notification powers and disables the hopper below.
    // An empty destination slot permits extraction only once that power ends.
    assert_eq!(power(&world), 15);
    assert!(!HopperLikeProperties::from_state_id(world.get_block_state_id(&below)).enabled);
    destination.tick(&world);
    assert!(!inventory.is_empty());
    assert_eq!(
        persisted(destination.as_ref()).get_int("TransferCooldown"),
        Some(0)
    );
    assert!(level_events(&observe(&mut reader, &player).await).is_empty());
    // Cat lasts 185 seconds plus 20 padded ticks; the next production tick
    // stops it and notifies the hopper. Do not force ENABLED or reset cooldown.
    let finishing = live.clone();
    let tick_world = world.clone();
    bounded(move || {
        for _ in 0..3721 {
            finishing.tick(&tick_world);
        }
    })
    .await;
    assert!(!live.is_playing());
    assert_eq!(power(&world), 0);
    assert!(HopperLikeProperties::from_state_id(world.get_block_state_id(&below)).enabled);
    assert!(world.is_current_block_entity(destination.as_ref()));
    assert!(world.is_current_block_entity(live.as_ref()));
    assert!(inventory.get_stack(0).are_equal(&disc(JukeboxSong::Cat, 1)));
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)]
    );
    destination.tick(&world);
    assert!(inventory.is_empty());
    assert_eq!(destination.get_stack(4).item.id, Item::MUSIC_DISC_11.id);
    assert_eq!(destination.get_stack(4).item_count, 1);
    // Primary SongPlayer.stop returns immediately without an active song.
    // Drain natural completion above so it cannot count as an extraction event.
    assert!(level_events(&observe(&mut reader, &player).await).is_empty());
    // Actual pushing hopper uses the same live setter and immediately starts song.
    world.remove_block_entity(&below);
    set_state(&world, below, Block::AIR.default_state.id);
    let above = POS.up();
    let source = Arc::new(HopperBlockEntity::new(above, FacingHopper::Down));
    source.set_stack(0, disc(JukeboxSong::Cat, 2));
    set_state(&world, above, hopper_state.to_state_id(&Block::HOPPER));
    world.add_block_entity(source.clone());
    observe(&mut reader, &player).await;
    source.tick(&world);
    assert!(live.is_playing());
    assert_eq!(live.get_record().item_count, 1);
    assert_eq!(source.get_stack(0).item_count, 1);
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1010, 5)]
    );
    assert!(!hooks.neighbors.lock().unwrap().is_empty());
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saved_signed_elapsed_and_guarded_live_edits_preserve_imperative_load() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let hooks = Hooks::register(&fixture);
    enable_tick_phase(&world).await;
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    let item = ItemStack::new(1, &Item::MUSIC_DISC_11);
    for elapsed in [
        None,
        Some(0),
        Some(-20),
        Some(1439),
        Some(1440),
        Some(i64::MAX),
    ] {
        world.remove_block_entity(&POS);
        set_state(
            &world,
            POS,
            JukeboxLikeProperties { has_record: true }.to_state_id(&Block::JUKEBOX),
        );
        let raw = record_nbt(&item, elapsed);
        world.add_block_entity_nbt(POS, &raw);
        hooks.events.lock().unwrap().clear();
        observe(&mut reader, &player).await;
        let live = world.get_block_entity(&POS).unwrap();
        let jukebox = live.as_any().downcast_ref::<JukeboxBlockEntity>().unwrap();
        let active = elapsed.is_some_and(|elapsed| elapsed < 1440);
        assert_eq!(jukebox.is_playing(), active);
        assert_eq!(jukebox.get_record().item.id, Item::MUSIC_DISC_11.id);
        assert_eq!(
            persisted(live.as_ref()).get_long(ELAPSED),
            elapsed.filter(|_| active)
        );
        assert!(level_events(&observe(&mut reader, &player).await).is_empty());
        assert!(
            hooks.events.lock().unwrap().is_empty(),
            "fresh restore is pure"
        );
        assert_eq!(power(&world), if active { 15 } else { 0 });
        assert_eq!(comparator(&world), 11);
        assert!(live.chunk_data_nbt().is_none());
        if matches!(elapsed, Some(-20 | 0)) {
            assert_eq!(
                world.tick_block_entities(&tokio::runtime::Handle::current()),
                1
            );
            assert_eq!(
                persisted(live.as_ref()).get_long(ELAPSED),
                elapsed.map(|value| value + 1)
            );
            let packets = observe(&mut reader, &player).await;
            assert!(level_events(&packets).is_empty());
            assert_eq!(notes(&packets), 1);
        }
    }
    let jukebox = install(&world);
    jukebox.set_record(item.clone());
    let live: Arc<dyn BlockEntity> = jukebox.clone();
    world
        .apply_block_entity_nbt(&live, &record_nbt(&item, Some(17)))
        .unwrap();
    observe(&mut reader, &player).await;
    hooks.events.lock().unwrap().clear();
    for elapsed in [None, Some(1440)] {
        // Explicit effective default and changed count remain the same song item.
        let equal = disc(JukeboxSong::Id11, 3);
        world
            .apply_block_entity_nbt(&live, &record_nbt(&equal, elapsed))
            .unwrap();
        assert!(jukebox.is_playing());
        assert_eq!(persisted(live.as_ref()).get_long(ELAPSED), Some(17));
        assert_eq!(jukebox.get_record().item_count, 3);
        assert!(level_events(&observe(&mut reader, &player).await).is_empty());
        assert!(hooks.events.lock().unwrap().is_empty());
    }
    world
        .apply_block_entity_nbt(&live, &record_nbt(&item, Some(0)))
        .unwrap();
    assert_eq!(persisted(live.as_ref()).get_long(ELAPSED), Some(0));
    world
        .apply_block_entity_nbt(&live, &record_nbt(&disc(JukeboxSong::Cat, 1), Some(-5)))
        .unwrap();
    assert!(jukebox.is_playing());
    assert_eq!(persisted(live.as_ref()).get_long(ELAPSED), Some(-5));
    assert_eq!(comparator(&world), 2);
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)],
        "live changed item stops old song, restore never replays"
    );
    assert_eq!(
        hooks.events.lock().unwrap().as_slice(),
        &[("jukebox_stop_play".into(), Some(-5))]
    );
    let before = persisted(live.as_ref());
    hooks.events.lock().unwrap().clear();
    assert!(
        !world
            .edit_block_entity_nbt(&live, &mut |nbt| {
                nbt.child_tags.clear();
                NbtEditDecision::Abort
            })
            .unwrap()
    );
    let mut invalid = before.clone();
    let mut invalid_record = invalid.get_compound("RecordItem").unwrap().clone();
    invalid_record.put_string("id", "unknown:private-item".into());
    invalid.put_compound("RecordItem", invalid_record);
    assert!(world.apply_block_entity_nbt(&live, &invalid).is_err());
    assert_eq!(persisted(live.as_ref()), before);
    assert!(hooks.events.lock().unwrap().is_empty());
    assert!(level_events(&observe(&mut reader, &player).await).is_empty());
    world
        .apply_block_entity_nbt(&live, &record_nbt(&item, None))
        .unwrap();
    assert!(!jukebox.is_playing());
    assert_eq!(power(&world), 0);
    assert_eq!(comparator(&world), 11);
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)]
    );
    world.flush_block_updates();
    assert!(world.block_entity_update_packets.lock().unwrap().is_empty());
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reentrant_inventory_and_replacement_discard_stale_effects_without_locking_callbacks() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let hooks = Hooks::register(&fixture);
    let live = install(&world);
    let inventory = live.clone().get_inventory().unwrap();
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    hooks.mode.store(1, Relaxed);
    let setter = inventory.clone();
    bounded(move || setter.set_stack(0, ItemStack::new(1, &Item::MUSIC_DISC_11))).await;
    assert_eq!(
        live.get_record()
            .get_data_component::<JukeboxPlayableImpl>()
            .unwrap()
            .song,
        JukeboxSong::Cat
    );
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1010, 5)],
        "no stale outer 11 start"
    );
    // Terminal callback publishes and starts a replacement. Its start cannot be
    // followed by the detached old entity's unconditional 1011 remainder.
    hooks.mode.store(2, Relaxed);
    let detached = world.clone();
    bounded(move || detached.remove_block_entity(&POS)).await;
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1010, 5)]
    );
    let replacement = world.published_block_entity(&POS).unwrap();
    assert!(!Arc::ptr_eq(
        &replacement,
        &(live.clone() as Arc<dyn BlockEntity>)
    ));
    assert!(
        replacement
            .as_any()
            .downcast_ref::<JukeboxBlockEntity>()
            .unwrap()
            .is_playing()
    );
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| inventory.clear())).is_err());
    assert!(
        replacement
            .as_any()
            .downcast_ref::<JukeboxBlockEntity>()
            .unwrap()
            .is_playing()
    );
    // Explicit add replacement uses the exact detached old allocation too.
    let fresh = Arc::new(JukeboxBlockEntity::new(POS));
    world.add_block_entity(fresh.clone());
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)]
    );
    assert!(!fresh.is_playing());
    let saved_old = persisted(replacement.as_ref());
    assert_eq!(
        saved_old.get_long(ELAPSED),
        Some(0),
        "terminal detach must preserve resumable elapsed"
    );
    assert!(saved_old.get_compound("RecordItem").is_some());
    world.remove_block_entity(&POS);
    observe(&mut reader, &player).await;
    let count = hooks.events.lock().unwrap().len();
    world.remove_block_entity(&POS);
    assert_eq!(hooks.events.lock().unwrap().len(), count);
    assert!(level_events(&observe(&mut reader, &player).await).is_empty());
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unique_retirement_releases_caller_lifecycle_and_restores_saved_active_song() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let hooks = Hooks::register(&fixture);
    enable_tick_phase(&world).await;
    let live = install(&world);
    live.set_record(ItemStack::new(1, &Item::MUSIC_DISC_11));
    for _ in 0..20 {
        world.tick_block_entities(&tokio::runtime::Handle::current());
    }
    let retained = live.clone().get_inventory().unwrap();
    drop(live);
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    hooks.events.lock().unwrap().clear();
    tokio::time::timeout(
        Duration::from_secs(5),
        world.clean_entity_chunks(&[POS.chunk_position()]),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        world.published_block_entity(&POS).is_some(),
        "retained Inventory blocks unique retirement"
    );
    assert!(hooks.events.lock().unwrap().is_empty());
    drop(retained);
    tokio::time::timeout(
        Duration::from_secs(5),
        world.clean_entity_chunks(&[POS.chunk_position()]),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(world.published_block_entity(&POS).is_none());
    assert_eq!(
        hooks.events.lock().unwrap().as_slice(),
        &[("jukebox_stop_play".into(), None)]
    );
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)]
    );
    let saved = world
        .level
        .read_chunk_sync(&POS.chunk_position(), |chunk| {
            chunk.pending_block_entities.lock().unwrap()[&POS].clone()
        })
        .unwrap();
    assert_eq!(saved.get_long(ELAPSED), Some(20));
    assert!(saved.get_compound("RecordItem").is_some());
    let restored = world.get_block_entity(&POS).unwrap();
    assert!(
        restored
            .as_any()
            .downcast_ref::<JukeboxBlockEntity>()
            .unwrap()
            .is_playing()
    );
    assert_eq!(persisted(restored.as_ref()).get_long(ELAPSED), Some(20));
    assert_eq!(power(&world), 15);
    assert_eq!(comparator(&world), 11);
    assert!(level_events(&observe(&mut reader, &player).await).is_empty());
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handler_consumes_original_slot_before_selection_callback_and_retains_sneak_placement_used()
{
    let mut fixture = fixture().await;
    let player = fixture.player.clone();
    let world = player.world();
    let hooks = Hooks::register(&fixture);
    let live = install(&world);
    player.inventory().set_selected_slot(0);
    player
        .inventory()
        .set_stack(0, ItemStack::new(3, &Item::MUSIC_DISC_11));
    player
        .inventory()
        .set_stack(1, ItemStack::new(7, &Item::DIAMOND));
    hooks.mode.store(4, Relaxed);
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    let played = player.get_stat(
        StatisticCategory::Custom,
        CustomStatistic::PlayRecord as i32,
    );
    use_on(&player, &fixture.server, Hand::Right);
    assert_eq!(player.inventory().get_selected_slot(), 1);
    assert_eq!(player.inventory().get_stack(0).item_count, 2);
    assert!(
        player
            .inventory()
            .get_stack(1)
            .are_equal(&ItemStack::new(7, &Item::DIAMOND))
    );
    assert_eq!(live.get_record().item_count, 1);
    assert!(live.is_playing());
    assert_eq!(
        player.get_stat(
            StatisticCategory::Custom,
            CustomStatistic::PlayRecord as i32
        ),
        played + 1
    );
    let packets = observe(&mut reader, &player).await;
    assert_eq!(level_events(&packets), vec![(1010, 0)]);
    assert_eq!(synced_slots(&packets), vec![(0, Item::MUSIC_DISC_11.id, 2)]);
    // Unchanged original selected slot is consumed exactly once as well.
    live.remove_stack(0);
    player.inventory().set_selected_slot(0);
    observe(&mut reader, &player).await;
    use_on(&player, &fixture.server, Hand::Right);
    assert_eq!(player.inventory().get_stack(0).item_count, 1);
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1010, 0)]
    );
    // A non-disc's sneaking item fallback is outside consuming jukebox accounting.
    player.get_entity().set_sneaking(true);
    player.get_entity().set_pos(Vector3::new(2.5, 65.0, 0.5));
    set_state(&world, POS.up(), Block::AIR.default_state.id);
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::new(3, &Item::STONE));
    let used = player.get_stat(StatisticCategory::Used, Item::STONE.id as i32);
    use_on(&player, &fixture.server, Hand::Right);
    assert_eq!(world.get_block(&POS.up()), &Block::STONE);
    assert_eq!(
        player.inventory().get_stack_in_hand(Hand::Right).item_count,
        2
    );
    assert_eq!(
        player.get_stat(StatisticCategory::Used, Item::STONE.id as i32),
        used + 1
    );
    assert!(live.is_playing(), "sneaking bypasses jukebox ejection");
    let disc_used = player.get_stat(StatisticCategory::Used, Item::MUSIC_DISC_11.id as i32);
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::new(2, &Item::MUSIC_DISC_11));
    use_on(&player, &fixture.server, Hand::Right);
    assert_eq!(
        player.get_stat(StatisticCategory::Used, Item::MUSIC_DISC_11.id as i32),
        disc_used
    );
    assert_eq!(
        player.inventory().get_stack_in_hand(Hand::Right).item_count,
        2
    );
    player.get_entity().set_sneaking(false);
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_nbt_edit_in_first_callback_keeps_start_but_physical_replacement_keeps_new_entity() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let hooks = Hooks::register(&fixture);
    let old = install(&world);
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    hooks.mode.store(6, Relaxed);
    let initial = old.clone();
    bounded(move || initial.set_record(ItemStack::new(1, &Item::MUSIC_DISC_11))).await;
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1010, 0)]
    );
    assert_eq!(power(&world), 15);
    assert_eq!(comparator(&world), 11);
    assert!(!hooks.neighbors.lock().unwrap().is_empty());
    assert_eq!(persisted(old.as_ref()).get_long(ELAPSED), Some(0));
    let drops = world.entities.load().len();
    hooks.mode.store(5, Relaxed);
    let replaced_world = world.clone();
    bounded(move || {
        replaced_world.set_block_state(
            &POS,
            Block::BARREL.default_state.id,
            BlockFlags::NOTIFY_LISTENERS,
        );
    })
    .await;
    let current = world.published_block_entity(&POS).unwrap();
    assert!(!Arc::ptr_eq(
        &current,
        &(old.clone() as Arc<dyn BlockEntity>)
    ));
    let new = current
        .as_any()
        .downcast_ref::<JukeboxBlockEntity>()
        .unwrap();
    assert!(new.is_playing());
    assert_eq!(
        new.get_record()
            .get_data_component::<JukeboxPlayableImpl>()
            .unwrap()
            .song,
        JukeboxSong::Cat
    );
    assert!(old.get_record().is_empty());
    assert_eq!(
        world.entities.load().len(),
        drops + 1,
        "detached old record still has one captured drop"
    );
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0), (1010, 5)]
    );
    assert_eq!(power(&world), 15);
    assert_eq!(comparator(&world), 2);
    world.update_block_entity(&current);
    let pending = world
        .level
        .read_chunk_sync(&POS.chunk_position(), |chunk| {
            chunk.pending_block_entities.lock().unwrap()[&POS].clone()
        })
        .unwrap();
    assert_eq!(pending.get_long(ELAPSED), Some(0));
    assert!(pending.get_compound("RecordItem").is_some());
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_producer_enqueues_without_waiting_for_callback_drainer() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let hooks = Hooks::register(&fixture);
    let live = install(&world);
    let (entered, notification) = mpsc::sync_channel(1);
    let (resume, release) = mpsc::sync_channel(1);
    *hooks.pause.lock().unwrap() = Some((entered, release));
    hooks.mode.store(7, Relaxed);
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    let first = live.clone();
    let outer = tokio::task::spawn_blocking(move || {
        first.set_stack(0, ItemStack::new(1, &Item::MUSIC_DISC_11));
    });
    bounded(move || {
        notification.recv_timeout(Duration::from_secs(5)).unwrap();
    })
    .await;
    let second = live.clone();
    bounded(move || second.set_stack(0, disc(JukeboxSong::Cat, 1))).await;
    assert_eq!(
        live.get_record()
            .get_data_component::<JukeboxPlayableImpl>()
            .unwrap()
            .song,
        JukeboxSong::Cat
    );
    resume.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), outer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1010, 5)]
    );
    assert!(live.is_playing());
    assert_eq!(power(&world), 15);
    drop(reader);
    close_fixture(&fixture).await;
}

fn manual_world(source: &World, dimension: Dimension) -> Arc<World> {
    Arc::new(World::load(
        source.level.clone(),
        source.level_info.clone(),
        dimension,
        source.block_registry.clone(),
        Weak::new(),
        Arc::new(Mutex::new(Scoreboard::new())),
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exact_weak_owner_preflights_and_current_dimension_filters_stale_recipients() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let unbound = manual_world(&world, Dimension::OVERWORLD);
    let live = Arc::new(JukeboxBlockEntity::new(POS));
    let before = persisted(live.as_ref());
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || live.set_record(disc(JukeboxSong::Id11, 1))
        ))
        .is_err()
    );
    set_state(&unbound, POS, Block::JUKEBOX.default_state.id);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || unbound.add_block_entity(live.clone())
        ))
        .is_err()
    );
    assert!(unbound.published_block_entity(&POS).is_none());
    assert_eq!(persisted(live.as_ref()), before);
    unbound.bind_self();
    unbound.bind_self();
    unbound.add_block_entity(live.clone());
    // Server lookup cannot work: this manual owner has Weak::new(), yet actual
    // Inventory mutation must synchronously update this exact World's state.
    assert!(unbound.server.upgrade().is_none());
    live.set_stack(0, ItemStack::new(1, &Item::MUSIC_DISC_11));
    assert_eq!(power(&unbound), 15);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| live.bind_world(&world))).is_err()
    );
    let saved = persisted(live.as_ref());
    let weak_owner = Arc::downgrade(&unbound);
    drop(unbound);
    // No WorldPortal was installed for this manual allocation; no preexisting
    // Level->WorldPortal cycle masks the new bridge's weak ownership here.
    assert!(weak_owner.upgrade().is_none());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| live.clear())).is_err());
    assert_eq!(persisted(live.as_ref()), saved);
    let attached = install(&world);
    enable_tick_phase(&world).await;
    let mut observer = Observer::new(&fixture.server, &world).await;
    let foreign = manual_world(&world, Dimension::THE_NETHER);
    foreign.bind_self();
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    observe(&mut observer.reader, &observer.player).await;
    // Retain stale old membership deliberately; recipient filtering must consult
    // the player's current dimension/world, not the collection they came from.
    observer.player.get_entity().world.store(foreign.clone());
    attached.set_record(ItemStack::new(1, &Item::MUSIC_DISC_11));
    world.tick_block_entities(&tokio::runtime::Handle::current());
    let packets = observe(&mut observer.reader, &observer.player).await;
    assert!(level_events(&packets).is_empty());
    assert_eq!(notes(&packets), 0);
    let packets = observe(&mut reader, &player).await;
    assert_eq!(level_events(&packets), vec![(1010, 0)]);
    assert_eq!(notes(&packets), 1);
    observer.player.get_entity().world.store(world.clone());
    observer.close().await;
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn handler_empty_placement_has_no_stop_and_same_target_barrel_keeps_exact_items() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let hooks = Hooks::register(&fixture);
    set_state(&world, POS, Block::STONE.default_state.id);
    set_state(&world, POS.up(), Block::AIR.default_state.id);
    player.get_entity().set_pos(Vector3::new(2.5, 65.0, 0.5));
    player
        .inventory()
        .set_stack_in_hand(Hand::Right, ItemStack::new(2, &Item::JUKEBOX));
    let (remote, _write) = fixture.remote.split();
    let mut reader = TCPNetworkDecoder::new(remote);
    observe(&mut reader, &player).await;
    use_on(&player, &fixture.server, Hand::Right);
    assert_eq!(world.get_block(&POS.up()), &Block::JUKEBOX);
    let placed = world.published_block_entity(&POS.up()).unwrap();
    let empty = placed
        .as_any()
        .downcast_ref::<JukeboxBlockEntity>()
        .unwrap();
    assert!(!empty.is_playing());
    assert!(empty.get_record().is_empty());
    assert_eq!(
        player.inventory().get_stack_in_hand(Hand::Right).item_count,
        1
    );
    let packets = observe(&mut reader, &player).await;
    assert!(
        level_events(&packets).is_empty(),
        "registry auto-create plus placed must not manufacture removal"
    );
    assert!(world.block_entity_update_packets.lock().unwrap().is_empty());
    world.remove_block_entity(&POS.up());
    set_state(&world, POS.up(), Block::AIR.default_state.id);
    observe(&mut reader, &player).await;
    let old = install(&world);
    old.set_record(ItemStack::new(1, &Item::MUSIC_DISC_11));
    observe(&mut reader, &player).await;
    hooks.mode.store(8, Relaxed);
    let outer = world.clone();
    bounded(move || {
        outer.set_block_state(
            &POS,
            Block::BARREL.default_state.id,
            BlockFlags::NOTIFY_LISTENERS,
        );
    })
    .await;
    assert_eq!(world.get_block(&POS), &Block::BARREL);
    let replacement = hooks.replacement.lock().unwrap().clone().unwrap();
    let current = world.published_block_entity(&POS).unwrap();
    assert!(
        Arc::ptr_eq(&current, &replacement),
        "same terrain still belongs to the callback's exact replacement"
    );
    assert_eq!(current.resource_location(), "minecraft:barrel");
    assert!(
        current
            .clone()
            .get_inventory()
            .unwrap()
            .get_stack(0)
            .are_equal(&ItemStack::new(7, &Item::DIAMOND))
    );
    let pending = world
        .level
        .read_chunk_sync(&POS.chunk_position(), |chunk| {
            chunk.pending_block_entities.lock().unwrap()[&POS].clone()
        })
        .unwrap();
    let items = pending.get_list("Items").unwrap();
    let saved = items
        .iter()
        .find_map(|tag| {
            tag.extract_compound()
                .filter(|item| item.get_byte("Slot") == Some(0))
        })
        .unwrap();
    assert_eq!(saved.get_string("id"), Some("minecraft:diamond"));
    assert_eq!(saved.get_int("count"), Some(7));
    assert_eq!(
        level_events(&observe(&mut reader, &player).await),
        vec![(1011, 0)]
    );
    assert!(old.get_record().is_empty());
    drop(reader);
    close_fixture(&fixture).await;
}
