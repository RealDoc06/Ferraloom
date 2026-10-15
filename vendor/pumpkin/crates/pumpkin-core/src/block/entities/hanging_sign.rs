use std::sync::{Arc, atomic::AtomicBool};

use super::{BlockEntity, BlockEntityLoadError};
use crate::block::entities::sign::{SignData, Text};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use std::sync::Mutex;

pub struct HangingSignBlockEntity {
    pub front_text: Text,
    pub back_text: Text,
    pub is_waxed: AtomicBool,
    position: BlockPos,
    pub currently_editing_player: Arc<Mutex<Option<uuid::Uuid>>>,
}

impl BlockEntity for HangingSignBlockEntity {
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
        let data = SignData::from_nbt(nbt);
        Self {
            position,
            front_text: data.front_text,
            back_text: data.back_text,
            is_waxed: AtomicBool::new(data.is_waxed),
            currently_editing_player: Arc::new(Mutex::new(None)),
        }
    }

    fn load_nbt(&self, nbt: &NbtCompound) -> Result<(), BlockEntityLoadError> {
        SignData::from_nbt(nbt).apply(&self.front_text, &self.back_text, &self.is_waxed);
        Ok(())
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        SignData::write_nbt(&self.front_text, &self.back_text, &self.is_waxed, nbt);
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.write_nbt(&mut nbt);
        Some(nbt)
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
            is_waxed: AtomicBool::new(false),
            front_text: if is_front {
                Text::from_messages(messages.clone())
            } else {
                Text::default()
            },
            back_text: if is_front {
                Text::default()
            } else {
                Text::from_messages(messages)
            },
            currently_editing_player: Arc::new(Mutex::new(None)),
        }
    }
    #[must_use]
    pub fn empty(position: BlockPos) -> Self {
        Self {
            position,
            is_waxed: AtomicBool::new(false),
            front_text: Text::default(),
            back_text: Text::default(),
            currently_editing_player: Arc::new(Mutex::new(None)),
        }
    }
}
