//! VillagerProfessionLayer's cached texture metadata and type-layer head visibility.
use super::{
    anim::Poses,
    render::{PROFESSIONS, VILLAGER_TYPES},
};
use std::{collections::HashMap, fs, path::Path};

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
enum Hat {
    #[default]
    None,
    Partial,
    Full,
}

#[derive(Default)]
pub struct VillagerHats(HashMap<String, Hat>);

impl VillagerHats {
    pub fn load(root: &Path) -> Self {
        let mut hats = HashMap::new();
        for folder in ["villager", "zombie_villager"] {
            for (group, names) in [
                ("type", VILLAGER_TYPES.as_slice()),
                ("profession", PROFESSIONS.as_slice()),
            ] {
                for name in names {
                    let texture = format!("entity/{folder}/{group}/{name}");
                    let path = root.join(format!("assets/minecraft/textures/{texture}.png.mcmeta"));
                    let metadata = fs::read(path)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
                    let hat = match metadata
                        .as_ref()
                        .and_then(|v| v["villager"]["hat"].as_str())
                    {
                        Some("partial") => Hat::Partial,
                        Some("full") => Hat::Full,
                        _ => Hat::None,
                    };
                    hats.insert(texture, hat);
                }
            }
        }
        Self(hats)
    }

    pub fn apply(&self, folder: &str, variant: &str, profession: &str, poses: &mut Poses) {
        let hat = |group, name| {
            self.0
                .get(&format!("{folder}/{group}/{name}"))
                .copied()
                .unwrap_or_default()
        };
        let visible = match hat("profession", profession) {
            Hat::None => true,
            Hat::Partial => hat("type", variant) != Hat::Full,
            Hat::Full => false,
        };
        // VillagerModel.createNoHatModel clears the head and every child.
        poses.visible("head", visible);
    }
}
