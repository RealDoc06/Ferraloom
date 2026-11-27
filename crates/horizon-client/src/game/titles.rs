//! Titles, the action bar and the selected item name: the state and layout
//! live in `hud_overlays` (`Titles`, `ToolHighlight`); this feeds and draws them.
use super::Game;
pub(super) use horizon_client::hud_overlays::Titles;
use horizon_client::{
    hud_overlays::HeldName,
    item_tooltip,
    text::{self, Text},
};

impl Game<'_> {
    /// The selected stack as `Hud.tick` compares it for the name highlight.
    fn held_name(&self) -> Option<HeldName> {
        let stack = self.inventory.get(self.selected)?.as_ref()?;
        let custom = stack.custom_name();
        let default_rarity = self.item_rarity(&stack.item);
        let rarity = item_tooltip::rarity(stack, &default_rarity);
        Some(HeldName {
            item: stack.item.to_string(),
            hover_name: Text::literal(item_tooltip::name_line(stack, &|key| {
                self.pack.names.display_get(key)
            })),
            rarity_color: match rarity {
                "uncommon" => text::YELLOW,
                "rare" => text::AQUA,
                "epic" => text::LIGHT_PURPLE,
                _ => text::WHITE,
            },
            custom_name: custom.is_some(),
        })
    }
    /// The title, action bar and selected-item parts of `Hud.tick`.
    pub(super) fn tick_hud(&mut self) {
        self.titles.tick();
        // `MinecraftServer.isCurrentlySaving` of single player's server.
        let saving = self
            .integrated
            .as_ref()
            .map_or(self.autosaving, |server| server.is_saving());
        self.saving_indicator.tick(saving);
        let held = self.held_name();
        self.tool_highlight
            .tick(held, f64::from(self.settings.notification_display_time));
    }
}
