use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    component::{DataComponent, DataComponentMap},
};

pub(super) fn load(
    tag: Option<&CompoundTag>,
    registries: &dyn ferraloom_protocol::codec::RegistryContext,
) -> DataComponentMap {
    DataComponentMap {
        entries: tag
            .into_iter()
            .flat_map(CompoundTag::iter)
            .filter_map(|(key, value)| {
                Identifier::parse(&key.to_string_lossy())
                    .ok()
                    .and_then(|key| {
                        if key.as_str() == "minecraft:banner_patterns" {
                            super::banner::read_patterns(value, registries)
                                .map(DataComponent::BannerPatterns)
                        } else {
                            DataComponent::from_nbt(&key, value).ok()
                        }
                    })
            })
            .collect(),
    }
}
pub(super) fn save(map: &DataComponentMap) -> ferraloom_protocol::Result<CompoundTag> {
    let mut result = CompoundTag::new();
    for value in &map.entries {
        result.insert(value.key().as_str(), value.to_nbt()?);
    }
    Ok(result)
}
pub(super) fn save_field(
    tag: &mut CompoundTag,
    map: &DataComponentMap,
) -> ferraloom_protocol::Result<()> {
    // DataComponentMap.CODEC encodes the empty map too.
    tag.insert("components", Tag::Compound(save(map)?));
    Ok(())
}
