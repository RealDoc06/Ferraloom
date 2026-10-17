use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::identifier::Identifier;
use pumpkin_util::text::TextComponent;

use std::sync::{Arc, Weak};

use super::{BlockEntityCloneData, BlockEntityLoadError};
use crate::server::Server;

/// The initial clone codec is deliberately limited to these three 27-slot
/// containers. Other terrain with a BE is an explicit unsupported projection.
pub(crate) fn clone_kind(
    state: pumpkin_data::BlockStateId,
) -> Result<Option<&'static str>, BlockEntityLoadError> {
    let block = state.to_block();
    if block == &pumpkin_data::Block::CHEST {
        Ok(Some("minecraft:chest"))
    } else if block == &pumpkin_data::Block::TRAPPED_CHEST {
        Ok(Some("minecraft:trapped_chest"))
    } else if block == &pumpkin_data::Block::BARREL {
        Ok(Some("minecraft:barrel"))
    } else if state.to_state().block_entity_type == u16::MAX {
        Ok(None)
    } else {
        Err(BlockEntityLoadError::Unsupported("clone block entity type"))
    }
}

/// Validate the exact pending record before the infallible supported decoder
/// can discard fields. Callers retain their pending-record guard throughout.
fn validate_clone_nbt(
    kind: &'static str,
    position: pumpkin_util::math::position::BlockPos,
    nbt: &NbtCompound,
) -> Result<(), BlockEntityLoadError> {
    if nbt.get_string("id") != Some(kind)
        || nbt.get_int("x") != Some(position.0.x)
        || nbt.get_int("y") != Some(position.0.y)
        || nbt.get_int("z") != Some(position.0.z)
    {
        return Err(BlockEntityLoadError::Unsupported(
            "clone block entity identity",
        ));
    }
    pumpkin_world::chunk::format::validate_saved_container_items(nbt, position)
        .map_err(|_| BlockEntityLoadError::Unsupported("clone container items"))?;
    if nbt.child_tags.keys().any(|key| {
        !matches!(
            key.as_ref(),
            "id" | "x"
                | "y"
                | "z"
                | "CustomName"
                | "components"
                | "Items"
                | "LootTable"
                | "LootTableSeed"
        )
    }) {
        return Err(BlockEntityLoadError::Unsupported(
            "clone container metadata",
        ));
    }
    ContainerMetadata::from_nbt(nbt).validate_clone()?;
    if nbt.get("LootTable").is_some()
        && nbt
            .get_string("LootTable")
            .and_then(|key| Identifier::parse(key).ok())
            .is_none()
    {
        return Err(BlockEntityLoadError::Unsupported(
            "clone deferred loot table",
        ));
    }
    if nbt.get("LootTableSeed").is_some_and(|tag| {
        !matches!(
            tag,
            NbtTag::Byte(_)
                | NbtTag::Short(_)
                | NbtTag::Int(_)
                | NbtTag::Long(_)
                | NbtTag::Float(_)
                | NbtTag::Double(_)
        )
    }) {
        return Err(BlockEntityLoadError::Unsupported(
            "clone deferred loot seed",
        ));
    }
    pumpkin_world::nbt_io::encode_compound(nbt.clone())
        .map_err(|_| BlockEntityLoadError::Unsupported("clone NBT encoding"))?;
    Ok(())
}

pub(crate) fn snapshot_pending_for_clone(
    kind: &'static str,
    position: pumpkin_util::math::position::BlockPos,
    nbt: &NbtCompound,
) -> Result<BlockEntityCloneData, BlockEntityLoadError> {
    // Java's chunk save adds this packing marker to both live and pending
    // records. It is not custom BE data and must never enter the clone payload.
    let mut detached = nbt.clone();
    match detached.child_tags.remove("keepPacked") {
        None | Some(NbtTag::Byte(0 | 1)) => {}
        Some(_) => return Err(BlockEntityLoadError::Unsupported("clone packing marker")),
    }
    validate_clone_nbt(kind, position, &detached)?;
    ChestState::<27>::from_nbt(&detached).snapshot_for_clone(kind, position)
}

/// Preserve the original tags even when the runtime only understands a subset.
/// 26.3 `BaseContainerBlockEntity` uses `CustomName`/lock; `BlockEntity` separately
/// persists its components map. These belong to the same transaction as Items.
pub(super) struct ContainerMetadata {
    custom_name: Option<NbtTag>,
    title: Option<TextComponent>,
    lock: Option<NbtTag>,
    predicate: ContainerLock,
    components: Option<NbtTag>,
}

enum ContainerLock {
    Any,
    CustomName(TextComponent),
    Unsupported,
}

impl ContainerLock {
    fn decode(tag: Option<&NbtTag>) -> Self {
        let Some(tag) = tag else { return Self::Any };
        let NbtTag::Compound(predicate) = tag else {
            // 26.3 LockCode's record codec has no partial for a non-map tag;
            // fromTag therefore falls back to NO_LOCK. Keep the raw tag on disk.
            return Self::Any;
        };
        // RecordCodecBuilder ignores unknown top-level fields. Known fields
        // remain conservative: malformed mixes can have DFU partials that retain
        // a valid custom_name predicate, so errors must not all become NO_LOCK.
        if predicate.get("items").is_some()
            || predicate.get("count").is_some()
            || predicate.get("predicates").is_some_and(
                |tag| !matches!(tag, NbtTag::Compound(value) if value.child_tags.is_empty()),
            )
        {
            return Self::Unsupported;
        }
        let Some(components) = predicate.get("components") else {
            return Self::Any;
        };
        let NbtTag::Compound(components) = components else {
            return Self::Unsupported;
        };
        if components.child_tags.is_empty() {
            return Self::Any;
        }
        if components.child_tags.len() != 1 {
            return Self::Unsupported;
        }
        let Some(name) = components
            .get("minecraft:custom_name")
            .or_else(|| components.get("custom_name"))
        else {
            return Self::Unsupported;
        };
        // The pinned item CustomNameImpl codec currently only retains literal
        // names. Refuse rich key predicates instead of promising broken matching.
        let Some(name) = container_name(name, 0) else {
            return Self::Unsupported;
        };
        if !name.0.style.is_empty() || !name.0.extra.is_empty() {
            return Self::Unsupported;
        }
        Self::CustomName(name)
    }

    fn matches(&self, main_hand: &ItemStack) -> bool {
        match self {
            Self::Any => true,
            // DataComponentExactPredicate compares component values, not the
            // flattened display name (nor item_name or the item's default name).
            Self::CustomName(expected) => main_hand.get_custom_name() == Some(expected),
            Self::Unsupported => false,
        }
    }
}

impl ContainerMetadata {
    /// Clone is a fresh destination load: opaque values cannot be retained just
    /// because ordinary in-place edits permit unchanged source metadata.
    fn validate_clone(&self) -> Result<(), BlockEntityLoadError> {
        if self
            .custom_name
            .as_ref()
            .is_some_and(|name| !matches!(name, NbtTag::String(_)))
        {
            return Err(BlockEntityLoadError::Unsupported(
                "clone container CustomName",
            ));
        }
        if self.lock.is_some() {
            return Err(BlockEntityLoadError::Unsupported("clone container lock"));
        }
        if self.components.as_ref().is_some_and(
            |components| !matches!(components, NbtTag::Compound(map) if map.is_empty()),
        ) {
            return Err(BlockEntityLoadError::Unsupported(
                "clone block entity components",
            ));
        }
        Ok(())
    }
    pub(super) fn from_nbt(nbt: &NbtCompound) -> Self {
        Self {
            custom_name: nbt.get("CustomName").cloned(),
            title: nbt.get("CustomName").and_then(|tag| container_name(tag, 0)),
            lock: nbt.get("lock").cloned(),
            predicate: ContainerLock::decode(nbt.get("lock")),
            components: nbt.get("components").cloned(),
        }
    }

    /// Unrelated edits may retain opaque loaded values; newly introduced forms
    /// must be understood before any state, loot token or publication changes.
    pub(super) fn validate_edit(&self, previous: &Self) -> Result<(), super::BlockEntityLoadError> {
        let unsupported = if self.custom_name != previous.custom_name
            && self.custom_name.is_some()
            && self.title.is_none()
        {
            Some("container CustomName component")
        } else if self.lock != previous.lock && matches!(self.predicate, ContainerLock::Unsupported)
        {
            Some("container lock predicate")
        } else if self.components != previous.components
            && self
                .components
                .as_ref()
                .is_some_and(|tag| !matches!(tag, NbtTag::Compound(_)))
        {
            Some("block entity components map")
        } else {
            None
        };
        unsupported.map_or(Ok(()), |field| {
            Err(super::BlockEntityLoadError::Unsupported(field))
        })
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        for (key, value) in [
            ("CustomName", &self.custom_name),
            ("lock", &self.lock),
            ("components", &self.components),
        ] {
            if let Some(value) = value {
                nbt.put(key, value.clone());
            }
        }
    }

    pub(super) fn can_unlock(&self, main_hand: &ItemStack) -> bool {
        self.predicate.matches(main_hand)
    }

    pub(super) fn custom_name(&self) -> Option<TextComponent> {
        self.custom_name.as_ref().map(|_| {
            self.title
                .clone()
                .unwrap_or_else(|| TextComponent::text("[Unsupported container name]"))
        })
    }
}

/// Bounded literal-component support. Unknown content/style forms remain opaque
/// on disk and are rejected on live replacement, never flattened or defaulted.
fn container_name(tag: &NbtTag, depth: usize) -> Option<TextComponent> {
    if depth > 64 {
        return None;
    }
    if let NbtTag::String(text) = tag {
        return Some(TextComponent::text(text.to_string()));
    }
    let NbtTag::Compound(nbt) = tag else {
        return None;
    };
    let mut name = TextComponent::text(nbt.get_string("text")?.to_owned());
    for (key, value) in &nbt.child_tags {
        match key.as_ref() {
            "text" | "type" => {
                if key.as_ref() == "type" && value.extract_string()? != "text" {
                    return None;
                }
            }
            "color" => {
                let color = value.extract_string()?;
                // Validate ASCII hex before the upstream decoder slices it.
                if color == "reset"
                    || (color.starts_with('#')
                        && (color.len() != 7
                            || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)))
                {
                    return None;
                }
                name.0.style.color =
                    Some(serde_json::from_value(serde_json::Value::String(color.into())).ok()?);
            }
            "bold" | "italic" | "underlined" | "strikethrough" | "obfuscated" => {
                let flag = match value {
                    NbtTag::Byte(value) => *value != 0,
                    NbtTag::Short(value) => *value != 0,
                    NbtTag::Int(value) => *value != 0,
                    NbtTag::Long(value) => *value != 0,
                    NbtTag::Float(value) => *value != 0.0,
                    NbtTag::Double(value) => *value != 0.0,
                    _ => return None,
                };
                match key.as_ref() {
                    "bold" => name.0.style.bold = Some(flag),
                    "italic" => name.0.style.italic = Some(flag),
                    "underlined" => name.0.style.underlined = Some(flag),
                    "strikethrough" => name.0.style.strikethrough = Some(flag),
                    "obfuscated" => name.0.style.obfuscated = Some(flag),
                    _ => return None,
                }
            }
            "insertion" => name.0.style.insertion = Some(value.extract_string()?.to_owned()),
            "extra" => {
                let NbtTag::List(extra) = value else {
                    return None;
                };
                if extra.is_empty() {
                    return None;
                }
                name.0.extra = extra
                    .iter()
                    .map(|tag| container_name(tag, depth + 1).map(|name| name.0))
                    .collect::<Option<_>>()?;
            }
            _ => return None,
        }
    }
    Some(name)
}

/// Contents, pending loot and metadata share a lock so access cannot cross a reload.
pub(super) struct ChestState<const N: usize> {
    pub items: [ItemStack; N],
    pub loot: ChestLootState,
    pub metadata: ContainerMetadata,
    pub load_token: Arc<()>,
    // Only a weak datapack source is needed by World::get_loot_table. Binding on
    // publication also covers hopper/comparator access before the first tick/open.
    pub server: Option<Weak<Server>>,
    #[cfg(test)]
    pub generated_hook: Option<Box<dyn FnOnce() + Send + Sync>>,
    #[cfg(test)]
    pub before_access_hook: Option<Box<dyn FnOnce() + Send + Sync>>,
    #[cfg(test)]
    pub edit_hook: Option<Box<dyn FnOnce() + Send + Sync>>,
    #[cfg(test)]
    pub snapshot_hook: std::sync::Mutex<Option<Box<dyn FnOnce() + Send + Sync>>>,
}

impl<const N: usize> ChestState<N> {
    /// Caller owns one live read lock, or this is a detached pending decode.
    /// Never consult inventory, pending NBT or a second metadata/state version.
    pub(super) fn snapshot_for_clone(
        &self,
        kind: &'static str,
        position: pumpkin_util::math::position::BlockPos,
    ) -> Result<BlockEntityCloneData, BlockEntityLoadError> {
        self.metadata.validate_clone()?;
        if self.loot.key.is_none() {
            for item in &self.items {
                pumpkin_data::item_stack::admission::validate_runtime_item_stack(item)
                    .map_err(|error| BlockEntityLoadError::Unsupported(error.reason()))?;
            }
        }
        let mut custom = NbtCompound::new();
        self.write_nbt(&mut custom);
        custom.child_tags.remove("components");
        let data = BlockEntityCloneData {
            kind,
            custom,
            components: NbtCompound::new(),
        };
        // Literal names, admitted static item codecs and the empty component map
        // need no contextual registry lookup. Rich/contextual forms refuse above.
        validate_clone_nbt(kind, position, &data.full_metadata(position))?;
        pumpkin_world::nbt_io::encode_compound(data.custom.clone())
            .map_err(|_| BlockEntityLoadError::Unsupported("clone custom encoding"))?;
        pumpkin_world::nbt_io::encode_compound(data.components.clone())
            .map_err(|_| BlockEntityLoadError::Unsupported("clone component encoding"))?;
        Ok(data)
    }

    pub(super) fn from_clone(
        data: &BlockEntityCloneData,
        kind: &'static str,
        position: pumpkin_util::math::position::BlockPos,
    ) -> Result<Self, BlockEntityLoadError> {
        if data.kind != kind
            || !data.components.is_empty()
            || ["id", "x", "y", "z", "components"]
                .iter()
                .any(|key| data.custom.get(key).is_some())
        {
            return Err(BlockEntityLoadError::Unsupported(
                "clone payload identity/components",
            ));
        }
        let nbt = data.full_metadata(position);
        validate_clone_nbt(kind, position, &nbt)?;
        Ok(Self::from_nbt(&nbt))
    }
    /// Pure serializer: never accesses Inventory or unpacks deferred loot.
    pub fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.metadata.write_nbt(nbt);
        if let Some(key) = &self.loot.key {
            nbt.put_string("LootTable", key.clone());
            if self.loot.seed != 0 {
                nbt.put_long("LootTableSeed", self.loot.seed);
            }
        } else {
            nbt.put_list("Items", Vec::new());
            pumpkin_inventory::sync_write_items_to_nbt(&self.items, nbt);
        }
    }

    pub fn snapshot(
        &self,
        id: &str,
        position: pumpkin_util::math::position::BlockPos,
    ) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_string("id", id.to_owned());
        nbt.put_int("x", position.0.x);
        nbt.put_int("y", position.0.y);
        nbt.put_int("z", position.0.z);
        self.write_nbt(&mut nbt);
        nbt
    }

    pub fn from_nbt(nbt: &NbtCompound) -> Self {
        let (items, loot) = read_chest_nbt(nbt);
        Self {
            items,
            loot,
            metadata: ContainerMetadata::from_nbt(nbt),
            load_token: Arc::new(()),
            server: None,
            #[cfg(test)]
            generated_hook: None,
            #[cfg(test)]
            before_access_hook: None,
            #[cfg(test)]
            edit_hook: None,
            #[cfg(test)]
            snapshot_hook: std::sync::Mutex::new(None),
        }
    }
}

/// The key and seed must be reloaded, saved and consumed as one state.
pub(super) struct ChestLootState {
    pub key: Option<String>,
    pub seed: i64,
}

/// Decode into detached values before committing. The inventory reader only fills listed slots.
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
                Self {
                    position,
                    state: std::sync::RwLock::new(
                        $crate::block::entities::chest_like_block_entity::ChestState::from_nbt(nbt),
                    ),
                    dirty: std::sync::atomic::AtomicBool::new(false),
                    comparator_dirty: std::sync::atomic::AtomicBool::new(false),
                    viewers: $crate::block::viewer::ContainerViewerCount::new(),
                }
            }

            fn load_nbt(
                &self,
                nbt: &pumpkin_nbt::compound::NbtCompound,
            ) -> Result<(), $crate::block::entities::BlockEntityLoadError> {
                let (items, loot) =
                    $crate::block::entities::chest_like_block_entity::read_chest_nbt(nbt);
                let metadata =
                    $crate::block::entities::chest_like_block_entity::ContainerMetadata::from_nbt(
                        nbt,
                    );
                {
                    let mut state = self
                        .state
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    metadata.validate_edit(&state.metadata)?;
                    state.items = items;
                    state.loot = loot;
                    state.metadata = metadata;
                    // Identity avoids ABA even when the replacement key/seed match.
                    state.load_token = std::sync::Arc::new(());
                }
                <Self as pumpkin_inventory::Inventory>::mark_dirty(self);
                Ok(())
            }

            fn write_nbt(&self, nbt: &mut pumpkin_nbt::compound::NbtCompound) {
                self.state
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .write_nbt(nbt);
            }

            fn snapshot_for_clone(
                &self,
            ) -> Result<
                $crate::block::entities::BlockEntityCloneData,
                $crate::block::entities::BlockEntityLoadError,
            > {
                self.state
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .snapshot_for_clone(Self::ID, self.position)
            }

            fn restore_from_clone(
                &self,
                data: &$crate::block::entities::BlockEntityCloneData,
                publish: &mut dyn FnMut(pumpkin_nbt::compound::NbtCompound),
            ) -> Result<(), $crate::block::entities::BlockEntityLoadError> {
                let decoded = $crate::block::entities::chest_like_block_entity::ChestState::<
                    { Self::INVENTORY_SIZE },
                >::from_clone(data, Self::ID, self.position)?;
                let canonical = decoded.snapshot(Self::ID, self.position);
                let mut state = self
                    .state
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.items = decoded.items;
                state.loot = decoded.loot;
                state.metadata = decoded.metadata;
                state.load_token = std::sync::Arc::new(());
                <Self as pumpkin_inventory::Inventory>::mark_dirty(self);
                publish(canonical);
                Ok(())
            }

            fn snapshot_nbt(&self, publish: &mut dyn FnMut(pumpkin_nbt::compound::NbtCompound)) {
                let state = self
                    .state
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let nbt = state.snapshot(Self::ID, self.position);
                #[cfg(test)]
                {
                    let hook = state.snapshot_hook.lock().unwrap().take();
                    if let Some(hook) = hook {
                        hook();
                    }
                }
                publish(nbt);
            }

            fn edit_nbt(
                &self,
                edit: &mut dyn FnMut(
                    &mut pumpkin_nbt::compound::NbtCompound,
                ) -> $crate::block::entities::NbtEditDecision,
                publish: &mut dyn FnMut(pumpkin_nbt::compound::NbtCompound),
            ) -> Result<bool, $crate::block::entities::BlockEntityLoadError> {
                let mut state = self
                    .state
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let mut candidate = state.snapshot(Self::ID, self.position);
                #[cfg(test)]
                if let Some(hook) = state.edit_hook.take() {
                    hook();
                }
                if edit(&mut candidate) == $crate::block::entities::NbtEditDecision::Abort {
                    return Ok(false);
                }
                let (items, loot) =
                    $crate::block::entities::chest_like_block_entity::read_chest_nbt(&candidate);
                let metadata =
                    $crate::block::entities::chest_like_block_entity::ContainerMetadata::from_nbt(
                        &candidate,
                    );
                metadata.validate_edit(&state.metadata)?;
                state.items = items;
                state.loot = loot;
                state.metadata = metadata;
                state.load_token = std::sync::Arc::new(());
                <Self as pumpkin_inventory::Inventory>::mark_dirty(self);
                publish(state.snapshot(Self::ID, self.position));
                Ok(true)
            }

            fn container_custom_name(&self) -> Option<pumpkin_util::text::TextComponent> {
                self.state
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .metadata
                    .custom_name()
            }

            fn can_open_container(
                &self,
                main_hand: &pumpkin_data::item_stack::ItemStack,
                spectator: bool,
            ) -> bool {
                let state = self
                    .state
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if spectator {
                    state.loot.key.is_none()
                } else {
                    state.metadata.can_unlock(main_hand)
                }
            }

            fn bind_world(&self, world: &$crate::world::World) {
                self.state
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .server = Some(world.server.clone());
            }

            fn unpack_loot_table(&self, world: &Arc<$crate::world::World>) {
                <Self as $crate::block::entities::BlockEntity>::bind_world(self, world);
                self.ensure_unpacked();
            }

            fn tick(&self, world: &Arc<$crate::world::World>) {
                self.viewers.tick(self, world, &self.position);
            }

            fn get_inventory(self: Arc<Self>) -> Option<Arc<dyn pumpkin_inventory::Inventory>> {
                Some(self)
            }

            fn get_menu_inventory(
                self: Arc<Self>,
                world: &Arc<$crate::world::World>,
            ) -> Option<Arc<dyn pumpkin_inventory::Inventory>> {
                Some(
                    $crate::block::container_inventory::ContainerMenuInventory::new(
                        self,
                        world.clone(),
                    ),
                )
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
                if let Ok(state) = self.state.try_read()
                    && state.loot.key.is_none()
                {
                    pumpkin_inventory::sync_write_items_to_nbt(&state.items, &mut nbt);
                }
                Some(nbt)
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn has_loot_table(&self) -> bool {
                self.state
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .loot
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
        impl $crate::block::container_inventory::ViewedContainer for $struct_name {
            fn viewers(&self) -> &$crate::block::viewer::ContainerViewerCount {
                &self.viewers
            }
        }

        impl pumpkin_inventory::Inventory for $struct_name {
            fn size(&self) -> usize {
                Self::INVENTORY_SIZE
            }

            fn is_empty(&self) -> bool {
                self.unpacked_state().items.iter().all(ItemStack::is_empty)
            }

            fn get_stack(&self, slot: usize) -> ItemStack {
                self.unpacked_state().items[slot].clone()
            }

            fn remove_stack(&self, slot: usize) -> ItemStack {
                let removed = std::mem::replace(
                    &mut self.unpacked_state().items[slot],
                    ItemStack::EMPTY.clone(),
                );
                self.mark_dirty();
                removed
            }

            fn remove_stack_specific(&self, slot: usize, amount: u8) -> ItemStack {
                let res = {
                    let mut state = self.unpacked_state();
                    if !state.items[slot].is_empty() && amount > 0 {
                        state.items[slot].split(amount)
                    } else {
                        ItemStack::EMPTY.clone()
                    }
                };
                self.mark_dirty();
                res
            }

            fn set_stack(&self, slot: usize, stack: ItemStack) {
                self.unpacked_state().items[slot] = stack;
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
                self.state
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .items
                    .fill_with(|| ItemStack::EMPTY.clone());
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

/// Shared construction, runtime viewer access and deferred loot for containers.
/// Chest-specific sound positioning remains in `impl_chest_helper_methods`.
#[macro_export]
macro_rules! impl_container_helper_methods {
    ($struct_name:ty) => {
        impl $struct_name {
            /// Returns the number of players currently viewing this chest
            pub fn get_viewer_count(&self) -> u16 {
                self.viewers.get_viewer_count()
            }

            #[must_use]
            pub fn new(position: pumpkin_util::math::position::BlockPos) -> Self {
                use std::sync::RwLock;
                use std::sync::atomic::AtomicBool;

                Self {
                    position,
                    state: RwLock::new(
                        $crate::block::entities::chest_like_block_entity::ChestState::from_nbt(
                            &pumpkin_nbt::compound::NbtCompound::new(),
                        ),
                    ),
                    dirty: AtomicBool::new(false),
                    comparator_dirty: AtomicBool::new(false),
                    viewers: $crate::block::viewer::ContainerViewerCount::new(),
                }
            }

            /// Generate without holding chest state or calling back into this inventory.
            /// A reload invalidates the detached result, including identical key/seed loads.
            #[allow(
                clippy::panic,
                reason = "Inventory cannot return an unbound-context error; this macro expansion does not fulfill expect."
            )]
            fn ensure_unpacked(&self) {
                loop {
                    let (token, key, seed, server) = {
                        let state = self
                            .state
                            .read()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let Some(key) = &state.loot.key else {
                            return;
                        };
                        (
                            state.load_token.clone(),
                            key.clone(),
                            state.loot.seed,
                            state.server.clone(),
                        )
                    };
                    // Inventory has no error return. Refuse unattached pending access
                    // before accepting an item that Java-compatible saving would omit.
                    let server = server.unwrap_or_else(|| {
                        panic!("pending chest loot requires a bound world before inventory access")
                    });
                    let detached: std::sync::Arc<dyn pumpkin_inventory::Inventory> =
                        std::sync::Arc::new(pumpkin_inventory::SimpleInventory::new(
                            Self::INVENTORY_SIZE,
                        ));
                    if let Some(table) =
                        $crate::world::loot::get_loot_table_for_server(&server, &key)
                    {
                        $crate::world::loot::fill_chest_inventory_handle(&detached, &table, seed);
                    }
                    // Vanilla's registry resolves a missing table to the empty table.
                    let items = std::array::from_fn(|slot| detached.get_stack(slot));

                    #[cfg(test)]
                    {
                        let hook = self.state.write().unwrap().generated_hook.take();
                        if let Some(hook) = hook {
                            hook(); // Real generation has finished; no state lock is held.
                        }
                    }

                    let committed = {
                        let mut state = self
                            .state
                            .write()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if std::sync::Arc::ptr_eq(&token, &state.load_token)
                            && state.loot.key.is_some()
                        {
                            state.items = items;
                            state.loot.key = None;
                            true
                        } else {
                            false
                        }
                    };
                    if committed {
                        <Self as pumpkin_inventory::Inventory>::mark_dirty(self);
                        return;
                    }
                }
            }

            /// The pending check and slot access share a lock. A load can install
            /// new loot after `ensure_unpacked` returns, in which case retry first.
            fn unpacked_state(
                &self,
            ) -> std::sync::RwLockWriteGuard<
                '_,
                $crate::block::entities::chest_like_block_entity::ChestState<
                    { Self::INVENTORY_SIZE },
                >,
            > {
                loop {
                    self.ensure_unpacked();
                    #[cfg(test)]
                    {
                        let hook = self.state.write().unwrap().before_access_hook.take();
                        if let Some(hook) = hook {
                            hook();
                        }
                    }
                    let state = self
                        .state
                        .write()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if state.loot.key.is_none() {
                        return state;
                    }
                }
            }

            #[cfg(test)]
            pub(super) fn set_loot_generated_hook(
                &self,
                hook: impl FnOnce() + Send + Sync + 'static,
            ) {
                self.state.write().unwrap().generated_hook = Some(Box::new(hook));
            }

            #[cfg(test)]
            pub(super) fn set_before_access_hook(
                &self,
                hook: impl FnOnce() + Send + Sync + 'static,
            ) {
                self.state.write().unwrap().before_access_hook = Some(Box::new(hook));
            }

            #[cfg(test)]
            pub(crate) fn set_edit_hook(&self, hook: impl FnOnce() + Send + Sync + 'static) {
                self.state.write().unwrap().edit_hook = Some(Box::new(hook));
            }

            #[cfg(test)]
            pub(crate) fn set_snapshot_hook(&self, hook: impl FnOnce() + Send + Sync + 'static) {
                *self.state.read().unwrap().snapshot_hook.lock().unwrap() = Some(Box::new(hook));
            }

            #[cfg(test)]
            pub(crate) fn load_token(&self) -> std::sync::Arc<()> {
                self.state.read().unwrap().load_token.clone()
            }

            #[cfg(test)]
            pub(crate) fn state_is_locked(&self) -> bool {
                matches!(
                    self.state.try_write(),
                    Err(std::sync::TryLockError::WouldBlock)
                )
            }
        }
    };
}

#[macro_export]
macro_rules! impl_chest_helper_methods {
    ($struct_name:ty) => {
        $crate::impl_container_helper_methods!($struct_name);
        impl $struct_name {
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
