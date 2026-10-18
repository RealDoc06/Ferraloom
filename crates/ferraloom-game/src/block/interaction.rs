//! Port of InteractionResult and BlockBehaviour's player-use call context.
use crate::entity::EntityRef;
use crate::{
    advancements::criteria,
    entity::player::ServerPlayer,
    inventory::{ContainerHandle, ContainerSlot, MenuError, StackOperation, StackRef, StackValue},
    level::{Level, LevelError},
};
use ferraloom_protocol::play::{BlockHitResult, InteractionHand};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwingSource {
    None,
    Predicted,
    ServerOnly,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ItemContext {
    pub was_item_interaction: bool,
    /// Identity is valid only in the caller's complete-use StackOperation.
    /// Cloning a result copies this handle, never the mutable ItemStack object.
    pub held_item_transformed_to: Option<StackRef>,
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

/// Port of ServerPlayerGameMode#useItemOn. None records an approved whole item
/// use effect skipped under Warn; it is not a fabricated InteractionResult.
/// The caller keeps this operation alive through criteria, hand installation
/// and the final result read. Transformed stack handles must not escape it.
pub fn use_item_on(
    context: &UseContext,
    operation: &mut StackOperation<'_>,
) -> Result<Option<InteractionResult>, LevelError> {
    use super::{BlockMethod, UnportedOperation};
    use crate::entity::player::{
        GameType,
        inventory::{count, set_count},
    };
    use ferraloom_data::BlockClass;
    let receiver = operation
        .pin(hand_slot(operation.level, context.player, context.hand)?)
        .map_err(item_receiver_error)?;
    let pos = context.hit.block_pos;
    let state = operation.level.get_block_state(pos)?;
    // The dedicated server currently publishes exactly FeatureFlags.VANILLA_SET.
    if !state.block().is_enabled_in_vanilla() {
        return Ok(Some(InteractionResult::Fail));
    }
    let player = operation.level.entity::<ServerPlayer>(context.player)?;
    if player.game_mode == GameType::Spectator {
        if let Some(provider) = operation
            .level
            .required_block_behavior(state.block(), BlockMethod::GetMenuProvider)?
            .get_menu_provider(state, pos, operation.level)?
        {
            crate::inventory::open_menu_live(context.player, provider.as_ref(), operation.level)
                .map_err(|error| match error {
                    crate::inventory::MenuError::Level(error) => error,
                    _ => LevelError::Inventory("spectator menu"),
                })?;
            return Ok(Some(InteractionResult::CONSUME));
        }
        if matches!(
            state.block().class(),
            BlockClass::NetherPortalBlock
                | BlockClass::EndPortalBlock
                | BlockClass::EndGatewayBlock
        ) {
            operation
                .level
                .unported_operation(UnportedOperation::PortalUse(state.block()))?;
            return Ok(Some(InteractionResult::CONSUME));
        }
        return Ok(Some(InteractionResult::Pass));
    }
    let suppress = player.is_secondary_use_active()
        && (!player.get_main_hand_item().is_empty() || !player.get_offhand_item().is_empty());
    let used_stack = operation
        .level
        .stack(receiver)
        .map_err(item_receiver_error)?;
    let used_stack = used_stack.copy_with_count(count(used_stack));
    if !suppress {
        let result = operation
            .level
            .required_block_behavior(state.block(), BlockMethod::UseItemOn)?
            .use_item_on(state, pos, context, operation.level)?;
        if result.consumes_action() {
            criteria::item_used_on_block(operation.level, context.player, pos, &used_stack)?;
            return Ok(Some(result));
        }
        if result == InteractionResult::TryWithEmptyHand
            && context.hand == InteractionHand::MainHand
        {
            let result = operation
                .level
                .required_block_behavior(state.block(), BlockMethod::UseWithoutItem)?
                .use_without_item(state, pos, context, operation.level)?;
            if result.consumes_action() {
                criteria::default_block_use(operation.level, context.player, pos)?;
                return Ok(Some(result));
            }
        }
    }
    let player = operation.level.entity::<ServerPlayer>(context.player)?;
    let stack = operation
        .level
        .stack(receiver)
        .map_err(item_receiver_error)?;
    if stack.is_empty() || player.cooldowns.is_on_cooldown(stack) {
        return Ok(Some(InteractionResult::Pass));
    }
    let infinite_materials = player.instabuild();
    let original_count = count(stack);
    let held = operation
        .pin(hand_slot(operation.level, context.player, context.hand)?)
        .map_err(item_receiver_error)?;
    let use_on = super::UseOnContext::new_retained(
        Some(context.player),
        context.hand,
        held,
        super::PlacementHit {
            pos,
            face: context.hit.direction,
            location: ferraloom_util::Vec3::new(
                f64::from(pos.x) + f64::from(context.hit.offset_x),
                f64::from(pos.y) + f64::from(context.hit.offset_y),
                f64::from(pos.z) + f64::from(context.hit.offset_z),
            ),
            inside: context.hit.inside,
        },
    );
    let result = crate::item::stack::use_on(receiver, &use_on, operation)?;
    if let Some(result) = &result {
        if infinite_materials {
            set_count(
                operation
                    .level
                    .stack_mut(receiver)
                    .map_err(item_receiver_error)?,
                original_count,
            );
        } else if let InteractionResult::Success { item_context, .. } = result {
            let transformed = match item_context.held_item_transformed_to {
                Some(stack) => stack,
                None => operation
                    .pin(hand_slot(operation.level, context.player, context.hand)?)
                    .map_err(item_receiver_error)?,
            };
            let same = std::ptr::eq(
                operation
                    .level
                    .stack(receiver)
                    .map_err(item_receiver_error)?,
                operation
                    .level
                    .stack(transformed)
                    .map_err(item_receiver_error)?,
            );
            if !same {
                operation
                    .level
                    .replace_stack_object(
                        hand_slot(operation.level, context.player, context.hand)?,
                        StackValue::Existing(transformed),
                    )
                    .map_err(item_receiver_error)?;
            }
        }
        if result.consumes_action() {
            criteria::item_used_on_block(operation.level, context.player, pos, &used_stack)?;
        }
    }
    Ok(result)
}

fn hand_slot(
    level: &Level,
    player: EntityRef,
    hand: InteractionHand,
) -> Result<ContainerSlot, LevelError> {
    let slot = match hand {
        InteractionHand::MainHand => level
            .entity::<ServerPlayer>(player)?
            .inventory
            .selected_slot(),
        InteractionHand::OffHand => 40,
    };
    Ok(ContainerSlot {
        container: ContainerHandle::PlayerInventory(player),
        slot,
    })
}
fn item_receiver_error(error: MenuError) -> LevelError {
    match error {
        MenuError::Level(error) => error,
        _ => LevelError::Inventory("item use receiver"),
    }
}
