//! The locator bar on this client (horizon_client::locator_bar): the
//! waypoints a server tracks for the player (`handleWaypoint`; on a served
//! world the host's own connection in lan/waypoints.rs), the experience
//! bar's priority after a change, and the dots drawn from the last frame's
//! camera.
use super::Game;

impl Game<'_> {
    /// `LocalPlayer.setExperienceValues`: a change of progress shows the
    /// experience bar for 100 ticks, except the first one
    /// (`experienceDisplayStartTick` starts at `Integer.MIN_VALUE`).
    pub(super) fn track_experience_display(&mut self) {
        let progress = self.experience.progress;
        let (last, seen, _) = &mut self.experience_display;
        if progress != *last {
            *last = progress;
            if *seen {
                self.experience_display.2 = self.ticks;
            } else {
                *seen = true;
            }
        }
        // EnchantmentScreen and AnvilScreen.containerTick keep it up.
        if self.station.as_ref().is_some_and(|s| {
            matches!(
                s.kind,
                super::stations::Kind::Enchanting | super::stations::Kind::Anvil
            )
        }) {
            self.experience_display.2 = self.ticks;
        }
    }
}
