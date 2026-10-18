//! Immutable structure registry/holder resolution shared by live queries.
use ferraloom_data::datapack::{ResourceManager, load_tags};
use ferraloom_worldgen::{
    Error, Identifier, generator::GenerationData, structure::pipeline::StructureRegistry,
};
use std::collections::BTreeMap;

/// The actual registry and datapack tags, independent of generator placement
/// eligibility. Flat worlds still have the full registry and empty chunk maps.
pub struct StructureServices {
    pub registry: StructureRegistry,
    pub tags: BTreeMap<Identifier, Vec<Identifier>>,
}
impl StructureServices {
    pub fn load(resources: &ResourceManager, data: &GenerationData) -> Result<Self, Error> {
        let registry = StructureRegistry::load(resources, data)?;
        let tags = load_tags(resources, "worldgen/structure", |id, _| {
            registry.structures.contains_key(id).then(|| id.clone())
        });
        if !tags.diagnostics.is_empty() {
            return Err(Error(format!(
                "invalid structure tags: {:?}",
                tags.diagnostics
            )));
        }
        Ok(Self {
            registry,
            tags: tags.values,
        })
    }
}
