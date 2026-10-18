//! CrafterBlock's edge trigger, recipe execution and captured native input list.
use crate::{
    block::{
        BlockBehavior, UPDATE_LIMIT, UpdateFlags,
        containers::{access::ContainerRef, crafter::CrafterBlockEntity},
        entity::{BlockEntityRef, BlockEntityState},
    },
    crafting::{CraftingInputAccess, CraftingInputIter, CraftingInputView},
    entity::EntityState,
    inventory::{Container, ContainerHandle, ContainerSlot, ItemStack, StackOperation, StackRef},
    level::{Level, LevelError, redstone::Orientation},
};
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_util::{Aabb, BlockPos, Direction, Vec3};

pub struct CrafterBlock;
pub static CRAFTER: CrafterBlock = CrafterBlock;

/// Port of CraftingInput#ofPositioned's list ownership. A full grid wraps the
/// original BE list; a cropped grid stores the original ItemStack references.
/// Whole-list replacement (ItemCommands, chest swaps and /data/structure/chunk/BlockItem loads) is unreachable during dispenseFrom.
#[derive(Debug)]
enum CapturedInput {
    Full {
        owner: BlockEntityRef,
        ingredients: usize,
    },
    Trimmed {
        width: usize,
        height: usize,
        ingredients: usize,
        items: Vec<StackRef>,
    },
}
impl CapturedInput {
    fn capture(
        owner: BlockEntityRef,
        operation: &mut StackOperation<'_>,
    ) -> Result<Self, LevelError> {
        let input = CraftingInputView::of_positioned(
            3,
            3,
            &operation
                .level
                .block_entity::<CrafterBlockEntity>(owner)?
                .parent
                .parent
                .items,
        )
        .map_err(|_| LevelError::Inventory("crafter input geometry"))?;
        let (width, height, ingredients, left, top) = (
            input.input.width(),
            input.input.height(),
            input.input.ingredient_count(),
            input.left,
            input.top,
        );
        if width == 3 && height == 3 {
            return Ok(Self::Full { owner, ingredients });
        }
        let mut items = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                items.push(
                    operation
                        .pin(ContainerSlot {
                            container: ContainerHandle::BlockEntity(owner),
                            slot: left + x + (top + y) * 3,
                        })
                        .map_err(super::stack_error)?,
                );
            }
        }
        Ok(Self::Trimmed {
            width,
            height,
            ingredients,
            items,
        })
    }
    fn borrow<'a>(&'a self, level: &'a Level) -> Result<InputView<'a>, LevelError> {
        let (width, height, ingredients, items) = match self {
            Self::Full { owner, ingredients } => (
                3,
                3,
                *ingredients,
                level
                    .block_entity::<CrafterBlockEntity>(*owner)?
                    .parent
                    .parent
                    .items
                    .iter()
                    .collect(),
            ),
            Self::Trimmed {
                width,
                height,
                ingredients,
                items,
            } => (
                *width,
                *height,
                *ingredients,
                items
                    .iter()
                    .map(|reference| level.stack(*reference).map_err(super::stack_error))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        };
        Ok(InputView {
            width,
            height,
            ingredients,
            items,
        })
    }
}
/// Only receiver references are collected; no stack values are copied. The
/// cached ingredient count belongs to native CraftingInput, even after callbacks.
#[derive(Debug)]
struct InputView<'a> {
    width: usize,
    height: usize,
    ingredients: usize,
    items: Vec<&'a ItemStack>,
}
impl CraftingInputAccess for InputView<'_> {
    fn width(&self) -> usize {
        self.width
    }
    fn height(&self) -> usize {
        self.height
    }
    fn size(&self) -> usize {
        self.items.len()
    }
    fn get_item(&self, x: usize, y: usize) -> &ItemStack {
        self.items[x + y * self.width]
    }
    fn ingredient_count(&self) -> usize {
        self.ingredients
    }
    fn iter(&self) -> CraftingInputIter<'_> {
        CraftingInputIter::new(self)
    }
}
fn front(state: BlockState) -> Direction {
    match state.get(p::ORIENTATION).unwrap() {
        p::Orientation::DownEast
        | p::Orientation::DownNorth
        | p::Orientation::DownSouth
        | p::Orientation::DownWest => Direction::Down,
        p::Orientation::UpEast
        | p::Orientation::UpNorth
        | p::Orientation::UpSouth
        | p::Orientation::UpWest => Direction::Up,
        p::Orientation::WestUp => Direction::West,
        p::Orientation::EastUp => Direction::East,
        p::Orientation::NorthUp => Direction::North,
        p::Orientation::SouthUp => Direction::South,
    }
}
/// Port of CrafterBlockEntity#serverTick. It uses the ticker's captured state,
/// writes the original BE generation, then publishes CRAFTING=false at zero.
pub fn server_tick(receiver: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
    let entity = level.block_entity_mut::<CrafterBlockEntity>(receiver)?;
    let remaining = entity.crafting_ticks_remaining.wrapping_sub(1);
    if remaining >= 0 {
        entity.crafting_ticks_remaining = remaining;
        let state = entity.parent.parent.parent.block_state;
        if remaining == 0 {
            level.set_block_and_update(receiver.pos, state.with(p::CRAFTING, false).unwrap())?;
        }
    }
    Ok(())
}
impl BlockBehavior for CrafterBlock {
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        _block: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let signal = level.has_neighbor_signal(pos)?;
        let triggered = state.get(p::TRIGGERED).unwrap();
        let receiver = level.block_entity_ref(pos)?;
        if signal != triggered {
            let transition = |level: &mut Level| {
                if signal {
                    level.schedule_tick(pos, state.block(), 4)?;
                }
                let mut changed = state.with(p::TRIGGERED, signal).unwrap();
                if !signal {
                    changed = changed.with(p::CRAFTING, false).unwrap();
                }
                level.set_block(pos, changed, UpdateFlags::CLIENTS, UPDATE_LIMIT)?;
                if let Some(receiver) = receiver
                    && let BlockEntityState::Crafter(entity) =
                        level.block_entity_state_mut(receiver)?
                {
                    entity.triggered = i32::from(signal);
                }
                Ok(())
            };
            match receiver {
                Some(receiver) => level.with_block_entity_callback(receiver, transition)?,
                None => transition(level)?,
            }
        }
        Ok(())
    }
    fn tick(&self, state: BlockState, pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
        let Some(receiver) = level.block_entity_ref(pos)? else {
            return Ok(());
        };
        if !matches!(
            level.block_entity_state(receiver)?,
            BlockEntityState::Crafter(_)
        ) {
            return Ok(());
        }
        level.with_block_entity_callback(receiver, |level| {
            dispense_from(receiver, state, pos, level)
        })
    }
}
fn dispense_from(
    receiver: BlockEntityRef,
    state: BlockState,
    pos: BlockPos,
    level: &mut Level,
) -> Result<(), LevelError> {
    let mut operation = StackOperation::new(level);
    let input = CapturedInput::capture(receiver, &mut operation)?;
    let services = operation.level.container_resources()?;
    let Some((recipe_id, result)) = operation
        .level
        .crafter_potential_results(receiver)
        .map_err(super::stack_error)?
    else {
        return operation.level.level_event(None, 1050, pos, 0);
    };
    if result.is_empty() {
        return operation.level.level_event(None, 1050, pos, 0);
    }
    operation
        .level
        .block_entity_mut::<CrafterBlockEntity>(receiver)?
        .crafting_ticks_remaining = 6;
    operation.level.set_block(
        pos,
        state.with(p::CRAFTING, true).unwrap(),
        UpdateFlags::CLIENTS,
        UPDATE_LIMIT,
    )?;
    // Item#onCraftedPostProcess is empty except the canonical map callback.
    if result.item() == ferraloom_data::Item::FILLED_MAP
        && result
            .component(ferraloom_protocol::component::ComponentTypeId::MAP_POST_PROCESSING)
            .map_err(|_| LevelError::Inventory("crafter result component"))?
            .is_some()
    {
        operation.level.unported_operation(
            crate::block::UnportedOperation::ItemCraftedPostProcess(result.item()),
        )?;
    }
    let recipe = services
        .recipes
        .by_key(&recipe_id)
        .expect("recipe from retained immutable services");
    dispense_item(receiver, state, pos, &result, recipe, operation.level)?;
    let remaining = recipe
        .value
        .remaining_items(&input.borrow(operation.level)?)
        .map_err(|_| LevelError::Inventory("crafter recipe remainders"))?;
    for remainder in remaining {
        if !remainder.is_empty() {
            dispense_item(receiver, state, pos, &remainder, recipe, operation.level)?;
        }
    }
    let entity = operation
        .level
        .block_entity_mut::<CrafterBlockEntity>(receiver)?;
    for item in &mut entity.parent.parent.items {
        if !item.is_empty() {
            crate::entity::player::inventory::set_count(
                item,
                crate::entity::player::inventory::count(item).wrapping_sub(1),
            );
        }
    }
    receiver.set_changed(operation.level)
}
fn dispense_item(
    receiver: BlockEntityRef,
    state: BlockState,
    pos: BlockPos,
    result: &ItemStack,
    recipe: &crate::crafting::RecipeHolder,
    level: &mut Level,
) -> Result<(), LevelError> {
    use crate::entity::player::inventory::{count, max_stack_size, set_count};
    let direction = front(state);
    let into = super::hopper::container_at(level, pos.relative(direction, 1))?;
    let mut remaining = result.clone(); // Native results.copy, after getContainerAt callbacks.
    if let Some(into) = into {
        let crafter = matches!(into,ContainerRef::Single(handle) if matches!(level.block_entity_state(handle)?,BlockEntityState::Crafter(_)));
        let source = ContainerRef::Single(receiver);
        let single_items = crafter || {
            let size = count(result);
            // CompoundContainer uses the lower capacity of its two receivers.
            let capacity = into
                .handles()
                .iter()
                .try_fold(i32::MAX, |limit, handle| {
                    Ok::<_, LevelError>(
                        limit.min(
                            level
                                .block_entity_state(*handle)?
                                .container()
                                .expect("canonical block container")
                                .max_stack_size(),
                        ),
                    )
                })?
                .min(max_stack_size(result));
            size > capacity
        };
        if single_items {
            while !remaining.is_empty() {
                let copy = remaining.copy_with_count(1);
                if !super::hopper::add_item(
                    Some(&source),
                    &into,
                    copy,
                    Some(direction.opposite()),
                    level,
                )?
                .is_empty()
                {
                    break;
                }
                let total = count(&remaining).wrapping_sub(1);
                set_count(&mut remaining, total);
            }
        } else {
            while !remaining.is_empty() {
                let prior = count(&remaining);
                remaining = super::hopper::add_item(
                    Some(&source),
                    &into,
                    remaining,
                    Some(direction.opposite()),
                    level,
                )?;
                if prior == count(&remaining) {
                    break;
                }
            }
        }
    }
    if !remaining.is_empty() {
        let center = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        let step = direction.step();
        super::dispenser::spawn_item(
            level,
            remaining,
            6,
            direction,
            Vec3::new(
                center.x + f64::from(step.x) * 0.7,
                center.y + f64::from(step.y) * 0.7,
                center.z + f64::from(step.z) * 0.7,
            ),
        )?;
        let candidates = level.entities_in(
            Aabb::new(
                center.x - 8.5,
                center.y - 8.5,
                center.z - 8.5,
                center.x + 8.5,
                center.y + 8.5,
                center.z + 8.5,
            ),
            |_, entity| matches!(entity, EntityState::Player(_)),
        );
        // The two-argument getEntitiesOfClass overload applies NO_SPECTATORS.
        // Capture its entire filtered list before the first criterion callback.
        let mut players = Vec::with_capacity(candidates.len());
        for player in candidates {
            if !level.entity_behavior(player)?.is_spectator(player, level)? {
                players.push(player);
            }
        }
        for player in players {
            let mut operation = StackOperation::new(level);
            let mut ingredients = Vec::with_capacity(9);
            for slot in 0..9 {
                ingredients.push(
                    operation
                        .pin(ContainerSlot {
                            container: ContainerHandle::BlockEntity(receiver),
                            slot,
                        })
                        .map_err(super::stack_error)?,
                );
            }
            crate::advancements::criteria::crafter_recipe_crafted(
                operation.level,
                player,
                recipe,
                &ingredients,
            )?;
        }
        level.level_event(None, 1049, pos, 0)?;
        level.level_event(None, 2010, pos, i32::from(direction as u8))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{
        containers::{access, storage::BaseContainerBlockEntity},
        entity::create,
    };
    use crate::entity::player::inventory::{count, set_count};
    use ferraloom_data::Item;
    const POS: BlockPos = BlockPos::new(8, 64, 8);
    fn describe(item: &ItemStack) -> String {
        format!("{},{}", item.item().protocol_id(), count(item))
    }
    fn install(level: &mut Level, state: BlockState) -> BlockEntityRef {
        level.set_block_entity(create(POS, state).unwrap()).unwrap();
        level.block_entity_ref(POS).unwrap().unwrap()
    }
    #[test]
    fn full_grid_remainders_follow_slot_replacement_and_trimmed_grid_retains_original_objects() {
        for (name, expected) in super::super::device_tests::cases("crafter-alias:") {
            let args: Vec<_> = name.split(':').skip(1).collect();
            let full = args[0] == "true";
            let empty = args[1] == "true";
            let state = Block::CRAFTER.default_state();
            let mut level = super::super::device_tests::level(state);
            let receiver = install(&mut level, state);
            let slot = if full { 0 } else { 4 };
            access::set_item(
                receiver,
                slot,
                ItemStack::new(Item::MILK_BUCKET, 1, Default::default()),
                &mut level,
            )
            .unwrap();
            if full {
                access::set_item(
                    receiver,
                    8,
                    ItemStack::new(Item::STONE, 1, Default::default()),
                    &mut level,
                )
                .unwrap();
            }
            level
                .with_block_entity_callback(receiver, |level| {
                    let mut operation = StackOperation::new(level);
                    let held = operation
                        .pin(ContainerSlot {
                            container: ContainerHandle::BlockEntity(receiver),
                            slot,
                        })
                        .unwrap();
                    let captured = CapturedInput::capture(receiver, &mut operation)?;
                    access::set_item(
                        receiver,
                        slot,
                        ItemStack::new(Item::DIRT, 5, Default::default()),
                        operation.level,
                    )?;
                    let original = operation.level.stack_mut(held).unwrap();
                    let total = if empty { 0 } else { count(original) + 2 };
                    set_count(original, total);
                    let input = captured.borrow(operation.level)?;
                    let services = operation.level.container_resources()?;
                    let recipe = services
                        .recipes
                        .by_key(&"minecraft:cake".parse().unwrap())
                        .unwrap();
                    let remaining = recipe.value.remaining_items(&input).unwrap();
                    assert_eq!(
                        format!(
                            "{},{},{}|{}|{}|{}",
                            input.width(),
                            input.height(),
                            input.ingredient_count(),
                            describe(input.get_item_index(0)),
                            std::ptr::eq(
                                input.get_item_index(0),
                                operation.level.stack(held).unwrap()
                            ),
                            remaining.iter().map(describe).collect::<Vec<_>>().join(";")
                        ),
                        expected,
                        "{name}"
                    );
                    Ok(())
                })
                .unwrap();
        }
    }
    #[test]
    fn crafter_edges_and_ticker_match_native_schedule_and_signed_countdown() {
        for (name, expected) in super::super::device_tests::cases("crafter-edge:") {
            let args: Vec<_> = name.split(':').skip(1).collect();
            let state = BlockState::from_id(args[0].parse().unwrap()).unwrap();
            let mut level = super::super::device_tests::level(state);
            let receiver = install(&mut level, state);
            // Native constructor initializes container data from the placed state.
            level
                .block_entity_mut::<CrafterBlockEntity>(receiver)
                .unwrap()
                .triggered = i32::from(state.get(p::TRIGGERED).unwrap());
            if args[1] == "true" {
                level
                    .set_block_state(POS.west(1), Block::REDSTONE_BLOCK.default_state())
                    .unwrap();
            }
            CRAFTER
                .neighbor_changed(state, POS, Block::STONE, None, false, &mut level)
                .unwrap();
            assert_eq!(
                format!(
                    "{}|{}|{}",
                    level.get_block_state(POS).unwrap().id(),
                    level
                        .block_entity::<CrafterBlockEntity>(receiver)
                        .unwrap()
                        .triggered
                        == 1,
                    level.has_scheduled_block_tick(POS, Block::CRAFTER)
                ),
                expected,
                "{name}"
            );
        }
        for (name, expected) in super::super::device_tests::cases("crafter-ticker:") {
            let state = Block::CRAFTER
                .default_state()
                .with(p::CRAFTING, true)
                .unwrap();
            let mut level = super::super::device_tests::level(state);
            let receiver = install(&mut level, state);
            level
                .block_entity_mut::<CrafterBlockEntity>(receiver)
                .unwrap()
                .crafting_ticks_remaining = name.split_once(':').unwrap().1.parse().unwrap();
            server_tick(receiver, &mut level).unwrap();
            assert_eq!(
                format!(
                    "{}|{}",
                    level
                        .block_entity::<CrafterBlockEntity>(receiver)
                        .unwrap()
                        .crafting_ticks_remaining,
                    level.get_block_state(POS).unwrap().id()
                ),
                expected,
                "{name}"
            );
        }
    }
    #[test]
    fn crafter_recipe_assembly_transfer_consumption_and_item_spawn_match_native() {
        use crate::level::effects::LevelEffect;
        use ferraloom_protocol::play::clientbound::Packet;
        use ferraloom_util::random::{LegacyRandomSource, Random, RandomSource};
        for (name, expected) in super::super::device_tests::cases("crafter-execute:") {
            let args: Vec<_> = name.split(':').skip(1).collect();
            let state = BlockState::from_id(args[0].parse().unwrap()).unwrap();
            let input = args[1].parse::<u8>().unwrap();
            let target = args[2].parse::<u8>().unwrap();
            let mut level = super::super::device_tests::level(state);
            let receiver = install(&mut level, state);
            if input == 1 || input == 2 {
                access::set_item(
                    receiver,
                    4,
                    ItemStack::new(
                        Item::OAK_LOG,
                        if input == 1 { 1 } else { 8 },
                        Default::default(),
                    ),
                    &mut level,
                )
                .unwrap();
            }
            if input == 3 {
                for slot in 0..9 {
                    access::set_item(
                        receiver,
                        slot,
                        ItemStack::new(Item::IRON_INGOT, 2, Default::default()),
                        &mut level,
                    )
                    .unwrap();
                }
            }
            let into = POS.relative(front(state), 1);
            if target != 0 {
                let barrel = Block::BARREL.default_state();
                level.set_block_state(into, barrel).unwrap();
                level
                    .set_block_entity(create(into, barrel).unwrap())
                    .unwrap();
                if target == 2 {
                    let handle = level.block_entity_ref(into).unwrap().unwrap();
                    for slot in 0..27 {
                        access::set_item(
                            handle,
                            slot,
                            ItemStack::new(Item::DIRT, 64, Default::default()),
                            &mut level,
                        )
                        .unwrap();
                    }
                }
            }
            level.set_random_streams(Random::Legacy(LegacyRandomSource::new(42)), 0);
            level.take_effects();
            CRAFTER.tick(state, POS, &mut level).unwrap();
            let effects = level.take_effects();
            let stored = if target == 0 {
                "-".into()
            } else {
                let handle = level.block_entity_ref(into).unwrap().unwrap();
                describe(
                    &level
                        .block_entity::<BaseContainerBlockEntity>(handle)
                        .unwrap()
                        .items[0],
                )
            };
            let spawned = level
                .entities_in(
                    Aabb::new(-32.0, -32.0, -32.0, 64.0, 128.0, 64.0),
                    |_, entity| matches!(entity, EntityState::Item(_)),
                )
                .into_iter()
                .map(|entity| {
                    let EntityState::Item(entity) = level.entity_state(entity).unwrap() else {
                        unreachable!()
                    };
                    describe(entity.item())
                })
                .collect::<Vec<_>>()
                .join(";");
            let gameplay = effects
                .iter()
                .filter_map(|effect| match effect {
                    LevelEffect::PlayerPacket { packet, .. } => match packet.as_ref() {
                        Packet::LevelEvent(event) => {
                            Some(format!("L:{}:{}:0", event.event_type, event.data))
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(";");
            let entity = level.block_entity::<CrafterBlockEntity>(receiver).unwrap();
            let countdown = entity.crafting_ticks_remaining;
            let item = describe(&entity.parent.parent.items[4]);
            assert_eq!(
                format!(
                    "{}|{}|{}|{}|{}|{}|{}",
                    level.get_block_state(POS).unwrap().id(),
                    countdown,
                    item,
                    stored,
                    spawned,
                    gameplay,
                    level.random().next_long()
                ),
                expected,
                "{name}"
            );
        }
    }
}

#[cfg(test)]
mod placement_tests {
    use super::*;
    use crate::block::{BlockPlaceContext, LevelReaderRef, PlacementHit, UseOnContext};
    use crate::entity::{
        EntityData, EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    };
    use ferraloom_protocol::play::InteractionHand;
    use ferraloom_util::Uuid;
    #[test]
    fn crafter_placement_matches_native_head_rotation_and_initial_signal() {
        for (name, expected) in super::super::device_tests::cases("crafter-placement:") {
            let args: Vec<_> = name.split(':').skip(1).collect();
            let pos = BlockPos::new(8, 64, 8);
            let mut level = super::super::device_tests::level(Block::AIR.default_state());
            if args[2] == "true" {
                level
                    .set_block_state(pos.west(1), Block::REDSTONE_BLOCK.default_state())
                    .unwrap();
            }
            let player = EntityRef(1);
            level
                .add_entity(
                    player,
                    EntityState::Player(Box::new(ServerPlayer::new(
                        Uuid::new(1, 2),
                        "redstone".into(),
                        1,
                        Vec3::new(8.5, 64.5, 8.5),
                        GameType::Survival,
                    ))),
                )
                .unwrap();
            let yaw = args[0].parse::<f32>().unwrap();
            let actor = level.entity_mut::<EntityData>(player).unwrap();
            actor.yaw = yaw;
            actor.pitch = args[1].parse().unwrap();
            level
                .entity_state_mut(player)
                .unwrap()
                .as_living_mut()
                .unwrap()
                .y_head_rot = yaw;
            let stack = ItemStack::new(ferraloom_data::Item::CRAFTER, 1, Default::default());
            let context = BlockPlaceContext::new(
                &LevelReaderRef::Server(&level),
                UseOnContext::new(
                    Some(player),
                    InteractionHand::MainHand,
                    &stack,
                    PlacementHit {
                        pos,
                        face: Direction::Up,
                        location: Vec3::new(8.5, 64.5, 8.5),
                        inside: false,
                    },
                ),
            )
            .unwrap();
            assert_eq!(
                level
                    .get_state_for_placement(Block::CRAFTER, &context)
                    .unwrap()
                    .unwrap()
                    .id()
                    .to_string(),
                expected,
                "{name}"
            );
        }
    }
}
