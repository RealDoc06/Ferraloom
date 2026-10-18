use super::{
    CraftingContext, CraftingInput, CraftingInputAccess, Error, Recipe, RecipeInput, RecipeType,
    Result, stack,
};
use ferraloom_data::{
    Item,
    datapack::{Identifier, ResourceManager, TagSet},
};
use std::{collections::BTreeMap, sync::Arc};

/// Port of RecipeHolder. IDs use Identifier's vanilla path-before-namespace order.
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeHolder {
    pub id: Identifier,
    pub value: Recipe,
}
/// Port of RecipeMap and RecipeManager. Registry encounter order is retained.
#[derive(Clone, Debug)]
pub struct RecipeManager {
    recipes: Vec<RecipeHolder>,
    by_key: BTreeMap<Identifier, usize>,
    identity: Arc<()>,
}
impl RecipeManager {
    pub fn new(recipes: Vec<RecipeHolder>) -> Result<Self> {
        let mut by_key = BTreeMap::new();
        for (index, recipe) in recipes.iter().enumerate() {
            if by_key.insert(recipe.id.clone(), index).is_some() {
                return Err(Error::Invalid(format!("duplicate recipe {}", recipe.id)));
            }
        }
        Ok(Self {
            recipes,
            by_key,
            identity: Arc::new(()),
        })
    }
    /// Port of ResourceManagerRegistryLoadTask: sorted resource registration, not
    /// hash traversal. Invalid recipe registry entries fail reload atomically.
    pub fn load(resources: &ResourceManager, tags: &TagSet<Item>) -> Result<Self> {
        let mut entries: Vec<_> = resources.registry("recipe").into_iter().collect();
        // Vanilla sorts resource filenames before stripping the directory and
        // extension. For example a-b.json precedes a.json, although id a < a-b.
        entries.sort_by_cached_key(|(id, _)| {
            id.with_path(format!("recipe/{}.json", id.path()))
                .expect("validated recipe resource path")
        });
        Self::new(
            entries
                .into_iter()
                .map(|(id, resource)| {
                    let json = resource.json()?;
                    let value = Recipe::decode(&json, tags)
                        .map_err(|error| Error::Invalid(format!("recipe {id}: {error}")))?;
                    Ok(RecipeHolder { id, value })
                })
                .collect::<Result<_>>()?,
        )
    }
    pub fn recipes(&self) -> &[RecipeHolder] {
        &self.recipes
    }
    pub fn by_key(&self, id: &Identifier) -> Option<&RecipeHolder> {
        self.by_key.get(id).map(|index| &self.recipes[*index])
    }
    pub fn learnable_recipes(&self) -> impl Iterator<Item = &RecipeHolder> {
        self.recipes
            .iter()
            .filter(|recipe| !recipe.value.is_special())
    }
    /// Port of RecipeManager#getRecipeFor, including hint precedence.
    pub fn get_recipe_for(
        &self,
        kind: RecipeType,
        input: RecipeInput<'_>,
        context: &impl CraftingContext,
        hint: Option<&Identifier>,
    ) -> Result<Option<&RecipeHolder>> {
        if let Some(recipe) = hint.and_then(|id| self.by_key(id))
            && recipe.value.recipe_type() == kind
            && recipe.value.matches(input, context)?
        {
            return Ok(Some(recipe));
        }
        if input.is_empty() {
            return Ok(None);
        }
        for recipe in &self.recipes {
            if recipe.value.recipe_type() == kind && recipe.value.matches(input, context)? {
                return Ok(Some(recipe));
            }
        }
        Ok(None)
    }
    pub fn get_crafting_recipe(
        &self,
        input: &dyn CraftingInputAccess,
        context: &impl CraftingContext,
    ) -> Result<Option<&RecipeHolder>> {
        self.get_recipe_for(
            RecipeType::Crafting,
            RecipeInput::Crafting(input),
            context,
            None,
        )
    }
    pub fn recipes_for<'a>(
        &'a self,
        kind: RecipeType,
        input: RecipeInput<'a>,
        context: &'a impl CraftingContext,
    ) -> impl Iterator<Item = Result<&'a RecipeHolder>> {
        self.recipes
            .iter()
            .filter(move |recipe| !input.is_empty() && recipe.value.recipe_type() == kind)
            .filter_map(move |recipe| match recipe.value.matches(input, context) {
                Ok(true) => Some(Ok(recipe)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
    }
}
/// Port of RecipeManager.CachedCheck. A miss preserves the last successful hint.
#[derive(Clone, Debug)]
pub struct CachedCheck {
    kind: RecipeType,
    last_recipe: Option<Identifier>,
}
impl CachedCheck {
    pub fn new(kind: RecipeType) -> Self {
        Self {
            kind,
            last_recipe: None,
        }
    }
    pub fn get_recipe_for<'a>(
        &mut self,
        manager: &'a RecipeManager,
        input: RecipeInput<'_>,
        context: &impl CraftingContext,
    ) -> Result<Option<&'a RecipeHolder>> {
        let recipe =
            manager.get_recipe_for(self.kind, input, context, self.last_recipe.as_ref())?;
        if let Some(recipe) = recipe {
            self.last_recipe = Some(recipe.id.clone());
        }
        Ok(recipe)
    }
}
#[derive(Clone, Debug)]
struct CacheEntry {
    input: CraftingInput,
    recipe: Option<Identifier>,
}
/// Port of RecipeCache: bounded move-to-front cache, including negative results.
/// Ingredient counts are ignored in keys; item types/components and dimensions are not.
#[derive(Clone, Debug)]
pub struct RecipeCache {
    capacity: usize,
    entries: Vec<CacheEntry>,
    manager: std::sync::Weak<()>,
}
impl RecipeCache {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            capacity,
            entries: Vec::new(),
            manager: std::sync::Weak::new(),
        }
    }
    pub fn get<'a>(
        &mut self,
        manager: &'a RecipeManager,
        input: &dyn CraftingInputAccess,
        context: &impl CraftingContext,
    ) -> Result<Option<&'a RecipeHolder>> {
        if input.is_empty() {
            return Ok(None);
        }
        if !self.manager.ptr_eq(&Arc::downgrade(&manager.identity)) {
            self.entries.clear();
            self.manager = Arc::downgrade(&manager.identity);
        }
        for index in 0..self.entries.len() {
            let key = &self.entries[index].input;
            if key.width() != input.width() || key.height() != input.height() {
                continue;
            }
            let mut matches = true;
            for (a, b) in key.items().iter().zip(input.iter()) {
                if !stack::same_item_components(a, b)? {
                    matches = false;
                    break;
                }
            }
            if matches {
                let entry = self.entries.remove(index);
                let recipe = entry.recipe.as_ref().and_then(|id| manager.by_key(id));
                self.entries.insert(0, entry);
                return Ok(recipe);
            }
        }
        let recipe = manager.get_crafting_recipe(input, context)?;
        // RecipeCache explicitly copies count-one keys in vanilla; the matcher
        // above reads the original input view, including canonical live cells.
        let key = CraftingInput::of(
            input.width(),
            input.height(),
            input
                .iter()
                .map(|stack| stack::copy_with_count(stack, 1))
                .collect(),
        )?;
        self.entries.insert(
            0,
            CacheEntry {
                input: key,
                recipe: recipe.map(|recipe| recipe.id.clone()),
            },
        );
        self.entries.truncate(self.capacity);
        Ok(recipe)
    }
}
