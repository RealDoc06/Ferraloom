//! The external-container resolver borrows canonical block entities only for the
//! pure menu operation. World callbacks run after all these borrows have ended.
use super::*;
#[cfg(test)]
use crate::level::game_event::test_support::{self as game_events, Observation};
use crate::{
    block::entity::{BlockEntityRef, BlockEntityState},
    entity::{
        EntityRef,
        player::{GameType, ServerPlayer},
    },
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
    fn stack_parts(&self, slot: usize) -> Option<(&StackGenerations, &ItemStack, usize)> {
        let (index, slot) = self.locate(slot);
        self.states[index].container().unwrap().stack_parts(slot)
    }
    fn stack_parts_mut(
        &mut self,
        slot: usize,
    ) -> Option<(&mut StackGenerations, &mut ItemStack, usize)> {
        let (index, slot) = self.locate(slot);
        self.states[index]
            .container_mut()
            .unwrap()
            .stack_parts_mut(slot)
    }
    fn reassign_item(&mut self, slot: usize) -> Result<()> {
        let (index, slot) = self.locate(slot);
        if let BlockEntityState::Crafter(crafter) = &mut *self.states[index] {
            crafter.set_slot_state(slot, true);
        }
        self.states[index]
            .container_mut()
            .unwrap()
            .reassign_item(slot)
    }
    fn set_item_count(&mut self, slot: usize, amount: i32) -> Result<()> {
        let (index, slot) = self.locate(slot);
        self.states[index]
            .container_mut()
            .unwrap()
            .set_item_count(slot, amount)
    }
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
    fn remove_item(&mut self, slot: usize, amount: i32) -> ItemStack {
        let (index, slot) = self.locate(slot);
        let hopper = matches!(&*self.states[index], BlockEntityState::Hopper(_));
        let container = self.states[index].container_mut().unwrap();
        if hopper {
            if amount <= 0 {
                ItemStack::default()
            } else {
                split(&mut container.items[slot], amount)
            }
        } else {
            container.remove_item(slot, amount)
        }
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
    ensure_unrooted_menu_values(player, level)?;
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
        let infinite_materials = player.instabuild();
        let mut ctx = MenuContext::player_with_abilities(
            &mut player.inventory,
            &mut player.parent.equipment,
            player.game_mode == GameType::Creative,
            infinite_materials,
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
            let infinite_materials = player.instabuild();
            let mut ctx = MenuContext::player_with_abilities(
                &mut player.inventory,
                &mut player.parent.equipment,
                player.game_mode == GameType::Creative,
                infinite_materials,
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

/// Legacy value adapters cannot represent a held mutable Java receiver. Live
/// handle operations bypass this adapter rather than clone or write back stacks.
pub(crate) fn ensure_unrooted_menu_values(player: EntityRef, level: &Level) -> Result<()> {
    let player = level.entity::<ServerPlayer>(player)?;
    let held = |parts: Option<(&StackGenerations, &ItemStack, usize)>| {
        parts.is_some_and(|(generations, _, index)| {
            generations.is_retained(generations.current(index).unwrap())
        })
    };
    let menu = player.menus.current();
    let mut rooted = (0..43)
        .any(|slot| held(player.inventory.stack_parts(slot, &player.parent.equipment)))
        || held(Some((&menu.carried_generations, &menu.carried, 0)));
    for container in &menu.local {
        rooted |= (0..container.size()).any(|slot| held(container.stack_parts(slot)));
    }
    for &handle in &menu.block_entities {
        let receiver = level.block_entity_state(handle)?;
        if let Some(container) = receiver.container() {
            rooted |= (0..container.size()).any(|slot| held(container.stack_parts(slot)));
        }
    }
    if rooted {
        return Err(MenuError::Unsupported(
            "retained stacks require live menu operations",
        ));
    }
    Ok(())
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
        assert!(
            super::super::open_menu_live(player, &provider, level)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn live_close_packet_precedes_container_close_callbacks() {
        use crate::level::effects::LevelEffect;
        use ferraloom_protocol::play::clientbound as cb;
        let (mut level, player, handle) = setup();
        game_events::observe(&mut level);
        open(&mut level, player, handle);
        game_events::take(&mut level);
        super::super::close_container_live(player, &mut level, true, false).unwrap();
        let effects = game_events::take(&mut level);
        let close = effects.iter().position(|effect| {
            matches!(effect, Observation::Effect(LevelEffect::PlayerPacket { recipients, packet })
                if recipients == &[player] && matches!(packet.as_ref(), cb::Packet::ContainerClose(_)))
        }).unwrap();
        let callback = effects
            .iter()
            .position(|effect| {
                matches!(effect, Observation::GameEvent { event, .. }
                if *event == ferraloom_data::registries::game_event::Id::CONTAINER_CLOSE)
            })
            .unwrap();
        assert!(close < callback);
        let player = level.entity::<ServerPlayer>(player).unwrap();
        assert!(player.menus.container_menu.is_none());
        assert!(player.menus.pending_packets.is_empty());
    }
    #[test]
    fn loot_source_retains_the_original_generation_without_a_dynamic_provider() {
        use crate::{
            block::containers::loot_runtime::LiveLootRuntime,
            loot::{LootParams, LootRandomSelection, LootRuntime, NameableName},
        };
        use ferraloom_nbt::Tag;
        use ferraloom_protocol::{
            chat::Component,
            component::{ComponentTypeId, DataComponent},
        };
        let (mut level, _, original) = setup();
        level
            .block_entity_state_mut(original)
            .unwrap()
            .container_mut()
            .unwrap()
            .custom_name = Some(Component::literal("before"));
        let params = LootParams {
            block_entity: Some(Tag::Compound(
                level
                    .block_entity_state(original)
                    .unwrap()
                    .save_with_full_metadata()
                    .unwrap(),
            )),
            block_entity_components: Some(
                level
                    .block_entity_state(original)
                    .unwrap()
                    .collect_components(),
            ),
            ..Default::default()
        };
        let mut runtime = LiveLootRuntime::new(&mut level, LootRandomSelection::Level);
        runtime.bind_block_entity_source(original).unwrap();
        runtime
            .level_mut()
            .set_block_entity(create(original.pos, Block::CHEST.default_state()).unwrap())
            .unwrap();
        runtime.level_mut().collect_detached_block_entities();
        assert_eq!(runtime.block_entity_source(), Some(original));
        assert_ne!(
            runtime.level().block_entity_ref(original.pos).unwrap(),
            Some(original)
        );
        assert!(runtime.level().block_entity_state(original).is_ok());
        let after = Component::literal("after");
        runtime
            .level_mut()
            .block_entity_state_mut(original)
            .unwrap()
            .container_mut()
            .unwrap()
            .custom_name = Some(after.clone());
        let Some(Tag::Compound(saved)) = runtime.block_entity_nbt(&params).unwrap() else {
            panic!("retained native tag")
        };
        assert_eq!(saved.get("CustomName"), Some(&after.to_nbt().unwrap()));
        assert_eq!(
            runtime
                .block_entity_components(&params)
                .unwrap()
                .unwrap()
                .get(ComponentTypeId::CUSTOM_NAME),
            Some(&DataComponent::CustomName(Box::new(after.clone())))
        );
        assert_eq!(
            runtime.block_entity_display_name(&params).unwrap(),
            Some(NameableName {
                custom_name: Some(after)
            })
        );
        runtime
            .level_mut()
            .block_entity_state_mut(original)
            .unwrap()
            .container_mut()
            .unwrap()
            .custom_name = None;
        assert_eq!(
            runtime.block_entity_display_name(&params).unwrap(),
            Some(NameableName { custom_name: None })
        );
        drop(runtime);
        level.collect_detached_block_entities();
        assert!(level.block_entity_state(original).is_err());
    }
    #[test]
    fn non_nameable_sign_keeps_its_class_even_with_a_custom_name_component() {
        use crate::{
            block::containers::loot_runtime::LiveLootRuntime,
            loot::{LootParams, LootRandomSelection, LootRuntime},
        };
        use ferraloom_nbt::{CompoundTag, Tag};
        use ferraloom_protocol::{chat::Component, component::DataComponent};
        let (mut level, _, old) = setup();
        let state = Block::OAK_SIGN.default_state();
        level.set_block_state(old.pos, state).unwrap();
        let mut receiver = create(old.pos, state).unwrap();
        receiver
            .data_mut()
            .components
            .entries
            .push(DataComponent::CustomName(Box::new(Component::literal(
                "component only",
            ))));
        level.set_block_entity(receiver).unwrap();
        let handle = level.block_entity_ref(old.pos).unwrap().unwrap();
        let params = LootParams {
            block_entity: Some(Tag::Compound(CompoundTag::new())),
            ..Default::default()
        };
        let mut runtime = LiveLootRuntime::new(&mut level, LootRandomSelection::Level);
        runtime.bind_block_entity_source(handle).unwrap();
        assert!(
            runtime
                .block_entity_components(&params)
                .unwrap()
                .unwrap()
                .entries
                .iter()
                .any(|component| matches!(component, DataComponent::CustomName(_)))
        );
        assert_eq!(runtime.block_entity_display_name(&params).unwrap(), None);
    }
    #[test]
    fn sign_edits_notify_at_set_text_and_after_clearing_editor() {
        use crate::block::containers::sign::{self, SignBlockEntity};
        let fixtures = include_str!("../../tests/java/blocks/entities/vectors.tsv");
        let mut checked = 0;
        for line in fixtures
            .lines()
            .filter(|line| line.starts_with("sign-edit-updates\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let (mut level, player, old) = setup();
            let state = Block::OAK_SIGN.default_state();
            level.set_block_state(old.pos, state).unwrap();
            level
                .set_block_entity(create(old.pos, state).unwrap())
                .unwrap();
            let handle = level.block_entity_ref(old.pos).unwrap().unwrap();
            let uuid = level.entity::<ServerPlayer>(player).unwrap().uuid;
            let sign = level.block_entity_mut::<SignBlockEntity>(handle).unwrap();
            sign.is_waxed = fields[1] == "true";
            sign.allowed_player_editor = Some(if fields[2] == "true" {
                uuid
            } else {
                Uuid::new(3, 4)
            });
            let lines = [
                ("first".into(), "first".into()),
                ("second".into(), String::new()),
                ("third".into(), "third".into()),
                ("fourth".into(), "fourth".into()),
            ];
            level.take_effects();
            let accepted =
                sign::update_sign_text(handle, uuid, true, &lines, fields[3] == "true", &mut level)
                    .unwrap();
            let updates = level
                .take_effects()
                .into_iter()
                .filter(|effect| {
                    matches!(
                        effect,
                        crate::level::effects::LevelEffect::BlockUpdated { .. }
                    )
                })
                .count();
            let expected_updates = if fields[4] == "[]" {
                0
            } else {
                fields[4].split(',').count()
            };
            assert_eq!(updates, expected_updates, "{line}");
            assert_eq!(accepted, expected_updates != 0, "{line}");
            let native = fixtures
                .lines()
                .find(|row| {
                    row.starts_with(&format!(
                        "sign-edit\t{}\t{}\t{}\t",
                        fields[1], fields[2], fields[3]
                    ))
                })
                .unwrap();
            let native: Vec<_> = native.split('\t').collect();
            assert_eq!(
                level
                    .block_entity_state(handle)
                    .unwrap()
                    .save_without_metadata()
                    .unwrap(),
                ferraloom_nbt::parse_compound(native[4]).unwrap(),
                "{line}"
            );
            assert_eq!(
                level
                    .block_entity::<SignBlockEntity>(handle)
                    .unwrap()
                    .allowed_player_editor
                    .is_none(),
                native[5] == "true",
                "{line}"
            );
            checked += 1;
        }
        assert_eq!(checked, 8);
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
        open_menu_live(player, &provider, &mut level).unwrap();
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
            open_menu_live(player, &provider, context.level)?;
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
        assert!(player_state.menus.pending_packets.is_empty());
        let packets = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                crate::level::effects::LevelEffect::PlayerPacket { recipients, packet }
                    if recipients == [player] =>
                {
                    Some(*packet)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
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
