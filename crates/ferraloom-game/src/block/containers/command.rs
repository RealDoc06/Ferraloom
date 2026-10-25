//! CommandBlockEntity data and placement initialization. Command execution stays
//! in the required CommandBlock tick virtual until that command runtime lands.
use crate::{
    block::{
        LevelReaderRef,
        entity::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState},
        placement::StackView,
    },
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    chat::Component,
    component::{ComponentTypeId, DataComponent, DataComponentMap},
};
use ferraloom_util::BlockPos;

/// Port of BaseCommandBlock's persisted command receiver state.
#[derive(Clone, Debug)]
pub struct BaseCommandBlock {
    pub command: String,
    pub success_count: i32,
    pub custom_name: Option<Component>,
    pub track_output: bool,
    pub last_output: Option<Component>,
    pub update_last_execution: bool,
    pub last_execution: i64,
}
impl Default for BaseCommandBlock {
    fn default() -> Self {
        Self {
            command: String::new(),
            success_count: 0,
            custom_name: None,
            track_output: true,
            last_output: None,
            update_last_execution: true,
            last_execution: -1,
        }
    }
}
impl BaseCommandBlock {
    fn load(&mut self, tag: &CompoundTag) {
        self.command = tag
            .get_string("Command")
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        self.success_count = tag.get_int_or("SuccessCount", 0);
        self.custom_name = tag
            .get("CustomName")
            .and_then(|v| Component::from_nbt(v.clone()).ok());
        self.track_output = tag.get_boolean_or("TrackOutput", true);
        self.last_output = if self.track_output {
            tag.get("LastOutput")
                .and_then(|v| Component::from_nbt(v.clone()).ok())
        } else {
            None
        };
        self.update_last_execution = tag.get_boolean_or("UpdateLastExecution", true);
        self.last_execution = if self.update_last_execution {
            tag.get_long_or("LastExecution", -1)
        } else {
            -1
        };
    }
    fn save(&self) -> ferraloom_protocol::Result<CompoundTag> {
        let mut tag = CompoundTag::new();
        tag.insert("Command", Tag::String(self.command.as_str().into()));
        tag.insert("SuccessCount", Tag::Int(self.success_count));
        if let Some(name) = &self.custom_name {
            tag.insert("CustomName", name.to_nbt()?);
        }
        tag.insert("TrackOutput", Tag::Byte(i8::from(self.track_output)));
        if self.track_output
            && let Some(output) = &self.last_output
        {
            tag.insert("LastOutput", output.to_nbt()?);
        }
        tag.insert(
            "UpdateLastExecution",
            Tag::Byte(i8::from(self.update_last_execution)),
        );
        if self.update_last_execution && self.last_execution != -1 {
            tag.insert("LastExecution", Tag::Long(self.last_execution));
        }
        Ok(tag)
    }
}
/// Port of CommandBlockEntity's canonical receiver data.
#[derive(Clone, Debug)]
pub struct CommandBlockEntity {
    pub parent: BlockEntityData,
    pub command_block: BaseCommandBlock,
    pub powered: bool,
    pub automatic: bool,
    pub condition_met: bool,
}
impl CommandBlockEntity {
    /// Port of CommandBlock#newBlockEntity: setAutomatic runs before setLevel,
    /// so construction stores the extracted default without scheduling a tick.
    pub fn new(parent: BlockEntityData) -> Self {
        let automatic = parent
            .block_state
            .block()
            .command_block_automatic()
            .expect("command constructor fact");
        Self {
            parent,
            command_block: BaseCommandBlock::default(),
            powered: false,
            automatic,
            condition_met: false,
        }
    }
    pub fn is_sequence(&self) -> bool {
        self.parent.block_state.block() == Block::CHAIN_COMMAND_BLOCK
    }
}
/// Port of CommandBlockEntity#markConditionMet, preserving the initial write and
/// live state/neighbor receiver reads before the final condition write.
fn mark_condition_met(handle: BlockEntityRef, level: &mut Level) -> Result<bool, LevelError> {
    level
        .block_entity_mut::<CommandBlockEntity>(handle)?
        .condition_met = true;
    let state = level.get_block_state(handle.pos)?;
    if state.block().class() != BlockClass::CommandBlock || state.get(p::CONDITIONAL) != Some(true)
    {
        return Ok(true);
    }
    let state = level.get_block_state(handle.pos)?;
    let neighbor = handle
        .pos
        .relative(super::menu::facing(state).opposite(), 1);
    let success = if level.get_block_state(neighbor)?.block().class() == BlockClass::CommandBlock {
        match level.block_entity_ref(neighbor)? {
            Some(other) => match level.block_entity_state(other)? {
                BlockEntityState::Command(command) => command.command_block.success_count > 0,
                _ => false,
            },
            None => false,
        }
    } else {
        false
    };
    level
        .block_entity_mut::<CommandBlockEntity>(handle)?
        .condition_met = success;
    Ok(success)
}
/// Port of CommandBlockEntity#setAutomatic and its private scheduleTick.
fn set_automatic(
    handle: BlockEntityRef,
    automatic: bool,
    previous: bool,
    level: &mut Level,
) -> Result<(), LevelError> {
    level
        .block_entity_mut::<CommandBlockEntity>(handle)?
        .automatic = automatic;
    let entity = level.block_entity::<CommandBlockEntity>(handle)?;
    let schedule = !previous
        && automatic
        && !entity.powered
        && entity.parent.has_level
        && !entity.is_sequence();
    let block = entity.parent.block_state.block();
    if schedule && block.class() == BlockClass::CommandBlock {
        mark_condition_met(handle, level)?;
        level.schedule_block_tick_after(handle.pos, block, 1)?;
    }
    Ok(())
}
/// Port of CommandBlock#setPlacedBy. Only the captured ItemStack's data-component
/// presence suppresses constructor/rule defaults; then power is sampled live.
pub(super) fn set_placed_by(
    state: BlockState,
    pos: BlockPos,
    stack: StackView<'_>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let Some(handle) = level.block_entity_ref(pos)? else {
        return Ok(());
    };
    if !matches!(
        level.block_entity_state(handle)?,
        BlockEntityState::Command(_)
    ) {
        return Ok(());
    }
    level.with_block_entity_callback(handle, |level| {
        let has_data = {
            let reader = LevelReaderRef::Server(level);
            stack
                .read(&reader)?
                .component(ComponentTypeId::BLOCK_ENTITY_DATA)
                .map_err(|_| LevelError::Inventory("command placement component"))?
                .is_some()
        };
        if !has_data {
            let feedback = level.rules().send_command_feedback;
            level
                .block_entity_mut::<CommandBlockEntity>(handle)?
                .command_block
                .track_output = feedback;
            let previous = level.block_entity::<CommandBlockEntity>(handle)?.automatic;
            set_automatic(
                handle,
                state
                    .block()
                    .command_block_automatic()
                    .expect("command constructor fact"),
                previous,
                level,
            )?;
        }
        let powered = LevelReaderRef::Server(level).has_neighbor_signal(pos)?;
        let entity = level.block_entity_mut::<CommandBlockEntity>(handle)?;
        if powered == entity.powered {
            return Ok(());
        }
        entity.powered = powered;
        if powered && !entity.automatic && !entity.is_sequence() {
            mark_condition_met(handle, level)?;
            level.schedule_block_tick_after(pos, state.block(), 1)?;
        }
        Ok(())
    })
}
pub struct CommandBehavior;
pub static COMMAND_ENTITY: CommandBehavior = CommandBehavior;
impl BlockEntity for CommandBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Command(entity) = state else {
            unreachable!()
        };
        entity.command_block.load(tag);
        entity.powered = tag.get_boolean_or("powered", false);
        entity.condition_met = tag.get_boolean_or("conditionMet", false);
        entity.automatic = tag.get_boolean_or("auto", false);
    }
    fn load_additional_live(
        &self,
        handle: BlockEntityRef,
        tag: &CompoundTag,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let previous = level.block_entity::<CommandBlockEntity>(handle)?.automatic;
        let context = level.context().clone();
        self.load_additional(
            level.block_entity_state_mut(handle)?,
            tag,
            context.registry_context()?,
        );
        let automatic = level.block_entity::<CommandBlockEntity>(handle)?.automatic;
        set_automatic(handle, automatic, previous, level)
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Command(entity) = state else {
            unreachable!()
        };
        let mut tag = entity.command_block.save()?;
        tag.insert("powered", Tag::Byte(i8::from(entity.powered)));
        tag.insert("conditionMet", Tag::Byte(i8::from(entity.condition_met)));
        tag.insert("auto", Tag::Byte(i8::from(entity.automatic)));
        Ok(tag)
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::Command(entity) = state else {
            unreachable!()
        };
        entity.command_block.custom_name = match map.get(ComponentTypeId::CUSTOM_NAME) {
            Some(DataComponent::CustomName(name)) => Some((**name).clone()),
            _ => None,
        };
        &[ComponentTypeId::CUSTOM_NAME]
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::Command(entity) = state else {
            unreachable!()
        };
        if let Some(name) = &entity.command_block.custom_name {
            super::storage::set(map, DataComponent::CustomName(Box::new(name.clone())));
        }
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        for key in ["CustomName", "conditionMet", "powered"] {
            tag.remove(key);
        }
    }
}
