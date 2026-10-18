use super::{Error, RecipeHolder, RecipeManager, RecipeSynchronization, Result};
use ferraloom_data::datapack::Identifier;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::play::{
    RecipeBookEntry, RecipeBookSettings, RecipeBookType, RecipeBookTypeSettings, RecipeDisplayId,
    clientbound::{RecipeBookAdd, RecipeBookRemove, RecipeBookSettingsPacket},
};

/// Port of ServerRecipeBook. Gameplay owns sending packets and advancement callbacks;
/// these methods return packet data before the caller performs those synchronous effects.
#[derive(Clone, Debug)]
pub struct ServerRecipeBook {
    known: Vec<Identifier>,
    highlight: Vec<Identifier>,
    pub settings: RecipeBookSettings,
}
impl Default for ServerRecipeBook {
    fn default() -> Self {
        Self {
            known: Vec::new(),
            highlight: Vec::new(),
            settings: RecipeBookSettings(std::array::from_fn(|_| RecipeBookTypeSettings {
                open: false,
                filtering: false,
            })),
        }
    }
}
#[derive(Clone, Debug)]
pub struct RecipeBookAddition {
    pub packet: Option<RecipeBookAdd>,
    pub unlocked: Vec<Identifier>,
}
impl ServerRecipeBook {
    pub fn contains(&self, id: &Identifier) -> bool {
        self.known.contains(id)
    }
    pub fn known(&self) -> &[Identifier] {
        &self.known
    }
    pub fn highlighted(&self, id: &Identifier) -> bool {
        self.highlight.contains(id)
    }
    pub fn add(&mut self, id: Identifier) {
        if !self.known.contains(&id) {
            self.known.push(id);
        }
    }
    pub fn remove(&mut self, id: &Identifier) {
        self.known.retain(|known| known != id);
        self.remove_highlight(id);
    }
    pub fn remove_highlight(&mut self, id: &Identifier) {
        self.highlight.retain(|highlight| highlight != id);
    }
    /// Port of ServerRecipeBook#addRecipes. `unlocked` drives RECIPE_UNLOCKED in
    /// encounter order, including recipes whose displays were feature-filtered.
    pub fn add_recipes<'a>(
        &mut self,
        recipes: impl IntoIterator<Item = &'a RecipeHolder>,
        sync: &RecipeSynchronization,
    ) -> RecipeBookAddition {
        let mut entries = Vec::new();
        let mut unlocked = Vec::new();
        for recipe in recipes {
            if !self.contains(&recipe.id) && !recipe.value.is_special() {
                self.add(recipe.id.clone());
                if !self.highlight.contains(&recipe.id) {
                    self.highlight.push(recipe.id.clone());
                }
                for display in sync.displays_for_recipe(&recipe.id) {
                    entries.push(RecipeBookEntry {
                        contents: display.clone(),
                        flags: 2 | u8::from(recipe.value.show_notification),
                    });
                }
                unlocked.push(recipe.id.clone());
            }
        }
        RecipeBookAddition {
            packet: (!entries.is_empty()).then_some(RecipeBookAdd {
                entries,
                replace: false,
            }),
            unlocked,
        }
    }
    pub fn remove_recipes<'a>(
        &mut self,
        recipes: impl IntoIterator<Item = &'a RecipeHolder>,
        sync: &RecipeSynchronization,
    ) -> Option<RecipeBookRemove> {
        let mut removed = Vec::new();
        for recipe in recipes {
            if self.contains(&recipe.id) {
                self.remove(&recipe.id);
                removed.extend(
                    sync.displays_for_recipe(&recipe.id)
                        .map(|display| display.id),
                );
            }
        }
        (!removed.is_empty()).then_some(RecipeBookRemove { recipes: removed })
    }
    pub fn mark_display_seen(&mut self, sync: &RecipeSynchronization, id: RecipeDisplayId) {
        if let Some(info) = sync.get_recipe_from_display(id) {
            self.remove_highlight(&info.parent);
        }
    }
    /// Settings precede replace=true entries when joining or reloading.
    pub fn initial_packets(
        &self,
        sync: &RecipeSynchronization,
    ) -> (RecipeBookSettingsPacket, RecipeBookAdd) {
        let entries = self
            .known
            .iter()
            .flat_map(|id| {
                sync.displays_for_recipe(id).map(|display| RecipeBookEntry {
                    contents: display.clone(),
                    flags: if self.highlighted(id) { 2 } else { 0 },
                })
            })
            .collect();
        (
            RecipeBookSettingsPacket {
                book_settings: self.settings.clone(),
            },
            RecipeBookAdd {
                entries,
                replace: true,
            },
        )
    }
    pub fn change_settings(&mut self, kind: RecipeBookType, open: bool, filtering: bool) {
        self.settings.0[kind as usize] = RecipeBookTypeSettings { open, filtering };
    }
    /// Port of ServerRecipeBook.Packed#CODEC. Embed under player data `recipeBook`.
    pub fn to_nbt(&self) -> Tag {
        let mut value = CompoundTag::new();
        for (index, (open, filter)) in SETTING_KEYS.into_iter().enumerate() {
            let settings = &self.settings.0[index];
            if settings.open {
                value.insert(open, Tag::Byte(1));
            }
            if settings.filtering {
                value.insert(filter, Tag::Byte(1));
            }
        }
        value.insert(
            "recipes",
            Tag::List(ListTag(
                self.known
                    .iter()
                    .map(|id| Tag::String(id.to_string().into()))
                    .collect(),
            )),
        );
        value.insert(
            "toBeDisplayed",
            Tag::List(ListTag(
                self.highlight
                    .iter()
                    .map(|id| Tag::String(id.to_string().into()))
                    .collect(),
            )),
        );
        Tag::Compound(value)
    }
    /// Port of loadUntrusted: unknown recipe ids are discarded, recognized ids are
    /// accepted even if special. Loading adds to existing known/highlight sets.
    pub fn load_untrusted(&mut self, nbt: &Tag, manager: &RecipeManager) -> Result<()> {
        let value = nbt
            .as_compound()
            .ok_or_else(|| Error::Invalid("recipeBook must be a compound".into()))?;
        let mut settings = self.settings.clone();
        for (index, (open, filter)) in SETTING_KEYS.into_iter().enumerate() {
            let boolean = |key| -> Result<bool> {
                Ok(match value.get(key) {
                    None => false,
                    Some(Tag::Byte(value)) => *value != 0,
                    Some(Tag::Short(value)) => *value as i8 != 0,
                    Some(Tag::Int(value)) => *value as i8 != 0,
                    Some(Tag::Long(value)) => *value as i8 != 0,
                    Some(Tag::Float(value)) => *value as i32 as i8 != 0,
                    Some(Tag::Double(value)) => *value as i32 as i8 != 0,
                    _ => return Err(Error::Invalid("recipe book setting must be numeric".into())),
                })
            };
            settings.0[index] = RecipeBookTypeSettings {
                open: boolean(open)?,
                filtering: boolean(filter)?,
            };
        }
        let mut lists = [Vec::new(), Vec::new()];
        for (index, field) in ["recipes", "toBeDisplayed"].into_iter().enumerate() {
            let Some(Tag::List(recipes)) = value.get(field) else {
                return Err(Error::Invalid(format!("missing recipeBook list {field}")));
            };
            for recipe in recipes.iter() {
                let Tag::String(id) = recipe else {
                    return Err(Error::Invalid("recipe book id must be a string".into()));
                };
                let id: Identifier = id.to_string_lossy().parse()?;
                if manager.by_key(&id).is_some() {
                    lists[index].push(id);
                }
            }
        }
        self.settings = settings;
        for (target, values) in [&mut self.known, &mut self.highlight]
            .into_iter()
            .zip(lists)
        {
            for id in values {
                if !target.contains(&id) {
                    target.push(id);
                }
            }
        }

        Ok(())
    }
}
const SETTING_KEYS: [(&str, &str); 4] = [
    ("isGuiOpen", "isFilteringCraftable"),
    ("isFurnaceGuiOpen", "isFurnaceFilteringCraftable"),
    (
        "isBlastingFurnaceGuiOpen",
        "isBlastingFurnaceFilteringCraftable",
    ),
    ("isSmokerGuiOpen", "isSmokerFilteringCraftable"),
];
