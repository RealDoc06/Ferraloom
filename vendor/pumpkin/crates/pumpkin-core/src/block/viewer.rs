use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use pumpkin_inventory::{
    Inventory, generic_container_screen_handler::GenericContainerScreenHandler,
    screen_handler::InventoryPlayer,
};
use pumpkin_util::math::position::BlockPos;

use crate::{block::entities::BlockEntity, world::World};

pub use pumpkin_inventory::ViewerCountTracker;

/// A menu's viewer callbacks precede publication of current_screen_handler.
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
}

/// Chest/trapped-chest/barrel recounts only. Other container trackers retain
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
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.current == 0 {
            state.ticks_until_recheck = 5;
        }
        state.current = state.current.saturating_add(1);
        state.revision = Arc::new(());
    }

    pub(crate) fn close_container(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A stale retained menu may close after a recount already removed it.
        state.current = state.current.saturating_sub(1);
        state.revision = Arc::new(());
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
            if state.current == 0 {
                None
            } else {
                state.ticks_until_recheck = state.ticks_until_recheck.saturating_sub(1);
                if state.ticks_until_recheck == 0 {
                    state.ticks_until_recheck = 5;
                    Some(state.revision.clone())
                } else {
                    None
                }
            }
        };
        if let Some(revision) = revision {
            #[cfg(test)]
            {
                let hook = self.before_recount.lock().unwrap().take();
                if let Some(hook) = hook {
                    hook();
                }
            }
            let count = count_container_viewers(entity, world);
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(count) = count
                && Arc::ptr_eq(&revision, &state.revision)
            {
                state.current = count;
                state.revision = Arc::new(());
            }
        }
        let (old, current) = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let old = state.published;
            state.published = state.current;
            (old, state.current)
        };
        // World, sound, redstone and menu callbacks are outside the viewer lock.
        if old != current {
            match (old, current) {
                (0, _) => entity.on_container_open(world, position),
                (_, 0) => entity.on_container_close(world, position),
                _ => {}
            }
            entity.on_viewer_count_update(world, position, old, current);
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
    // registered players. Only active containers reach here, every five BE ticks.
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
