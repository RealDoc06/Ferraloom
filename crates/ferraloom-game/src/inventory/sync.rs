use super::{ItemStack, Result, matches};
use ferraloom_protocol::{codec::VarInt, component::HashedStack, play::clientbound as cb};

/// Port of RemoteSlot, including the placeholder before a synchronizer is installed.
#[derive(Clone, Debug, Default)]
pub enum RemoteSlot {
    #[default]
    Placeholder,
    Unknown,
    Stack(ItemStack),
    Hash(HashedStack),
}
impl RemoteSlot {
    pub(super) fn force_owned(&mut self, stack: ItemStack) {
        if !matches!(self, Self::Placeholder) {
            *self = Self::Stack(stack);
        }
    }
    pub(super) fn matches_readonly(&self, stack: &ItemStack) -> Result<bool> {
        match self {
            Self::Placeholder => Ok(true),
            Self::Unknown => Ok(false),
            Self::Stack(remote) => Ok(matches(remote, stack)),
            Self::Hash(hash) => Ok(hash.matches(stack)?),
        }
    }
    pub fn force(&mut self, stack: &ItemStack) {
        if !matches!(self, Self::Placeholder) {
            *self = Self::Stack(stack.clone());
        }
    }
    pub fn receive(&mut self, hash: HashedStack) {
        if !matches!(self, Self::Placeholder) {
            *self = Self::Hash(hash);
        }
    }
    pub fn matches(&mut self, stack: &ItemStack) -> Result<bool> {
        match self {
            Self::Placeholder => Ok(true),
            Self::Unknown => Ok(false),
            Self::Stack(remote) => Ok(matches(remote, stack)),
            Self::Hash(hash) => {
                let equal = hash.matches(stack)?;
                if equal {
                    *self = Self::Stack(stack.clone());
                }
                Ok(equal)
            }
        }
    }
}

/// Owned commands emitted at vanilla's synchronizer call sites, in call order.
#[derive(Clone, Debug, PartialEq)]
pub enum ContainerUpdate {
    RecipeBookAdd(cb::RecipeBookAdd),
    Initial {
        container_id: i32,
        state_id: i32,
        items: Vec<ItemStack>,
        carried: ItemStack,
        data: Vec<i32>,
    },
    Slot {
        container_id: i32,
        state_id: i32,
        slot: usize,
        stack: ItemStack,
    },
    Carried(ItemStack),
    Data {
        container_id: i32,
        id: usize,
        value: i32,
    },
}
impl ContainerUpdate {
    pub fn into_packets(self) -> Vec<cb::Packet> {
        match self {
            Self::RecipeBookAdd(packet) => vec![cb::Packet::RecipeBookAdd(packet)],
            Self::Initial {
                container_id,
                state_id,
                items,
                carried,
                data,
            } => {
                let mut packets = vec![cb::Packet::ContainerSetContent(cb::ContainerSetContent {
                    container_id: VarInt(container_id),
                    state_id: VarInt(state_id),
                    items,
                    carried_item: carried,
                })];
                packets.extend(data.into_iter().enumerate().map(|(id, value)| {
                    cb::Packet::ContainerSetData(cb::ContainerSetData {
                        container_id: VarInt(container_id),
                        id: id as i16,
                        value: value as i16,
                    })
                }));
                packets
            }
            Self::Slot {
                container_id,
                state_id,
                slot,
                stack,
            } => vec![cb::Packet::ContainerSetSlot(cb::ContainerSetSlot {
                container_id: VarInt(container_id),
                state_id: VarInt(state_id),
                slot: slot as i16,
                item_stack: stack,
            })],
            Self::Carried(contents) => {
                vec![cb::Packet::SetCursorItem(cb::SetCursorItem { contents })]
            }
            Self::Data {
                container_id,
                id,
                value,
            } => vec![cb::Packet::ContainerSetData(cb::ContainerSetData {
                container_id: VarInt(container_id),
                id: id as i16,
                value: value as i16,
            })],
        }
    }
}

/// Port of ContainerListener notifications, independent of remote suppression.
#[derive(Clone, Debug, PartialEq)]
pub enum ContainerChange {
    Slot(usize, ItemStack),
    Data(usize, i32),
}
