use std::{
    any::Any,
    collections::VecDeque,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use pumpkin_data::{
    Block,
    block_properties::JukeboxLikeProperties,
    data_component::DataComponent,
    data_component_impl::{DataComponentImpl, JukeboxPlayableImpl},
    entity::EntityType,
    item_stack::{
        ItemStack,
        admission::{validate_item_stack, validate_runtime_item_stack},
    },
    jukebox_song::JukeboxSong,
};
use pumpkin_inventory::{Clearable, Inventory};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_world::world::BlockFlags;
use rand::{RngExt, rng};

use crate::{
    block::entities::{BlockEntity, BlockEntityLoadError, NbtEditDecision},
    entity::{Entity, item::ItemEntity},
    world::World,
};

const RECORD_ITEM: &str = "RecordItem";
const ELAPSED: &str = "ticks_since_song_started";

/// Item, active song and signed elapsed time share one transition owner. The
/// separate binding lock is callback-free even under World publication guards.
pub struct JukeboxBlockEntity {
    position: BlockPos,
    world: OnceLock<Weak<World>>,
    state: Mutex<JukeboxState>,
    dirty: AtomicBool,
    comparator_dirty: AtomicBool,
}

struct JukeboxState {
    record: ItemStack,
    song: Option<JukeboxSong>,
    elapsed: i64,
    revision: Arc<()>,
    pending: VecDeque<Effects>,
    draining: bool,
    detached: bool,
}

struct Effects {
    world: Weak<World>,
    revision: Arc<()>,
    actions: Vec<Action>,
}

enum Action {
    RecordPresence(bool),
    Start(JukeboxSong),
    Stop,
    Periodic,
    Changed,
    Dirty,
    // Once removed from storage, this captured item remains owned by its drop
    // even if a callback replaces the BE or starts a newer transition.
    Drop(ItemStack),
}

struct DrainGuard<'a>(Option<&'a Mutex<JukeboxState>>);

impl Drop for DrainGuard<'_> {
    fn drop(&mut self) {
        if let Some(state) = self.0 {
            state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .draining = false;
        }
    }
}

fn song_from_stack(stack: &ItemStack) -> Option<JukeboxSong> {
    (!stack.is_empty())
        .then(|| stack.get_data_component::<JukeboxPlayableImpl>())
        .flatten()
        .map(|playable| playable.song)
}

fn has_finished(song: JukeboxSong, elapsed: i64) -> bool {
    // The exact accepted builtin profile has integral seconds, but vanilla's
    // arithmetic is float multiplication followed by ceil, plus 20 padded ticks.
    elapsed >= ((song.length_in_seconds() as f32 * 20.0).ceil() as i64 + 20)
}

fn effective_component(stack: &ItemStack, id: DataComponent) -> Option<&dyn DataComponentImpl> {
    if let Some((_, value)) = stack.patch.iter().find(|(key, _)| *key == id) {
        return value.as_deref();
    }
    stack
        .item
        .components
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, value)| *value)
}

/// Java compares effective components, so an explicit builtin default does not
/// stop playback on load. Count changes alone also do not change the song item.
fn same_item_components(left: &ItemStack, right: &ItemStack) -> bool {
    left.item.id == right.item.id
        && left.patch.iter().chain(&right.patch).all(|(id, _)| {
            match (
                effective_component(left, *id),
                effective_component(right, *id),
            ) {
                (None, None) => true,
                (Some(left), Some(right)) => left.equal(right),
                _ => false,
            }
        })
}

impl JukeboxState {
    fn new() -> Self {
        Self {
            record: ItemStack::EMPTY.clone(),
            song: None,
            elapsed: 0,
            revision: Arc::new(()),
            pending: VecDeque::new(),
            draining: false,
            detached: false,
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.child_tags.remove(RECORD_ITEM);
        nbt.child_tags.remove(ELAPSED);
        if !self.record.is_empty() {
            let mut record = NbtCompound::new();
            self.record.write_item_stack(&mut record);
            nbt.put_compound(RECORD_ITEM, record);
        }
        if self.song.is_some() {
            nbt.put_long(ELAPSED, self.elapsed);
        }
    }

    fn snapshot(&self, position: BlockPos) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_string("id", JukeboxBlockEntity::ID.into());
        nbt.put_int("x", position.0.x);
        nbt.put_int("y", position.0.y);
        nbt.put_int("z", position.0.z);
        self.write_nbt(&mut nbt);
        nbt
    }

    fn enqueue(&mut self, world: &Weak<World>, actions: Vec<Action>) {
        if !actions.is_empty() {
            self.pending.push_back(Effects {
                world: world.clone(),
                revision: self.revision.clone(),
                actions,
            });
        }
    }

    fn stop(&mut self, actions: &mut Vec<Action>) {
        if self.song.take().is_some() {
            self.elapsed = 0;
            actions.extend([Action::Stop, Action::Changed]);
        }
    }

    fn replace(&mut self, record: ItemStack, actions: &mut Vec<Action>) -> ItemStack {
        self.revision = Arc::new(());
        let old = std::mem::replace(&mut self.record, record);
        actions.push(Action::RecordPresence(!self.record.is_empty()));
        if let Some(song) = song_from_stack(&self.record) {
            self.song = Some(song);
            self.elapsed = 0;
            actions.extend([Action::Start(song), Action::Changed]);
        } else {
            self.stop(actions);
            actions.push(Action::Dirty);
        }
        old
    }

    fn load(&mut self, record: ItemStack, elapsed: Option<i64>, actions: &mut Vec<Action>) {
        let same_record = same_item_components(&record, &self.record)
            && record.item_count == self.record.item_count;
        let previous_playback = (self.song, self.elapsed);
        if !self.record.is_empty() && !same_item_components(&record, &self.record) {
            self.stop(actions);
        }
        self.record = record;
        // Imperative load: absent/finished elapsed leaves existing active state
        // untouched. Fresh objects start inactive; restoring never emits 1010.
        if let Some(elapsed) = elapsed
            && let Some(song) = song_from_stack(&self.record)
            && !has_finished(song, elapsed)
        {
            self.song = Some(song);
            self.elapsed = elapsed;
        }
        // A no-op guarded edit during block_change must not cancel the queued
        // Start/Changed belonging to this exact transition. A timing-only edit
        // still invalidates old periodic work; its reentrant audio timing is not
        // covered by the no-op guarantee.
        if !same_record || previous_playback != (self.song, self.elapsed) {
            self.revision = Arc::new(());
        }
    }
}

fn decode(nbt: &NbtCompound) -> Result<(ItemStack, Option<i64>), BlockEntityLoadError> {
    let record = if let Some(value) = nbt.get(RECORD_ITEM) {
        let record = value
            .extract_compound()
            .ok_or(BlockEntityLoadError::Unsupported("RecordItem"))?;
        validate_item_stack(record)
            .map_err(|error| BlockEntityLoadError::Unsupported(error.reason()))?;
        ItemStack::read_item_stack(record).ok_or(BlockEntityLoadError::Unsupported("RecordItem"))?
    } else {
        ItemStack::EMPTY.clone()
    };
    Ok((record, nbt.get_long(ELAPSED)))
}

impl JukeboxBlockEntity {
    pub const ID: &'static str = "minecraft:jukebox";

    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            world: OnceLock::new(),
            state: Mutex::new(JukeboxState::new()),
            dirty: AtomicBool::new(false),
            comparator_dirty: AtomicBool::new(false),
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "Infallible Inventory mutations must reject an absent live owner before committing state"
    )]
    fn owner(&self) -> Arc<World> {
        let world = self
            .world
            .get()
            .and_then(Weak::upgrade)
            .expect("live jukebox mutation requires its bound World");
        assert!(
            world.is_current_block_entity(self),
            "jukebox is no longer published in its owning World"
        );
        assert!(
            !self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .detached,
            "a detached jukebox allocation cannot resume live mutation"
        );
        world
    }

    pub fn get_record(&self) -> ItemStack {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .record
            .clone()
    }

    pub fn is_playing(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .song
            .is_some()
    }

    /// Every live setter/remover uses this transition; max stack size is an
    /// insertion rule and does not clamp a direct full-stack assignment.
    pub fn set_record(&self, record: ItemStack) {
        let world = self.owner();
        assert!(
            validate_runtime_item_stack(&record).is_ok(),
            "unsupported jukebox item"
        );
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut actions = Vec::new();
            state.replace(record, &mut actions);
            state.enqueue(&Arc::downgrade(&world), actions)
        };
        self.drain();
    }

    fn take_record(&self, eject: bool) -> ItemStack {
        let world = self.owner();
        let old = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if eject && state.record.is_empty() {
                return ItemStack::EMPTY.clone();
            }
            let mut actions = Vec::new();
            let old = state.replace(ItemStack::EMPTY.clone(), &mut actions);
            if eject {
                actions.push(Action::Drop(old.clone()));
                actions.push(Action::Changed);
            }
            state.enqueue(&Arc::downgrade(&world), actions);
            old
        };
        self.drain();
        old
    }

    pub fn eject_record(&self) {
        self.take_record(true);
    }

    fn is_current(&self, world: &World, revision: &Arc<()>) -> bool {
        world.is_current_block_entity(self) && {
            let state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            !state.detached && Arc::ptr_eq(&state.revision, revision)
        }
    }

    fn mark_changed(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        self.comparator_dirty.store(true, Ordering::Relaxed);
    }

    /// One synchronous drainer, with nested/concurrent transitions enqueueing
    /// without waiting. No state, inventory, viewer or map guard crosses effects.
    fn drain(&self) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.draining || state.pending.is_empty() {
                return;
            }
            state.draining = true
        };
        let mut release = DrainGuard(Some(&self.state));
        loop {
            let effects = {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let Some(effects) = state.pending.pop_front() else {
                    state.draining = false;
                    release.0 = None;
                    return;
                };
                effects
            };
            let Some(world) = effects.world.upgrade() else {
                continue;
            };
            for action in effects.actions {
                match action {
                    Action::Drop(record) => {
                        let position = Vector3::new(
                            f64::from(self.position.0.x) + 0.5 + rng().random_range(-0.35..0.35),
                            f64::from(self.position.0.y) + 1.01,
                            f64::from(self.position.0.z) + 0.5 + rng().random_range(-0.35..0.35),
                        );
                        world.spawn_entity(Arc::new(ItemEntity::new(
                            Entity::new(world.clone(), position, &EntityType::ITEM),
                            record,
                        )));
                    }
                    _ if !self.is_current(&world, &effects.revision) => {}
                    Action::RecordPresence(has_record) => {
                        let current = world.get_block_state_id(&self.position);
                        if current.to_block() == &Block::JUKEBOX {
                            world.set_block_state_if(
                                &self.position,
                                JukeboxLikeProperties { has_record }.to_state_id(&Block::JUKEBOX),
                                BlockFlags::NOTIFY_LISTENERS,
                                |id| id == current,
                            );
                            if self.is_current(&world, &effects.revision)
                                && world.get_block(&self.position) == &Block::JUKEBOX
                            {
                                world.emit_game_event(
                                    "block_change",
                                    self.position.to_centered_f64(),
                                );
                            }
                        }
                    }
                    Action::Start(song) => world.jukebox_level_event(self.position, Some(song)),
                    Action::Stop => {
                        world.emit_game_event("jukebox_stop_play", self.position.to_centered_f64());
                        if self.is_current(&world, &effects.revision) {
                            world.jukebox_level_event(self.position, None);
                        }
                    }
                    Action::Periodic => {
                        world.emit_game_event("jukebox_play", self.position.to_centered_f64());
                        if self.is_current(&world, &effects.revision) {
                            world.jukebox_note(self.position);
                        }
                    }
                    Action::Changed => {
                        world.update_neighbors_at(&self.position, &Block::JUKEBOX, None);
                        if self.is_current(&world, &effects.revision) {
                            self.mark_changed();
                        }
                    }
                    Action::Dirty => self.mark_changed(),
                }
            }
        }
    }
}

impl BlockEntity for JukeboxBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }
    fn get_position(&self) -> BlockPos {
        self.position
    }
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn bind_world(&self, world: &World) {
        let owner = world.weak_self();
        let bound = self.world.get_or_init(|| owner.clone());
        assert!(
            Weak::ptr_eq(bound, owner),
            "jukebox cannot be rebound to a foreign World"
        );
    }

    fn from_nbt(nbt: &NbtCompound, position: BlockPos) -> Self {
        // Raw region admission checks RecordItem before this infallible boundary.
        let entity = Self::new(position);
        if let Ok((record, elapsed)) = decode(nbt) {
            entity
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .load(record, elapsed, &mut Vec::new());
        }
        entity
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .write_nbt(nbt);
    }

    fn load_nbt(&self, nbt: &NbtCompound) -> Result<(), BlockEntityLoadError> {
        let (record, elapsed) = decode(nbt)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut actions = Vec::new();
        state.load(record, elapsed, &mut actions);
        if let Some(world) = self.world.get() {
            state.enqueue(world, actions);
        }
        self.mark_changed();
        Ok(())
    }

    fn snapshot_nbt(&self, publish: &mut dyn FnMut(NbtCompound)) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        publish(state.snapshot(self.position));
    }

    fn edit_nbt(
        &self,
        edit: &mut dyn FnMut(&mut NbtCompound) -> NbtEditDecision,
        publish: &mut dyn FnMut(NbtCompound),
    ) -> Result<bool, BlockEntityLoadError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut candidate = state.snapshot(self.position);
        if edit(&mut candidate) == NbtEditDecision::Abort {
            return Ok(false);
        }
        let (record, elapsed) = decode(&candidate)?;
        let mut actions = Vec::new();
        state.load(record, elapsed, &mut actions);
        if let Some(world) = self.world.get() {
            state.enqueue(world, actions);
        }
        self.mark_changed();
        publish(state.snapshot(self.position));
        Ok(true)
    }

    fn after_nbt_commit(&self, _world: &Arc<World>) {
        self.drain();
    }

    fn on_removed(&self, world: &World) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.detached {
                return;
            }
            state.detached = true;
            state.revision = Arc::new(())
        };
        // Official setRemoved is unconditional and separate from player.stop.
        // Preserve active elapsed for the final saved snapshot and fresh restore.
        world.emit_game_event("jukebox_stop_play", self.position.to_centered_f64());
        let replacement_playing =
            world
                .published_block_entity(&self.position)
                .is_some_and(|entity| {
                    entity
                        .as_any()
                        .downcast_ref::<Self>()
                        .is_some_and(|replacement| {
                            !std::ptr::eq(replacement, self) && replacement.is_playing()
                        })
                });
        // A notification may reentrantly start the replacement; never let the
        // older terminal packet stop that new playback.
        if !replacement_playing {
            world.jukebox_level_event(self.position, None);
        }
        self.drain();
    }

    fn on_block_replaced(self: Arc<Self>, _world: &Arc<World>, _position: &BlockPos) {
        self.eject_record();
    }

    fn tick(&self, world: &Arc<World>) {
        if !world.is_current_block_entity(self)
            || world.get_block(&self.position) != &Block::JUKEBOX
            || !JukeboxLikeProperties::from_state_id(world.get_block_state_id(&self.position))
                .has_record
        {
            return;
        }
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(song) = state.song else { return };
            let mut actions = Vec::new();
            if has_finished(song, state.elapsed) {
                state.revision = Arc::new(());
                state.stop(&mut actions);
            } else {
                if state.elapsed % 20 == 0 {
                    actions.push(Action::Periodic);
                }
                // Commit before guard-free plugin notifications. Vanilla emits
                // before incrementing; this notification-only hook observes +1.
                // Its plugin/vibration timing is not a vanilla parity claim.
                state.elapsed += 1;
            }
            state.enqueue(&Arc::downgrade(world), actions)
        };
        self.drain();
    }

    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }
    fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }
    fn is_comparator_dirty(&self) -> bool {
        self.comparator_dirty.load(Ordering::Relaxed)
    }
    fn clear_comparator_dirty(&self) {
        self.comparator_dirty.store(false, Ordering::Relaxed);
    }
    fn get_inventory(self: Arc<Self>) -> Option<Arc<dyn Inventory>> {
        Some(self)
    }
}

impl Inventory for JukeboxBlockEntity {
    fn size(&self) -> usize {
        1
    }
    fn is_empty(&self) -> bool {
        self.get_record().is_empty()
    }
    fn get_max_count_per_stack(&self) -> u8 {
        1
    }
    fn get_stack(&self, slot: usize) -> ItemStack {
        if slot == 0 {
            self.get_record()
        } else {
            ItemStack::EMPTY.clone()
        }
    }
    fn set_stack(&self, slot: usize, stack: ItemStack) {
        if slot == 0 {
            self.set_record(stack);
        }
    }
    fn remove_stack(&self, slot: usize) -> ItemStack {
        if slot == 0 {
            self.take_record(false)
        } else {
            ItemStack::EMPTY.clone()
        }
    }
    fn remove_stack_specific(&self, slot: usize, _amount: u8) -> ItemStack {
        self.remove_stack(slot)
    }
    fn is_valid_slot_for(&self, slot: usize, stack: &ItemStack) -> bool {
        slot == 0 && stack.get_data_component::<JukeboxPlayableImpl>().is_some() && self.is_empty()
    }
    fn can_transfer_to(
        &self,
        destination: &dyn Inventory,
        slot: usize,
        _stack: &ItemStack,
    ) -> bool {
        slot == 0 && (0..destination.size()).any(|slot| destination.get_stack(slot).is_empty())
    }
    fn mark_dirty(&self) {
        self.mark_changed();
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Clearable for JukeboxBlockEntity {
    fn clear(&self) {
        self.remove_stack(0);
    }
}
