//! Field of view (`Camera.tickFov`, `calculateFov`,
//! `AbstractClientPlayer.getFieldOfViewModifier`), view and hurt bobbing
//! (`GameRenderer.bobHurt`, `bobView`), the portal and Nausea spin
//! (`GameRenderer.renderLevel`) and the spyglass (`SpyglassItem`,
//! `Hud.extractSpyglassOverlay`). The arithmetic is `horizon_client::camera`.
use super::Game;

impl Game<'_> {
    /// `AbstractClientPlayer.tick` and `aiStep` (`ClientAvatarState.tick`,
    /// `updateBob`), `rideTick`'s `resetBob` and `LocalPlayer.move`'s walked
    /// distance, after the player's movement this tick. `before` is the
    /// ground state, velocity and position at the start of the tick.
    pub(super) fn tick_avatar(&mut self, before: (bool, [f64; 3], [f64; 3]), riding: bool) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let (ground, velocity, start) = before;
        let swimming = p.pose == horizon_client::pose::Pose::Swimming;
        let alive = !p.health.dead_or_dying();
        let (dx, dz) = (
            (p.position.x - start[0]) as f32,
            (p.position.z - start[2]) as f32,
        );
        self.avatar.tick();
        self.avatar
            .update_bob(ground && alive && !swimming, velocity);
        if riding {
            self.avatar.reset_bob();
        } else {
            self.avatar.add_walked(dx, dz);
        }
    }
    /// `SpyglassItem.use`: start scoping.
    pub(super) fn use_spyglass(&mut self) {
        if self.using.is_none() {
            self.using = Some((self.held_slot(), 0));
            let at = self.eye_position();
            self.audio.play("minecraft:item.spyglass.use", at, 1.0, 1.0);
        }
    }
    /// `SpyglassItem.releaseUsing` / `finishUsingItem` (after 1200 ticks).
    pub(super) fn stop_spyglass(&mut self) {
        let at = self.eye_position();
        self.audio
            .play("minecraft:item.spyglass.stop_using", at, 1.0, 1.0);
    }
}
