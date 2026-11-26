//! Extras that are official mods now (`horizon_client::moved_extras`): on
//! the first start after the update, a player who had one on gets the mod
//! installed from Horizon's registry, turned on, with their settings, and
//! one line in the chat saying so. The download runs on its own thread; if
//! it fails (offline), the game tries again at the next start.

#[cfg(not(feature = "mods"))]
impl super::Game<'_> {
    pub(super) fn start_moved_extras(&mut self) {}
}

#[cfg(feature = "mods")]
mod with_mods {
    use super::super::Game;
    use horizon_client::moved_extras::{self, Moved};
    use std::sync::mpsc::{Receiver, channel};

    type Done = Vec<(&'static Moved, Result<(), String>)>;

    #[derive(Default)]
    pub(in crate::game) struct MovedState {
        /// The installs under way.
        running: Option<Receiver<Done>>,
    }

    impl Game<'_> {
        /// At start: remembers the moved extras that were off, and installs
        /// the ones that were on, in the background.
        pub(in crate::game) fn start_moved_extras(&mut self) {
            let off = moved_extras::nothing_to_do(&self.settings.extras);
            let pending = moved_extras::pending(&self.settings.extras);
            if off.is_empty() && pending.is_empty() {
                return;
            }
            if !off.is_empty() {
                let e = &mut self.settings.extras;
                e.moved.extend(off.iter().map(|m| m.id.to_owned()));
                self.settings.save(&Self::options_path());
            }
            if pending.is_empty() {
                return;
            }
            let extras = self.settings.extras.clone();
            let dir = Self::options_path()
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_default();
            let (tx, rx) = channel();
            self.mods.moved.running = Some(rx);
            let spawned = std::thread::Builder::new()
                .name("Moved extras".into())
                .spawn(move || {
                    let done = match moved_extras::fetch_index() {
                        Ok(index) => pending
                            .into_iter()
                            .map(|m| (m, moved_extras::install(&index, m, &extras, &dir)))
                            .collect(),
                        Err(e) => pending.into_iter().map(|m| (m, Err(e.clone()))).collect(),
                    };
                    let _ = tx.send(done);
                });
            if spawned.is_err() {
                self.mods.moved.running = None;
            }
        }
    }
}
#[cfg(feature = "mods")]
pub(super) use with_mods::MovedState;
