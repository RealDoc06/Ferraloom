//! Captured workstation world access. Dimension identity is retained; the Level
//! is resolved by the caller for each short, synchronous evaluation.
use super::{AbstractContainerMenu, MenuError, MenuKind, Result};
use crate::{entity::EntityRef, level::Level};
use ferraloom_data::{Block, tags::BlockTags};
use ferraloom_util::BlockPos;
use ferraloom_worldgen::Identifier;

/// Port of ContainerLevelAccess#create. An unbound menu stores None, matching
/// ContainerLevelAccess.NULL; a different dimension also evaluates as absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerLevelAccess {
    pub dimension: Identifier,
    pub pos: BlockPos,
}
impl ContainerLevelAccess {
    pub fn create(level: &Level, pos: BlockPos) -> Result<Self> {
        Ok(Self {
            dimension: level.dimension_key()?.clone(),
            pos,
        })
    }
    /// Port of ContainerLevelAccess#evaluate. Absence leaves selection of the
    /// native default to the concrete menu; errors in an actual query propagate.
    pub fn evaluate<R>(
        &self,
        level: &Level,
        operation: impl FnOnce(&Level, BlockPos) -> Result<R>,
    ) -> Result<Option<R>> {
        if level.dimension_key()? != &self.dimension {
            return Ok(None);
        }
        operation(level, self.pos).map(Some)
    }
    /// Mutable evaluations end every canonical borrow before world callbacks.
    pub fn evaluate_mut<R>(
        &self,
        level: &mut Level,
        operation: impl FnOnce(&mut Level, BlockPos) -> Result<R>,
    ) -> Result<Option<R>> {
        if level.dimension_key()? != &self.dimension {
            return Ok(None);
        }
        operation(level, self.pos).map(Some)
    }
    /// Port of ContainerLevelAccess#execute: an unavailable captured dimension
    /// performs no action, rather than substituting the supplied current Level.
    pub fn execute(
        &self,
        level: &mut Level,
        operation: impl FnOnce(&mut Level, BlockPos) -> Result<()>,
    ) -> Result<()> {
        self.evaluate_mut(level, operation)?;
        Ok(())
    }
}

impl MenuKind {
    pub(super) fn is_workstation(self) -> bool {
        matches!(
            self,
            Self::Crafting
                | Self::Anvil
                | Self::Loom
                | Self::Stonecutter
                | Self::Grindstone
                | Self::Smithing
                | Self::Cartography
        )
    }
    fn workstation_valid_block(self, level: &Level, pos: BlockPos) -> Result<bool> {
        let state = level.get_block_state(pos)?;
        Ok(match self {
            Self::Crafting => state.block() == Block::CRAFTING_TABLE,
            Self::Anvil => level
                .block_tags()?
                .contains_key(BlockTags::ANVIL, state.block()),
            Self::Loom => state.block() == Block::LOOM,
            Self::Stonecutter => state.block() == Block::STONECUTTER,
            Self::Grindstone => state.block() == Block::GRINDSTONE,
            Self::Smithing => state.block() == Block::SMITHING_TABLE,
            Self::Cartography => state.block() == Block::CARTOGRAPHY_TABLE,
            _ => return Err(MenuError::Unsupported("workstation validity kind")),
        })
    }
}
impl AbstractContainerMenu {
    /// Port of workstation constructors taking ContainerLevelAccess.create.
    /// Uses the same slots and state algorithm as the unbound constructors.
    pub fn new_bound_workstation(
        kind: MenuKind,
        id: i32,
        pos: BlockPos,
        level: &Level,
    ) -> Result<Self> {
        if !kind.is_workstation() {
            return Err(MenuError::Unsupported("bound workstation menu kind"));
        }
        let access = ContainerLevelAccess::create(level, pos)?;
        let mut menu = Self::new(kind, id)?;
        menu.level_access = Some(access);
        Ok(menu)
    }
    /// Port of AbstractContainerMenu#stillValid and ItemCombinerMenu#stillValid.
    pub(super) fn workstation_still_valid(&self, player: EntityRef, level: &Level) -> Result<bool> {
        let Some(access) = &self.level_access else {
            return Ok(true);
        };
        Ok(access
            .evaluate(level, |level, pos| {
                if !self.kind.workstation_valid_block(level, pos)? {
                    return Ok(false);
                }
                Ok(level
                    .entity::<crate::entity::player::ServerPlayer>(player)?
                    .is_within_block_interaction_range(pos, 4.0))
            })?
            .unwrap_or(true))
    }
}
