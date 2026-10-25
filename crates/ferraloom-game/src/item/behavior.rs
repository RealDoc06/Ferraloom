//! Per-virtual dispatch by the actual native declaring class, including inherited bases.
use crate::{
    block::{BlockPlaceContext, InteractionResult, LevelReaderRef, UseOnContext},
    inventory::{StackOperation, StackRef},
    level::LevelError,
};
use ferraloom_data::{Block, BlockState, Item, ItemClass, ItemMethod, registries::sound_event};

/// Stateless Item/BlockItem virtuals. Every callback shares the caller's complete
/// use operation; transformed receiver handles must remain rooted in that frame.
pub trait ItemBehavior: Sync {
    /// Port of Item#useOn. BlockItem's explicit super call selects this body.
    fn use_on(
        &self,
        item: Item,
        _receiver: StackRef,
        context: &UseOnContext<'_>,
        operation: &mut StackOperation<'_>,
    ) -> Result<Option<InteractionResult>, LevelError> {
        base_use_on(item, context, operation)
    }

    /// Item#use's air-use virtual. Unported derived bodies are required errors;
    /// the complete game-mode air-use pipeline follows in slice D.
    fn use_item(
        &self,
        item: Item,
        _player: crate::entity::EntityRef,
        _hand: ferraloom_protocol::play::InteractionHand,
        _operation: &mut StackOperation<'_>,
    ) -> Result<Option<InteractionResult>, LevelError> {
        Err(LevelError::UnsupportedItemMethod(item, ItemMethod::Use))
    }

    /// Port of BlockItem#place.
    fn place(
        &self,
        item: Item,
        context: BlockPlaceContext<'_>,
        operation: &mut StackOperation<'_>,
    ) -> Result<InteractionResult, LevelError> {
        super::block_item::place(item, context, operation)
    }

    /// Port of BlockItem#updatePlacementContext's identity default.
    fn update_placement_context<'stack>(
        &self,
        _item: Item,
        context: BlockPlaceContext<'stack>,
        _operation: &mut StackOperation<'_>,
    ) -> Result<Option<BlockPlaceContext<'stack>>, LevelError> {
        Ok(Some(context))
    }

    /// Port of BlockItem#getPlacementState.
    fn get_placement_state(
        &self,
        item: Item,
        context: &BlockPlaceContext<'_>,
        operation: &mut StackOperation<'_>,
    ) -> Result<Option<BlockState>, LevelError> {
        base_get_placement_state(item, context, operation)
    }

    /// Port of StandingAndWallBlockItem#canPlace's distinct overload.
    fn can_place_standing_or_wall(
        &self,
        _item: Item,
        state: BlockState,
        pos: ferraloom_util::BlockPos,
        level: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        level.can_survive(state, pos)
    }

    /// Port of BlockItem#canPlace, with the native short circuit and placement context.
    fn can_place(
        &self,
        item: Item,
        context: &BlockPlaceContext<'_>,
        state: BlockState,
        operation: &mut StackOperation<'_>,
    ) -> Result<bool, LevelError> {
        if required_behavior(item, ItemMethod::MustSurvive)?.must_survive(item, operation)?
            && !operation.level.can_survive(state, context.clicked_pos())?
        {
            return Ok(false);
        }
        let collision = crate::block::BlockCollisionContext::placement_context(
            context.player(),
            operation.level,
        )?;
        operation
            .level
            .is_unobstructed(state, context.clicked_pos(), &collision)
    }

    /// Port of BlockItem#mustSurvive.
    fn must_survive(
        &self,
        _item: Item,
        _operation: &mut StackOperation<'_>,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }

    /// Port of BlockItem#placeBlock (UPDATE_ALL_IMMEDIATE = 11).
    fn place_block(
        &self,
        _item: Item,
        context: &BlockPlaceContext<'_>,
        state: BlockState,
        operation: &mut StackOperation<'_>,
    ) -> Result<bool, LevelError> {
        operation.level.set_block(
            context.clicked_pos(),
            state,
            crate::block::UpdateFlags::ALL_IMMEDIATE,
            crate::block::UPDATE_LIMIT,
        )
    }

    /// Port of BlockItem#getBlock; the association is immutable extracted data.
    fn get_block(
        &self,
        item: Item,
        _operation: &mut StackOperation<'_>,
    ) -> Result<Block, LevelError> {
        item.data().block.ok_or(LevelError::UnsupportedItemMethod(
            item,
            ItemMethod::GetBlock,
        ))
    }

    /// Port of BlockItem#getPlaceSound.
    fn get_place_sound(
        &self,
        _item: Item,
        state: BlockState,
        _operation: &mut StackOperation<'_>,
    ) -> Result<sound_event::Id, LevelError> {
        Ok(state.data().sound.place_sound)
    }
}

struct ItemBase;
impl ItemBehavior for ItemBase {}
static ITEM: ItemBase = ItemBase;

struct BoneMeal;
impl ItemBehavior for BoneMeal {
    fn use_on(
        &self,
        _item: Item,
        _receiver: StackRef,
        context: &UseOnContext<'_>,
        operation: &mut StackOperation<'_>,
    ) -> Result<Option<InteractionResult>, LevelError> {
        super::bone_meal::use_on(context, operation)
    }
}
static BONE_MEAL: BoneMeal = BoneMeal;

struct BlockItem;
impl ItemBehavior for BlockItem {
    fn use_on(
        &self,
        item: Item,
        _receiver: StackRef,
        context: &UseOnContext<'_>,
        operation: &mut StackOperation<'_>,
    ) -> Result<Option<InteractionResult>, LevelError> {
        super::block_item::use_on(item, context, operation)
    }
}
static BLOCK_ITEM: BlockItem = BlockItem;

/// A real reflected override never falls through to an inherited base.
pub fn ported_behavior(item: Item, method: ItemMethod) -> Option<&'static dyn ItemBehavior> {
    let owner = method.owner(item)?;
    if let Some(behavior) = super::placement::ported_behavior(method, owner) {
        return Some(behavior);
    }
    match (method, owner) {
        (ItemMethod::UseOn, ItemClass::Item) => Some(&ITEM),
        (ItemMethod::UseOn, ItemClass::BoneMealItem) => Some(&BONE_MEAL),
        (
            ItemMethod::UseOn
            | ItemMethod::Place
            | ItemMethod::UpdatePlacementContext
            | ItemMethod::GetPlacementState
            | ItemMethod::CanPlace
            | ItemMethod::MustSurvive
            | ItemMethod::PlaceBlock
            | ItemMethod::GetBlock
            | ItemMethod::GetPlaceSound,
            ItemClass::BlockItem,
        ) => Some(&BLOCK_ITEM),
        _ => None,
    }
}

/// Required query/placement stages fail under both policies when unported.
pub fn required_behavior(
    item: Item,
    method: ItemMethod,
) -> Result<&'static dyn ItemBehavior, LevelError> {
    ported_behavior(item, method).ok_or(LevelError::UnsupportedItemMethod(item, method))
}

pub(super) fn base_use_on(
    item: Item,
    context: &UseOnContext<'_>,
    operation: &mut StackOperation<'_>,
) -> Result<Option<InteractionResult>, LevelError> {
    use ferraloom_protocol::component::ComponentTypeId;
    let reader = LevelReaderRef::Server(operation.level);
    let stack = context.item_in_hand(&reader)?;
    let transformer = match stack
        .component(ComponentTypeId::BLOCK_TRANSFORMER)
        .map_err(|error| super::stack::item_data_error(item, error))?
    {
        Some(ferraloom_protocol::component::DataComponent::BlockTransformer(holder)) => Some(
            holder
                .0
                .to_string()
                .parse()
                .expect("validated transformer identifier"),
        ),
        _ => None,
    };
    if let Some(transformer) = transformer {
        super::block_transformer::transform_block(item, &transformer, context, operation)
    } else {
        Ok(Some(InteractionResult::Pass))
    }
}

/// BlockItem's explicit base body still invokes its actual derived virtuals.
pub(super) fn base_get_placement_state(
    item: Item,
    context: &BlockPlaceContext<'_>,
    operation: &mut StackOperation<'_>,
) -> Result<Option<BlockState>, LevelError> {
    let block = required_behavior(item, ItemMethod::GetBlock)?.get_block(item, operation)?;
    let Some(state) = operation.level.get_state_for_placement(block, context)? else {
        return Ok(None);
    };
    Ok(required_behavior(item, ItemMethod::CanPlace)?
        .can_place(item, context, state, operation)?
        .then_some(state))
}
