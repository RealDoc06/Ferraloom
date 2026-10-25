use super::{Error, Result};
use ferraloom_protocol::component::ItemStack;

/// Read-only CraftingInput access shared by standalone values and a short
/// immutable borrow of the canonical live grid. Recipe evaluation owns no slots.
pub trait CraftingInputAccess: std::fmt::Debug {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn size(&self) -> usize;
    fn get_item(&self, x: usize, y: usize) -> &ItemStack;
    fn ingredient_count(&self) -> usize;
    fn iter(&self) -> CraftingInputIter<'_>;
    fn is_empty(&self) -> bool {
        self.ingredient_count() == 0
    }
    fn get_item_index(&self, index: usize) -> &ItemStack {
        self.get_item(index % self.width(), index / self.width())
    }
}

/// A row-major iterator that returns the actual input receivers, including
/// cropped rows whose cells are not contiguous in the original grid slice.
pub struct CraftingInputIter<'a> {
    input: &'a dyn CraftingInputAccess,
    front: usize,
    back: usize,
}
impl<'a> CraftingInputIter<'a> {
    pub(crate) fn new(input: &'a dyn CraftingInputAccess) -> Self {
        Self {
            input,
            front: 0,
            back: input.size(),
        }
    }
}
impl<'a> Iterator for CraftingInputIter<'a> {
    type Item = &'a ItemStack;
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        Some(self.input.get_item_index(index))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.back - self.front;
        (remaining, Some(remaining))
    }
}
impl DoubleEndedIterator for CraftingInputIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.input.get_item_index(self.back))
    }
}
impl ExactSizeIterator for CraftingInputIter<'_> {}
impl std::iter::FusedIterator for CraftingInputIter<'_> {}

#[derive(Clone, Copy, Debug, Default)]
struct InputBounds {
    width: usize,
    height: usize,
    left: usize,
    top: usize,
    ingredient_count: usize,
}
fn bounds(width: usize, height: usize, items: &[ItemStack]) -> Result<InputBounds> {
    if width == 0 || height == 0 {
        return Ok(InputBounds::default());
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
        return Ok(InputBounds::default());
    }
    Ok(InputBounds {
        width: right - left + 1,
        height: bottom - top + 1,
        left,
        top,
        ingredient_count,
    })
}

/// Port of CraftingInput.Positioned as a borrow of the live grid, retaining
/// original stack objects rather than copying the occupied rectangle.
#[derive(Clone, Copy, Debug)]
pub struct CraftingInputView<'a> {
    items: &'a [ItemStack],
    row_stride: usize,
    bounds: InputBounds,
}
#[derive(Clone, Copy, Debug)]
pub struct PositionedCraftingInputView<'a> {
    pub input: CraftingInputView<'a>,
    pub left: usize,
    pub top: usize,
}
impl<'a> CraftingInputView<'a> {
    pub fn of(width: usize, height: usize, items: &'a [ItemStack]) -> Result<Self> {
        Ok(Self::of_positioned(width, height, items)?.input)
    }
    pub fn of_positioned(
        width: usize,
        height: usize,
        items: &'a [ItemStack],
    ) -> Result<PositionedCraftingInputView<'a>> {
        let bounds = bounds(width, height, items)?;
        Ok(PositionedCraftingInputView {
            input: Self {
                items,
                row_stride: width,
                bounds,
            },
            left: bounds.left,
            top: bounds.top,
        })
    }
}
impl CraftingInputAccess for CraftingInputView<'_> {
    fn width(&self) -> usize {
        self.bounds.width
    }
    fn height(&self) -> usize {
        self.bounds.height
    }
    fn size(&self) -> usize {
        self.bounds.width * self.bounds.height
    }
    fn get_item(&self, x: usize, y: usize) -> &ItemStack {
        &self.items[self.bounds.left + x + (self.bounds.top + y) * self.row_stride]
    }
    fn ingredient_count(&self) -> usize {
        self.bounds.ingredient_count
    }
    fn iter(&self) -> CraftingInputIter<'_> {
        CraftingInputIter::new(self)
    }
}

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
        let InputBounds {
            width: new_width,
            height: new_height,
            left,
            top,
            ingredient_count,
        } = bounds(width, height, &items)?;
        if ingredient_count == 0 {
            return Ok(PositionedCraftingInput::default());
        }
        let items = if new_width == width && new_height == height {
            items
        } else {
            (top..top + new_height)
                .flat_map(|y| {
                    items[y * width + left..y * width + left + new_width]
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
impl CraftingInputAccess for CraftingInput {
    fn width(&self) -> usize {
        self.width
    }
    fn height(&self) -> usize {
        self.height
    }
    fn size(&self) -> usize {
        self.items.len()
    }
    fn get_item(&self, x: usize, y: usize) -> &ItemStack {
        CraftingInput::get_item(self, x, y)
    }
    fn ingredient_count(&self) -> usize {
        self.ingredient_count
    }
    fn iter(&self) -> CraftingInputIter<'_> {
        CraftingInputIter::new(self)
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
    Crafting(&'a dyn CraftingInputAccess),
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
