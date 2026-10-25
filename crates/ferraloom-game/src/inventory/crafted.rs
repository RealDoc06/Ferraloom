//! Item callbacks reached by result slots, separate from their stat producers.
use super::{Result, StackRef};
use crate::{block::UnportedOperation, level::Level};
use ferraloom_data::Item;
use ferraloom_protocol::component::ComponentTypeId;

/// Port of Item#onCraftedBy / MapItem#onCraftedPostProcess. Map saved-data
/// postprocessing is one policy effect, including removing its component.
pub fn crafted_item_post_process(level: &mut Level, stack: StackRef) -> Result<()> {
    let value = level.stack(stack)?;
    let item = value.item();
    if item == Item::FILLED_MAP
        && value
            .component(ComponentTypeId::MAP_POST_PROCESSING)?
            .is_some()
    {
        level.unported_operation(UnportedOperation::ItemCraftedPostProcess(item))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block::UnportedBehavior,
        entity::{
            EntityRef, EntityState,
            player::{GameType, ServerPlayer},
        },
        inventory::{ContainerHandle, ContainerSlot, ItemStack},
        level::{LevelContext, LevelError, Region},
    };
    use ferraloom_protocol::component::{DataComponent, DataComponentPatch, MapPostProcessing};
    use ferraloom_util::{Uuid, Vec3};

    #[test]
    fn crafted_map_effect_is_a_unit_and_base_callbacks_are_empty() {
        static BEHAVIORS: crate::Behaviors = crate::Behaviors::EMPTY;
        let mut level = {
            let mut fixture_level =
                Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        let player = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        level
            .add_entity(
                player,
                EntityState::Player(Box::new(ServerPlayer::new(
                    Uuid::new(0, 1),
                    "crafted".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        let mut patch = DataComponentPatch::default();
        patch.set(DataComponent::MapPostProcessing(MapPostProcessing::Lock));
        let flagged = ItemStack::new(Item::FILLED_MAP, 1, patch);
        for stack in [
            ItemStack::new(Item::STONE, 1, Default::default()),
            ItemStack::new(Item::FILLED_MAP, 1, Default::default()),
            flagged.clone(),
        ] {
            level
                .entity_mut::<ServerPlayer>(player)
                .unwrap()
                .inventory
                .set_selected_item(stack);
            let reference = level
                .stack_ref(ContainerSlot {
                    container: ContainerHandle::PlayerInventory(player),
                    slot: 0,
                })
                .unwrap();
            if level.stack(reference).unwrap() == &flagged {
                assert!(matches!(
                    crafted_item_post_process(&mut level, reference),
                    Err(crate::inventory::MenuError::Level(
                        LevelError::UnsupportedItemCraftedPostProcess(Item::FILLED_MAP)
                    ))
                ));
                assert_eq!(level.stack(reference).unwrap(), &flagged);
                level.set_unported_behavior(UnportedBehavior::Warn);
                crafted_item_post_process(&mut level, reference).unwrap();
                crafted_item_post_process(&mut level, reference).unwrap();
                assert_eq!(level.stack(reference).unwrap(), &flagged);
                assert_eq!(level.unported_behavior_stats().skipped_calls, 2);
                assert_eq!(level.unported_behavior_stats().distinct_calls, 1);
            } else {
                crafted_item_post_process(&mut level, reference).unwrap();
                assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
            }
        }
    }
}
