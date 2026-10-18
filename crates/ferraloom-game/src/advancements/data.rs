use super::{AdvancementRequirements, AdvancementTree, Identifier, Result, triggers::Criterion};
use crate::advancements::optional;
use crate::loot::{HolderSet, LootTable, codec};
use ferraloom_data::datapack::ResourceManager;
use ferraloom_protocol::{chat::Component, component::ItemStackTemplate, play};
use serde_json::Value;

/// Port of DisplayInfo. Network forms deliberately omit announce_to_chat.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    pub icon: ItemStackTemplate,
    pub title: Component,
    pub description: Component,
    pub background: Option<Identifier>,
    pub frame: play::AdvancementType,
    pub show_toast: bool,
    pub announce_to_chat: bool,
    pub hidden: bool,
}
impl DisplayInfo {
    pub fn decode(v: &Value) -> Result<Self> {
        Ok(Self {
            icon: codec::persistent(codec::field(v, "icon")?)?,
            title: Component::from_json(&codec::field(v, "title")?.to_string())?,
            description: Component::from_json(&codec::field(v, "description")?.to_string())?,
            background: optional(v, "background", codec::id)?,
            frame: match optional(v, "frame", codec::string)?.unwrap_or("task") {
                "task" => play::AdvancementType::Task,
                "goal" => play::AdvancementType::Goal,
                "challenge" => play::AdvancementType::Challenge,
                _ => return Err(codec::error("unknown advancement frame")),
            },
            show_toast: optional(v, "show_toast", codec::boolean)?.unwrap_or(true),
            announce_to_chat: optional(v, "announce_to_chat", codec::boolean)?.unwrap_or(true),
            hidden: optional(v, "hidden", codec::boolean)?.unwrap_or(false),
        })
    }
    pub fn network(&self) -> play::AdvancementDisplay {
        play::AdvancementDisplay {
            icon: self.icon.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            background: self.background.clone(),
            advancement_type: self.frame,
            show_toast: self.show_toast,
            hidden: self.hidden,
        }
    }
}
/// Port of AdvancementRewards.CODEC. Loot holders reuse the loot registry.
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancementRewards {
    pub experience: i32,
    pub loot: HolderSet<LootTable>,
    pub recipes: Vec<Identifier>,
    pub function: Option<Identifier>,
}
impl Default for AdvancementRewards {
    fn default() -> Self {
        Self {
            experience: 0,
            loot: HolderSet::Direct(Vec::new()),
            recipes: Vec::new(),
            function: None,
        }
    }
}
impl AdvancementRewards {
    pub fn decode(v: &Value) -> Result<Self> {
        Ok(Self {
            experience: optional(v, "experience", codec::int)?.unwrap_or(0),
            loot: optional(v, "loot", |v| HolderSet::decode(v, LootTable::decode))?
                .unwrap_or(HolderSet::Direct(Vec::new())),
            recipes: optional(v, "recipes", |v| codec::list(v, codec::id))?.unwrap_or_default(),
            function: optional(v, "function", codec::id)?,
        })
    }
}
/// Port of Advancement, including 26.3's visible-root background validation.
#[derive(Clone, Debug, PartialEq)]
pub struct Advancement {
    pub parent: Option<Identifier>,
    pub display: Option<DisplayInfo>,
    pub rewards: AdvancementRewards,
    pub criteria: Vec<(String, Criterion)>,
    pub requirements: AdvancementRequirements,
    pub sends_telemetry_event: bool,
}
impl Advancement {
    pub fn decode(v: &Value) -> Result<Self> {
        Self::decode_with_tags(
            v,
            &ferraloom_data::datapack::TagSet::<ferraloom_data::Item>::load(
                &ResourceManager::new(Vec::new()),
            )?,
        )
    }
    pub fn decode_with_tags(
        v: &Value,
        tags: &ferraloom_data::datapack::TagSet<ferraloom_data::Item>,
    ) -> Result<Self> {
        let criteria = codec::object(codec::field(v, "criteria")?)?
            .iter()
            .map(|(name, v)| Ok((name.clone(), Criterion::decode_with_tags(v, tags)?)))
            .collect::<Result<Vec<_>>>()?;
        if criteria.is_empty() {
            return Err(codec::error("Advancement criteria cannot be empty"));
        }
        let requirements = optional(v, "requirements", AdvancementRequirements::decode)?
            .unwrap_or_else(|| {
                AdvancementRequirements::all_of(criteria.iter().map(|(s, _)| s.clone()))
            });
        requirements.validate(criteria.iter().map(|(s, _)| s.as_str()))?;
        let parent = optional(v, "parent", codec::id)?;
        let display = optional(v, "display", DisplayInfo::decode)?;
        if let Some(display) = &display {
            if parent.is_none() && display.background.is_none() {
                return Err(codec::error(
                    "Visible advancement roots must have background",
                ));
            }
            if parent.is_some() && display.background.is_some() {
                return Err(codec::error("Only advancement roots can have background"));
            }
        }
        Ok(Self {
            parent,
            display,
            criteria,
            requirements,
            rewards: optional(v, "rewards", AdvancementRewards::decode)?.unwrap_or_default(),
            sends_telemetry_event: optional(v, "sends_telemetry_event", codec::boolean)?
                .unwrap_or(false),
        })
    }
    pub fn network(&self) -> play::Advancement {
        play::Advancement {
            parent: self.parent.clone(),
            display: self.display.as_ref().map(DisplayInfo::network),
            requirements: self.requirements.0.clone(),
            sends_telemetry_event: self.sends_telemetry_event,
        }
    }
}
/// Port of AdvancementHolder. Identity is its resource identifier.
#[derive(Clone, Debug)]
pub struct AdvancementHolder {
    pub id: Identifier,
    pub value: Advancement,
}
impl PartialEq for AdvancementHolder {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for AdvancementHolder {}
impl AdvancementHolder {
    pub fn network(&self) -> play::AdvancementHolder {
        play::AdvancementHolder {
            id: self.id.clone(),
            value: self.value.network(),
        }
    }
}
/// Port of ServerAdvancementManager. Invalid registry elements abort a reload;
/// missing parents/cycles remain manager entries but are omitted from its tree.
#[derive(Clone, Debug, Default)]
pub struct ServerAdvancementManager {
    pub advancements: Vec<AdvancementHolder>,
    pub tree: AdvancementTree,
    pub validation_problems: Vec<super::AdvancementValidationProblem>,
    pub registry_bindings: Vec<crate::loot::RegistryHolder>,
}
impl ServerAdvancementManager {
    pub fn load(
        resources: &ResourceManager,
        context: &super::AdvancementRegistryContext<'_>,
    ) -> Result<Self> {
        let tags = ferraloom_data::datapack::TagSet::<ferraloom_data::Item>::load(resources)?;
        let advancements = resources
            .registry("advancement")
            .into_iter()
            .map(|(id, resource)| {
                Ok(AdvancementHolder {
                    id: Identifier::parse(&id.to_string())?,
                    value: Advancement::decode_with_tags(&resource.json()?, &tags)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut manager = Self::new(advancements);
        for holder in &manager.advancements {
            let (bindings, problems) = holder.value.bind_and_validate(&holder.id, context)?;
            manager.registry_bindings.extend(bindings);
            manager.validation_problems.extend(problems);
        }
        manager.registry_bindings.sort();
        manager.registry_bindings.dedup();
        Ok(manager)
    }
    pub fn new(advancements: Vec<AdvancementHolder>) -> Self {
        let mut tree = AdvancementTree::default();
        tree.add_all(&advancements);
        tree.reposition_nodes();
        Self {
            advancements,
            tree,
            validation_problems: Vec::new(),
            registry_bindings: Vec::new(),
        }
    }
    pub fn get(&self, id: &Identifier) -> Option<&AdvancementHolder> {
        self.advancements.iter().find(|a| &a.id == id)
    }
}
