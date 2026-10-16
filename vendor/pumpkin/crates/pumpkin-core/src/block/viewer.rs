use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use pumpkin_inventory::{
    Inventory, generic_container_screen_handler::GenericContainerScreenHandler,
};
use pumpkin_util::math::position::BlockPos;

use crate::{block::entities::BlockEntity, world::World};

pub use pumpkin_inventory::ViewerCountTracker;

/// A menu's viewer callbacks precede publication of `current_screen_handler`.
/// Nested/overlapping opens and closes must keep recounts deferred until every
/// transition has published or returned. No mutex is held across callbacks.
#[derive(Default)]
pub(crate) struct MenuPublication {
    depth: AtomicUsize,
}

impl MenuPublication {
    pub(crate) fn begin(&self) -> MenuPublicationGuard<'_> {
        self.depth.fetch_add(1, Ordering::AcqRel);
        MenuPublicationGuard(self)
    }

    pub(crate) fn is_active(&self) -> bool {
        self.depth.load(Ordering::Acquire) != 0
    }
}

#[must_use = "keep the guard alive until the menu transition is published"]
pub(crate) struct MenuPublicationGuard<'a>(&'a MenuPublication);

impl Drop for MenuPublicationGuard<'_> {
    fn drop(&mut self) {
        self.0.depth.fetch_sub(1, Ordering::AcqRel);
    }
}

struct ContainerViewers {
    current: u16,
    published: u16,
    ticks_until_recheck: u8,
    // A retained allocation cannot ABA, unlike a wrapping revision integer.
    revision: Arc<()>,
    pending: VecDeque<ViewerUpdate>,
    publishing: bool,
}

struct ViewerUpdate {
    world: Arc<World>,
    old: u16,
    current: u16,
}

/// Release ownership on callback unwind so a later transition can drain the
/// remaining queue. A partially published update is not replayed.
struct ViewerDrainGuard<'a> {
    state: Option<&'a Mutex<ContainerViewers>>,
}

impl Drop for ViewerDrainGuard<'_> {
    fn drop(&mut self) {
        if let Some(state) = self.state {
            state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .publishing = false;
        }
    }
}

impl ContainerViewers {
    /// Claim the synchronous drain while holding only the counter lock. Nested
    /// or concurrent callbacks append in count order; they never wait on a
    /// publisher which might itself need their menu/world locks.
    fn enqueue(&mut self, world: &Arc<World>, recounted: bool) -> bool {
        if recounted || self.published != self.current {
            self.pending.push_back(ViewerUpdate {
                world: world.clone(),
                old: self.published,
                current: self.current,
            });
            self.published = self.current;
        }
        if self.publishing || self.pending.is_empty() {
            return false;
        }
        self.publishing = true;
        true
    }
}

/// Chest/trapped-chest/barrel transitions and recounts. Other trackers retain
/// their existing behavior until their own menu contracts are integrated.
pub(crate) struct ContainerViewerCount {
    state: Mutex<ContainerViewers>,
    #[cfg(test)]
    before_recount: Mutex<Option<Box<dyn FnOnce() + Send + Sync>>>,
}

impl ContainerViewerCount {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(ContainerViewers {
                current: 0,
                published: 0,
                ticks_until_recheck: 0,
                revision: Arc::new(()),
                pending: VecDeque::new(),
                publishing: false,
            }),
            #[cfg(test)]
            before_recount: Mutex::new(None),
        }
    }

    pub(crate) fn get_viewer_count(&self) -> u16 {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .current
    }

    pub(crate) fn open_container(&self) {
        self.change(true, None);
    }

    pub(crate) fn close_container(&self) {
        self.change(false, None);
    }

    fn change(&self, opening: bool, world: Option<&Arc<World>>) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if opening {
            if state.current == 0 && state.ticks_until_recheck == 0 {
                state.ticks_until_recheck = 5;
            }
            state.current = state.current.saturating_add(1);
        } else {
            // A stale retained menu may close after a recount already removed it.
            state.current = state.current.saturating_sub(1);
        }
        state.revision = Arc::new(());
        world.is_some_and(|world| state.enqueue(world, false))
    }

    /// The callback claiming the drain publishes without a BE tick. Nested or
    /// concurrent callers can return before its drain reaches their updates.
    pub(crate) fn change_in_world<T>(&self, entity: &T, world: &Arc<World>, opening: bool)
    where
        T: BlockEntity + ViewerCountListener + 'static,
    {
        if self.change(opening, Some(world)) {
            self.drain(entity, &entity.get_position());
        }
    }

    fn drain<T>(&self, entity: &T, position: &BlockPos)
    where
        T: BlockEntity + ViewerCountListener + 'static,
    {
        let mut release = ViewerDrainGuard {
            state: Some(&self.state),
        };
        loop {
            let update = {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(update) = state.pending.pop_front() else {
                    state.publishing = false;
                    // Empty check, release and disarm share the producer lock:
                    // no lost wakeup, and Drop cannot clear a new owner's flag.
                    release.state = None;
                    return;
                };
                update
            };
            let ViewerUpdate {
                world,
                old,
                current,
            } = update;
            // A retained menu still owns its old entity, never a replacement at
            // the same coordinates. Like Java's removed check, suppress external
            // effects once that entity is detached. This lookup only validates
            // identity; it is never used to choose the counter to mutate.
            let is_attached = || {
                world.get_block_entity(position).is_some_and(|live| {
                    live.as_any()
                        .downcast_ref::<T>()
                        .is_some_and(|live| std::ptr::eq(live, entity))
                })
            };
            if !is_attached() {
                continue;
            }
            // World, sound, redstone and menu callbacks are outside the viewer
            // and ChestState locks. Reentry appends to this drain.
            if old != current {
                match (old, current) {
                    (0, _) => entity.on_container_open(&world, position),
                    (_, 0) => entity.on_container_close(&world, position),
                    _ => {}
                }
            }
            if is_attached() {
                entity.on_viewer_count_update(&world, position, old, current);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn before_recount(&self, hook: impl FnOnce() + Send + Sync + 'static) {
        *self.before_recount.lock().unwrap() = Some(Box::new(hook));
    }

    pub(crate) fn tick<T>(&self, entity: &T, world: &Arc<World>, position: &BlockPos)
    where
        T: BlockEntity + Inventory + ViewerCountListener + 'static,
    {
        // Snapshot BEFORE any player/guard/menu access. Producers enter the
        // publication guard before on_open/on_close changes this revision.
        let revision = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.ticks_until_recheck == 0 {
                None
            } else {
                state.ticks_until_recheck = state.ticks_until_recheck.saturating_sub(1);
                (state.ticks_until_recheck == 0).then(|| {
                    state.ticks_until_recheck = 5;
                    state.revision.clone()
                })
            }
        };
        let recount = revision.and_then(|revision| {
            #[cfg(test)]
            {
                let hook = self.before_recount.lock().unwrap().take();
                if let Some(hook) = hook {
                    hook();
                }
            }
            count_container_viewers(entity, world).map(|count| (revision, count))
        });
        let publish = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut recounted = false;
            if let Some((revision, count)) = recount
                && Arc::ptr_eq(&revision, &state.revision)
            {
                state.current = count;
                state.revision = Arc::new(());
                if count == 0 {
                    state.ticks_until_recheck = 0;
                }
                recounted = true;
            }
            // Accepted unchanged recounts still resend chest lids (N3). Stale
            // revisions and deferred scans do not enqueue a recount callback.
            state.enqueue(world, recounted)
        };
        if publish {
            self.drain(entity, position);
        }
    }
}

fn count_container_viewers<T: Inventory + 'static>(entity: &T, world: &World) -> Option<u16> {
    let mut count = 0u16;
    // BarrelBlockEntity.isOwnContainer only accepts its direct inventory;
    // ChestBlockEntity also accepts a CompoundContainer containing this half.
    let accepts_double = entity
        .as_any()
        .is::<super::entities::chest::ChestBlockEntity>()
        || entity
            .as_any()
            .is::<super::entities::trapped_chest::TrappedChestBlockEntity>();
    // Unlike Java's spatial ContainerUser query, this bounded port enumerates
    // registered players. Opens arm five-tick rechecks, including one after the
    // last close so a deduplicated lid event cannot leave the client open.
    for player in world.players.load().iter() {
        if player.menu_publication.is_active() {
            return None;
        }
        if player.is_spectator() {
            continue;
        }
        let menu = player.current_screen_handler.try_lock().ok()?.clone();
        // Defer rather than wait on a callback that may itself tick this entity.
        let menu = menu.try_lock().ok()?;
        if let Some(menu) = menu
            .as_any()
            .downcast_ref::<GenericContainerScreenHandler>()
        {
            let inventory = menu.inventory.as_any();
            let owns = inventory
                .downcast_ref::<T>()
                .is_some_and(|held| std::ptr::eq(held, entity))
                || accepts_double
                    && inventory
                        .downcast_ref::<super::container_inventory::DoubleContainerInventory>()
                        .is_some_and(|double| double.contains(entity));
            if owns {
                count = count.saturating_add(1);
            }
        }
    }
    Some(count)
}

pub trait ViewerCountTrackerExt {
    fn update_viewer_count<T>(&self, entity: &T, world: &Arc<World>, position: &BlockPos)
    where
        T: BlockEntity + ViewerCountListener + 'static;
}

impl ViewerCountTrackerExt for ViewerCountTracker {
    fn update_viewer_count<T>(&self, entity: &T, world: &Arc<World>, position: &BlockPos)
    where
        T: BlockEntity + ViewerCountListener + 'static,
    {
        let current = self.current.load(Ordering::Relaxed);
        let old = self.old.swap(current, Ordering::Relaxed);
        if old != current {
            match (old, current) {
                (n, 0) if n > 0 => {
                    entity.on_container_close(world, position);
                }
                (0, n) if n > 0 => {
                    entity.on_container_open(world, position);
                }
                _ => {} // Ignore
            }

            entity.on_viewer_count_update(world, position, old, current);
        }
    }
}

pub trait ViewerCountListener: Send + Sync {
    fn on_container_open(&self, _world: &Arc<World>, _position: &BlockPos) {}

    fn on_container_close(&self, _world: &Arc<World>, _position: &BlockPos) {}

    fn on_viewer_count_update(
        &self,
        _world: &Arc<World>,
        _position: &BlockPos,
        _old: u16,
        _new: u16,
    ) {
    }
}
