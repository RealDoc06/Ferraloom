//! Port of InteractionResult and BlockBehaviour's player-use call context.
use crate::entity::EntityRef;
use ferraloom_protocol::{
    component::ItemStack,
    play::{BlockHitResult, InteractionHand},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwingSource {
    None,
    Predicted,
    ServerOnly,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ItemContext {
    pub was_item_interaction: bool,
    pub held_item_transformed_to: Option<ItemStack>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum InteractionResult {
    Success {
        swing_source: SwingSource,
        item_context: ItemContext,
    },
    Fail,
    Pass,
    TryWithEmptyHand,
}
impl InteractionResult {
    pub const SUCCESS: Self = Self::Success {
        swing_source: SwingSource::Predicted,
        item_context: ItemContext {
            was_item_interaction: true,
            held_item_transformed_to: None,
        },
    };
    pub const SUCCESS_SERVER: Self = Self::Success {
        swing_source: SwingSource::ServerOnly,
        item_context: ItemContext {
            was_item_interaction: true,
            held_item_transformed_to: None,
        },
    };
    pub const CONSUME: Self = Self::Success {
        swing_source: SwingSource::None,
        item_context: ItemContext {
            was_item_interaction: true,
            held_item_transformed_to: None,
        },
    };
    pub fn consumes_action(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}
/// Item access goes through the player's current canonical hand at the call
/// site. The context never retains a stack borrowed across a world callback.
#[derive(Clone, Debug)]
pub struct UseContext {
    pub player: EntityRef,
    pub hand: InteractionHand,
    pub hit: BlockHitResult,
}
