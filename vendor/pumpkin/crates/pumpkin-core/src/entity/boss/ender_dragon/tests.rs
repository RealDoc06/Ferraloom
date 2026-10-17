//! Authored UNRUN: source-only exact26.3 candidate. Uses registered `PlayerFixture`,
//! production factories/packet dispatch/projectile handlers and owner ticks.
#![allow(
    clippy::too_many_lines,
    reason = "Keep each actual-path scenario in one fixture"
)]

use super::*;
use crate::{
    data::player_server::test::{
        PlayerFixture, close_fixture, drive_packet_future, player_fixture, stop_chunk_workers,
    },
    entity::{
        RemovalReason,
        mob::{piglin::PiglinEntity, piglin_ai::PiglinAi},
        projectile::{
            ProjectileHit, ThrownItemEntity, arrow::ArrowEntity, trident::TridentEntity,
            wind_charge::WindChargeEntity,
        },
    },
    plugin::{
        EventHandler,
        api::events::{
            EventPriority,
            entity::{
                entity_damage::EntityDamageEvent,
                entity_damage_by_entity::EntityDamageByEntityEvent, entity_death::EntityDeathEvent,
                entity_spawn::EntitySpawnEvent, projectile_hit::ProjectileHitEvent,
            },
            world::generic_game::GenericGameEvent,
        },
    },
    server::Server,
    world::{World, dragon_fight::DragonFight},
};
use futures::future::BoxFuture;
use pumpkin_data::{
    attributes::Attributes, entity::EntityPose, item::Item, item_stack::ItemStack,
    packet::CURRENT_MC_VERSION,
};
use pumpkin_inventory::Inventory;
use pumpkin_protocol::{
    ClientPacket, RawPacket,
    java::{
        client::play::{
            CDamageEvent, CEntityStatus, CKeepAlive, CRemoveEntities, CSetEntityMetadata,
            CSpawnEntity, CWorldEvent, Metadata,
        },
        packet_decoder::TCPNetworkDecoder,
        server::play::{SAttack, SInteract},
    },
    packet::MultiVersionJavaPacket,
    ser::{NetworkReadExt, NetworkWriteExt},
};
use pumpkin_util::{Hand, math::boundingbox::BoundingBox};
use std::{
    sync::{Condvar, atomic::AtomicI32, mpsc},
    time::{Duration, Instant},
};
use tokio::io::{AsyncRead, BufReader};
use uuid::Uuid;

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
    fixture
}

fn factory(world: &Arc<World>, uuid: Uuid) -> Arc<dyn EntityBase> {
    crate::entity::r#type::from_type(
        &EntityType::ENDER_DRAGON,
        Vector3::new(8.0, 80.0, 8.0),
        world,
        uuid,
    )
}

fn dragon(entity: &Arc<dyn EntityBase>) -> &EnderDragonEntity {
    entity.cast_any().downcast_ref().unwrap()
}

fn spawn(world: &Arc<World>) -> Arc<dyn EntityBase> {
    let entity = factory(world, Uuid::new_v4());
    assert!(world.spawn_entity(entity.clone()));
    entity.get_mob().unwrap().get_mob_entity().set_no_ai(true);
    entity
}

fn hit(target: &dyn EntityBase, player: &Player, amount: f32, damage_type: DamageType) -> bool {
    target.damage_with_context(
        target,
        amount,
        damage_type,
        Some(Vector3::new(7.0, 80.0, 8.0)),
        Some(player),
        Some(player),
    )
}

async fn attack(fixture: &PlayerFixture, id: i32) {
    let mut packet = Vec::new();
    packet
        .put_var_int(&VarInt(SAttack::to_id(CURRENT_MC_VERSION)))
        .unwrap();
    SAttack {
        entity_id: VarInt(id),
    }
    .write_packet_data(&mut packet, &CURRENT_MC_VERSION)
    .unwrap();
    let mut frame = Vec::new();
    frame
        .put_var_int(&VarInt(packet.len().try_into().unwrap()))
        .unwrap();
    frame.extend(packet);
    let mut decoder = TCPNetworkDecoder::new(BufReader::new(frame.as_slice()));
    let decoded = decoder.get_raw_packet().await.unwrap();
    assert!(
        fixture
            .client
            .java()
            .unwrap()
            .handle_play_packet(&fixture.player, &fixture.server, &decoded)
            .unwrap()
            .is_none()
    );
}

async fn observe<R: AsyncRead + Unpin>(
    reader: &mut TCPNetworkDecoder<R>,
    player: &Player,
) -> Vec<RawPacket> {
    player.try_send_client_packet(&CKeepAlive::new(28_263));
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut packets = Vec::new();
        for _ in 0..2048 {
            let packet = reader.get_raw_packet().await.unwrap();
            if packet.id == CKeepAlive::to_id(CURRENT_MC_VERSION) {
                assert_eq!((&packet.payload[..]).get_i64_be().unwrap(), 28_263);
                return packets;
            }
            packets.push(packet);
        }
        panic!("dragon observer FIFO marker missing");
    })
    .await
    .unwrap()
}

#[derive(Default)]
struct Events {
    cancel_damage: AtomicI32,
    cancel_spawn: AtomicI32,
    damage: Mutex<Vec<(i32, f32)>>,
    direct: Mutex<Vec<(i32, i32)>>,
    deaths: Mutex<Vec<i32>>,
    spawns: Mutex<Vec<i32>>,
    hits: Mutex<Vec<(i32, Option<i32>)>>,
    game_deaths: Mutex<Vec<Vector3<f64>>>,
    interactions: Mutex<Vec<i32>>,
}

impl Events {
    fn register(server: &Arc<Server>) -> Arc<Self> {
        let hooks = Arc::new(Self {
            cancel_damage: AtomicI32::new(i32::MIN),
            cancel_spawn: AtomicI32::new(i32::MIN),
            ..Self::default()
        });
        server.plugin_manager.register::<EntityDamageEvent, _>(
            hooks.clone(),
            EventPriority::Normal,
            true,
        );
        server
            .plugin_manager
            .register::<EntityDamageByEntityEvent, _>(hooks.clone(), EventPriority::Normal, true);
        server.plugin_manager.register::<EntityDeathEvent, _>(
            hooks.clone(),
            EventPriority::Normal,
            true,
        );
        server.plugin_manager.register::<EntitySpawnEvent, _>(
            hooks.clone(),
            EventPriority::Normal,
            true,
        );
        server.plugin_manager.register::<ProjectileHitEvent, _>(
            hooks.clone(),
            EventPriority::Normal,
            true,
        );
        server.plugin_manager.register::<GenericGameEvent, _>(
            hooks.clone(),
            EventPriority::Normal,
            true,
        );
        server.plugin_manager.register::<crate::plugin::api::events::player::player_interact_entity_event::PlayerInteractEntityEvent, _>(hooks.clone(), EventPriority::Normal, true);
        hooks
    }
}

impl
    EventHandler<
        crate::plugin::api::events::player::player_interact_entity_event::PlayerInteractEntityEvent,
    > for Events
{
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut crate::plugin::api::events::player::player_interact_entity_event::PlayerInteractEntityEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.interactions
                .lock()
                .unwrap()
                .push(event.target.get_entity().entity_id);
        })
    }
}

impl EventHandler<EntityDamageEvent> for Events {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut EntityDamageEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.damage
                .lock()
                .unwrap()
                .push((event.entity_id, event.damage));
            event.cancelled |= event.entity_id == self.cancel_damage.load(Ordering::Relaxed);
        })
    }
}
impl EventHandler<EntityDamageByEntityEvent> for Events {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut EntityDamageByEntityEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.direct
                .lock()
                .unwrap()
                .push((event.entity_id, event.damager_id));
        })
    }
}
impl EventHandler<EntityDeathEvent> for Events {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut EntityDeathEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.deaths.lock().unwrap().push(event.entity_id);
        })
    }
}
impl EventHandler<EntitySpawnEvent> for Events {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut EntitySpawnEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.spawns.lock().unwrap().push(event.entity_id);
            event.cancelled |= event.entity_id == self.cancel_spawn.load(Ordering::Relaxed);
        })
    }
}
impl EventHandler<ProjectileHitEvent> for Events {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut ProjectileHitEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.hits
                .lock()
                .unwrap()
                .push((event.entity_id, event.hit_entity_id));
        })
    }
}
impl EventHandler<GenericGameEvent> for Events {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut GenericGameEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.event_key == "minecraft:entity_die" {
                self.game_deaths.lock().unwrap().push(event.position);
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interleaved_unpublished_allocation_keeps_nine_ids_and_exact_current_parts() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let early = Entity::new(
        world.clone(),
        Vector3::new(8.0, 80.0, 8.0),
        &EntityType::ENDER_DRAGON,
    );
    let abandoned = early.entity_id;
    let other_early = Entity::new(
        world.clone(),
        Vector3::new(8.0, 100.0, 8.0),
        &EntityType::ENDER_DRAGON,
    );
    // Deterministic interleaving: ordinary publication between early parent
    // allocation and the consuming dragon constructor, without allocator resets.
    let cow = crate::entity::r#type::from_type(
        &EntityType::COW,
        Vector3::new(1.0, 65.0, 1.0),
        &world,
        Uuid::new_v4(),
    );
    assert!(world.spawn_entity(cow.clone()));
    let live = EnderDragonEntity::new(early);
    assert!(world.spawn_entity(live.clone()));
    let other = EnderDragonEntity::new(other_early);
    assert!(world.spawn_entity(other.clone()));
    let base = live.get_entity().entity_id;
    assert_ne!(base, abandoned);
    assert!(std::ptr::eq(
        world
            .get_entity_by_id(cow.get_entity().entity_id)
            .unwrap()
            .get_entity(),
        cow.get_entity()
    ));
    for (index, part) in live.parts().iter().enumerate() {
        let id = base.wrapping_add(index as i32 + 1);
        assert_eq!(part.entity.entity_id, id);
        assert_ne!(id, cow.get_entity().entity_id);
        assert_eq!(part.kind, DragonPartKind::ALL[index]);
        assert_eq!(
            part.kind.name(),
            [
                "head", "neck", "body", "tail", "tail", "tail", "wing", "wing"
            ][index]
        );
        let (width, height) = [
            (1.0, 1.0),
            (3.0, 3.0),
            (5.0, 3.0),
            (2.0, 2.0),
            (2.0, 2.0),
            (2.0, 2.0),
            (4.0, 2.0),
            (4.0, 2.0),
        ][index];
        let bounds = part.entity.bounding_box.load();
        assert_eq!(bounds.max.x - bounds.min.x, width);
        assert_eq!(bounds.max.y - bounds.min.y, height);
        assert!(world.get_entity_by_id(id).is_none());
        assert!(std::ptr::eq(
            world.get_entity_or_part(id).unwrap().get_entity(),
            &raw const part.entity
        ));
        assert!(Arc::ptr_eq(&part.current_parent().unwrap(), &live));
        assert!(world.entity_tracker.get_tracked_entity(id).is_none());
    }
    assert!(!live.can_hit());
    let mut allocated = std::collections::HashSet::new();
    assert!(allocated.insert(cow.get_entity().entity_id));
    for parent in [&live, &other] {
        assert!(allocated.insert(parent.get_entity().entity_id));
        for (index, part) in parent.parts().iter().enumerate() {
            assert!(allocated.insert(part.entity.entity_id));
            assert_eq!(
                part.entity.entity_id,
                parent.get_entity().entity_id.wrapping_add(index as i32 + 1)
            );
            assert!(std::ptr::eq(
                world
                    .get_entity_or_part(part.entity.entity_id)
                    .unwrap()
                    .get_entity(),
                &raw const part.entity
            ));
        }
    }
    assert_eq!(allocated.len(), 19);
    assert_eq!(world.entities.load().len(), 3);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn latest_encoded_attack_routes_every_part_and_parent_only_observer_metadata() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let hooks = Events::register(&fixture.server);
    let live = spawn(&world);
    let d = dragon(&live);
    let base = live.get_entity().entity_id;
    let tracked = world.entity_tracker.get_tracked_entity(base).unwrap();
    tracked.seen_by.insert(player.gameprofile.id);
    tracked.add_pairing(&player);
    let mut reader = TCPNetworkDecoder::new(BufReader::new(&mut fixture.remote));
    let packets = observe(&mut reader, &player).await;
    let spawn_ids: Vec<_> = packets
        .iter()
        .filter(|p| p.id == CSpawnEntity::to_id(CURRENT_MC_VERSION))
        .map(|p| (&p.payload[..]).get_var_int().unwrap().0)
        .collect();
    assert_eq!(spawn_ids, [base]);
    for packet in packets
        .iter()
        .filter(|p| p.id == CSetEntityMetadata::to_id(CURRENT_MC_VERSION))
    {
        assert_eq!((&packet.payload[..]).get_var_int().unwrap().0, base);
    }
    d.set_phase(EnderDragonPhase::SitScanning);
    tracked.send_changes(&world);
    let packets = observe(&mut reader, &player).await;
    let mut expected = Vec::new();
    Metadata::new(
        pumpkin_data::tracked_data::ender_dragon::DATA_PHASE,
        VarInt(6),
    )
    .write(&mut expected, &CURRENT_MC_VERSION)
    .unwrap();
    assert!(packets.iter().any(|p| {
        p.id == CSetEntityMetadata::to_id(CURRENT_MC_VERSION)
            && p.payload
                .windows(expected.len())
                .any(|bytes| bytes == expected)
    }));
    drop(reader);
    let mut expected_spawns = vec![base];
    for (index, expected_loss) in [8.0, 8.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0]
        .into_iter()
        .enumerate()
    {
        // Each packet gets a fresh actual parent, so the preceding part's hit
        // cannot be rejected by the ordinary hurt cooldown.
        let hit_parent = if index == 0 {
            live.clone()
        } else {
            let parent = spawn(&world);
            expected_spawns.push(parent.get_entity().entity_id);
            parent
        };
        let hit_dragon = dragon(&hit_parent);
        let part = &hit_dragon.parts()[index];
        let part_pos = part.entity.pos.load();
        fixture
            .player
            .get_entity()
            .set_pos(Vector3::new(part_pos.x, part_pos.y, part_pos.z - 2.0));
        fixture
            .player
            .living_entity
            .set_attribute_base(&Attributes::ATTACK_DAMAGE, 8.0);
        fixture
            .player
            .last_attacked_ticks
            .store(100, Ordering::Relaxed);
        let before = hooks.damage.lock().unwrap().len();
        assert_eq!(hit_dragon.mob_entity.living_entity.health.load(), 200.0);
        attack(&fixture, part.entity.entity_id).await;
        assert_eq!(hooks.damage.lock().unwrap().len(), before + 1);
        assert_eq!(
            hooks.damage.lock().unwrap().last().unwrap().0,
            hit_parent.get_entity().entity_id
        );
        assert_eq!(
            hit_dragon.mob_entity.living_entity.health.load(),
            200.0 - expected_loss,
            "encoded attack did not commit the expected loss for {:?}",
            part.kind
        );
    }
    // An unknown ID leaves this connection usable for a following real attack.
    attack(&fixture, i32::MIN).await;
    let next = spawn(&world);
    expected_spawns.push(next.get_entity().entity_id);
    let next_head = &dragon(&next).parts()[0];
    let part_pos = next_head.entity.pos.load();
    fixture
        .player
        .get_entity()
        .set_pos(Vector3::new(part_pos.x, part_pos.y, part_pos.z - 2.0));
    fixture
        .player
        .inventory()
        .set_stack(0, ItemStack::new(1, &Item::DIAMOND_SWORD));
    fixture
        .player
        .last_attacked_ticks
        .store(100, Ordering::Relaxed);
    attack(&fixture, next_head.entity.entity_id).await;
    assert!(dragon(&next).mob_entity.living_entity.health.load() < 200.0);
    assert!(
        fixture
            .player
            .inventory()
            .get_stack_in_hand(Hand::Right)
            .get_damage()
            > 0
    );
    assert_eq!(
        hooks.spawns.lock().unwrap().as_slice(),
        expected_spawns.as_slice()
    );
    // This asserts the existing successful Weapon route, not unported
    // ItemStack.hurtEnemy/postHurtEnemy durability/enchantment parity.
    world.remove_entity(live.as_ref());
    let mut reader = TCPNetworkDecoder::new(BufReader::new(&mut fixture.remote));
    let packets = observe(&mut reader, &player).await;
    let mut removed = Vec::new();
    for packet in packets
        .iter()
        .filter(|p| p.id == CRemoveEntities::to_id(CURRENT_MC_VERSION))
    {
        let mut payload = &packet.payload[..];
        let count = payload.get_var_int().unwrap().0;
        for _ in 0..count {
            removed.push(payload.get_var_int().unwrap().0);
        }
    }
    assert_eq!(removed, [base]);
    assert!(d.parts().iter().all(|part| !part.entity.is_removed()));
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn latest_encoded_interact_uses_current_part_and_unknown_id_keeps_connection() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let live = spawn(&world);
    let hooks = Events::register(&fixture.server);
    let part_id = dragon(&live).parts()[0].entity.entity_id;
    for id in [part_id, i32::MIN, part_id] {
        let mut packet = Vec::new();
        packet
            .put_var_int(&VarInt(SInteract::to_id(CURRENT_MC_VERSION)))
            .unwrap();
        SInteract {
            entity_id: VarInt(id),
            r#type: VarInt(2),
            hand: Some(VarInt(0)),
            target_position: Some(Vector3::new(0.0, 0.0, 0.0)),
            sneaking: false,
        }
        .write_packet_data(&mut packet, &CURRENT_MC_VERSION)
        .unwrap();
        let mut frame = Vec::new();
        frame
            .put_var_int(&VarInt(packet.len().try_into().unwrap()))
            .unwrap();
        frame.extend(packet);
        let mut decoder = TCPNetworkDecoder::new(BufReader::new(frame.as_slice()));
        let packet = decoder.get_raw_packet().await.unwrap();
        assert!(
            fixture
                .client
                .java()
                .unwrap()
                .handle_play_packet(&fixture.player, &fixture.server, &packet)
                .unwrap()
                .is_none()
        );
        assert!(!fixture.client.java().unwrap().is_closed());
    }
    assert_eq!(
        hooks.interactions.lock().unwrap().as_slice(),
        &[part_id, part_id]
    );
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_part_and_parent_damage_scaling_gates_cancellation_and_sitting_loss() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let hooks = Events::register(&fixture.server);
    for index in 0..9 {
        let live = spawn(&world);
        let d = dragon(&live);
        let target: &dyn EntityBase = if index == 8 {
            live.as_ref()
        } else {
            d.parts()[index].as_ref()
        };
        assert!(hit(target, &fixture.player, 8.0, DamageType::PLAYER_ATTACK));
        let loss = if index < 2 { 8.0 } else { 3.0 };
        assert_eq!(d.mob_entity.living_entity.health.load(), 200.0 - loss);
        assert_eq!(*d.sitting_damage_received.lock().unwrap(), loss);
        assert_eq!(
            hooks.damage.lock().unwrap().last().unwrap().0,
            live.get_entity().entity_id
        );
    }
    let live = spawn(&world);
    let d = dragon(&live);
    let head = d.parts()[0].as_ref();
    assert!(head.damage(head, 10.0, DamageType::GENERIC));
    assert_eq!(d.mob_entity.living_entity.health.load(), 200.0);
    hooks
        .cancel_damage
        .store(live.get_entity().entity_id, Ordering::Relaxed);
    assert!(hit(head, &fixture.player, 10.0, DamageType::PLAYER_ATTACK));
    assert_eq!(*d.sitting_damage_received.lock().unwrap(), 0.0);
    hooks.cancel_damage.store(i32::MIN, Ordering::Relaxed);
    assert!(!hit(head, &fixture.player, 0.0, DamageType::PLAYER_ATTACK));
    assert_eq!(*d.sitting_damage_received.lock().unwrap(), 0.0);
    d.mob_entity.living_entity.set_absorption(10.0);
    assert!(hit(head, &fixture.player, 10.0, DamageType::GENERIC_KILL));
    assert_eq!(d.mob_entity.living_entity.health.load(), 200.0);
    assert_eq!(*d.sitting_damage_received.lock().unwrap(), 0.0);
    assert!(hit(head, &fixture.player, 50.0, DamageType::GENERIC_KILL));
    assert_eq!(d.current_phase(), EnderDragonPhase::Hovering);
    assert_eq!(*d.sitting_damage_received.lock().unwrap(), 50.0);
    assert!(hit(head, &fixture.player, 0.5, DamageType::GENERIC_KILL));
    assert_eq!(d.current_phase(), EnderDragonPhase::TakingOff);
    assert_eq!(*d.sitting_damage_received.lock().unwrap(), 0.0);
    d.set_phase(EnderDragonPhase::Dying);
    assert!(!hit(head, &fixture.player, 10.0, DamageType::PLAYER_ATTACK));
    close_fixture(&fixture).await;
}

struct NestedDamage {
    target: Arc<dyn EntityBase>,
    player: Arc<Player>,
    once: AtomicBool,
}
impl EventHandler<EntityDamageEvent> for NestedDamage {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut EntityDamageEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.entity_id == self.target.get_entity().entity_id
                && !self.once.swap(true, Ordering::SeqCst)
            {
                assert!(hit(
                    dragon(&self.target).parts()[0].as_ref(),
                    &self.player,
                    10.0,
                    DamageType::GENERIC_KILL
                ));
            }
        })
    }
}

struct PausedDamage {
    id: i32,
    arrived: Mutex<usize>,
    ready: Condvar,
}
impl EventHandler<EntityDamageEvent> for PausedDamage {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut EntityDamageEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.entity_id == self.id {
                let mut arrived = self.arrived.lock().unwrap();
                *arrived += 1;
                self.ready.notify_all();
                let (arrived, timeout) = self
                    .ready
                    .wait_timeout_while(arrived, Duration::from_secs(5), |count| *count < 2)
                    .unwrap();
                drop(arrived);
                assert!(
                    !timeout.timed_out(),
                    "both real damage calls must reach their event"
                );
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nested_and_paused_actual_damage_count_each_committed_loss_once() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let nested = spawn(&world);
    fixture
        .server
        .plugin_manager
        .register::<EntityDamageEvent, _>(
            Arc::new(NestedDamage {
                target: nested.clone(),
                player: fixture.player.clone(),
                once: AtomicBool::new(false),
            }),
            EventPriority::Normal,
            true,
        );
    assert!(hit(
        dragon(&nested).parts()[0].as_ref(),
        &fixture.player,
        10.0,
        DamageType::GENERIC_KILL
    ));
    assert_eq!(nested.get_living_entity().unwrap().health.load(), 180.0);
    assert_eq!(
        *dragon(&nested).sitting_damage_received.lock().unwrap(),
        20.0
    );
    let paused = spawn(&world);
    fixture
        .server
        .plugin_manager
        .register::<EntityDamageEvent, _>(
            Arc::new(PausedDamage {
                id: paused.get_entity().entity_id,
                arrived: Mutex::new(0),
                ready: Condvar::new(),
            }),
            EventPriority::Normal,
            true,
        );
    std::thread::scope(|scope| {
        let a = scope.spawn(|| {
            hit(
                dragon(&paused).parts()[0].as_ref(),
                &fixture.player,
                10.0,
                DamageType::GENERIC_KILL,
            )
        });
        let b = scope.spawn(|| {
            hit(
                dragon(&paused).parts()[1].as_ref(),
                &fixture.player,
                10.0,
                DamageType::GENERIC_KILL,
            )
        });
        assert!(a.join().unwrap());
        assert!(b.join().unwrap());
    });
    assert_eq!(paused.get_living_entity().unwrap().health.load(), 180.0);
    assert_eq!(
        *dragon(&paused).sitting_damage_received.lock().unwrap(),
        20.0
    );
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn phase_entry_blocks_readers_until_reset_and_competing_entry_cannot_be_stale() {
    let fixture = fixture().await;
    let live = spawn(&fixture.player.world());
    let d = dragon(&live);
    let origin = BlockPos::new(0, 0, 0);
    d.set_fight_origin(origin);
    let target = fixture.player.world().dragon_death_target(origin);
    live.get_entity()
        .set_pos(target + Vector3::new(10.0, 0.0, 0.0));
    d.set_phase(EnderDragonPhase::Dying);
    d.phase_manager.get_phase(EnderDragonPhase::Dying).tick(d);
    assert_eq!(
        d.phase_manager
            .get_phase(EnderDragonPhase::Dying)
            .get_fly_target_location(),
        Some(target)
    );
    d.set_phase(EnderDragonPhase::Hovering);
    let charge = d.fireball_charge.lock().unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let first = scope.spawn(|| d.set_phase(EnderDragonPhase::Strafing));
        let deadline = Instant::now() + Duration::from_secs(5);
        while d.phase.try_lock().is_ok() {
            assert!(
                Instant::now() < deadline,
                "entry must reach the held charge lock"
            );
            std::thread::yield_now();
        }
        let reader = scope.spawn(|| {
            let current = d.current_phase();
            done_tx.send(current).unwrap();
        });
        let second = scope.spawn(|| d.set_phase(EnderDragonPhase::Dying));
        assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
        drop(charge);
        first.join().unwrap();
        second.join().unwrap();
        reader.join().unwrap();
    });
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        EnderDragonPhase::Strafing | EnderDragonPhase::Dying
    ));
    assert_eq!(d.current_phase(), EnderDragonPhase::Dying);
    assert_eq!(
        d.phase_manager
            .get_phase(EnderDragonPhase::Dying)
            .get_fly_target_location(),
        None
    );
    d.phase_manager.get_phase(d.current_phase()).tick(d);
    assert_eq!(d.mob_entity.living_entity.health.load(), 1.0);
    assert_eq!(
        d.phase_manager
            .get_phase(EnderDragonPhase::Dying)
            .get_fly_target_location(),
        Some(target)
    );
    close_fixture(&fixture).await;
}

fn projectile_hit(target: Arc<dyn EntityBase>) -> ProjectileHit {
    ProjectileHit::Entity {
        hit_pos: target.get_entity().pos.load(),
        entity: target,
        normal: Vector3::new(-1.0, 0.0, 0.0),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_player_arrow_hit_stops_piglin_admiration_and_commits_health() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let hooks = Events::register(&fixture.server);
    let target = crate::entity::r#type::from_type(
        &EntityType::PIGLIN,
        Vector3::new(3.0, 65.0, 0.5),
        &world,
        Uuid::new_v4(),
    );
    assert!(world.spawn_entity(target.clone()));
    let piglin = target.cast_any().downcast_ref::<PiglinEntity>().unwrap();
    piglin.mob_entity.set_no_ai(true);
    piglin.set_dancing(true);
    piglin.celebration_timer.store(60, Ordering::Relaxed);
    let mut gold = ItemStack::new(1, &Item::GOLD_INGOT);
    assert!(piglin.mob_interact(&fixture.player, &mut gold));
    assert!(piglin.is_admiring());
    assert_eq!(piglin.off_hand_item().item, &Item::GOLD_INGOT);
    assert!(!piglin.is_admiring_disabled());

    let arrow = Arc::new(ArrowEntity::new(
        Entity::new(
            world.clone(),
            Vector3::new(1.0, 65.5, 0.5),
            &EntityType::ARROW,
        ),
        Some(fixture.player.entity_id()),
    ));
    arrow
        .get_entity()
        .velocity
        .store(Vector3::new(2.0, 0.0, 0.0));
    assert!(world.spawn_entity(arrow.clone()));
    let living = &piglin.mob_entity.living_entity;
    let before = living.health.load();
    assert!(before > 4.0);
    arrow.on_hit(projectile_hit(target.clone()));

    assert_eq!(living.health.load(), before - 4.0);
    assert!(!living.dead.load(Ordering::Relaxed));
    assert!(!piglin.is_admiring());
    assert!(piglin.off_hand_item().is_empty());
    assert!(piglin.admiring_item.lock().unwrap().is_none());
    assert_eq!(piglin.celebration_timer.load(Ordering::Relaxed), 0);
    assert!(!piglin.is_dancing.load(Ordering::Relaxed));
    assert_eq!(
        piglin.admiring_disabled_timer.load(Ordering::Relaxed),
        PiglinAi::ADMIRING_DISABLED_DURATION
    );
    assert_eq!(
        hooks.direct.lock().unwrap().as_slice(),
        &[(target.get_entity().entity_id, arrow.get_entity().entity_id)]
    );
    assert_eq!(
        living.last_hurt_by_player_id.load(Ordering::Relaxed),
        fixture.player.entity_id()
    );
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_damage_packets_order_responsible_then_direct_with_melee_and_sourceless_controls() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let mut expected = Vec::new();
    for kind in 0..3 {
        let target = crate::entity::r#type::from_type(
            &EntityType::PIGLIN,
            Vector3::new(3.0, 65.0, 0.5),
            &world,
            Uuid::new_v4(),
        );
        assert!(world.spawn_entity(target.clone()));
        target.get_mob().unwrap().get_mob_entity().set_no_ai(true);
        let target_id = target.get_entity().entity_id;
        let tracked = world.entity_tracker.get_tracked_entity(target_id).unwrap();
        tracked.seen_by.insert(player.gameprofile.id);
        tracked.add_pairing(&player);
        let living = target.get_living_entity().unwrap();
        let before = living.health.load();
        let (damage_type, cause_id, direct_id, loss) = match kind {
            0 => {
                let arrow = Arc::new(ArrowEntity::new(
                    Entity::new(
                        world.clone(),
                        Vector3::new(1.0, 65.5, 0.5),
                        &EntityType::ARROW,
                    ),
                    Some(player.entity_id()),
                ));
                arrow
                    .get_entity()
                    .velocity
                    .store(Vector3::new(2.0, 0.0, 0.0));
                assert!(world.spawn_entity(arrow.clone()));
                let direct_id = arrow.get_entity().entity_id;
                assert_ne!(direct_id, player.entity_id());
                arrow.on_hit(projectile_hit(target.clone()));
                (
                    DamageType::ARROW,
                    player.entity_id() + 1,
                    direct_id + 1,
                    4.0,
                )
            }
            1 => {
                player
                    .living_entity
                    .set_attribute_base(&Attributes::ATTACK_DAMAGE, 4.0);
                player.last_attacked_ticks.store(100, Ordering::Relaxed);
                player.attack(&target);
                (
                    DamageType::PLAYER_ATTACK,
                    player.entity_id() + 1,
                    player.entity_id() + 1,
                    4.0,
                )
            }
            _ => {
                assert!(target.damage(target.as_ref(), 2.0, DamageType::GENERIC));
                (DamageType::GENERIC, 0, 0, 2.0)
            }
        };
        assert_eq!(living.health.load(), before - loss);
        expected.push([target_id, i32::from(damage_type.id), cause_id, direct_id]);
    }
    let mut reader = TCPNetworkDecoder::new(BufReader::new(&mut fixture.remote));
    let packets = observe(&mut reader, &player).await;
    let received: Vec<_> = packets
        .iter()
        .filter(|packet| packet.id == CDamageEvent::to_id(CURRENT_MC_VERSION))
        .map(|packet| {
            let mut payload = &packet.payload[..];
            [
                payload.get_var_int().unwrap().0,
                payload.get_var_int().unwrap().0,
                payload.get_var_int().unwrap().0,
                payload.get_var_int().unwrap().0,
            ]
        })
        .collect();
    assert_eq!(received, expected);
    drop(reader);
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_arrow_hit_wither_separates_responsible_immunity_from_direct_projectile() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    for (owner_type, powered, accepted) in [
        (&EntityType::PLAYER, false, true),
        (&EntityType::PLAYER, true, false),
        (&EntityType::WITHER, false, false),
        (&EntityType::WITHER_SKELETON, false, false),
    ] {
        let target = crate::entity::r#type::from_type(
            &EntityType::WITHER,
            Vector3::new(3.0, 65.0, 0.5),
            &world,
            Uuid::new_v4(),
        );
        assert!(world.spawn_entity(target.clone()));
        target.get_mob().unwrap().get_mob_entity().set_no_ai(true);
        let living = target.get_living_entity().unwrap();
        let before = living.get_max_health() * if powered { 0.5 } else { 1.0 };
        living.set_health(before);
        let owner: Arc<dyn EntityBase> = if owner_type == &EntityType::PLAYER {
            fixture.player.clone()
        } else {
            let owner = crate::entity::r#type::from_type(
                owner_type,
                Vector3::new(0.5, 65.0, 0.5),
                &world,
                Uuid::new_v4(),
            );
            assert!(world.spawn_entity(owner.clone()));
            owner
        };
        let arrow = Arc::new(ArrowEntity::new(
            Entity::new(
                world.clone(),
                Vector3::new(1.0, 65.5, 0.5),
                &EntityType::ARROW,
            ),
            Some(owner.get_entity().entity_id),
        ));
        arrow
            .get_entity()
            .velocity
            .store(Vector3::new(2.0, 0.0, 0.0));
        assert!(world.spawn_entity(arrow.clone()));
        arrow.on_hit(projectile_hit(target.clone()));
        assert_eq!(
            living.health.load(),
            before - if accepted { 4.0 } else { 0.0 }
        );
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_projectile_handlers_ignite_direct_arrows_and_player_wind_charge_only() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let hooks = Events::register(&fixture.server);
    for phase in [
        EnderDragonPhase::SitBreathing,
        EnderDragonPhase::SitScanning,
        EnderDragonPhase::SitAttacking,
        EnderDragonPhase::Hovering,
    ] {
        for kind in 0..4 {
            let live = spawn(&world);
            let d = dragon(&live);
            d.set_phase(phase);
            let base = Entity::new(
                world.clone(),
                Vector3::new(2.0, 80.5, 8.0),
                match kind {
                    0 => &EntityType::ARROW,
                    1 => &EntityType::TRIDENT,
                    2 => &EntityType::WIND_CHARGE,
                    _ => &EntityType::BREEZE_WIND_CHARGE,
                },
            );
            let direct: Arc<dyn EntityBase> = match kind {
                0 => Arc::new(ArrowEntity::new(base, Some(fixture.player.entity_id()))),
                1 => Arc::new(TridentEntity::new(base, Some(fixture.player.entity_id()))),
                2 => Arc::new(WindChargeEntity::new_normal(ThrownItemEntity::new(
                    base,
                    fixture.player.get_entity(),
                    0.0,
                ))),
                _ => Arc::new(WindChargeEntity::new_breeze(ThrownItemEntity::new(
                    base,
                    fixture.player.get_entity(),
                    0.0,
                ))),
            };
            direct
                .get_entity()
                .velocity
                .store(Vector3::new(4.0, 0.0, 0.0));
            direct.get_entity().set_frozen_ticks(90);
            let long_burn = kind == 1;
            if long_burn {
                direct.get_entity().fire_ticks.store(70, Ordering::Relaxed);
            }
            let shooter_fire = fixture
                .player
                .get_entity()
                .fire_ticks
                .load(Ordering::Relaxed);
            direct.on_hit(projectile_hit(d.parts()[0].clone()));
            let rejected = phase != EnderDragonPhase::Hovering && kind < 3;
            if rejected {
                assert_eq!(
                    direct.get_entity().fire_ticks.load(Ordering::Relaxed),
                    if long_burn { 70 } else { 20 }
                );
                assert_eq!(direct.get_entity().get_frozen_ticks(), 0);
                assert_eq!(d.mob_entity.living_entity.health.load(), 200.0);
            } else {
                assert_eq!(
                    direct.get_entity().fire_ticks.load(Ordering::Relaxed),
                    if long_burn { 70 } else { -1 }
                );
                assert_eq!(direct.get_entity().get_frozen_ticks(), 90);
                assert_eq!(
                    d.mob_entity.living_entity.health.load(),
                    if kind < 2 { 192.0 } else { 199.0 }
                );
                assert!(
                    hooks
                        .direct
                        .lock()
                        .unwrap()
                        .contains(&(live.get_entity().entity_id, direct.get_entity().entity_id))
                );
                assert_eq!(
                    d.mob_entity
                        .living_entity
                        .last_hurt_by_player_id
                        .load(Ordering::Relaxed),
                    fixture.player.entity_id()
                );
            }
            assert_eq!(
                fixture
                    .player
                    .get_entity()
                    .fire_ticks
                    .load(Ordering::Relaxed),
                shooter_fire
            );
            world.remove_entity(live.as_ref());
        }
    }
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_projectile_ticks_hit_parts_before_parent_box_and_exclude_parent_owner() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let hooks = Events::register(&fixture.server);
    for owner_is_parent in [false, true] {
        for kind in 0..3 {
            let live = spawn(&world);
            let d = dragon(&live);
            d.set_phase(EnderDragonPhase::SitScanning);
            for part in d.parts() {
                part.entity.set_pos(Vector3::new(20.0, 80.0, 8.0));
            }
            d.parts()[0].entity.set_pos(Vector3::new(8.0, 80.0, 8.0));
            let owner = if owner_is_parent {
                live.get_entity()
            } else {
                fixture.player.get_entity()
            };
            let base = Entity::new(
                world.clone(),
                Vector3::new(2.0, 80.5, 8.0),
                match kind {
                    0 => &EntityType::ARROW,
                    1 => &EntityType::TRIDENT,
                    _ => &EntityType::WIND_CHARGE,
                },
            );
            let direct: Arc<dyn EntityBase> = match kind {
                0 => Arc::new(ArrowEntity::new(base, Some(owner.entity_id))),
                1 => Arc::new(TridentEntity::new(base, Some(owner.entity_id))),
                _ => Arc::new(WindChargeEntity::new_normal(ThrownItemEntity::new(
                    base, owner, 0.0,
                ))),
            };
            direct.get_entity().set_pos(Vector3::new(2.0, 80.5, 8.0));
            direct
                .get_entity()
                .velocity
                .store(Vector3::new(10.0, 0.0, 0.0));
            assert!(world.spawn_entity(direct.clone()));
            direct.tick(direct.as_ref(), &fixture.server);
            assert_eq!(d.mob_entity.living_entity.health.load(), 200.0);
            if owner_is_parent {
                assert!(direct.get_entity().fire_ticks.load(Ordering::Relaxed) <= 0);
                assert!(!direct.get_entity().is_removed());
            } else {
                assert!(direct.get_entity().fire_ticks.load(Ordering::Relaxed) >= 20);
                if kind == 0 {
                    assert!(hooks.hits.lock().unwrap().contains(&(
                        direct.get_entity().entity_id,
                        Some(d.parts()[0].entity.entity_id)
                    )));
                }
            }
            world.remove_entity(direct.as_ref());
            world.remove_entity(live.as_ref());
        }
    }
    // Generic AABB lookup keeps its old ordinary-parent semantics.
    let live = spawn(&world);
    let bounds = BoundingBox::new(Vector3::new(0.0, 79.0, 0.0), Vector3::new(16.0, 90.0, 16.0));
    assert!(
        world
            .get_entities_at_box(&bounds)
            .iter()
            .any(|e| std::ptr::eq(e.get_entity(), live.get_entity()))
    );
    assert!(
        world
            .get_projectile_entities_at_box(&bounds, None)
            .iter()
            .all(|e| !std::ptr::eq(e.get_entity(), live.get_entity()))
    );
    close_fixture(&fixture).await;
}

fn end_world(fixture: &PlayerFixture) -> Arc<World> {
    let source = fixture.player.world();
    let world = Arc::new(World::load(
        source.level.clone(),
        source.level_info.clone(),
        pumpkin_data::dimension::Dimension::THE_END,
        source.block_registry.clone(),
        Arc::downgrade(&fixture.server),
        source.scoreboard.clone(),
    ));
    world.bind_self();
    world
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn factory_nbt_natural_and_matching_loaded_fight_keep_phase_and_origin_contracts() {
    let fixture = fixture().await;
    let world = end_world(&fixture);
    let fresh = factory(&world, Uuid::new_v4());
    assert_eq!(dragon(&fresh).current_phase(), EnderDragonPhase::Hovering);
    fresh.read_nbt_non_mut(&NbtCompound::new());
    assert_eq!(dragon(&fresh).current_phase(), EnderDragonPhase::Hovering);
    for id in -1..=11 {
        let loaded = factory(&world, Uuid::new_v4());
        let mut nbt = NbtCompound::new();
        nbt.put_int("DragonPhase", id);
        nbt.put_int("DragonDeathTime", 155);
        nbt.put_float("sitting_damage_received", 17.5);
        loaded.read_nbt_non_mut(&nbt);
        assert_eq!(
            dragon(&loaded).current_phase().network_id(),
            if (0..11).contains(&id) { id } else { 0 }
        );
        let mut saved = NbtCompound::new();
        loaded.write_nbt(&mut saved);
        assert_eq!(saved.get_int("DragonDeathTime"), Some(155));
        assert_eq!(saved.get_float("sitting_damage_received"), Some(17.5));
        assert_eq!(
            saved.get_int("DragonPhase"),
            Some(dragon(&loaded).current_phase().network_id())
        );
    }
    let manager = world.dragon_fight.as_ref().unwrap();
    let natural_uuid = {
        let mut fight = manager.lock().unwrap();
        fight.previously_killed = true;
        fight.origin = BlockPos::new(8, 16, 8);
        fight.exit_portal_location = Some(BlockPos::new(1, 70, 1));
        fight.create_new_dragon(&world).unwrap()
    };
    let natural = world.get_entity_by_uuid(natural_uuid).unwrap();
    assert_eq!(
        natural.get_entity().pos.load(),
        Vector3::new(8.0, 144.0, 8.0)
    );
    assert_eq!(dragon(&natural).current_phase(), EnderDragonPhase::Circling);
    let origin = *dragon(&natural).fight_origin.lock().unwrap();
    manager
        .lock()
        .unwrap()
        .update_dragon(&world, natural_uuid, 123.0, 200.0, None);
    assert_eq!(*dragon(&natural).fight_origin.lock().unwrap(), origin);
    assert!(Arc::ptr_eq(
        &dragon(&natural).attached_fight().unwrap(),
        manager
    ));

    world.remove_entity(natural.as_ref());
    let adopted = factory(&world, natural_uuid);
    assert!(dragon(&adopted).attached_fight().is_none());
    // Loaded health-zero continuation must attach before the AI health guard.
    adopted.get_living_entity().unwrap().set_health(0.0);
    assert!(world.spawn_entity(adopted.clone()));
    adopted.tick(adopted.as_ref(), &fixture.server);
    assert!(Arc::ptr_eq(
        &dragon(&adopted).attached_fight().unwrap(),
        manager
    ));
    assert_eq!(*dragon(&adopted).dragon_death_time.lock().unwrap(), 1);
    let unmatched = spawn(&world);
    dragon(&unmatched).ai_step();
    assert!(dragon(&unmatched).attached_fight().is_none());
    let unpublished_same_uuid = factory(&world, natural_uuid);
    dragon(&unpublished_same_uuid).ai_step();
    assert!(dragon(&unpublished_same_uuid).attached_fight().is_none());
    // The old exact object cannot bind the adopted replacement's manager.
    *dragon(&natural).fight.lock().unwrap() = None;
    dragon(&natural).ai_step();
    assert!(dragon(&natural).attached_fight().is_none());
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_lethal_hook_and_phase_handoff_own_shared_clock_without_generic_retirement() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let hooks = Events::register(&fixture.server);
    let live = spawn(&world);
    let d = dragon(&live);
    d.set_phase(EnderDragonPhase::Circling);
    d.mob_entity.living_entity.set_health(5.0);
    assert!(hit(
        d.parts()[0].as_ref(),
        &fixture.player,
        8.0,
        DamageType::PLAYER_ATTACK
    ));
    assert_eq!(d.mob_entity.living_entity.health.load(), 1.0);
    assert!(!d.mob_entity.living_entity.dead.load(Ordering::Relaxed));
    assert_eq!(d.current_phase(), EnderDragonPhase::Dying);
    assert_eq!(
        d.phase_manager
            .get_phase(EnderDragonPhase::Dying)
            .get_fly_speed(),
        3.0
    );
    assert!(live.get_entity().pose.load() == EntityPose::Dying);
    assert_eq!(
        hooks.deaths.lock().unwrap().as_slice(),
        &[live.get_entity().entity_id]
    );
    let target = world.dragon_death_target(BlockPos::new(0, 0, 0));
    // Set the actual clipped-collision fields only to select phase branches;
    // the movement clipping producer has a separate world control below.
    live.get_entity()
        .set_pos(target + Vector3::new(10.0, 0.0, 0.0));
    for _ in 0..2 {
        live.get_entity()
            .horizontal_collision
            .store(false, Ordering::Relaxed);
        live.get_entity()
            .vertical_collision
            .store(false, Ordering::Relaxed);
        live.get_entity()
            .set_pos(target + Vector3::new(10.0, 0.0, 0.0));
        live.tick(live.as_ref(), &fixture.server);
        assert_eq!(*d.dragon_death_time.lock().unwrap(), 0);
        assert_eq!(d.mob_entity.living_entity.health.load(), 1.0);
    }
    live.get_entity()
        .vertical_collision
        .store(true, Ordering::Relaxed);
    live.tick(live.as_ref(), &fixture.server);
    assert_eq!(d.mob_entity.living_entity.health.load(), 0.0);
    assert_eq!(*d.dragon_death_time.lock().unwrap(), 0);
    live.tick(live.as_ref(), &fixture.server);
    assert_eq!(*d.dragon_death_time.lock().unwrap(), 1);
    for _ in 1..20 {
        live.tick(live.as_ref(), &fixture.server);
    }
    assert_eq!(*d.dragon_death_time.lock().unwrap(), 20);
    assert!(!live.get_entity().is_removed());
    assert_eq!(
        hooks.deaths.lock().unwrap().as_slice(),
        &[live.get_entity().entity_id]
    );

    for phase in [
        EnderDragonPhase::SitBreathing,
        EnderDragonPhase::SitScanning,
        EnderDragonPhase::SitAttacking,
        EnderDragonPhase::Hovering,
    ] {
        let sitting = spawn(&world);
        let d = dragon(&sitting);
        d.set_phase(phase);
        d.mob_entity.living_entity.set_health(5.0);
        assert!(hit(
            d.parts()[0].as_ref(),
            &fixture.player,
            8.0,
            DamageType::PLAYER_ATTACK
        ));
        assert_eq!(d.mob_entity.living_entity.health.load(), 0.0);
        assert!(!d.mob_entity.living_entity.dead.load(Ordering::Relaxed));
        assert_eq!(d.current_phase(), phase);
        assert_eq!(*d.sitting_damage_received.lock().unwrap(), 5.0);
        sitting.tick(sitting.as_ref(), &fixture.server);
        assert_eq!(*d.dragon_death_time.lock().unwrap(), 1);
    }
    close_fixture(&fixture).await;
}

fn collect_xp(player: &Arc<Player>, server: &Arc<Server>, world: &Arc<World>) -> i32 {
    let before = player.get_total_experience();
    let orbs: Vec<_> = world
        .entities
        .load()
        .iter()
        .filter(|e| e.get_entity().entity_type == &EntityType::EXPERIENCE_ORB)
        .cloned()
        .collect();
    for orb in orbs {
        // Consume actual pickup delay through the real player tick; no access to
        // the orb's private amount and no synthetic XP summation model.
        player.tick(server);
        player.tick(server);
        orb.on_player_collision(player);
        assert!(orb.get_entity().is_removed());
    }
    player.get_total_experience() - before
}

fn death_world_events(packets: &[RawPacket]) -> usize {
    packets
        .iter()
        .filter(|p| {
            p.id == CWorldEvent::to_id(CURRENT_MC_VERSION)
                && (&p.payload[..]).get_i32_be().unwrap() == 1028
        })
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_death_ticks_1_20_155_180_200_award_move_and_retire_once() {
    for (drops, silent, first_fight) in [
        (true, false, false),
        (false, true, false),
        (true, false, true),
    ] {
        let mut fixture = fixture().await;
        let world = fixture.player.world();
        let hooks = Events::register(&fixture.server);
        world.level_info.rcu(|current| {
            let mut next = (**current).clone();
            next.game_rules.mob_drops = drops;
            next
        });
        let live = spawn(&world);
        let d = dragon(&live);
        live.get_entity().set_silent(silent);
        let fight = first_fight.then(|| {
            let fight = Arc::new(Mutex::new(DragonFight::new()));
            {
                let mut f = fight.lock().unwrap();
                f.dragon_uuid = Some(live.get_entity().entity_uuid);
                f.exit_portal_location = Some(BlockPos::new(8, 64, 8));
                f.gateways.clear();
            };
            d.attach_fight(&fight);
            fight
        });
        d.mob_entity.living_entity.set_health(5.0);
        assert!(hit(
            d.parts()[0].as_ref(),
            &fixture.player,
            8.0,
            DamageType::PLAYER_ATTACK
        ));
        let player = fixture.player.clone();
        let tracked = world
            .entity_tracker
            .get_tracked_entity(live.get_entity().entity_id)
            .unwrap();
        tracked.seen_by.insert(player.gameprofile.id);
        tracked.add_pairing(&player);
        let initial = live.get_entity().pos.load();
        let part_initial: Vec<_> = d.parts().iter().map(|p| p.entity.pos.load()).collect();
        let mut reader = TCPNetworkDecoder::new(BufReader::new(&mut fixture.remote));
        observe(&mut reader, &player).await;
        let mut sounds = 0;
        let mut collected = 0;
        let total = if first_fight { 12000 } else { 500 };
        for tick in 1..=200 {
            live.tick(live.as_ref(), &fixture.server);
            assert_eq!(*d.dragon_death_time.lock().unwrap(), tick);
            if [1, 20, 155, 180, 200].contains(&tick) {
                let expected_y = initial.y + f64::from(tick) * 0.10000000149011612;
                assert!((live.get_entity().pos.load().y - expected_y).abs() < 1e-9);
                for (part, start) in d.parts().iter().zip(&part_initial) {
                    assert!(
                        (part.entity.pos.load().y
                            - (start.y + f64::from(tick) * 0.10000000149011612))
                            .abs()
                            < 1e-9
                    );
                    assert!(!part.entity.is_removed());
                }
                sounds += death_world_events(&observe(&mut reader, &player).await);
                assert_eq!(live.get_entity().is_removed(), tick == 200);
                if tick <= 150 {
                    assert!(
                        !world
                            .entities
                            .load()
                            .iter()
                            .any(|e| e.get_entity().entity_type == &EntityType::EXPERIENCE_ORB)
                    );
                }
                collected += collect_xp(&player, &fixture.server, &world);
                let expected = if !drops || tick < 155 {
                    0
                } else if tick == 155 {
                    total * 8 / 100
                } else if tick == 180 {
                    total * 48 / 100
                } else {
                    total
                };
                assert_eq!(
                    collected, expected,
                    "actual XP pickups at death tick {tick}"
                );
            }
        }
        assert_eq!(sounds, usize::from(!silent));
        assert!(live.get_entity().removal_reason.load() == Some(RemovalReason::Killed));
        assert!(
            world
                .entity_tracker
                .get_tracked_entity(live.get_entity().entity_id)
                .is_none()
        );
        assert!(
            d.parts()
                .iter()
                .all(|p| world.get_entity_or_part(p.entity.entity_id).is_none())
        );
        assert_eq!(
            hooks
                .deaths
                .lock()
                .unwrap()
                .iter()
                .filter(|id| **id == live.get_entity().entity_id)
                .count(),
            1
        );
        assert_eq!(hooks.game_deaths.lock().unwrap().len(), 2);
        live.tick(live.as_ref(), &fixture.server);
        assert_eq!(*d.dragon_death_time.lock().unwrap(), 200);
        assert_eq!(hooks.game_deaths.lock().unwrap().len(), 2);
        if let Some(fight) = fight {
            assert!(fight.lock().unwrap().dragon_killed);
        }
        drop(reader);
        assert_eq!(collect_xp(&player, &fixture.server, &world), 0);
        assert_eq!(collected, if drops { total } else { 0 });
        close_fixture(&fixture).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ordinary_mob_twenty_tick_death_and_registered_player_respawn_second_death() {
    let mut fixture = fixture().await;
    let world = fixture.player.world();
    let player = fixture.player.clone();
    let hooks = Events::register(&fixture.server);
    world.level_info.rcu(|current| {
        let mut next = (**current).clone();
        next.spawn_x = 8;
        next.spawn_y = 64;
        next.spawn_z = 8;
        next.game_rules.keep_inventory = true;
        next
    });
    let cow = crate::entity::r#type::from_type(
        &EntityType::COW,
        Vector3::new(8.0, 65.0, 8.0),
        &world,
        Uuid::new_v4(),
    );
    assert!(world.spawn_entity(cow.clone()));
    cow.get_mob().unwrap().get_mob_entity().set_no_ai(true);
    let tracked = world
        .entity_tracker
        .get_tracked_entity(cow.get_entity().entity_id)
        .unwrap();
    tracked.seen_by.insert(player.gameprofile.id);
    tracked.add_pairing(&player);
    let mut reader = TCPNetworkDecoder::new(BufReader::new(&mut fixture.remote));
    observe(&mut reader, &player).await;
    assert!(hit(cow.as_ref(), &player, 100.0, DamageType::GENERIC_KILL));
    assert!(
        cow.get_living_entity()
            .unwrap()
            .dead
            .load(Ordering::Relaxed)
    );
    let packets = observe(&mut reader, &player).await;
    assert_eq!(
        packets
            .iter()
            .filter(|p| p.id == CEntityStatus::to_id(CURRENT_MC_VERSION)
                && p.payload.as_ref()
                    == [cow.get_entity().entity_id.to_be_bytes().as_slice(), &[3]].concat())
            .count(),
        1
    );
    for _ in 0..19 {
        cow.tick(cow.as_ref(), &fixture.server);
    }
    assert!(!cow.get_entity().is_removed());
    cow.tick(cow.as_ref(), &fixture.server);
    assert!(cow.get_entity().is_removed());
    assert!(cow.get_entity().removal_reason.load() == Some(RemovalReason::Discarded));
    assert!(world.get_entity_by_id(cow.get_entity().entity_id).is_none());
    assert_eq!(
        hooks
            .deaths
            .lock()
            .unwrap()
            .iter()
            .filter(|id| **id == cow.get_entity().entity_id)
            .count(),
        1
    );
    drop(reader);
    for death in 1..=2 {
        assert!(player.damage_with_context(
            player.as_ref(),
            100.0,
            DamageType::GENERIC_KILL,
            None,
            None,
            None
        ));
        assert!(player.living_entity.dead.load(Ordering::Relaxed));
        for _ in 0..20 {
            player.tick(&fixture.server);
        }
        assert!(world.get_player_by_id(player.entity_id()).is_some());
        assert_eq!(
            hooks
                .deaths
                .lock()
                .unwrap()
                .iter()
                .filter(|id| **id == player.entity_id())
                .count(),
            death
        );
        if death == 1 {
            assert_eq!(
                Box::pin(tokio::time::timeout(
                    Duration::from_secs(10),
                    drive_packet_future(&fixture.server, world.respawn_player(&player, false))
                ))
                .await
                .unwrap(),
                crate::world::RespawnOutcome::Committed
            );
            assert!(player.living_entity.health.load() > 0.0);
            assert!(!player.living_entity.dead.load(Ordering::Relaxed));
        }
    }
    close_fixture(&fixture).await;
}

struct ReplaceOnDamage {
    old: Arc<dyn EntityBase>,
    replacement: Arc<dyn EntityBase>,
    once: AtomicBool,
}
impl EventHandler<EntityDamageEvent> for ReplaceOnDamage {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut EntityDamageEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.entity_id == self.old.get_entity().entity_id
                && !self.once.swap(true, Ordering::SeqCst)
            {
                let world = self.old.get_entity().world.load_full();
                world.remove_entity(self.old.as_ref());
                assert!(world.spawn_entity(self.replacement.clone()));
            }
        })
    }
}

struct ReplaceOnXp {
    old: Arc<dyn EntityBase>,
    replacement: Arc<dyn EntityBase>,
    once: AtomicBool,
}
impl EventHandler<EntitySpawnEvent> for ReplaceOnXp {
    fn handle_blocking<'a>(
        &'a self,
        _: &'a Arc<Server>,
        event: &'a mut EntitySpawnEvent,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if event.entity_type == EntityType::EXPERIENCE_ORB.id.to_string()
                && !self.once.swap(true, Ordering::SeqCst)
            {
                event.world.remove_entity(self.old.as_ref());
                assert!(event.world.spawn_entity(self.replacement.clone()));
            }
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canceled_removed_and_reentrant_same_uuid_parts_never_retarget_replacement() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let hooks = Events::register(&fixture.server);
    let canceled = factory(&world, Uuid::new_v4());
    hooks
        .cancel_spawn
        .store(canceled.get_entity().entity_id, Ordering::Relaxed);
    assert!(!world.spawn_entity(canceled.clone()));
    let canceled_part = dragon(&canceled).parts()[0].clone();
    assert!(canceled_part.current_parent().is_none());
    assert!(
        world
            .get_entity_or_part(canceled_part.entity.entity_id)
            .is_none()
    );
    assert!(!hit(
        canceled_part.as_ref(),
        &fixture.player,
        10.0,
        DamageType::PLAYER_ATTACK
    ));
    let old = spawn(&world);
    let replacement = factory(&world, old.get_entity().entity_uuid);
    let retained = dragon(&old).parts()[0].clone();
    fixture
        .server
        .plugin_manager
        .register::<EntityDamageEvent, _>(
            Arc::new(ReplaceOnDamage {
                old: old.clone(),
                replacement: replacement.clone(),
                once: AtomicBool::new(false),
            }),
            EventPriority::Normal,
            true,
        );
    assert!(hit(
        retained.as_ref(),
        &fixture.player,
        10.0,
        DamageType::PLAYER_ATTACK
    ));
    // The original's post-callback health timing is deliberately not asserted.
    assert_eq!(
        replacement.get_living_entity().unwrap().health.load(),
        200.0
    );
    assert!(retained.current_parent().is_none());
    assert!(!hit(
        retained.as_ref(),
        &fixture.player,
        10.0,
        DamageType::PLAYER_ATTACK
    ));
    assert!(
        world
            .get_entity_or_part(retained.entity.entity_id)
            .is_none()
    );
    assert!(
        world
            .entity_tracker
            .get_tracked_entity(replacement.get_entity().entity_id)
            .is_some()
    );
    world.remove_entity_with_reason(old.as_ref(), RemovalReason::Killed);
    assert!(old.get_entity().removal_reason.load() == Some(RemovalReason::Discarded));
    assert!(std::ptr::eq(
        world
            .get_entity_by_uuid(old.get_entity().entity_uuid)
            .unwrap()
            .get_entity(),
        replacement.get_entity()
    ));
    close_fixture(&fixture).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn terminal_xp_callback_replacement_keeps_new_parent_tracker_and_fight_alive() {
    let fixture = fixture().await;
    let world = fixture.player.world();
    let old = spawn(&world);
    let replacement = factory(&world, old.get_entity().entity_uuid);
    let d = dragon(&old);
    let counts = format!("{:?}", world.spawn_state.load().mob_category_counts);
    let fight = Arc::new(Mutex::new(DragonFight::new()));
    fight.lock().unwrap().dragon_uuid = Some(old.get_entity().entity_uuid);
    d.attach_fight(&fight);
    d.mob_entity.living_entity.set_health(0.0);
    *d.dragon_death_time.lock().unwrap() = 199;
    fixture
        .server
        .plugin_manager
        .register::<EntitySpawnEvent, _>(
            Arc::new(ReplaceOnXp {
                old: old.clone(),
                replacement: replacement.clone(),
                once: AtomicBool::new(false),
            }),
            EventPriority::Normal,
            true,
        );
    old.tick(old.as_ref(), &fixture.server);
    assert_eq!(*d.dragon_death_time.lock().unwrap(), 200);
    assert!(old.get_entity().removal_reason.load() == Some(RemovalReason::Discarded));
    assert!(!fight.lock().unwrap().dragon_killed);
    assert!(world.is_current_entity(replacement.as_ref()));
    assert!(
        world
            .entity_tracker
            .get_tracked_entity(replacement.get_entity().entity_id)
            .is_some()
    );
    assert_eq!(
        replacement.get_living_entity().unwrap().health.load(),
        200.0
    );
    assert_eq!(*dragon(&replacement).dragon_death_time.lock().unwrap(), 0);
    assert_eq!(
        format!("{:?}", world.spawn_state.load().mob_category_counts),
        counts
    );
    for part in dragon(&replacement).parts() {
        assert!(part.current_parent().is_some());
    }
    close_fixture(&fixture).await;
}
