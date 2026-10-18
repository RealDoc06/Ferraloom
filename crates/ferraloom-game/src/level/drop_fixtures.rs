//! Actual vanilla loot services for tests that execute synchronous block drops.
use super::Level;
use crate::{
    block::containers::runtime::ContainerResources,
    crafting::RecipeManager,
    loot::{LootData, RegistryHolder, RegistryKey, RegistryLookup, RegistryReference},
};
use ferraloom_data::datapack::{Pack, ResourceManager, TagSet};
use std::sync::{Arc, LazyLock};

struct Registries<'a>(&'a ResourceManager);
impl RegistryLookup for Registries<'_> {
    fn lookup(
        &self,
        registry: RegistryKey,
        reference: &RegistryReference,
    ) -> crate::loot::Result<Option<RegistryHolder>> {
        let id = reference.id();
        let exists = match reference {
            RegistryReference::Value(_) => {
                ferraloom_data::registry_entries(&format!("minecraft:{}", registry.name()))
                    .is_some_and(|entries| entries.contains(&id.as_str()))
                    || self
                        .0
                        .get(
                            &format!("{}:{}/{}.json", id.namespace(), registry.name(), id.path())
                                .parse()
                                .unwrap(),
                        )
                        .is_some()
            }
            RegistryReference::Tag(_) => self
                .0
                .get(
                    &format!(
                        "{}:tags/{}/{}.json",
                        id.namespace(),
                        registry.name(),
                        id.path()
                    )
                    .parse()
                    .unwrap(),
                )
                .is_some(),
        };
        Ok(exists.then(|| RegistryHolder {
            registry,
            reference: reference.clone(),
        }))
    }
}

pub(crate) fn install(level: &mut Level) {
    static SERVICES: LazyLock<Arc<ContainerResources>> = LazyLock::new(|| {
        let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder vanilla jar");
        let resources =
            ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
        Arc::new(ContainerResources {
            crafting: crate::inventory::RuntimeCraftingContext::fixture(),
            recipes: Arc::new(
                RecipeManager::load(&resources, &TagSet::load(&resources).unwrap()).unwrap(),
            ),
            loot: Arc::new(LootData::load(&resources, Some(&Registries(&resources))).unwrap()),
        })
    });
    level.set_container_resources(SERVICES.clone(), level.seed());
    level.rules_mut().block_drops = Some(true);
}
