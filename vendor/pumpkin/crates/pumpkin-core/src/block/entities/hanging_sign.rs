use std::sync::Arc;

use super::{BlockEntity, BlockEntityLoadError, NbtEditDecision};
use crate::block::entities::sign::{SignState, Text};
use crate::world::World;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;

pub struct HangingSignBlockEntity {
    pub(super) state: SignState,
    position: BlockPos,
}

impl BlockEntity for HangingSignBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }
    fn get_position(&self) -> BlockPos {
        self.position
    }
    fn from_nbt(nbt: &NbtCompound, position: BlockPos) -> Self {
        Self {
            position,
            state: SignState::from_nbt(nbt),
        }
    }
    fn load_nbt(&self, nbt: &NbtCompound) -> Result<(), BlockEntityLoadError> {
        self.state.load_nbt(nbt)
    }
    fn edit_nbt(
        &self,
        edit: &mut dyn FnMut(&mut NbtCompound) -> NbtEditDecision,
        publish: &mut dyn FnMut(NbtCompound),
    ) -> Result<bool, BlockEntityLoadError> {
        self.state.edit_nbt(Self::ID, self.position, edit, publish)
    }
    fn snapshot_nbt(&self, publish: &mut dyn FnMut(NbtCompound)) {
        self.state.snapshot_nbt(Self::ID, self.position, publish);
    }
    fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.state.write_nbt(nbt, false);
    }
    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.state.write_nbt(&mut nbt, true);
        Some(nbt)
    }
    fn tick(&self, world: &Arc<World>) {
        self.state.tick(world, &self.position);
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl HangingSignBlockEntity {
    pub const ID: &'static str = "minecraft:hanging_sign";
    #[must_use]
    pub fn new(position: BlockPos, is_front: bool, messages: [Box<str>; 4]) -> Self {
        Self {
            position,
            state: SignState::new(is_front, messages),
        }
    }
    #[must_use]
    pub fn empty(position: BlockPos) -> Self {
        Self::new(position, true, Text::empty_messages())
    }
}
