//! The options screens bound to the game: drawing, input, applying settings
//! (video, sound, chat, font, language) and the F11 fullscreen toggle.
use super::Game;

impl Game<'_> {
    pub(super) fn options_path() -> std::path::PathBuf {
        std::env::current_dir()
            .unwrap_or_default()
            .join("options.txt")
    }
}
