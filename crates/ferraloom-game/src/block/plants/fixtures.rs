//! Controlled native-level fixtures backed by the real vanilla datapack.
use crate::{
    block::containers::runtime::ContainerResources,
    crafting::RecipeManager,
    level::Level,
    loot::{LootData, RegistryHolder, RegistryKey, RegistryLookup, RegistryReference},
};
use ferraloom_data::{
    Block, EntityType, Item,
    datapack::{Pack, ResourceManager, TagSet, load_tags},
};
use std::{
    path::Path,
    sync::{Arc, OnceLock},
};

struct FixtureLookup<'a>(&'a ResourceManager);
impl FixtureLookup<'_> {
    fn value_exists(&self, registry: RegistryKey, id: &ferraloom_protocol::Identifier) -> bool {
        use ferraloom_data::registries as r;
        match registry {
            RegistryKey::Item => Item::from_name(id.as_str()).is_some(),
            RegistryKey::Block => Block::from_name(id.as_str()).is_some(),
            RegistryKey::EntityType => EntityType::from_name(id.as_str()).is_some(),
            RegistryKey::Fluid => r::fluid::from_name(id.as_str()).is_some(),
            RegistryKey::Potion => r::potion::from_name(id.as_str()).is_some(),
            RegistryKey::MobEffect => r::mob_effect::from_name(id.as_str()).is_some(),
            RegistryKey::Attribute => r::attribute::from_name(id.as_str()).is_some(),
            RegistryKey::VillagerType => r::villager_type::from_name(id.as_str()).is_some(),
            RegistryKey::EnvironmentAttribute => {
                r::environment_attribute::from_name(id.as_str()).is_some()
            }
            _ => self
                .0
                .get(
                    &format!("{}:{}/{}.json", id.namespace(), registry.name(), id.path())
                        .parse()
                        .unwrap(),
                )
                .is_some(),
        }
    }
}
impl RegistryLookup for FixtureLookup<'_> {
    fn lookup(
        &self,
        registry: RegistryKey,
        reference: &RegistryReference,
    ) -> crate::loot::Result<Option<RegistryHolder>> {
        let exists = match reference {
            RegistryReference::Value(id) => self.value_exists(registry, id),
            RegistryReference::Tag(id) => {
                let tags = load_tags(self.0, registry.name(), |value, _| {
                    let value = ferraloom_protocol::Identifier::parse(&value.to_string()).unwrap();
                    self.value_exists(registry, &value).then_some(value)
                });
                assert!(
                    tags.diagnostics.is_empty(),
                    "native {} tags: {:?}",
                    registry.name(),
                    tags.diagnostics
                );
                tags.values.contains_key(&id.as_str().parse().unwrap())
            }
        };
        Ok(exists.then(|| RegistryHolder {
            registry,
            reference: reference.clone(),
        }))
    }
}
pub(super) fn install_loot(level: &mut Level) {
    type Services = (
        Arc<ContainerResources>,
        Arc<TagSet<Item>>,
        Arc<TagSet<EntityType>>,
    );
    static SERVICES: OnceLock<Services> = OnceLock::new();
    let (resources, items, entities) = SERVICES.get_or_init(|| {
        let jar =
            std::env::var_os("FERRALOOM_VANILLA_JAR").expect("builder supplies native datapack");
        let resources = ResourceManager::new(vec![Pack::vanilla_jar(Path::new(&jar)).unwrap()]);
        let items = load_tags(&resources, "item", |id, _| Item::from_name(&id.to_string()));
        let entities = load_tags(&resources, "entity_type", |id, _| {
            EntityType::from_name(&id.to_string())
        });
        assert!(items.diagnostics.is_empty());
        assert!(entities.diagnostics.is_empty());
        let items = Arc::new(TagSet::from_tags(items, ferraloom_data::ITEMS.len()).unwrap());
        let entities =
            Arc::new(TagSet::from_tags(entities, ferraloom_data::ENTITY_TYPES.len()).unwrap());
        let loot = LootData::load(&resources, Some(&FixtureLookup(&resources))).unwrap();
        let recipes = Arc::new(RecipeManager::load(&resources, &items).unwrap());
        let recipe_synchronization = Arc::new(crate::crafting::RecipeSynchronization::new(
            &recipes,
            |_| true,
        ));
        let tags = load_tags(&resources, "recipe", |id, _| {
            recipes.by_key(id).map(|recipe| {
                ferraloom_protocol::Identifier::parse(&recipe.id.to_string()).unwrap()
            })
        });
        assert!(tags.diagnostics.is_empty());
        let recipe_tags = tags
            .values
            .into_iter()
            .map(|(id, values)| {
                (
                    ferraloom_protocol::Identifier::parse(&id.to_string()).unwrap(),
                    values,
                )
            })
            .collect();
        let advancements = Arc::new(
            crate::advancements::ServerAdvancementManager::load(
                &resources,
                &crate::advancements::AdvancementRegistryContext {
                    loot: &loot,
                    recipes: &recipes,
                    recipe_tags: &recipe_tags,
                    lookup: Some(&FixtureLookup(&resources)),
                },
            )
            .unwrap(),
        );
        (
            Arc::new(ContainerResources {
                crafting: crate::inventory::RuntimeCraftingContext::fixture(),
                recipes,
                loot: Arc::new(loot),
                advancements,
                recipe_tags,
                recipe_synchronization,
            }),
            items,
            entities,
        )
    });
    level.set_container_resources(resources.clone(), level.context().seed);
    level.set_item_tags(items.clone());
    level.set_entity_type_tags(entities.clone());
}

/// Read a fresh real-level oracle when the explicit integration gate supplies
/// one; ordinary unit tests retain their checked-in native captures.
pub(super) fn native_text(kind: &str, baseline: &[u8]) -> String {
    use std::io::Read;
    let bytes = std::env::var_os("FERRALOOM_PLANTS_ORACLE")
        .map(|path| std::fs::read(path).expect("fresh native plant oracle"));
    let mut text = String::new();
    flate2::read::GzDecoder::new(bytes.as_deref().unwrap_or(baseline))
        .read_to_string(&mut text)
        .unwrap();
    let result = text
        .lines()
        .filter(|line| line.split('\t').next() == Some(kind))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!result.is_empty(), "missing native plant records: {kind}");
    result
}
