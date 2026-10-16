use crate::world::World;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_world::{cylindrical_chunk_iterator::Cylindrical, level::Level};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use uuid::Uuid;

#[derive(Clone)]
struct WatchTarget {
    world: Weak<World>,
    world_id: Uuid,
    view: Cylindrical,
}

#[derive(Default)]
pub struct RequestedWatches {
    revision: u64,
    target: Option<WatchTarget>,
    paused: bool,
    closed: bool,
    blocked_world: Option<Uuid>,
    retired_tickets: Vec<HeldChunkTickets>,
}

impl RequestedWatches {
    pub(crate) fn accepts(&self, world_id: Uuid) -> bool {
        !self.closed && !self.paused && self.blocked_world != Some(world_id)
    }

    pub(crate) fn view(&self, world_id: Uuid) -> Option<Cylindrical> {
        self.target
            .as_ref()
            .filter(|target| target.world_id == world_id)
            .map(|target| target.view)
    }

    pub(crate) fn retire(&mut self, tickets: HeldChunkTickets) {
        self.retired_tickets.push(tickets);
    }

    /// Publish the new ticket tuple before retiring its predecessor. The watch
    /// coordinator releases the predecessor after preserving departing live data.
    pub(crate) fn update_tickets(
        &mut self,
        held: &mut Option<HeldChunkTickets>,
        level: &Arc<Level>,
        center: Vector2<i32>,
        view: Option<i8>,
        simulation: Option<i8>,
    ) -> bool {
        if held
            .as_ref()
            .is_some_and(|held| held.matches(level, center, view, simulation))
        {
            return false;
        }
        let mut loading = level
            .chunk_loading
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(view) = view {
            loading.add_ticket(center, view);
        }
        if let Some(simulation) = simulation {
            loading.add_ticket(center, simulation);
        }
        if let Some(old) = held.replace(HeldChunkTickets::new(level, center, view, simulation)) {
            self.retire(old);
        }
        loading.send_change();
        true
    }

    pub(crate) fn publish(&mut self, world: &Arc<World>, view: Cylindrical) -> u64 {
        self.revision = self.revision.wrapping_add(1);
        self.blocked_world = None;
        self.target = Some(WatchTarget {
            world: Arc::downgrade(world),
            world_id: world.uuid,
            view,
        });
        self.revision
    }
}

/// Requested views may change before their tasks run. Only `applied` owns Level
/// watcher references; serializing its transitions makes stale tasks harmless.
#[derive(Default)]
pub struct PlayerChunkWatches {
    requested: Mutex<RequestedWatches>,
    applied: tokio::sync::Mutex<Option<WatchTarget>>,
}

impl PlayerChunkWatches {
    /// Held only during synchronous view/ticket/packet publication, never an await.
    pub(crate) fn request(&self) -> MutexGuard<'_, RequestedWatches> {
        self.requested
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn requests_world(&self, world_id: Uuid) -> bool {
        let requested = self.request();
        !requested.closed
            && !requested.paused
            && requested
                .target
                .as_ref()
                .is_some_and(|target| target.world_id == world_id)
    }

    pub(crate) async fn apply(&self, revision: u64) -> Option<(Arc<World>, Vec<Vector2<i32>>)> {
        let mut applied = self.applied.lock().await;
        let (target, world, retired) = {
            let mut requested = self.request();
            if requested.closed || requested.paused || requested.revision != revision {
                return None;
            }
            let target = requested.target.clone()?;
            let world = target.world.upgrade()?;
            (
                target,
                world,
                std::mem::take(&mut requested.retired_tickets),
            )
        };
        if let Some(old) = applied.take_if(|old| old.world_id != target.world_id)
            && let Some(old_world) = old.world.upgrade()
        {
            let removed: Vec<_> = old.view.all_chunks_within().collect();
            old_world.update_chunk_watches(&[], &removed).await;
        }
        let (added, removed): (Vec<_>, Vec<_>) = if let Some(old) = applied.as_ref() {
            let (added, removed) = Cylindrical::changed_chunks(old.view, target.view);
            (added.collect(), removed.collect())
        } else {
            (target.view.all_chunks_within().collect(), Vec::new())
        };
        world.update_chunk_watches(&added, &removed).await;
        *applied = Some(target);
        // Old generation tickets pin live block data until its unload snapshot exists.
        for tickets in retired {
            let _ = tickets.release();
        }
        Some((world, added))
    }

    /// Invalidate queued work before waiting for an in-flight application. A
    /// transfer blocks the departed world until a destination view is published;
    /// disconnect closes this owner permanently and is safe to repeat.
    pub(crate) async fn clear(&self, world_id: Uuid, disconnect: bool) -> Vec<Vector2<i32>> {
        let revision = {
            let mut requested = self.request();
            requested.revision = requested.revision.wrapping_add(1);
            requested.target = None;
            requested.paused = true;
            requested.closed |= disconnect;
            requested.blocked_world = Some(world_id);
            requested.revision
        };
        let mut applied = self.applied.lock().await;
        let removed = if let Some(old) = applied.take() {
            let removed: Vec<_> = old.view.all_chunks_within().collect();
            if let Some(world) = old.world.upgrade() {
                world.update_chunk_watches(&[], &removed).await;
            }
            removed
        } else {
            Vec::new()
        };
        let mut requested = self.request();
        for tickets in std::mem::take(&mut requested.retired_tickets) {
            let _ = tickets.release();
        }
        if requested.revision == revision && !requested.closed {
            requested.paused = false;
        }
        removed
    }
}

/// Tickets carry the level and center that acquired them, rather than deriving
/// ownership from the player's current position after movement or transfer.
pub struct HeldChunkTickets {
    level: Weak<Level>,
    center: Vector2<i32>,
    view: Option<i8>,
    simulation: Option<i8>,
}

impl HeldChunkTickets {
    fn matches(
        &self,
        level: &Arc<Level>,
        center: Vector2<i32>,
        view: Option<i8>,
        simulation: Option<i8>,
    ) -> bool {
        self.level.ptr_eq(&Arc::downgrade(level))
            && self.center == center
            && self.view == view
            && self.simulation == simulation
    }

    pub(crate) fn new(
        level: &Arc<Level>,
        center: Vector2<i32>,
        view: Option<i8>,
        simulation: Option<i8>,
    ) -> Self {
        Self {
            level: Arc::downgrade(level),
            center,
            view,
            simulation,
        }
    }

    pub(crate) fn release(self) -> Option<Arc<Level>> {
        if let Some(level) = self.level.upgrade() {
            let mut loading = level
                .chunk_loading
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(view) = self.view {
                loading.remove_ticket(self.center, view);
            }
            if let Some(simulation) = self.simulation {
                loading.remove_ticket(self.center, simulation);
            }
            loading.send_change();
            drop(loading);
            return Some(level);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use pumpkin_config::world::LevelConfig;
    use pumpkin_data::dimension::Dimension;
    use pumpkin_util::world_seed::Seed;
    use pumpkin_world::world_info::LevelData;
    use std::num::NonZero;

    async fn world() -> (tempfile::TempDir, Arc<World>) {
        let folder = tempfile::tempdir().unwrap();
        let level = Level::from_root_folder(
            &LevelConfig::default(),
            folder.path().into(),
            0,
            Dimension::OVERWORLD,
        );
        level.shutdown().await.unwrap();
        let world = Arc::new(World::load(
            level,
            Arc::new(ArcSwap::from_pointee(LevelData::default(Seed(0)))),
            Dimension::OVERWORLD,
            crate::block::registry::default_registry(),
            Weak::new(),
            Arc::default(),
        ));
        (folder, world)
    }

    fn view(x: i32) -> Cylindrical {
        // Distance 1 is the empty/uninitialized sentinel, not a real player view.
        let view = Cylindrical::new(Vector2::new(x, 0), NonZero::new(2).unwrap());
        assert!(view.is_within_distance(x, 0));
        assert!(
            view.all_chunks_within()
                .any(|position| position == view.center)
        );
        view
    }

    fn publish(owner: &PlayerChunkWatches, world: &Arc<World>, view: Cylindrical) -> u64 {
        let mut requested = owner.request();
        assert!(requested.accepts(world.uuid));
        requested.publish(world, view)
    }

    fn assert_watched(world: &World, view: Cylindrical, watched: bool) {
        assert!(
            view.all_chunks_within().len() > 0,
            "watcher assertions require a real nonempty view"
        );
        for position in view.all_chunks_within() {
            assert_eq!(
                world.level.is_chunk_watched(&position),
                watched,
                "{position:?}"
            );
        }
    }

    #[tokio::test]
    async fn chunk_watches_delayed_add_after_disconnect_cannot_steal_another_players_refs() {
        let (_folder, world) = world().await;
        let first = PlayerChunkWatches::default();
        let second = Arc::new(PlayerChunkWatches::default());
        let section = view(0);
        let first_revision = publish(&first, &world, section);
        first.apply(first_revision).await.unwrap();
        let stale_revision = publish(&second, &world, section);
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let delayed = {
            let second = second.clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                second.apply(stale_revision).await.is_none()
            })
        };
        assert!(second.clear(world.uuid, true).await.is_empty());
        assert_watched(&world, section, true);
        barrier.wait().await;
        assert!(delayed.await.unwrap());
        assert!(second.clear(world.uuid, true).await.is_empty());
        assert!(!second.request().accepts(world.uuid));
        assert_watched(&world, section, true);
        first.clear(world.uuid, true).await;
        assert_watched(&world, section, false);
    }

    #[tokio::test]
    async fn chunk_watches_superseded_moves_and_world_transfer_release_only_applied_refs() {
        let (_source_folder, source) = world().await;
        let (_destination_folder, destination) = world().await;
        let first = PlayerChunkWatches::default();
        let moving = PlayerChunkWatches::default();
        let section = view(0);
        first
            .apply(publish(&first, &source, section))
            .await
            .unwrap();
        let stale = publish(&moving, &source, view(10));
        let latest = publish(&moving, &source, section);
        let (_, added) = moving.apply(latest).await.unwrap();
        assert_eq!(added.len(), section.all_chunks_within().count());
        assert!(moving.apply(stale).await.is_none());
        assert_watched(&source, view(10), false);
        moving.clear(source.uuid, false).await;
        assert_watched(&source, section, true);
        assert!(!moving.request().accepts(source.uuid));
        let destination_revision = publish(&moving, &destination, section);
        moving.apply(destination_revision).await.unwrap();
        assert_watched(&destination, section, true);
        assert!(moving.apply(stale).await.is_none());
        moving.clear(destination.uuid, true).await;
        assert_watched(&destination, section, false);
        assert_watched(&source, section, true);
        first.clear(source.uuid, true).await;
        assert_watched(&source, section, false);
    }

    #[tokio::test]
    async fn chunk_watches_queued_transfer_then_current_world_move_preserves_positions_and_refs() {
        use crate::entity::Entity;
        use pumpkin_data::entity::EntityType;
        use pumpkin_util::math::vector3::Vector3;

        let (_source_folder, source) = world().await;
        let (_destination_folder, destination) = world().await;
        let source_view = view(0);
        let first_view = view(20);
        let second_view = view(40);
        let first_position = Vector3::new(320.0, 100.0, 0.0);
        let second_position = Vector3::new(640.0, 110.0, 0.0);
        let owner = Arc::new(PlayerChunkWatches::default());
        let neighbor = PlayerChunkWatches::default();
        owner
            .apply(publish(&owner, &source, source_view))
            .await
            .unwrap();
        neighbor
            .apply(publish(&neighbor, &source, source_view))
            .await
            .unwrap();
        let entity = Arc::new(Entity::from_uuid(
            Uuid::new_v4(),
            source.clone(),
            Vector3::new(0.0, 100.0, 0.0),
            &EntityType::PLAYER,
        ));
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let (arrived, first_arrived) = tokio::sync::oneshot::channel();
        let (finish, finish_first) = tokio::sync::oneshot::channel();
        let first = {
            let gate = gate.clone();
            let entity = entity.clone();
            let owner = owner.clone();
            let destination = destination.clone();
            tokio::spawn(async move {
                let _transfer = gate.lock().await;
                let previous = entity.world.load_full();
                assert!(!Arc::ptr_eq(&previous, &destination));
                owner.clear(previous.uuid, false).await;
                entity.set_world(destination.clone());
                entity.set_pos(first_position);
                owner
                    .apply(publish(&owner, &destination, first_view))
                    .await
                    .unwrap();
                arrived.send(()).unwrap();
                finish_first.await.unwrap();
            })
        };
        first_arrived.await.unwrap();
        assert_eq!(entity.pos.load(), first_position);
        assert_watched(&destination, first_view, true);
        assert_watched(&source, source_view, true);
        let (queued, second_queued) = tokio::sync::oneshot::channel();
        let second = {
            let gate = gate.clone();
            let entity = entity.clone();
            let owner = owner.clone();
            let destination = destination.clone();
            tokio::spawn(async move {
                queued.send(()).unwrap();
                let _transfer = gate.lock().await;
                // Read the current world after the first transfer completes.
                let current = entity.world.load_full();
                assert!(Arc::ptr_eq(&current, &destination));
                // A same-world teleport moves the view without clearing its owner.
                entity.set_pos(second_position);
                assert!(owner.request().accepts(current.uuid));
                owner
                    .apply(publish(&owner, &current, second_view))
                    .await
                    .unwrap();
            })
        };
        second_queued.await.unwrap();
        assert_eq!(entity.pos.load(), first_position);
        finish.send(()).unwrap();
        first.await.unwrap();
        second.await.unwrap();
        assert_eq!(entity.pos.load(), second_position);
        assert_eq!(entity.chunk_pos.load(), second_view.center);
        assert_watched(&destination, first_view, false);
        assert_watched(&destination, second_view, true);
        assert_watched(&source, source_view, true);
        owner.clear(destination.uuid, true).await;
        assert_watched(&destination, second_view, false);
        neighbor.clear(source.uuid, true).await;
        assert_watched(&source, source_view, false);
    }

    #[tokio::test]
    async fn chunk_watches_movement_pins_old_terrain_until_live_chest_snapshot() {
        use crate::block::entities::{BlockEntity, chest::ChestBlockEntity};
        use pumpkin_data::{Block, item::Item, item_stack::ItemStack};
        use pumpkin_inventory::Inventory;
        use pumpkin_util::math::position::BlockPos;
        use pumpkin_world::chunk::{ChunkData, format::anvil::SingleChunkDataSerializer};
        use pumpkin_world::chunk_system::ChunkLoading;

        let (_folder, world) = world().await;
        let owner = Arc::new(PlayerChunkWatches::default());
        let original = Vector2::new(0, 0);
        let destination = Vector2::new(10, 0);
        let position = BlockPos::new(1, 100, 1);
        let chunk = ChunkData::empty_sync(original.x, original.y);
        chunk.set_block_absolute_y(
            (position.0.x & 15) as usize,
            position.0.y,
            (position.0.z & 15) as usize,
            Block::CHEST.default_state.id,
        );
        world.level.loaded_chunks.insert(original, chunk.clone());
        let chest = Arc::new(ChestBlockEntity::new(position));
        world
            .block_entities
            .entry(original)
            .or_default()
            .insert(position, chest.clone());
        let first = publish(&owner, &world, view(original.x));
        owner.apply(first).await.unwrap();
        assert!(world.level.is_chunk_watched(&original));
        chest.set_stack(0, ItemStack::new(5, &Item::DIAMOND));
        let level = ChunkLoading::FULL_CHUNK_LEVEL;
        {
            let mut loading = world.level.chunk_loading.lock().unwrap();
            loading.add_ticket(original, level);
            loading.add_ticket(destination, level);
        }
        let revision = {
            let mut requested = owner.request();
            requested.retire(HeldChunkTickets::new(
                &world.level,
                original,
                Some(level),
                None,
            ));
            requested.publish(&world, view(destination.x))
        };
        // Hold the real coordinator gate while a movement task is queued.
        let held = owner.applied.lock().await;
        let (started, ready) = tokio::sync::oneshot::channel();
        let moving = {
            let owner = owner.clone();
            tokio::spawn(async move {
                started.send(()).unwrap();
                owner.apply(revision).await.is_some()
            })
        };
        ready.await.unwrap();
        {
            let loading = world.level.chunk_loading.lock().unwrap();
            assert!(loading.ticket.contains_key(&original));
            // This is the scheduler's live-ticket check before unpublishing terrain.
            assert!(
                loading
                    .pos_level
                    .get(&original)
                    .is_some_and(|value| *value <= level)
            );
        }
        assert!(chunk.pending_block_entities.lock().unwrap().is_empty());
        assert!(world.block_entities.contains_key(&original));
        drop(chest);
        drop(held);
        assert!(moving.await.unwrap());
        assert!(!world.level.is_chunk_watched(&original));
        assert!(world.level.is_chunk_watched(&destination));
        assert!(
            !world
                .level
                .chunk_loading
                .lock()
                .unwrap()
                .ticket
                .contains_key(&original)
        );
        assert!(!world.block_entities.contains_key(&original));
        let reloaded = ChunkData::from_bytes(&chunk.to_bytes().unwrap(), original).unwrap();
        assert_eq!(
            reloaded.section.get_block_absolute_y(
                (position.0.x & 15) as usize,
                position.0.y,
                (position.0.z & 15) as usize,
            ),
            Some(Block::CHEST.default_state.id),
        );
        let saved = reloaded
            .pending_block_entities
            .lock()
            .unwrap()
            .get(&position)
            .unwrap()
            .clone();
        let restored = ChestBlockEntity::from_nbt(&saved, position);
        assert_eq!(restored.get_stack(0).item_count, 5);
        assert_eq!(restored.get_stack(0).get_item().id, Item::DIAMOND.id);
        owner.clear(world.uuid, true).await;
    }

    #[tokio::test]
    async fn chunk_watches_ticket_release_uses_original_level_and_center() {
        let (_source_folder, source) = world().await;
        let (_destination_folder, destination) = world().await;
        let original = Vector2::new(-9, 3);
        let current = Vector2::new(14, -7);
        let level = pumpkin_world::chunk_system::ChunkLoading::FULL_CHUNK_LEVEL;
        {
            let mut loading = source.level.chunk_loading.lock().unwrap();
            loading.add_ticket(original, level);
            loading.add_ticket(current, level);
        }
        destination
            .level
            .chunk_loading
            .lock()
            .unwrap()
            .add_ticket(current, level);
        let tickets = HeldChunkTickets::new(&source.level, original, Some(level), None);
        tickets.release().unwrap();
        let source_loading = source.level.chunk_loading.lock().unwrap();
        assert!(!source_loading.ticket.contains_key(&original));
        assert_eq!(source_loading.ticket.get(&current), Some(&vec![level]));
        assert_eq!(
            destination
                .level
                .chunk_loading
                .lock()
                .unwrap()
                .ticket
                .get(&current),
            Some(&vec![level])
        );
    }

    #[tokio::test]
    async fn chunk_watches_same_view_simulation_and_generation_changes_retire_exact_tickets() {
        use pumpkin_world::chunk_system::ChunkLoading;

        let (_folder, world) = world().await;
        let owner = PlayerChunkWatches::default();
        let neighbor = PlayerChunkWatches::default();
        let section = view(0);
        neighbor
            .apply(publish(&neighbor, &world, section))
            .await
            .unwrap();
        let view_level = ChunkLoading::get_level_from_view_distance(3);
        let old_simulation = ChunkLoading::get_level_from_simulation_distance(4);
        let new_simulation = ChunkLoading::get_level_from_simulation_distance(7);
        assert_ne!(old_simulation, new_simulation);
        let mut held = None;
        let first = {
            let mut requested = owner.request();
            assert!(requested.update_tickets(
                &mut held,
                &world.level,
                section.center,
                Some(view_level),
                Some(old_simulation),
            ));
            requested.publish(&world, section)
        };
        owner.apply(first).await.unwrap();
        let changed_simulation = {
            let mut requested = owner.request();
            assert!(requested.update_tickets(
                &mut held,
                &world.level,
                section.center,
                Some(view_level),
                Some(new_simulation),
            ));
            requested.publish(&world, section)
        };
        assert!(
            world.level.chunk_loading.lock().unwrap().ticket[&section.center]
                .contains(&old_simulation)
        );
        let (_, added) = owner.apply(changed_simulation).await.unwrap();
        assert!(
            added.is_empty(),
            "policy changes preserve the same watch references"
        );
        {
            let loading = world.level.chunk_loading.lock().unwrap();
            let tickets = &loading.ticket[&section.center];
            assert_eq!(tickets.len(), 2);
            assert!(tickets.contains(&view_level));
            assert!(tickets.contains(&new_simulation));
        }
        let spectator = {
            let mut requested = owner.request();
            assert!(requested.update_tickets(&mut held, &world.level, section.center, None, None,));
            requested.publish(&world, section)
        };
        assert!(!world.level.chunk_loading.lock().unwrap().ticket.is_empty());
        assert!(owner.apply(spectator).await.unwrap().1.is_empty());
        assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
        assert_watched(&world, section, true);
        assert!(!owner.request().update_tickets(
            &mut held,
            &world.level,
            section.center,
            None,
            None,
        ));
        let survival = {
            let mut requested = owner.request();
            assert!(requested.update_tickets(
                &mut held,
                &world.level,
                section.center,
                Some(view_level),
                Some(new_simulation),
            ));
            requested.publish(&world, section)
        };
        assert!(owner.apply(survival).await.unwrap().1.is_empty());
        assert_eq!(
            world.level.chunk_loading.lock().unwrap().ticket[&section.center].len(),
            2
        );
        owner.clear(world.uuid, true).await;
        held.take().unwrap().release().unwrap();
        assert!(world.level.chunk_loading.lock().unwrap().ticket.is_empty());
        assert_watched(&world, section, true);
        neighbor.clear(world.uuid, true).await;
        assert_watched(&world, section, false);
    }
}
