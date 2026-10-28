//! JukeboxBlockEntity data and JukeboxSongPlayer's cold restore/stop boundaries.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::{
    entity::player::inventory::is_same_item_same_components,
    inventory::StackGenerations,
    level::{Level, LevelError},
};
use ferraloom_data::{properties as p, registries::game_event::Id as GameEvent};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    codec::{Holder, RegistryContext},
    component::{ComponentTypeId, DataComponent, ItemStack, JukeboxSong, Persistent},
};
use ferraloom_util::Vec3;

/// A bound immutable registry holder, decoded from the owning resource snapshot.
#[derive(Clone, Debug)]
pub struct BoundJukeboxSong {
    pub key: Identifier,
    pub value: JukeboxSong,
}
/// Port of JukeboxSongPlayer's canonical current song and elapsed counter.
#[derive(Clone, Debug, Default)]
pub struct JukeboxSongPlayer {
    pub song: Option<BoundJukeboxSong>,
    pub ticks_since_song_started: i64,
}
impl JukeboxSongPlayer {
    /// Port of JukeboxSongPlayer#setSongWithoutPlaying. A finished input leaves
    /// both existing fields intact; f32 multiplication precedes the ceil/cast.
    fn set_song_without_playing(&mut self, song: BoundJukeboxSong, ticks: i64) {
        let length =
            ferraloom_util::mth::ceil(f64::from(song.value.length_in_seconds.0 * 20.0_f32));
        if ticks < i64::from(length.wrapping_add(20)) {
            self.song = Some(song);
            self.ticks_since_song_started = ticks;
        }
    }
}
/// Port of JukeboxBlockEntity. ITEM replacements retain the original receiver;
/// insertion/ejection and playback ticking remain separate required behavior.
#[derive(Clone, Debug)]
pub struct JukeboxBlockEntity {
    pub parent: BlockEntityData,
    pub item: ItemStack,
    pub song_player: JukeboxSongPlayer,
    stack_generations: StackGenerations,
}
impl JukeboxBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            item: ItemStack::default(),
            song_player: JukeboxSongPlayer::default(),
            stack_generations: StackGenerations::new(1),
        }
    }
    fn replace_item(&mut self, item: ItemStack) {
        self.stack_generations.replace(0, &mut self.item, item);
    }
    pub fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        (slot == 0).then_some((&self.stack_generations, &self.item, 0))
    }
    pub fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        (slot == 0).then_some((&mut self.stack_generations, &mut self.item, 0))
    }
    pub fn has_server_ticker(&self) -> bool {
        self.parent.block_state.get(p::HAS_RECORD) == Some(true)
    }
    pub fn comparator_output(&self, registries: &dyn RegistryContext) -> i32 {
        song_from_stack(&self.item, registries).map_or(0, |song| song.value.comparator_output.0)
    }
    fn load_item_and_time(
        &mut self,
        item: ItemStack,
        tag: &CompoundTag,
        registries: &dyn RegistryContext,
    ) {
        self.replace_item(item);
        if let Some(ticks) = tag.get_long("ticks_since_song_started")
            && let Some(song) = song_from_stack(&self.item, registries)
        {
            self.song_player.set_song_without_playing(song, ticks);
        }
    }
}
fn song_registry() -> Identifier {
    Identifier::parse("minecraft:jukebox_song").unwrap()
}
fn song_from_stack(
    stack: &ItemStack,
    registries: &dyn RegistryContext,
) -> Option<BoundJukeboxSong> {
    let Some(DataComponent::JukeboxPlayable(song)) =
        stack.component(ComponentTypeId::JUKEBOX_PLAYABLE).ok()?
    else {
        return None;
    };
    let Holder::Reference(key) = &song.0 else {
        return None;
    };
    let value = registries.entry_value(&song_registry(), key)?;
    Some(BoundJukeboxSong {
        key: key.clone(),
        value: JukeboxSong::from_json(&value).ok()?,
    })
}
/// TagValueInput keeps the component patch's successful partial entries. An
/// unbound song override is omitted, preserving the item's native prototype.
fn load_item(tag: &CompoundTag, registries: &dyn RegistryContext) -> ItemStack {
    let mut item = tag
        .get("RecordItem")
        .and_then(|tag| ItemStack::from_nbt_with_partial(tag).ok())
        .unwrap_or_default();
    if let Some(payload) = &mut item.0 {
        payload
            .components
            .added
            .retain(|component| match component {
                DataComponent::JukeboxPlayable(song) => match &song.0 {
                    Holder::Reference(key) => registries.entry_id(&song_registry(), key).is_some(),
                    _ => false,
                },
                _ => true,
            });
    }
    item
}
fn center(handle: BlockEntityRef) -> Vec3 {
    Vec3::new(
        f64::from(handle.pos.x) + 0.5,
        f64::from(handle.pos.y) + 0.5,
        f64::from(handle.pos.z) + 0.5,
    )
}
/// Port of JukeboxSongPlayer#stop and JukeboxBlockEntity#onSongChanged, with each
/// callback resolving the original generation after the previous one returns.
fn stop(handle: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
    let entity = level.block_entity_mut::<JukeboxBlockEntity>(handle)?;
    if entity.song_player.song.is_none() {
        return Ok(());
    }
    entity.song_player.song = None;
    entity.song_player.ticks_since_song_started = 0;
    let state = entity.parent.block_state;
    level.game_event(
        GameEvent::JUKEBOX_STOP_PLAY,
        center(handle),
        None,
        Some(state),
    )?;
    level.level_event(None, 1011, handle.pos, 0)?;
    let block = level.block_entity_state(handle)?.data().block_state.block();
    level.update_neighbors_at(handle.pos, block)?;
    handle.set_changed(level)
}
pub(super) struct JukeboxBehavior;
pub(super) static JUKEBOX_ENTITY: JukeboxBehavior = JukeboxBehavior;
impl BlockEntity for JukeboxBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        registries: &dyn RegistryContext,
    ) {
        let BlockEntityState::Jukebox(entity) = state else {
            unreachable!()
        };
        let item = load_item(tag, registries);
        if !entity.item.is_empty()
            && !is_same_item_same_components(&item, &entity.item)
            && entity.song_player.song.is_some()
        {
            entity.song_player.song = None;
            entity.song_player.ticks_since_song_started = 0;
        }
        entity.load_item_and_time(item, tag, registries);
    }
    fn load_additional_live(
        &self,
        handle: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let context = level.context().clone();
        let registries = context.registry_context()?;
        let item = load_item(tag, registries);
        let old = level.block_entity::<JukeboxBlockEntity>(handle)?;
        if !old.item.is_empty() && !is_same_item_same_components(&item, &old.item) {
            stop(handle, level)?;
        }
        level
            .block_entity_mut::<JukeboxBlockEntity>(handle)?
            .load_item_and_time(item, tag, registries);
        Ok(())
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Jukebox(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if !entity.item.is_empty() {
            tag.insert("RecordItem", entity.item.to_nbt()?);
        }
        if entity.song_player.song.is_some() {
            tag.insert(
                "ticks_since_song_started",
                Tag::Long(entity.song_player.ticks_since_song_started),
            );
        }
        Ok(tag)
    }
    fn tick(&self, handle: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        if level
            .block_entity::<JukeboxBlockEntity>(handle)?
            .song_player
            .song
            .is_some()
        {
            return Err(LevelError::UnsupportedBlockEntity(
                ferraloom_data::Block::JUKEBOX,
            ));
        }
        Ok(())
    }
    /// Native setRemoved sets the flag first, then emits both notifications even
    /// without a current song. This is distinct from conditional playback stop.
    fn set_removed_live(
        &self,
        handle: BlockEntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let data = level.block_entity_mut::<BlockEntityData>(handle)?;
        data.set_removed();
        let state = data.block_state;
        level.game_event(
            GameEvent::JUKEBOX_STOP_PLAY,
            center(handle),
            None,
            Some(state),
        )?;
        level.level_event(None, 1011, handle.pos, 0)
    }
}
