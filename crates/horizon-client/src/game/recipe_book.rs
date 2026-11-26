//! The recipe book of the inventory, crafting table and furnace screens
//! (`RecipeBookComponent`): the green book button, the tabs by category, the
//! search box, the craftable filter, the pages of recipe buttons with the
//! alternatives overlay, ghost recipes, and placing a recipe in the grid
//! (`ServerPlaceRecipe`). Which recipes are known comes from the advancement
//! rewards; the open and filter settings are saved with the player.
use super::Game;
use horizon_client::{
    items::ItemStack,
    recipe_book::{Book, Tab},
    ui::EditBox,
};

/// A ghost recipe slot: the items it cycles through.
struct Ghost {}
/// The alternatives overlay of a recipe button (`OverlayRecipeComponent`).
struct Overlay {}

#[derive(Default)]
pub(super) struct RecipeUi {
    book: Option<Book>,
    tab: Option<Tab>,
    search: Option<EditBox>,
    page: usize,
    ghost: Vec<Ghost>,
    last_placed: Option<String>,
    overlay: Option<Overlay>,
    view_key: u64,
}

impl RecipeUi {
    /// A new screen starts without a ghost recipe, overlay or search focus.
    pub(super) fn reset_screen(&mut self) {
        self.ghost.clear();
        self.last_placed = None;
        self.overlay = None;
        self.tab = None;
        self.page = 0;
        if let Some(search) = self.search.as_mut() {
            search.focused = false;
            search.set_value("");
        }
        self.view_key = 0;
    }
}

impl Game<'_> {
    /// `RecipeToast.addOrUpdate` for each display of the recipes just
    /// unlocked: its crafting station and result join the toast still
    /// queued or shown, or a new one.
    pub(super) fn recipe_toast(&mut self, recipes: &[String]) {
        if recipes.is_empty() || self.adv.quiet {
            return;
        }
        self.ensure_book();
        let index = &self.res.stations().recipe_index;
        let book = self.recipe_ui.book.as_ref();
        // SmithingTrimDemoSlotDisplay draws from a random of its own.
        let mut random = super::dispenser::entity_random();
        let mut items = Vec::new();
        for id in recipes {
            let Some(station) = index.toast_station(id) else {
                continue;
            };
            for display in index.display_ids(id) {
                // The book's displays carry their resolved results; the
                // other recipe types name theirs, and trims are demos.
                let result = book
                    .and_then(|b| {
                        b.entry_of_display(display)
                            .map(|i| b.entries[i].result.clone())
                    })
                    .or_else(|| index.result(id).map(|r| ItemStack::new(r, 1)))
                    .or_else(|| {
                        self.res.stations().recipes.trim_demo(
                            &self.res.stations().enchantments,
                            id,
                            &mut |n| {
                                horizon_core::random::RandomSource::next_int_bound(
                                    &mut random,
                                    n as i32,
                                )
                                .unwrap_or(0) as usize
                            },
                        )
                    });
                if let Some(result) = result {
                    items.push((station.to_owned(), result));
                }
            }
        }
        if items.is_empty() {
            return;
        }
        match self.adv.toasts.iter_mut().find(|t| t.is_recipe()) {
            Some(toast) => toast.add_recipes(items),
            None => self
                .adv
                .toasts
                .push_back(super::advancements::Toast::recipe(items)),
        }
    }

    // ---- recipes and what can be crafted ----

    fn ensure_book(&mut self) {
        if self.recipe_ui.book.is_none() {
            let mut book = Book::build(self.res.recipes(), self.res.items());
            book.set_display_ids(&self.res.stations().recipe_index);
            self.recipe_ui.book = Some(book);
        }
        if self.recipe_ui.search.is_none() {
            let mut search = EditBox::new(0.0, 0.0, 81.0, 14.0);
            search.max_length = 50;
            search.focused = false;
            search.text_color = u32::MAX;
            self.recipe_ui.search = Some(search);
        }
    }

    // ---- geometry ----

    // ---- clicks ----

    // ---- placing a recipe ----

    // ---- a server's recipe book ----

    // ---- ghost recipe ----

    // ---- drawing ----
}
