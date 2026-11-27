//! Horizon's optional extras (lighting, inventory tools): their settings,
//! kept in `horizon-extras.txt` next to `options.txt` so that file stays
//! exactly vanilla's. Every extra is off by default. Horizon's own switches
//! outside the Extras page (No AI, Agent Control, Discord Rich Presence) are
//! kept here too.
use std::{fs, path::Path};

pub const FILE: &str = "horizon-extras.txt";

#[derive(Clone, Debug, PartialEq)]
pub struct Extras {
    pub fullbright: bool,
    /// Held, dropped and burning light sources light their surroundings (`dynamic_light`).
    pub dynamic_lights: bool,
    /// Retry lost multiplayer connections (`reconnect`).
    pub auto_reconnect: bool,
    /// Mouse Tweaks-style drags in the inventory and storage containers.
    pub mouse_tweaks: bool,
    /// With them, the wheel over a slot moves its items one at a time.
    pub mouse_tweaks_wheel: bool,
    /// Sort buttons on the inventory and chest-like containers.
    pub sort: bool,
    /// A vanilla key name: sorts the part under the cursor.
    pub sort_key: String,
    /// No AI: every AI feature hidden and off (`horizon_core::no_ai`).
    pub no_ai: bool,
    /// Opens the control socket for scripts (off by default; `--control-socket` always opens it).
    pub agent_control: bool,
    /// "Playing Horizon" on the player's Discord profile (`discord_presence`; on by default).
    pub discord_presence: bool,
}
impl Default for Extras {
    fn default() -> Self {
        Self {
            fullbright: false,
            dynamic_lights: false,
            auto_reconnect: false,
            mouse_tweaks: false,
            mouse_tweaks_wheel: true,
            sort: false,
            sort_key: "key.keyboard.unknown".into(),
            no_ai: false,
            agent_control: false,
            discord_presence: true,
        }
    }
}
fn bool_text(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}
impl Extras {
    pub fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join(FILE))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }
    pub fn parse(text: &str) -> Self {
        let mut e = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            let flag = |d: bool| match value {
                "true" => true,
                "false" => false,
                _ => d,
            };
            match key.trim() {
                "fullbright" => e.fullbright = flag(e.fullbright),
                "dynamicLights" => e.dynamic_lights = flag(e.dynamic_lights),
                "autoReconnect" => e.auto_reconnect = flag(e.auto_reconnect),
                "mouseTweaks" => e.mouse_tweaks = flag(e.mouse_tweaks),
                "mouseTweaksWheel" => e.mouse_tweaks_wheel = flag(e.mouse_tweaks_wheel),
                "sort" => e.sort = flag(e.sort),
                "sortKey" if value.starts_with("key.") => e.sort_key = value.to_owned(),
                horizon_core::no_ai::KEY => e.no_ai = flag(e.no_ai),
                "agentControl" => e.agent_control = flag(e.agent_control),
                "discordPresence" => e.discord_presence = flag(e.discord_presence),
                _ => {}
            }
        }
        e
    }
    pub fn serialize(&self) -> String {
        format!(
            "fullbright:{}\ndynamicLights:{}\nautoReconnect:{}\nmouseTweaks:{}\nmouseTweaksWheel:{}\nsort:{}\nsortKey:{}\n{}:{}\nagentControl:{}\ndiscordPresence:{}\n",
            bool_text(self.fullbright),
            bool_text(self.dynamic_lights),
            bool_text(self.auto_reconnect),
            bool_text(self.mouse_tweaks),
            bool_text(self.mouse_tweaks_wheel),
            bool_text(self.sort),
            self.sort_key,
            horizon_core::no_ai::KEY,
            bool_text(self.no_ai),
            bool_text(self.agent_control),
            bool_text(self.discord_presence),
        )
    }
    /// Written only once something differs from the defaults, so a vanilla
    /// setup gains no file.
    pub fn save(&self, dir: &Path) {
        let path = dir.join(FILE);
        if *self == Self::default() && !path.exists() {
            return;
        }
        let _ = fs::write(path, self.serialize());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_lines_keep_defaults() {
        let e = Extras::parse("fullbright:maybe\nsortKey:z\nnoise\nagentControl:true");
        assert!(!e.fullbright);
        assert_eq!(e.sort_key, "key.keyboard.unknown");
        assert!(e.agent_control);
    }
}
