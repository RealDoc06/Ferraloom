//! Port of RandomizableContainer#unpackLootTable. Ordered fill placements are
//! delivered to the original receiver, with synchronous callbacks between writes.
use super::loot_runtime::LiveLootRuntime;
use super::storage::RandomizableContainerBlockEntity;
use crate::{
    advancements::criteria,
    block::entity::BlockEntityRef,
    entity::{EntityRef, player::ServerPlayer},
    level::{Level, LevelError},
    loot::{ContextKeySet, LootParams},
};
use ferraloom_protocol::Identifier;
use ferraloom_util::Vec3;

pub fn unpack_loot_table(
    handle: BlockEntityRef,
    player: Option<EntityRef>,
    level: &mut Level,
) -> Result<(), LevelError> {
    let receiver = level.block_entity_state(handle)?;
    let Some(table) = receiver
        .randomizable()
        .and_then(|container| container.loot_table.clone())
    else {
        return Ok(());
    };
    if !receiver.data().has_level {
        return Ok(());
    }
    let resources = level.container_resources()?;
    if !resources.loot.tables.contains_key(&table) {
        return Err(LevelError::Inventory("container loot table lookup"));
    }
    level.with_block_entity_callback(handle, |level| {
        if let Some(player) = player {
            criteria::generate_loot(level, player, &table)?;
        }
        // Clear before evaluation so recursive container reads cannot generate it
        // again. The seed field is retained; NBT emits it only with a loot table.
        let seed = {
            let container = level.block_entity_mut::<RandomizableContainerBlockEntity>(handle)?;
            container.loot_table = None;
            container.loot_table_seed
        };
        let mut params = LootParams {
            origin: Some(Vec3::new(
                f64::from(handle.pos.x) + 0.5,
                f64::from(handle.pos.y) + 0.5,
                f64::from(handle.pos.z) + 0.5,
            )),
            this_entity: player,
            ..Default::default()
        };
        if let Some(player) = player {
            params.luck = level.entity::<ServerPlayer>(player)?.luck();
        }
        params
            .validate(
                &ContextKeySet::vanilla(
                    &Identifier::parse("minecraft:chest").expect("built-in context"),
                )
                .expect("chest context"),
            )
            .map_err(|_| LevelError::Inventory("container loot context"))?;
        let placements = {
            let mut runtime = LiveLootRuntime::for_table(level, &resources.loot, &table, seed)
                .map_err(|_| LevelError::Inventory("container loot stream"))?;
            resources
                .loot
                .fill_placements_with_runtime(&table, &params, &mut runtime, |runtime| {
                    let receiver = runtime
                        .level()
                        .block_entity_state(handle)
                        .map_err(|error| {
                            crate::loot::LootError::Unsupported(format!(
                                "container available slots: {error:?}"
                            ))
                        })?;
                    Ok(receiver
                        .container()
                        .expect("randomizable container")
                        .items
                        .iter()
                        .enumerate()
                        .filter_map(|(slot, stack)| stack.is_empty().then_some(slot))
                        .collect())
                })
                .map_err(|_| LevelError::Inventory("container loot evaluation"))?
        };
        for (slot, stack) in placements {
            super::access::set_item(handle, slot, stack, level)?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        block::containers::{runtime::ContainerResources, storage::BaseContainerBlockEntity},
        block::{
            UnportedBehavior,
            entity::{BlockEntityState, create},
        },
        crafting::RecipeManager,
        entity::{EntityState, player::GameType},
        level::{Region, chunk::LevelChunk},
        loot::{LootData, LootTable},
    };
    use ferraloom_data::{Block, Item};
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::{BlockPos, ChunkPos, Uuid};
    use std::sync::Arc;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    #[test]
    fn lazy_fill_preserves_seed_occupied_slots_and_criterion_boundary() {
        let mut level = {
            let mut fixture_level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        let pos = BlockPos::new(3, 3, 3);
        let section = ferraloom_chunk::LevelChunkSection::new(
            ferraloom_chunk::Strategy::block_states(),
            ferraloom_chunk::Strategy::biomes(Arc::new(
                ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
            )),
            ferraloom_chunk::BiomeId(0),
        );
        let mut chunk = LevelChunk::new(ChunkPos::new(0, 0), 0..1, section);
        chunk.set_block_state(pos, Block::CHEST.default_state());
        level.add_chunk(chunk).unwrap();
        let table = Identifier::parse("test:chest").unwrap();
        let mut entity = create(pos, Block::CHEST.default_state()).unwrap();
        let container = entity.randomizable_mut().unwrap();
        container.loot_table = Some(table.clone());
        container.loot_table_seed = -9123456789;
        container.parent.items[0] = ItemStack::new(Item::DIAMOND, 3, Default::default());
        level.set_block_entity(entity).unwrap();
        let handle = level.block_entity_ref(pos).unwrap().unwrap();
        let mut loot = LootData::default();
        loot.tables.insert(table, LootTable::decode(&serde_json::json!({"type":"chest","pools":[{"rolls":1,"entries":[{"type":"item","name":"stone"}]}]})).unwrap());
        let recipes = Arc::new(RecipeManager::new(Vec::new()).unwrap());
        let recipe_synchronization = Arc::new(crate::crafting::RecipeSynchronization::new(
            &recipes,
            |_| true,
        ));
        let advancement_id = Identifier::parse("test:unsupported_food").unwrap();
        let advancements =
            Arc::new(crate::advancements::ServerAdvancementManager::new(
                vec![crate::advancements::AdvancementHolder {
            id: advancement_id,
            value: crate::advancements::Advancement::decode(&serde_json::json!({
                "criteria":{"food":{"trigger":"player_generates_container_loot","conditions":{
                    "loot_tables":"test:chest",
                    "player":{"type":"entity_properties","entity":"this","predicate":{
                        "minecraft:type_specific/player":{"food":{"level":20}}
                    }}
                }}}
            })).unwrap(),
        }],
            ));
        level.set_container_resources(
            Arc::new(ContainerResources {
                crafting: crate::inventory::RuntimeCraftingContext::fixture(),
                recipes: Arc::clone(&recipes),
                loot: Arc::new(loot),
                advancements: Arc::clone(&advancements),
                recipe_tags: Default::default(),
                recipe_synchronization,
            }),
            0,
        );
        let player = level.next_entity_ref();
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::NIL,
                    "test".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .advancements = crate::advancements::PlayerAdvancements::new(&advancements);
        assert!(matches!(
            unpack_loot_table(handle, Some(player), &mut level),
            Err(LevelError::UnsupportedAdvancementPredicate {
                trigger: criteria::Trigger::GenerateLoot,
                ..
            })
        ));
        assert!(
            level
                .block_entity_state(handle)
                .unwrap()
                .randomizable()
                .unwrap()
                .loot_table
                .is_some()
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        level.set_unported_behavior(UnportedBehavior::Warn);
        unpack_loot_table(handle, Some(player), &mut level).unwrap();
        let container = level
            .block_entity::<RandomizableContainerBlockEntity>(handle)
            .unwrap();
        assert!(container.loot_table.is_none());
        assert_eq!(container.loot_table_seed, -9123456789);
        assert_eq!(
            container.parent.items[0],
            ItemStack::new(Item::DIAMOND, 3, Default::default())
        );
        assert_eq!(
            container
                .parent
                .items
                .iter()
                .filter(|stack| !stack.is_empty())
                .count(),
            2
        );
        assert_eq!(container.parent.times_changed, 1);
        assert_eq!(level.unported_behavior_stats().skipped_calls, 1);
        unpack_loot_table(handle, None, &mut level).unwrap();
        assert_eq!(
            level
                .block_entity::<BaseContainerBlockEntity>(handle)
                .unwrap()
                .times_changed,
            1
        );
        assert!(matches!(
            level.block_entity_state(handle).unwrap(),
            BlockEntityState::Chest(_)
        ));
    }
}
