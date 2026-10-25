//! Pure ItemStack damage queries shared by living equipment and dropped items.
use crate::{
    entity::damage::DamageSource,
    level::{Level, LevelError},
};
use ferraloom_data::registries::data_component_type::Id as ComponentType;
use ferraloom_protocol::{
    codec::HolderSet,
    component::{DataComponent, ItemStack},
};

/// Port of ItemStack#isDamageableItem. Effective component reads hide retained
/// zero/negative-count payloads, while an ordinary undamaged tool has DAMAGE=0.
pub fn is_damageable_item(stack: &ItemStack) -> bool {
    stack
        .component(ComponentType::MAX_DAMAGE)
        .expect("canonical item components")
        .is_some()
        && stack
            .component(ComponentType::UNBREAKABLE)
            .expect("canonical item components")
            .is_none()
        && stack
            .component(ComponentType::DAMAGE)
            .expect("canonical item components")
            .is_some()
}
/// Port of ItemStack#canBeHurtBy and DamageResistant#isResistantTo. Empty stacks
/// have no effective DAMAGE_RESISTANT component, so this query returns true.
pub fn can_be_hurt_by(
    stack: &ItemStack,
    source: &DamageSource,
    level: &Level,
) -> Result<bool, LevelError> {
    let Some(DataComponent::DamageResistant(types)) = stack
        .component(ComponentType::DAMAGE_RESISTANT)
        .expect("canonical item components")
    else {
        return Ok(true);
    };
    let resistant = match &types.0 {
        HolderSet::Direct(ids) => {
            let name = source.type_holder().name().to_string();
            ids.iter().any(|id| id.as_str() == name)
        }
        HolderSet::Named(name) => {
            let name = ferraloom_data::datapack::Identifier::new(name.namespace(), name.path())
                .map_err(|_| LevelError::MissingDamageTypeTag)?;
            let tags = level.damage_types().tags();
            // TODO(5a): registry-aware decoding rejects unbound named sets before this query.
            let tag = tags
                .resolve_name(&name)
                .ok_or(LevelError::MissingDamageTypeTag)?;
            tags.contains(tag, source.type_holder().id())
        }
    };
    Ok(!resistant)
}

#[cfg(test)]
mod tests;
