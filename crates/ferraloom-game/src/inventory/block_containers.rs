//! The external-container resolver borrows canonical block entities only for the
//! pure menu operation. World callbacks run after all these borrows have ended.
use super::*;
use crate::{
    block::entity::{BlockEntityRef, BlockEntityState},
    entity::{
        EntityRef,
        player::{GameType, ServerPlayer},
    },
    level::Level,
};
use ferraloom_util::{Aabb, Vec3};

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
/// Current player gameplay supports the native base range and creative modifier.
pub fn block_menu_still_valid(
    player: EntityRef,
    refs: &[BlockEntityRef],
    level: &Level,
) -> Result<bool> {
    let player = level.entity::<ServerPlayer>(player)?;
    let base = &player.parent.parent;
    let eye = Vec3::new(
        base.position.x,
        base.position.y + f64::from(base.eye_height),
        base.position.z,
    );
    let range =
        4.5 + if player.game_mode == GameType::Creative {
            0.5
        } else {
            0.0
        } + 4.0;
    for &handle in refs {
        if level.block_entity_ref(handle.pos)? != Some(handle) {
            return Ok(false);
        }
        let pos = handle.pos;
        let bounds = Aabb::new(
            f64::from(pos.x),
            f64::from(pos.y),
            f64::from(pos.z),
            f64::from(pos.x) + 1.0,
            f64::from(pos.y) + 1.0,
            f64::from(pos.z) + 1.0,
        );
        if bounds.distance_to_sqr(eye) >= range * range {
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
        level.set_changed(handle.pos)?;
        let block = level.block_entity_state(handle)?.data().block_state.block();
        level.update_neighbour_for_output_signal(handle.pos, block)?;
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
        entity::EntityState,
        level::{Region, chunk::LevelChunk},
    };
    use ferraloom_data::{Block, Item};
    use ferraloom_util::{BlockPos, ChunkPos, Uuid};
    use std::sync::Arc;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    fn setup() -> (Level, EntityRef, BlockEntityRef) {
        let mut level = Level::new(Region::new(&BEHAVIORS));
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
