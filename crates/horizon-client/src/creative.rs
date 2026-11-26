//! Creative inventory (CreativeModeInventoryScreen): vanilla's tabs and their
//! contents as built by the game's own display generators
//! (`creative-inventory.json`), a 9x5 scrolling grid and the search tab.
use crate::{Result, items::ItemStack, ui::EditBox};
use serde_json::Value;
use std::{collections::HashMap, fs, path::Path};

#[derive(Clone, Debug)]
pub struct Tab {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub items: Vec<ItemStack>,
    pub top: bool,
    pub column: u32,
    pub search: bool,
    pub inventory: bool,
}
pub struct Creative {
    pub tabs: Vec<Tab>,
    pub selected: usize,
    /// Scroll position 0..1 (scrollOffs).
    pub scroll: f32,
    /// The scrollbar is held (scrolling): dragging moves it.
    pub scrolling: bool,
    /// CreativeModeInventoryScreen's search box: an unbordered `EditBox` of 50.
    pub search: EditBox,
}
/// Display names from the pinned en_us language file.
#[derive(Default)]
pub struct Names {
    map: HashMap<String, String>,
}
impl Names {
    pub fn load(root: &Path) -> Self {
        let read = |namespace: &str| {
            fs::read(root.join(format!("assets/{namespace}/lang/en_us.json")))
                .ok()
                .and_then(|b| serde_json::from_slice::<HashMap<String, String>>(&b).ok())
                .unwrap_or_default()
        };
        let mut map = read("minecraft");
        // Content mods' names (docs/modpacks.md); none in a vanilla root.
        for namespace in crate::mod_content::namespaces(root) {
            map.extend(read(&namespace));
        }
        Self { map }
    }
    pub fn item(&self, id: &str) -> String {
        let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
        self.map
            .get(&format!("item.{namespace}.{path}"))
            .or_else(|| self.map.get(&format!("block.{namespace}.{path}")))
            .cloned()
            .unwrap_or_else(|| path.replace('_', " "))
    }
    /// An item's display name in the selected language: `item.*` then
    /// `block.*`, then the pinned English map (`Names::item`).
    pub fn display_item(&self, id: &str) -> String {
        let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
        if let Some(lang) = crate::lang::current() {
            for prefix in ["item", "block"] {
                if let Some(name) = lang.get(&format!("{prefix}.{namespace}.{path}")) {
                    return name.to_owned();
                }
            }
        }
        self.item(id)
    }
    /// A translation key's text in the selected language, else the pinned
    /// English map (`HoverName` and tooltip keys such as potion names).
    pub fn display_get(&self, key: &str) -> Option<String> {
        crate::lang::current()
            .and_then(|lang| lang.get(key).map(str::to_owned))
            .or_else(|| self.get(key).map(str::to_owned))
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }
}

impl Creative {
    /// Visible player slots in the creative inventory tab (game slot ids).
    pub fn inventory_slots() -> impl Iterator<Item = usize> {
        (0..36).chain(500..505)
    }

    /// Game slot id of an item tab's first display slot (ItemPickerMenu's
    /// `CustomCreativeSlot`s, 9 by 5); its hotbar row keeps ids 0..9.
    pub const DISPLAY: usize = 900;

    /// Visible slots of an item tab (game slot ids).
    pub fn tab_slots() -> impl Iterator<Item = usize> {
        (Self::DISPLAY..Self::DISPLAY + 45).chain(0..9)
    }

    /// `creative-inventory.json` (VanillaCreativeTabsExporter); resources
    /// prepared before it fall back to `creative-tabs.json`'s item ids.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("creative-inventory.json");
        if path.exists() {
            Self::from_json(&serde_json::from_slice(&fs::read(path)?)?)
        } else {
            Self::from_legacy(&serde_json::from_slice(&fs::read(
                root.join("creative-tabs.json"),
            )?)?)
        }
    }
    pub fn from_json(raw: &Value) -> Result<Self> {
        let mut tabs = Vec::new();
        for t in raw["tabs"].as_array().into_iter().flatten() {
            let mut items = Vec::new();
            for v in t["items"].as_array().into_iter().flatten() {
                let snbt = v.as_str().unwrap_or("-");
                if let Some(stack) = crate::item_data::parse_stack(snbt)? {
                    items.push(stack);
                }
            }
            let kind = t["type"].as_str().unwrap_or("");
            if kind == "HOTBAR" || t["visible"] == false {
                continue;
            }
            let icon = t["icon"]
                .as_str()
                .and_then(|s| crate::item_data::parse_stack(s).ok().flatten());
            tabs.push(Tab {
                id: t["id"].as_str().unwrap_or("").to_owned(),
                title: t["title"].as_str().unwrap_or("").to_owned(),
                icon: icon.map_or_else(|| "minecraft:stone".to_owned(), |s| s.item.to_string()),
                items,
                top: t["row"] == "TOP",
                column: t["column"].as_u64().unwrap_or(0) as u32,
                search: kind == "SEARCH",
                inventory: kind == "INVENTORY",
            });
        }
        Ok(Self::new(tabs))
    }
    /// The old export: item ids per tab, no components, tooltips or tags.
    fn from_legacy(raw: &Value) -> Result<Self> {
        let mut tabs = Vec::new();
        for t in raw.as_array().into_iter().flatten() {
            let items: Vec<ItemStack> = t["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(|id| ItemStack::new(id, 1)))
                .collect();
            if t["id"] == "minecraft:hotbar" || t["id"] == "minecraft:op_blocks" {
                continue;
            }
            let kind = t["type"].as_str().unwrap_or("");
            tabs.push(Tab {
                id: t["id"].as_str().unwrap_or("").to_owned(),
                title: t["title"].as_str().unwrap_or("").to_owned(),
                icon: t["icon"].as_str().unwrap_or("minecraft:stone").to_owned(),
                items,
                top: t["row"] == "TOP",
                column: t["column"].as_u64().unwrap_or(0) as u32,
                search: kind == "SEARCH",
                inventory: kind == "INVENTORY",
            });
        }
        Ok(Self::new(tabs))
    }
    fn new(tabs: Vec<Tab>) -> Self {
        Self {
            tabs,
            selected: 0,
            scroll: 0.0,
            scrolling: false,
            search: {
                let mut search = EditBox::new(0.0, 0.0, 80.0, 9.0);
                search.bordered = false;
                search.max_length = 50;
                search.text_color = u32::MAX;
                search
            },
        }
    }
    pub fn tab(&self) -> &Tab {
        &self.tabs[self.selected.min(self.tabs.len() - 1)]
    }
}
