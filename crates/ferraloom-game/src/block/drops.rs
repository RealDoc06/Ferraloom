//! Port of Block's loot contexts and synchronous resource spawning.
use super::{BlockMethod, entity::BlockEntityRef};
use crate::{
    entity::EntityRef,
    level::{Level, LevelError},
    loot::{ContextKeySet, LootParams},
};
use ferraloom_data::BlockState;
use ferraloom_nbt::Tag;
use ferraloom_protocol::{Identifier, component::ItemStack};
use ferraloom_util::{BlockPos, Vec3};

/// Port of Block#getDrops's overload with an empty tool and no breaker.
pub fn get_drops(
    state: BlockState,
    pos: BlockPos,
    block_entity: Option<BlockEntityRef>,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    get_drops_with_tool(state, pos, block_entity, None, &ItemStack::default(), level)
}
pub fn get_drops_with_tool(
    state: BlockState,
    pos: BlockPos,
    block_entity: Option<BlockEntityRef>,
    breaker: Option<EntityRef>,
    tool: &ItemStack,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    let mut params = LootParams {
        origin: Some(Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        )),
        tool: Some(tool.clone()),
        this_entity: breaker,
        ..Default::default()
    };
    if let Some(handle) = block_entity {
        let receiver = level.block_entity_state(handle)?;
        params.block_entity =
            Some(Tag::Compound(receiver.save_with_full_metadata().map_err(
                |_| LevelError::Inventory("block drop entity NBT"),
            )?));
        params.block_entity_components = Some(receiver.collect_components());
    }
    get_drops_from_params(state, params, level)
}
/// Callers such as explosions can supply the native optional BLOCK parameters.
pub fn get_drops_from_params(
    state: BlockState,
    params: LootParams,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    level
        .required_block_behavior(state.block(), BlockMethod::GetDrops)?
        .get_drops(state, params, level)
}
/// Port of BlockBehaviour#getDrops; absence returns before context validation.
pub(super) fn get_base_drops(
    state: BlockState,
    mut params: LootParams,
    level: &mut Level,
) -> Result<Vec<ItemStack>, LevelError> {
    let Some(name) = state.block().loot_table_name() else {
        return Ok(Vec::new());
    };
    params.block_state = Some(state);
    params
        .validate(
            &ContextKeySet::vanilla(&Identifier::parse("block").expect("built-in context"))
                .expect("block context"),
        )
        .map_err(|_| LevelError::Inventory("block loot context"))?;
    let resources = level.container_resources()?;
    let table = Identifier::parse(name).expect("extracted loot-table identifier");
    level
        .with_loot_sources(|sources| {
            resources.loot.get_random_items_seeded(
                &table,
                &params,
                0,
                sources,
                resources.facts.as_ref(),
            )
        })
        .map_err(|_| LevelError::Inventory("block loot evaluation"))
}
