//! Port of Containers#dropItemStack and #dropContents. Stack mutations finish
//! their canonical borrow before constructors or add_entity can reenter the level.
use crate::{
    block::entity::BlockEntityRef,
    entity::{EntityRef, EntityState, item::ItemEntity, player::inventory::split},
    inventory::Container,
    level::{Level, LevelError},
};
use ferraloom_data::EntityType;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::{Vec3, random::RandomSource};

fn scatter_origin(level: &mut Level, position: Vec3) -> Vec3 {
    let width = f64::from(EntityType::ITEM.data().width);
    let range = 1.0 - width;
    let random = level.random();
    Vec3::new(
        position.x.floor() + random.next_double() * range + width / 2.0,
        position.y.floor() + random.next_double() * range,
        position.z.floor() + random.next_double() * range + width / 2.0,
    )
}
fn spawn_piece(
    level: &mut Level,
    position: Vec3,
    stack: ItemStack,
) -> Result<EntityRef, LevelError> {
    let mut item = ItemEntity::new_at(level, position, stack);
    let random = level.random();
    item.parent.velocity = Vec3::new(
        random.triangle(0.0, 0.11485000171139836),
        random.triangle(0.2, 0.11485000171139836),
        random.triangle(0.0, 0.11485000171139836),
    );
    let id = level.next_entity_ref();
    level.add_entity(id, EntityState::Item(Box::new(item)))?;
    Ok(id)
}
/// The input must be caller-owned, never a borrow into Level. Even an empty
/// stack consumes the three scatter draws before the emptiness check, as vanilla.
pub fn drop_item_stack(
    level: &mut Level,
    position: Vec3,
    stack: &mut ItemStack,
) -> Result<Vec<EntityRef>, LevelError> {
    let origin = scatter_origin(level, position);
    let mut result = Vec::new();
    while !stack.is_empty() {
        let amount = level.random().next_int_bound(21) + 10;
        let piece = split(stack, amount);
        result.push(spawn_piece(level, origin, piece)?);
    }
    Ok(result)
}
/// Live-slot counterpart of Containers#dropContents. The same receiver generation
/// is reborrowed after every spawn, even if an entity callback replaced its block.
pub fn drop_contents(entity: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
    use super::storage::BaseContainerBlockEntity;
    let size = match level.block_entity_state(entity)? {
        crate::block::entity::BlockEntityState::ChiseledBookShelf(entity) => entity.items.len(),
        _ => level
            .block_entity::<BaseContainerBlockEntity>(entity)?
            .size(),
    };
    let mut operation = crate::inventory::StackOperation::new(level);
    for slot in 0..size {
        let stack = operation
            .pin(crate::inventory::ContainerSlot {
                container: crate::inventory::ContainerHandle::BlockEntity(entity),
                slot,
            })
            .map_err(|_| LevelError::Inventory("container drop stack receiver"))?;
        let origin = scatter_origin(
            operation.level,
            Vec3::new(
                f64::from(entity.pos.x),
                f64::from(entity.pos.y),
                f64::from(entity.pos.z),
            ),
        );
        while !operation
            .level
            .stack(stack)
            .map_err(|_| LevelError::Inventory("container drop stack receiver"))?
            .is_empty()
        {
            let amount = operation.level.random().next_int_bound(21) + 10;
            let piece = split(
                operation
                    .level
                    .stack_mut(stack)
                    .map_err(|_| LevelError::Inventory("container drop stack receiver"))?,
                amount,
            );
            spawn_piece(operation.level, origin, piece)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Behaviors, level::Region};
    use ferraloom_data::Item;
    use ferraloom_util::random::{LegacyRandomSource, Random};
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    #[test]
    fn scatter_matches_vanilla_positions_velocities_counts_and_rng() {
        for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("scatter\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let seed = fields[1].parse().unwrap();
            let count = fields[2].parse().unwrap();
            let mut level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            level.set_random_streams(Random::Legacy(LegacyRandomSource::new(seed)), 0);
            let mut stack = ItemStack::new(Item::STONE, count, Default::default());
            let ids = drop_item_stack(&mut level, Vec3::new(-1.2, 64.7, 2.8), &mut stack).unwrap();
            assert!(stack.is_empty());
            let bits = |v: Vec3| {
                format!(
                    "{:x},{:x},{:x}",
                    v.x.to_bits(),
                    v.y.to_bits(),
                    v.z.to_bits()
                )
            };
            let actual = ids
                .into_iter()
                .map(|id| {
                    let item = level.entity::<ItemEntity>(id).unwrap();
                    format!(
                        "{}|{}|{}",
                        crate::entity::player::inventory::count(item.item()),
                        bits(item.parent.position),
                        bits(item.parent.velocity)
                    )
                })
                .collect::<Vec<_>>()
                .join(";");
            assert_eq!(actual, fields[3], "{line}");
            assert_eq!(
                level.random().next_int().to_string(),
                fields[4],
                "next RNG {line}"
            );
        }
    }
}
