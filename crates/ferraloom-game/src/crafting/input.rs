use super::{Error, Result};
use ferraloom_protocol::component::ItemStack;

/// Port of CraftingInput. Only the smallest occupied rectangle is retained.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftingInput {
    width: usize,
    height: usize,
    items: Vec<ItemStack>,
    ingredient_count: usize,
}
/// Port of CraftingInput.Positioned. Offsets refer to the original menu grid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PositionedCraftingInput {
    pub input: CraftingInput,
    pub left: usize,
    pub top: usize,
}
impl CraftingInput {
    pub fn of(width: usize, height: usize, items: Vec<ItemStack>) -> Result<Self> {
        Ok(Self::of_positioned(width, height, items)?.input)
    }
    pub fn of_positioned(
        width: usize,
        height: usize,
        items: Vec<ItemStack>,
    ) -> Result<PositionedCraftingInput> {
        if width == 0 || height == 0 {
            return Ok(PositionedCraftingInput::default());
        }
        if width.checked_mul(height) != Some(items.len()) {
            return Err(Error::Invalid(
                "crafting grid size differs from width * height".into(),
            ));
        }
        let mut left = width;
        let mut top = height;
        let mut right = 0;
        let mut bottom = 0;
        let mut ingredient_count = 0;
        for (index, stack) in items.iter().enumerate() {
            if !stack.is_empty() {
                left = left.min(index % width);
                right = right.max(index % width);
                top = top.min(index / width);
                bottom = bottom.max(index / width);
                ingredient_count += 1;
            }
        }
        if ingredient_count == 0 {
            return Ok(PositionedCraftingInput::default());
        }
        let new_width = right - left + 1;
        let new_height = bottom - top + 1;
        let items = if new_width == width && new_height == height {
            items
        } else {
            (top..=bottom)
                .flat_map(|y| {
                    items[y * width + left..y * width + right + 1]
                        .iter()
                        .cloned()
                })
                .collect()
        };
        Ok(PositionedCraftingInput {
            input: Self {
                width: new_width,
                height: new_height,
                items,
                ingredient_count,
            },
            left,
            top,
        })
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn size(&self) -> usize {
        self.items.len()
    }
    pub fn items(&self) -> &[ItemStack] {
        &self.items
    }
    pub fn get_item(&self, x: usize, y: usize) -> &ItemStack {
        &self.items[x + y * self.width]
    }
    pub fn ingredient_count(&self) -> usize {
        self.ingredient_count
    }
    pub fn is_empty(&self) -> bool {
        self.ingredient_count == 0
    }
}

/// Port of SmithingRecipeInput; optional recipe ingredients require empty slots.
#[derive(Clone, Debug, PartialEq)]
pub struct SmithingRecipeInput {
    pub template: ItemStack,
    pub base: ItemStack,
    pub addition: ItemStack,
}

/// Typed recipe inputs prevent matching a furnace recipe against a crafting grid.
#[derive(Clone, Copy, Debug)]
pub enum RecipeInput<'a> {
    Crafting(&'a CraftingInput),
    Single(&'a ItemStack),
    Smithing(&'a SmithingRecipeInput),
    Brewing {
        input: &'a ItemStack,
        reagent: &'a ItemStack,
    },
}
impl RecipeInput<'_> {
    pub fn is_empty(self) -> bool {
        match self {
            Self::Crafting(input) => input.is_empty(),
            Self::Single(input) => input.is_empty(),
            Self::Smithing(input) => {
                input.base.is_empty() && input.template.is_empty() && input.addition.is_empty()
            }
            Self::Brewing { input, reagent } => input.is_empty() && reagent.is_empty(),
        }
    }
}
