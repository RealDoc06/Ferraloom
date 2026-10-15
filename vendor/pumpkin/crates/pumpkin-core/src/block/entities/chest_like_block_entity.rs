use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::identifier::Identifier;

/// The key and seed must be reloaded, saved and consumed as one state.
#[derive(Default)]
pub(super) struct ChestLootState {
    pub key: Option<String>,
    pub seed: i64,
}

/// Decode before taking live locks. The inventory reader only fills listed slots.
/// Java 26.3 ChestBlockEntity.loadAdditional starts empty and skips Items for loot.
pub(super) fn read_chest_nbt<const N: usize>(
    nbt: &NbtCompound,
) -> ([ItemStack; N], ChestLootState) {
    let loot = ChestLootState {
        key: nbt
            .get_string("LootTable")
            .and_then(|key| Identifier::parse(key).ok())
            .map(|key| key.to_string()),
        // ValueInput.getLongOr accepts every numeric tag. In 26.3 FloatTag
        // truncates toward zero, while DoubleTag floors before converting.
        seed: match nbt.get("LootTableSeed") {
            Some(NbtTag::Byte(seed)) => i64::from(*seed),
            Some(NbtTag::Short(seed)) => i64::from(*seed),
            Some(NbtTag::Int(seed)) => i64::from(*seed),
            Some(NbtTag::Long(seed)) => *seed,
            Some(NbtTag::Float(seed)) => *seed as i64,
            Some(NbtTag::Double(seed)) => seed.floor() as i64,
            _ => 0,
        },
    };
    let mut items = std::array::from_fn(|_| ItemStack::EMPTY.clone());
    if loot.key.is_none() {
        pumpkin_inventory::sync_read_items_from_nbt(nbt, &mut items);
    }
    (items, loot)
}

/// Implements the `BlockEntity` trait for chest-like block entities.
/// Parameters:
/// - $`struct_name`: The type of the chest struct (e.g., `ChestBlockEntity`)
#[macro_export]
macro_rules! impl_block_entity_for_chest {
    ($struct_name:ty) => {
        impl $crate::block::entities::BlockEntity for $struct_name {
            fn resource_location(&self) -> &'static str {
                Self::ID
            }

            fn get_position(&self) -> BlockPos {
                self.position
            }

            fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
            where
                Self: Sized,
            {
                let (items, loot) =
                    $crate::block::entities::chest_like_block_entity::read_chest_nbt(nbt);
                Self {
                    position,
                    items: std::sync::RwLock::new(items),
                    dirty: std::sync::atomic::AtomicBool::new(false),
                    comparator_dirty: std::sync::atomic::AtomicBool::new(false),
                    viewers: $crate::block::viewer::ViewerCountTracker::new(),
                    loot: StdMutex::new(loot),
                }
            }

            fn load_nbt(
                &self,
                nbt: &pumpkin_nbt::compound::NbtCompound,
            ) -> Result<(), $crate::block::entities::BlockEntityLoadError> {
                let (items, loot) =
                    $crate::block::entities::chest_like_block_entity::read_chest_nbt(nbt);
                {
                    // All two-lock operations take items before loot. No world or
                    // inventory callbacks run under either lock.
                    let mut live_items = self
                        .items
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let mut live_loot = self
                        .loot
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    *live_items = items;
                    *live_loot = loot;
                }
                <Self as pumpkin_inventory::Inventory>::mark_dirty(self);
                Ok(())
            }

            fn write_nbt(&self, nbt: &mut pumpkin_nbt::compound::NbtCompound) {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let loot = self
                    .loot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(key) = &loot.key {
                    // Persist deferred loot: write the key and seed; skip items.
                    nbt.put_string("LootTable", key.clone());
                    if loot.seed != 0 {
                        nbt.put_long("LootTableSeed", loot.seed);
                    }
                } else {
                    // Loot has already been generated, so persist the actual items.
                    nbt.put_list("Items", Vec::new());
                    pumpkin_inventory::sync_write_items_to_nbt(&*items, nbt);
                }
            }

            fn tick(&self, world: &Arc<$crate::world::World>) {
                $crate::block::viewer::ViewerCountTrackerExt::update_viewer_count::<$struct_name>(
                    &self.viewers,
                    self,
                    world,
                    &self.position,
                );
            }

            fn get_inventory(self: Arc<Self>) -> Option<Arc<dyn pumpkin_inventory::Inventory>> {
                Some(self)
            }

            fn is_comparator_dirty(&self) -> bool {
                self.comparator_dirty
                    .load(std::sync::atomic::Ordering::Relaxed)
            }

            fn clear_comparator_dirty(&self) {
                self.comparator_dirty
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }

            fn is_dirty(&self) -> bool {
                self.dirty.load(std::sync::atomic::Ordering::Relaxed)
            }

            fn clear_dirty(&self) {
                self.dirty
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }

            fn chunk_data_nbt(&self) -> Option<pumpkin_nbt::compound::NbtCompound> {
                let mut nbt = pumpkin_nbt::compound::NbtCompound::new();
                if let Ok(items) = self.items.try_read() {
                    let loot = self
                        .loot
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if loot.key.is_none() {
                        pumpkin_inventory::sync_write_items_to_nbt(&*items, &mut nbt);
                    }
                }
                Some(nbt)
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn take_loot_table(&self) -> Option<(String, i64)> {
                let mut guard = self
                    .loot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.key.take().map(|key| (key, guard.seed))
            }

            fn has_loot_table(&self) -> bool {
                self.loot
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .key
                    .is_some()
            }
        }
    };
}

/// Implements the Inventory trait for chest-like block entities.
#[macro_export]
macro_rules! impl_inventory_for_chest {
    ($struct_name:ty) => {
        impl pumpkin_inventory::Inventory for $struct_name {
            fn size(&self) -> usize {
                Self::INVENTORY_SIZE
            }

            fn is_empty(&self) -> bool {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items.iter().all(|s| s.is_empty())
            }

            fn get_stack(&self, slot: usize) -> ItemStack {
                let items = self
                    .items
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[slot].clone()
            }

            fn remove_stack(&self, slot: usize) -> ItemStack {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let removed = std::mem::replace(&mut items[slot], ItemStack::EMPTY.clone());
                self.mark_dirty();
                removed
            }

            fn remove_stack_specific(&self, slot: usize, amount: u8) -> ItemStack {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let res = if !items[slot].is_empty() && amount > 0 {
                    items[slot].split(amount)
                } else {
                    ItemStack::EMPTY.clone()
                };
                self.mark_dirty();
                res
            }

            fn set_stack(&self, slot: usize, stack: ItemStack) {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items[slot] = stack;
                self.mark_dirty();
            }

            fn on_open(&self) {
                self.viewers.open_container();
            }

            fn on_close(&self) {
                self.viewers.close_container();
            }

            fn mark_dirty(&self) {
                self.dirty.store(true, std::sync::atomic::Ordering::Relaxed);
                self.comparator_dirty
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
    };
}

/// Implements the Clearable trait for chest-like block entities.
#[macro_export]
macro_rules! impl_clearable_for_chest {
    ($struct_name:ty) => {
        impl pumpkin_inventory::Clearable for $struct_name {
            fn clear(&self) {
                let mut items = self
                    .items
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                items.fill_with(|| ItemStack::EMPTY.clone());
                <$struct_name as pumpkin_inventory::Inventory>::mark_dirty(self);
            }
        }
    };
}

/// Implements the `ViewerCountListener` trait for chest-like block entities.
///
/// The behavior is controlled by the `EMITS_REDSTONE` constant on the struct.
/// When `EMITS_REDSTONE` is true, updates neighbors for redstone signals when viewer count changes.
#[macro_export]
macro_rules! impl_viewer_count_listener_for_chest {
    ($struct_name:ty) => {
        impl $crate::block::viewer::ViewerCountListener for $struct_name {
            fn on_container_open(
                &self,
                world: &Arc<$crate::world::World>,
                _position: &pumpkin_util::math::position::BlockPos,
            ) {
                self.play_sound(world, pumpkin_data::sound::Sound::BlockChestOpen);
            }

            fn on_container_close(
                &self,
                world: &Arc<$crate::world::World>,
                _position: &pumpkin_util::math::position::BlockPos,
            ) {
                self.play_sound(world, pumpkin_data::sound::Sound::BlockChestClose);
            }

            fn on_viewer_count_update(
                &self,
                world: &Arc<$crate::world::World>,
                position: &pumpkin_util::math::position::BlockPos,
                old: u16,
                new: u16,
            ) {
                // Trigger block animation
                world.add_synced_block_event(*position, Self::LID_ANIMATION_EVENT_TYPE, new as u8);

                // Update neighbors for redstone signal when viewer count changes
                // This is controlled by the EMITS_REDSTONE constant on the struct
                if Self::EMITS_REDSTONE && old != new {
                    // Update direct neighbors
                    world.update_neighbors(position, None);

                    // Also update neighbors of the block below (strongly powered block)
                    // This ensures redstone components adjacent to the block below are notified
                    let below_pos = position.down();
                    world.update_neighbors(&below_pos, None);
                }
            }
        }
    };
}

/// Implements helper methods for chest-like block entities.
///
/// Includes the `play_sound` method which handles sound positioning for single and double chests,
/// as well as `new()` and `get_viewer_count()` methods.
#[macro_export]
macro_rules! impl_chest_helper_methods {
    ($struct_name:ty) => {
        impl $struct_name {
            /// Returns the number of players currently viewing this chest
            pub fn get_viewer_count(&self) -> u16 {
                self.viewers.get_viewer_count()
            }

            #[must_use]
            pub fn new(position: pumpkin_util::math::position::BlockPos) -> Self {
                use std::array::from_fn;
                use std::sync::Mutex as StdMutex;
                use std::sync::RwLock;
                use std::sync::atomic::AtomicBool;

                Self {
                    position,
                    items: RwLock::new(from_fn(|_| ItemStack::EMPTY.clone())),
                    dirty: AtomicBool::new(false),
                    comparator_dirty: AtomicBool::new(false),
                    viewers: $crate::block::viewer::ViewerCountTracker::new(),
                    loot: StdMutex::default(),
                }
            }

            fn play_sound(
                &self,
                world: &Arc<$crate::world::World>,
                sound: pumpkin_data::sound::Sound,
            ) {
                let mut rng = pumpkin_util::random::xoroshiro128::Xoroshiro::from_seed(
                    pumpkin_util::random::get_seed(),
                );

                let state = world.get_block_state(&self.position);
                let properties =
                    pumpkin_data::block_properties::ChestLikeProperties::from_state_id(state.id);
                let position = match properties.r#type {
                    pumpkin_data::block_properties::ChestType::Left => return,
                    pumpkin_data::block_properties::ChestType::Single => {
                        pumpkin_util::math::vector3::Vector3::new(
                            self.position.0.x as f64 + 0.5,
                            self.position.0.y as f64 + 0.5,
                            self.position.0.z as f64 + 0.5,
                        )
                    }
                    pumpkin_data::block_properties::ChestType::Right => {
                        let direction = pumpkin_data::HorizontalFacingExt::to_block_direction(
                            &properties.facing,
                        )
                        .to_offset();
                        pumpkin_util::math::vector3::Vector3::new(
                            self.position.0.x as f64 + 0.5 + direction.x as f64 * 0.5,
                            self.position.0.y as f64 + 0.5,
                            self.position.0.z as f64 + 0.5 + direction.z as f64 * 0.5,
                        )
                    }
                };

                world.play_sound_fine(
                    sound,
                    pumpkin_data::sound::SoundCategory::Blocks,
                    &position,
                    0.5,
                    pumpkin_util::random::RandomImpl::next_f32(&mut rng) * 0.1 + 0.9,
                );
                let bedrock_sound = match sound {
                    pumpkin_data::sound::Sound::BlockChestOpen => "chest.open",
                    pumpkin_data::sound::Sound::BlockChestClose => "chest.closed",
                    _ => return,
                };
                world.play_bedrock_level_sound(bedrock_sound, &position, 0);
            }
        }
    };
}
