//! Shared placement queries. DirectionalPlaceContext is used by falling blocks
//! and dispensers; required answers never use Warn's unit-effect skip policy.
use super::{BlockMethod, LevelReaderRef};
use crate::level::{Level, LevelError};
use ferraloom_data::BlockState;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{BlockPos, Direction};

/// Port of DirectionalPlaceContext. The inherited BlockPlaceContext constructor
/// queries replacement before the subclass direction field is initialized.
pub struct DirectionalPlaceContext<'stack> {
    pos: BlockPos,
    direction: Option<Direction>,
    held_item: &'stack ItemStack,
    clicked_face: Direction,
}
impl<'stack> DirectionalPlaceContext<'stack> {
    pub fn new(
        reader: &LevelReaderRef<'_, '_>,
        pos: BlockPos,
        direction: Direction,
        held_item: &'stack ItemStack,
        clicked_face: Direction,
    ) -> Result<Self, LevelError> {
        let mut context = Self {
            pos,
            direction: None,
            held_item,
            clicked_face,
        };
        // replaceClicked is not subsequently observed by DirectionalPlaceContext,
        // whose clicked-pos, canPlace and replacingClickedOnBlock all override it.
        let state = reader.get_block_state(pos)?;
        reader.can_be_replaced(state, &context)?;
        context.direction = Some(direction);
        Ok(context)
    }
    pub fn clicked_pos(&self) -> BlockPos {
        self.pos
    }
    pub fn item_in_hand(&self) -> &ItemStack {
        self.held_item
    }
    pub fn clicked_face(&self) -> Direction {
        self.clicked_face
    }
    /// None is observable only during the inherited constructor's replacement
    /// query, matching Java's not-yet-initialized subclass field.
    pub fn direction(&self) -> Option<Direction> {
        self.direction
    }
    pub fn nearest_looking_direction(&self) -> Direction {
        Direction::Down
    }
    pub fn is_secondary_use_active(&self) -> bool {
        false
    }
    pub fn can_place(&self, reader: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
        reader.can_be_replaced(reader.get_block_state(self.pos)?, self)
    }
    pub fn replacing_clicked_on_block(
        &self,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        self.can_place(reader)
    }
}
impl LevelReaderRef<'_, '_> {
    pub fn can_be_replaced(
        &self,
        state: BlockState,
        context: &DirectionalPlaceContext<'_>,
    ) -> Result<bool, LevelError> {
        let behavior = self.required_behavior(state.block(), BlockMethod::CanBeReplaced)?;
        behavior.can_be_replaced(state, context, self)
    }
    pub fn can_survive(&self, state: BlockState, pos: BlockPos) -> Result<bool, LevelError> {
        let behavior = self.required_behavior(state.block(), BlockMethod::CanSurvive)?;
        behavior.can_survive(state, pos, self)
    }
}
impl Level {
    pub fn can_be_replaced(
        &self,
        state: BlockState,
        context: &DirectionalPlaceContext<'_>,
    ) -> Result<bool, LevelError> {
        LevelReaderRef::Server(self).can_be_replaced(state, context)
    }
    pub fn can_survive(&self, state: BlockState, pos: BlockPos) -> Result<bool, LevelError> {
        LevelReaderRef::Server(self).can_survive(state, pos)
    }
    /// Port of Level#setBlockAndUpdate.
    pub fn set_block_and_update(
        &mut self,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<bool, LevelError> {
        self.set_block(pos, state, super::UpdateFlags::ALL, super::UPDATE_LIMIT)
    }
}
