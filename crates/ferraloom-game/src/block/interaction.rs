//! Port of InteractionResult and BlockBehaviour's player-use call context.
use crate::entity::EntityRef;
use crate::{
    advancements::criteria,
    entity::player::ServerPlayer,
    level::{Level, LevelError},
};
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

/// Port of ServerPlayerGameMode#useItemOn. None records an approved whole item
/// use effect skipped under Warn; it is not a fabricated InteractionResult.
pub fn use_item_on(
    context: &UseContext,
    level: &mut Level,
) -> Result<Option<InteractionResult>, LevelError> {
    use super::{BlockMethod, UnportedOperation};
    use crate::entity::player::{
        GameType,
        inventory::{count, set_count},
    };
    use ferraloom_data::BlockClass;
    let pos = context.hit.block_pos;
    let state = level.get_block_state(pos)?;
    // The dedicated server currently publishes exactly FeatureFlags.VANILLA_SET.
    if !state.block().is_enabled_in_vanilla() {
        return Ok(Some(InteractionResult::Fail));
    }
    let player = level.entity::<ServerPlayer>(context.player)?;
    if player.game_mode == GameType::Spectator {
        if let Some(provider) = level
            .required_block_behavior(state.block(), BlockMethod::GetMenuProvider)?
            .get_menu_provider(state, pos, level)?
        {
            crate::inventory::open_menu_live(context.player, provider.as_ref(), level).map_err(
                |error| match error {
                    crate::inventory::MenuError::Level(error) => error,
                    _ => LevelError::Inventory("spectator menu"),
                },
            )?;
            return Ok(Some(InteractionResult::CONSUME));
        }
        if matches!(
            state.block().class(),
            BlockClass::NetherPortalBlock
                | BlockClass::EndPortalBlock
                | BlockClass::EndGatewayBlock
        ) {
            level.unported_operation(UnportedOperation::PortalUse(state.block()))?;
            return Ok(Some(InteractionResult::CONSUME));
        }
        return Ok(Some(InteractionResult::Pass));
    }
    let suppress = player.is_secondary_use_active()
        && (!player.get_main_hand_item().is_empty() || !player.get_offhand_item().is_empty());
    let used_stack = player.get_item_in_hand(context.hand).clone();
    if !suppress {
        let result = level
            .required_block_behavior(state.block(), BlockMethod::UseItemOn)?
            .use_item_on(state, pos, context, level)?;
        if result.consumes_action() {
            criteria::item_used_on_block(level, context.player, pos, &used_stack)?;
            return Ok(Some(result));
        }
        if result == InteractionResult::TryWithEmptyHand
            && context.hand == InteractionHand::MainHand
        {
            let result = level
                .required_block_behavior(state.block(), BlockMethod::UseWithoutItem)?
                .use_without_item(state, pos, context, level)?;
            if result.consumes_action() {
                criteria::default_block_use(level, context.player, pos)?;
                return Ok(Some(result));
            }
        }
    }
    let player = level.entity::<ServerPlayer>(context.player)?;
    let stack = player.get_item_in_hand(context.hand);
    if stack.is_empty() || player.cooldowns.is_on_cooldown(stack) {
        return Ok(Some(InteractionResult::Pass));
    }
    let infinite_materials = player.instabuild();
    let original_count = count(stack);
    let item = stack.0.as_ref().expect("nonempty hand").item;
    let result = item_use_on(item, level)?;
    if let Some(result) = &result {
        if infinite_materials {
            let player = level.entity_mut::<ServerPlayer>(context.player)?;
            let mut stack = player.get_item_in_hand(context.hand).clone();
            set_count(&mut stack, original_count);
            player.set_item_in_hand(context.hand, stack);
        } else if let InteractionResult::Success { item_context, .. } = result
            && let Some(stack) = &item_context.held_item_transformed_to
        {
            level
                .entity_mut::<ServerPlayer>(context.player)?
                .set_item_in_hand(context.hand, stack.clone());
        }
        if result.consumes_action() {
            criteria::item_used_on_block(level, context.player, pos, &used_stack)?;
        }
    }
    Ok(result)
}

fn item_use_on(
    item: ferraloom_data::Item,
    level: &mut Level,
) -> Result<Option<InteractionResult>, LevelError> {
    level.unported_operation(super::UnportedOperation::ItemUseOn(item))?;
    Ok(None)
}
