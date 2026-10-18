//! The external-container resolver borrows canonical block entities only for the
//! pure menu operation. World callbacks run after all these borrows have ended.
use super::*;
use crate::{
    block::entity::{BlockEntityRef, BlockEntityState},
    entity::{EntityRef, player::ServerPlayer},
    level::Level,
};
#[cfg(test)]
use ferraloom_util::Vec3;

pub(crate) struct BlockMenuContainer<'a> {
    pub states: Vec<&'a mut BlockEntityState>,
    pub valid: bool,
}
impl BlockMenuContainer<'_> {
    fn locate(&self, mut slot: usize) -> (usize, usize) {
        for (i, state) in self.states.iter().enumerate() {
            let size = state.container().expect("block container").size();
            if slot < size {
                return (i, slot);
            }
            slot -= size;
        }
        panic!("block container slot out of range");
    }
}
impl Container for BlockMenuContainer<'_> {
    fn size(&self) -> usize {
        self.states
            .iter()
            .map(|state| state.container().expect("block container").size())
            .sum()
    }
    fn get_item(&self, slot: usize) -> ItemStack {
        let (i, slot) = self.locate(slot);
        self.states[i].container().unwrap().get_item(slot)
    }
    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        let (i, slot) = self.locate(slot);
        if let BlockEntityState::Crafter(crafter) = &mut *self.states[i] {
            crafter.set_slot_state(slot, true);
        }
        self.states[i]
            .container_mut()
            .unwrap()
            .set_item(slot, stack);
    }
    fn remove_item_no_update(&mut self, slot: usize) -> ItemStack {
        let (i, slot) = self.locate(slot);
        self.states[i]
            .container_mut()
            .unwrap()
            .remove_item_no_update(slot)
    }
    fn can_place_item(&self, slot: usize, stack: &ItemStack) -> bool {
        let (i, slot) = self.locate(slot);
        match &*self.states[i] {
            BlockEntityState::Crafter(crafter) => crafter.can_place_item(slot, stack),
            _ => true,
        }
    }
    fn still_valid(&self) -> bool {
        self.valid
    }
}
/// Port of Container#stillValidBlockEntity and Player#isWithinBlockInteractionRange.
/// Uses the canonical live interaction-range attribute and exact generation.
pub fn block_menu_still_valid(
    player: EntityRef,
    refs: &[BlockEntityRef],
    level: &Level,
) -> Result<bool> {
    let player = level.entity::<ServerPlayer>(player)?;
    for &handle in refs {
        if level.block_entity_ref(handle.pos)? != Some(handle) {
            return Ok(false);
        }
        if !player.is_within_block_interaction_range(handle.pos, 4.0) {
            return Ok(false);
        }
    }
    Ok(true)
}
/// Runtime packet handlers use the same resolver as open/tick, so menu clicks
/// mutate the canonical slots and never a copy of the block inventory.
pub fn with_block_menu_context<R>(
    player: EntityRef,
    level: &mut Level,
    operation: impl FnOnce(
        &mut MenuSession,
        &mut crate::crafting::ServerRecipeBook,
        &mut MenuContext<'_>,
    ) -> Result<R>,
) -> Result<R> {
    let refs = level
        .entity::<ServerPlayer>(player)?
        .menus
        .current()
        .block_entities
        .clone();
    if refs.is_empty() {
        let player = level.entity_mut::<ServerPlayer>(player)?;
        if player
            .menus
            .current()
            .slots
            .iter()
            .any(|slot| matches!(slot.container, ContainerId::External(_)))
        {
            return Err(MenuError::Unsupported("mount container resolver"));
        }
        let mut ctx = MenuContext::player(
            &mut player.inventory,
            &mut player.parent.equipment,
            player.game_mode == GameType::Creative,
        );
        return operation(&mut player.menus, &mut player.recipe_book, &mut ctx);
    }
    let valid = block_menu_still_valid(player, &refs, level)?;
    let (result, changed) = {
        let (player, states) = level.player_and_retained_block_entities_mut(player, &refs)?;
        // Native menus hold the receiver's ContainerData. Refresh its network
        // view at each operation, including initial synchronization and ticks.
        match states.first().map(|state| &**state) {
            Some(BlockEntityState::Furnace(furnace)) => {
                player
                    .menus
                    .current_mut()
                    .data
                    .copy_from_slice(&furnace.data());
            }
            Some(BlockEntityState::Crafter(crafter)) => {
                let menu = player.menus.current_mut();
                for (slot, &disabled) in crafter.disabled_slots.iter().enumerate() {
                    menu.data[slot] = i32::from(disabled);
                    menu.slots[slot].disabled = disabled;
                }
                menu.data[9] = crafter.triggered;
            }
            _ => {}
        }
        let before: Vec<_> = states
            .iter()
            .map(|state| state.container().expect("block container").times_changed)
            .collect();
        let mut container = BlockMenuContainer { states, valid };
        let result = {
            let mut external: [&mut dyn Container; 1] = [&mut container];
            let mut ctx = MenuContext::player(
                &mut player.inventory,
                &mut player.parent.equipment,
                player.game_mode == GameType::Creative,
            );
            ctx.external = &mut external;
            operation(&mut player.menus, &mut player.recipe_book, &mut ctx)
        };
        let changed = container
            .states
            .iter()
            .zip(before)
            .enumerate()
            .filter(|(_, (state, before))| state.container().unwrap().times_changed != *before)
            .map(|(i, _)| refs[i])
            .collect::<Vec<_>>();
        (result, changed)
    };
    for handle in changed {
        handle.set_changed(level)?;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        block::{
            containers::menu::ContainerMenuProvider,
            entity::{BlockEntityData, create},
        },
        entity::{EntityState, player::GameType},
        level::{Region, chunk::LevelChunk},
    };
    use ferraloom_data::{Block, Item};
    use ferraloom_util::{BlockPos, ChunkPos, Uuid};
    use std::sync::Arc;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    fn setup() -> (Level, EntityRef, BlockEntityRef) {
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let biome = ferraloom_chunk::BiomeId(0);
        let section = ferraloom_chunk::LevelChunkSection::new(
            ferraloom_chunk::Strategy::block_states(),
            ferraloom_chunk::Strategy::biomes(Arc::new(
                ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
            )),
            biome,
        );
        level
            .add_chunk(LevelChunk::new(ChunkPos::new(0, 0), 0..2, section))
            .unwrap();
        let pos = BlockPos::new(2, 1, 2);
        let state = Block::CHEST.default_state();
        level.set_block_state(pos, state).unwrap();
        level.set_block_entity(create(pos, state).unwrap()).unwrap();
        let handle = level.block_entity_ref(pos).unwrap().unwrap();
        let player = EntityRef(1);
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(1, 2),
                    "container".into(),
                    1,
                    Vec3::new(2.5, 1.0, 3.5),
                    GameType::Survival,
                ))),
            )
            .unwrap();
        (level, player, handle)
    }
    fn open(level: &mut Level, player: EntityRef, handle: BlockEntityRef) {
        let provider = ContainerMenuProvider {
            entities: vec![handle],
            kind: MenuKind::Chest { rows: 3 },
            title: ferraloom_protocol::chat::Component::literal("Chest"),
        };
        let packets = super::super::open_menu(player, &provider, level).unwrap();
        assert!(matches!(
            packets[0],
            ferraloom_protocol::play::clientbound::Packet::OpenScreen(_)
        ));
    }
    #[test]
    fn furnace_menu_reads_current_canonical_timer_data() {
        use crate::block::containers::furnace::AbstractFurnaceBlockEntity;
        let (mut level, player, old) = setup();
        let state = Block::FURNACE.default_state();
        level.set_block_state(old.pos, state).unwrap();
        level
            .set_block_entity(create(old.pos, state).unwrap())
            .unwrap();
        let handle = level.block_entity_ref(old.pos).unwrap().unwrap();
        let provider = ContainerMenuProvider {
            entities: vec![handle],
            kind: MenuKind::Furnace,
            title: ferraloom_protocol::chat::Component::literal("Furnace"),
        };
        open_menu(player, &provider, &mut level).unwrap();
        let furnace = level
            .block_entity_mut::<AbstractFurnaceBlockEntity>(handle)
            .unwrap();
        furnace.lit_time_remaining = 123;
        furnace.lit_total_time = 1600;
        furnace.cooking_timer = 17;
        furnace.cooking_total_time = 200;
        with_block_menu_context(player, &mut level, |menus, _, _| {
            assert_eq!(menus.current().data, [123, 1600, 17, 200]);
            Ok(())
        })
        .unwrap();
    }
    #[test]
    fn live_slot_access_keeps_detached_menu_and_block_receivers_during_reentry() {
        let (mut level, player, old) = setup();
        open(&mut level, player, old);
        let menu = level
            .entity::<ServerPlayer>(player)
            .unwrap()
            .menus
            .current_ref();
        with_menu_callback(player, menu, &mut level, |context| {
            close_container(player, context.level, false, false)?;
            let state = Block::CHEST.default_state();
            context
                .level
                .set_block_entity(create(old.pos, state).unwrap())?;
            let replacement = context.level.block_entity_ref(old.pos)?.unwrap();
            let provider = ContainerMenuProvider {
                entities: vec![replacement],
                kind: MenuKind::Chest { rows: 3 },
                title: ferraloom_protocol::chat::Component::literal("replacement"),
            };
            open_menu(player, &provider, context.level)?;
            context.level.collect_detached_block_entities();
            let slot = ContainerSlot {
                container: ContainerHandle::BlockEntity(old),
                slot: 0,
            };
            context.set_item(slot, ItemStack::new(Item::STONE, 5, Default::default()))?;
            assert_eq!(
                context.get_item(slot)?,
                ItemStack::new(Item::STONE, 5, Default::default())
            );
            assert!(!context.still_valid(slot.container)?);
            assert!(
                context
                    .get_item(ContainerSlot {
                        container: ContainerHandle::BlockEntity(replacement),
                        slot: 0
                    })?
                    .is_empty()
            );
            context
                .level
                .entity_mut::<ServerPlayer>(player)?
                .menus
                .menu_mut(menu)?
                .carried = ItemStack::new(Item::DIRT, 2, Default::default());
            assert!(
                context
                    .level
                    .entity::<ServerPlayer>(player)?
                    .menus
                    .current()
                    .carried
                    .is_empty()
            );
            Ok(())
        })
        .unwrap();
        assert!(matches!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .menus
                .menu(menu),
            Err(MenuError::MissingMenu(_))
        ));
        level.collect_detached_block_entities();
        assert!(level.block_entity_state(old).is_err());
    }
    #[test]
    fn external_menu_mutations_reach_canonical_slots_and_replacement_closes_after_broadcast() {
        let (mut level, player, handle) = setup();
        open(&mut level, player, handle);
        with_block_menu_context(player, &mut level, |menus, _book, ctx| {
            menus.current_mut().set_item(
                ctx,
                0,
                ItemStack::new(Item::DIAMOND, 7, Default::default()),
            );
            Ok(())
        })
        .unwrap();
        assert_eq!(
            count(
                &level
                    .block_entity_state(handle)
                    .unwrap()
                    .container()
                    .unwrap()
                    .items[0]
            ),
            7
        );
        let state = level
            .block_entity::<BlockEntityData>(handle)
            .unwrap()
            .block_state;
        level
            .set_block_entity(create(handle.pos, state).unwrap())
            .unwrap();
        level
            .block_entity_state_mut(handle)
            .unwrap()
            .container_mut()
            .unwrap()
            .set_item(0, ItemStack::new(Item::DIAMOND, 9, Default::default()));
        super::super::tick_container_menu(player, &mut level).unwrap();
        let player_state = level.entity::<ServerPlayer>(player).unwrap();
        assert!(player_state.menus.container_menu.is_none());
        let packets = &player_state.menus.pending_packets;
        let slot = packets.iter().position(|packet|matches!(packet,ferraloom_protocol::play::clientbound::Packet::ContainerSetSlot(slot) if slot.slot == 0 && count(&slot.item_stack) == 9)).unwrap();
        let close = packets
            .iter()
            .position(|packet| {
                matches!(
                    packet,
                    ferraloom_protocol::play::clientbound::Packet::ContainerClose(_)
                )
            })
            .unwrap();
        assert!(slot < close);
        level.collect_detached_block_entities();
        assert!(level.block_entity_state(handle).is_err());
    }
    #[test]
    fn external_menu_checks_live_generation_and_eye_to_block_distance() {
        let (mut level, player, handle) = setup();
        assert!(block_menu_still_valid(player, &[handle], &level).unwrap());
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .parent
            .parent
            .set_pos(Vec3::new(11.5, 0.0, 2.5));
        assert!(!block_menu_still_valid(player, &[handle], &level).unwrap());
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .parent
            .parent
            .set_pos(Vec3::new(11.49, 0.0, 2.5));
        assert!(block_menu_still_valid(player, &[handle], &level).unwrap());
        level.remove_block_entity(handle.pos).unwrap();
        assert!(!block_menu_still_valid(player, &[handle], &level).unwrap());
    }
}
