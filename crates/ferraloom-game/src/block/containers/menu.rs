//! Port of container MenuProvider bindings and native opener side effects.
use super::{chest::ChestBlockEntity, storage::BaseContainerBlockEntity};
use crate::{
    block::{
        UpdateFlags,
        entity::{BlockEntityRef, BlockEntityState, BlockEntityType},
    },
    entity::{
        EntityRef,
        player::{GameType, ServerPlayer},
    },
    inventory::{AbstractContainerMenu, Container, ContainerId, MenuKind, MenuProvider},
    level::{Level, LevelError, events::BlockEvent},
};
use ferraloom_data::{
    Block, BlockState, properties as p,
    registries::{game_event::Id as GameEvent, sound_event::Id as SoundEvent},
};
use ferraloom_protocol::{chat::Component, play::SoundSource};
use ferraloom_util::{BlockPos, Direction, Vec3, random::RandomSource};

pub struct ContainerMenuProvider {
    pub entities: Vec<BlockEntityRef>,
    pub kind: MenuKind,
    pub title: Component,
}
/// Port of the container blocks' getMenuProvider overrides.
pub fn provider(
    state: BlockState,
    pos: BlockPos,
    level: &Level,
) -> Result<Option<ContainerMenuProvider>, LevelError> {
    let refs = if matches!(
        state.block().block_entity_type(),
        Some(BlockEntityType::CHEST | BlockEntityType::TRAPPED_CHEST)
    ) {
        combine_chest(state, pos, false, level)?
    } else {
        level.block_entity_ref(pos)?.into_iter().collect()
    };
    if refs.is_empty() {
        return Ok(None);
    }
    if level.block_entity_state(refs[0])?.container().is_none() {
        return Ok(None);
    }
    let kind = match state.block().block_entity_type().unwrap() {
        BlockEntityType::CHEST | BlockEntityType::TRAPPED_CHEST | BlockEntityType::BARREL => {
            MenuKind::Chest {
                rows: if refs.len() == 2 { 6 } else { 3 },
            }
        }
        BlockEntityType::FURNACE => MenuKind::Furnace,
        BlockEntityType::SMOKER => MenuKind::Smoker,
        BlockEntityType::BLAST_FURNACE => MenuKind::BlastFurnace,
        BlockEntityType::CRAFTER => MenuKind::Crafter,
        BlockEntityType::SHULKER_BOX => MenuKind::ShulkerBox,
        BlockEntityType::DISPENSER | BlockEntityType::DROPPER => MenuKind::Dispenser,
        BlockEntityType::HOPPER => MenuKind::Hopper,
        _ => return Err(LevelError::UnsupportedBlockEntity(state.block())),
    };
    let custom = refs.iter().find_map(|&handle| {
        level
            .block_entity::<BaseContainerBlockEntity>(handle)
            .ok()
            .and_then(|entity| entity.custom_name.clone())
    });
    let key = match kind {
        MenuKind::Chest { rows: 6 } => "container.chestDouble",
        MenuKind::Chest { .. } if state.block() == Block::BARREL => "container.barrel",
        MenuKind::Chest { .. } => "container.chest",
        MenuKind::Furnace => "container.furnace",
        MenuKind::Smoker => "container.smoker",
        MenuKind::BlastFurnace => "container.blast_furnace",
        MenuKind::Crafter => "container.crafter",
        MenuKind::ShulkerBox => "container.shulkerBox",
        MenuKind::Dispenser if state.block() == Block::DROPPER => "container.dropper",
        MenuKind::Dispenser => "container.dispenser",
        MenuKind::Hopper => "container.hopper",
        _ => unreachable!(),
    };
    Ok(Some(ContainerMenuProvider {
        entities: refs,
        kind,
        title: custom.unwrap_or_else(|| Component::translatable(key, Vec::new())),
    }))
}
impl MenuProvider for ContainerMenuProvider {
    fn block_entity_roots(&self) -> &[BlockEntityRef] {
        &self.entities
    }
    fn display_name(&self) -> Component {
        self.title.clone()
    }
    fn create_menu(
        &self,
        id: i32,
        player: EntityRef,
        level: &mut Level,
    ) -> crate::inventory::Result<Option<AbstractContainerMenu>> {
        let spectator = level.entity::<ServerPlayer>(player)?.game_mode == GameType::Spectator;
        let hand = level
            .entity::<ServerPlayer>(player)?
            .get_main_hand_item()
            .clone();
        for &handle in &self.entities {
            let state = level.block_entity_state(handle)?;
            if spectator
                && state
                    .randomizable()
                    .is_some_and(|container| container.loot_table.is_some())
            {
                return Ok(None);
            }
            if !spectator
                && let Some(lock) = &state.container().expect("container provider").lock
                && !crate::predicates::ItemPredicate(lock.clone())
                    .matches(&hand, level)
                    .map_err(|_| {
                        crate::inventory::MenuError::Unsupported("container lock predicate facts")
                    })?
            {
                return Ok(None);
            }
        }
        for &handle in &self.entities {
            super::loot::unpack_loot_table(handle, Some(player), level)?;
        }
        let mut menu = AbstractContainerMenu::new(self.kind, id)?;
        let actual = self
            .entities
            .iter()
            .map(|&handle| {
                level
                    .block_entity::<BaseContainerBlockEntity>(handle)
                    .map(Container::size)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum();
        let expected = menu.local[0].size();
        if actual < expected {
            return Err(crate::inventory::MenuError::ContainerTooSmall { actual, expected });
        }
        for slot in &mut menu.slots {
            if slot.container == ContainerId::Local(0) {
                slot.container = ContainerId::External(0);
            }
        }
        menu.block_entities = self.entities.clone();
        for &handle in &self.entities {
            start_open(handle, player, level)?;
        }
        Ok(Some(menu))
    }
}
pub fn facing(state: BlockState) -> Direction {
    match state.get(p::FACING).expect("container facing") {
        p::Facing::Down => Direction::Down,
        p::Facing::Up => Direction::Up,
        p::Facing::North => Direction::North,
        p::Facing::South => Direction::South,
        p::Facing::West => Direction::West,
        p::Facing::East => Direction::East,
    }
}
pub fn connected_direction(state: BlockState) -> Direction {
    if state.get(p::TYPE_) == Some(p::Type::Left) {
        facing(state).clockwise()
    } else {
        facing(state).counter_clockwise()
    }
}
fn chest_blocked(pos: BlockPos, level: &Level) -> Result<bool, LevelError> {
    let above = pos.above(1);
    let state = level.get_block_state(above)?;
    if crate::block::LevelReaderRef::Server(level).is_redstone_conductor(state, above)? {
        return Ok(true);
    }
    let bounds = ferraloom_util::Aabb::new(
        f64::from(pos.x),
        f64::from(pos.y) + 1.0,
        f64::from(pos.z),
        f64::from(pos.x) + 1.0,
        f64::from(pos.y) + 2.0,
        f64::from(pos.z) + 1.0,
    );
    for id in level.entities_in(bounds, |_, _| true) {
        if level.entity::<crate::entity::EntityData>(id)?.entity_type
            == ferraloom_data::EntityType::CAT
        {
            return Err(LevelError::Inventory(
                "cat sitting pose for chest obstruction",
            ));
        }
    }
    Ok(false)
}
/// Port of ChestBlock#combine and DoubleBlockCombiner. RIGHT is the first half.
pub fn combine_chest(
    state: BlockState,
    pos: BlockPos,
    ignore_blocked: bool,
    level: &Level,
) -> Result<Vec<BlockEntityRef>, LevelError> {
    let Some(entity) = level.block_entity_ref(pos)? else {
        return Ok(Vec::new());
    };
    if level.block_entity::<ChestBlockEntity>(entity).is_err() {
        return Ok(Vec::new());
    }
    if !ignore_blocked && chest_blocked(pos, level)? {
        return Ok(Vec::new());
    }
    let kind = state.get(p::TYPE_).expect("chest type");
    if kind == p::Type::Single {
        return Ok(vec![entity]);
    }
    let neighbor_pos = pos.relative(connected_direction(state), 1);
    let neighbor_state = level.get_block_state(neighbor_pos)?;
    if neighbor_state.block() == state.block()
        && neighbor_state.get(p::TYPE_) != Some(p::Type::Single)
        && neighbor_state.get(p::TYPE_) != Some(kind)
        && neighbor_state.get(p::FACING) == state.get(p::FACING)
    {
        if !ignore_blocked && chest_blocked(neighbor_pos, level)? {
            return Ok(Vec::new());
        }
        if let Some(neighbor) = level.block_entity_ref(neighbor_pos)?
            && level.block_entity::<ChestBlockEntity>(neighbor).is_ok()
        {
            return Ok(if kind == p::Type::Right {
                vec![entity, neighbor]
            } else {
                vec![neighbor, entity]
            });
        }
    }
    Ok(vec![entity])
}
fn counter(state: &mut BlockEntityState) -> Option<&mut i32> {
    match state {
        BlockEntityState::Chest(entity) => Some(&mut entity.open_count),
        BlockEntityState::Barrel(entity) => Some(&mut entity.open_count),
        BlockEntityState::ShulkerBox(entity) => Some(&mut entity.open_count),
        _ => None,
    }
}
pub fn start_open(
    entity: BlockEntityRef,
    player: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    change_openers(entity, Some(player), 1, level)
}
pub fn stop_open(
    entity: BlockEntityRef,
    player: EntityRef,
    level: &mut Level,
) -> Result<(), LevelError> {
    change_openers(entity, Some(player), -1, level)
}
/// Port of ChestBlockEntity#getEntitiesWithContainerOpen and
/// ContainerOpenersCounter#getEntitiesWithContainerOpen. Menu membership uses
/// the receiver generation, including either member of a compound container.
pub fn has_entities_with_container_open(
    receiver: BlockEntityRef,
    level: &Level,
) -> Result<bool, LevelError> {
    let range = level
        .block_entity::<ChestBlockEntity>(receiver)?
        .max_interaction_range
        + 4.0;
    let pos = receiver.pos;
    let bounds = ferraloom_util::Aabb::new(
        f64::from(pos.x),
        f64::from(pos.y),
        f64::from(pos.z),
        f64::from(pos.x) + 1.0,
        f64::from(pos.y) + 1.0,
        f64::from(pos.z) + 1.0,
    )
    .inflate_uniform(range);
    let mut found = false;
    // Native getEntities evaluates the entire predicate before isEmpty is read.
    for entity in level.entities_in(bounds, |_, _| true) {
        let state = level.entity_state(entity)?;
        if let crate::entity::EntityState::Player(player) = state {
            let menu = player.menus.current();
            found |= player.game_mode != GameType::Spectator
                && matches!(menu.kind, MenuKind::Chest { .. })
                && menu.block_entities.contains(&receiver);
        } else if state.entity_type() == ferraloom_data::EntityType::COPPER_GOLEM {
            return Err(LevelError::UnsupportedEntity(
                ferraloom_data::EntityType::COPPER_GOLEM,
            ));
        }
    }
    Ok(found)
}
fn change_openers(
    entity: BlockEntityRef,
    player: Option<EntityRef>,
    delta: i32,
    level: &mut Level,
) -> Result<(), LevelError> {
    if player.is_some_and(|player| {
        level
            .entity::<ServerPlayer>(player)
            .is_ok_and(|player| player.game_mode == GameType::Spectator)
    }) {
        return Ok(());
    }
    level.with_block_entity_callback(entity, |level| {
        if level.block_entity_state(entity)?.data().removed {
            return Ok(());
        }
        // The native startOpen argument is evaluated before incrementOpeners
        // invokes sounds, game events, opener notifications or neighbors.
        let opening_range = if delta > 0 {
            player
                .map(|player| {
                    level
                        .entity::<ServerPlayer>(player)
                        .map(ServerPlayer::block_interaction_range)
                })
                .transpose()?
        } else {
            None
        };
        let (state, kind, previous, current) = {
            let receiver = level.block_entity_state_mut(entity)?;
            if receiver.data().removed {
                return Ok(());
            }
            let state = receiver.data().block_state;
            let kind = receiver.data().kind;
            let Some(count) = counter(receiver) else {
                return Ok(());
            };
            let previous = *count;
            *count += delta;
            (state, kind, previous, *count)
        };
        let transition = (delta > 0 && previous == 0) || (delta < 0 && current == 0);
        if transition {
            let open = delta > 0;
            let mut position = Vec3::new(
                f64::from(entity.pos.x) + 0.5,
                f64::from(entity.pos.y) + 0.5,
                f64::from(entity.pos.z) + 0.5,
            );
            let (sound, play) = match kind {
                BlockEntityType::CHEST | BlockEntityType::TRAPPED_CHEST => {
                    let half = state.get(p::TYPE_).unwrap();
                    if half == p::Type::Right {
                        position = position + connected_direction(state).unit_vec3().scale(0.5);
                    }
                    let sound =
                        if state.block() == Block::CHEST || state.block() == Block::TRAPPED_CHEST {
                            if open {
                                "minecraft:block.chest.open"
                            } else {
                                "minecraft:block.chest.close"
                            }
                        } else if open {
                            "minecraft:block.copper_chest.open"
                        } else {
                            "minecraft:block.copper_chest.close"
                        };
                    (sound, half != p::Type::Left)
                }
                BlockEntityType::BARREL => {
                    position = position + facing(state).unit_vec3().scale(0.5);
                    (
                        if open {
                            "minecraft:block.barrel.open"
                        } else {
                            "minecraft:block.barrel.close"
                        },
                        true,
                    )
                }
                BlockEntityType::SHULKER_BOX => (
                    if open {
                        "minecraft:block.shulker_box.open"
                    } else {
                        "minecraft:block.shulker_box.close"
                    },
                    true,
                ),
                _ => return Ok(()),
            };
            if play {
                let pitch = level.random().next_float() * 0.1 + 0.9;
                level.play_sound(
                    None,
                    position,
                    SoundEvent::from_name(sound).expect("container sound"),
                    SoundSource::Blocks,
                    0.5,
                    pitch,
                )?;
            }
            if kind == BlockEntityType::BARREL {
                let state = state.with(p::OPEN, open).unwrap();
                level.set_block(
                    entity.pos,
                    state,
                    UpdateFlags::ALL,
                    crate::block::UPDATE_LIMIT,
                )?;
            }
            level.game_event(
                if open {
                    GameEvent::CONTAINER_OPEN
                } else {
                    GameEvent::CONTAINER_CLOSE
                },
                Vec3::new(
                    f64::from(entity.pos.x) + 0.5,
                    f64::from(entity.pos.y) + 0.5,
                    f64::from(entity.pos.z) + 0.5,
                ),
                player,
                None,
            )?;
            if open && kind != BlockEntityType::SHULKER_BOX {
                level.schedule_block_tick_after(entity.pos, state.block(), 5)?;
            }
        }
        if delta < 0
            && current == 0
            && let BlockEntityState::Chest(chest) = level.block_entity_state_mut(entity)?
        {
            chest.max_interaction_range = 0.0;
        }
        if kind == BlockEntityType::CHEST
            || kind == BlockEntityType::TRAPPED_CHEST
            || kind == BlockEntityType::SHULKER_BOX
        {
            level.block_event(BlockEvent {
                pos: entity.pos,
                block: state.block(),
                param_a: 1,
                param_b: current,
            });
        }
        if kind == BlockEntityType::TRAPPED_CHEST && previous != current {
            level.update_neighbors_at(entity.pos, state.block())?;
            level.update_neighbors_at(entity.pos.below(1), state.block())?;
        }
        if let Some(range) = opening_range
            && let BlockEntityState::Chest(chest) = level.block_entity_state_mut(entity)?
        {
            chest.max_interaction_range =
                ferraloom_util::java::max(range, chest.max_interaction_range);
        }
        Ok(())
    })
}

#[cfg(test)]
mod opener_tests;
