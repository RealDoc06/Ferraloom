use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{Identifier, component::{DataComponent, DataComponentMap, DataComponentPatch}};

pub(super) fn load(tag: Option<&CompoundTag>) -> DataComponentMap {
    DataComponentMap { entries: tag.into_iter().flat_map(CompoundTag::iter)
        .filter_map(|(key,value)| Identifier::parse(&key.to_string_lossy()).ok()
            .and_then(|key| DataComponent::from_nbt(&key,value).ok())).collect() }
}
pub(super) fn save(map: &DataComponentMap) -> ferraloom_protocol::Result<CompoundTag> {
    let mut result = CompoundTag::new();
    for value in &map.entries { result.insert(value.key().as_str(),value.to_nbt()?); }
    Ok(result)
}
pub(crate) fn set(map: &mut DataComponentMap, component: DataComponent) {
    map.apply(&DataComponentPatch { added: vec![component], removed: Vec::new() });
}
pub(super) fn save_field(tag: &mut CompoundTag, map: &DataComponentMap) -> ferraloom_protocol::Result<()> {
    // DataComponentMap.CODEC encodes the empty map too.
    tag.insert("components",Tag::Compound(save(map)?));
    Ok(())
}
