//! `Minecraft.getSituationalMusic` and `getMusicVolume` for the music manager,
//! the local player's ambient sound handlers and the sound engine tick.
use super::Game;
use horizon_client::blocks::Fluid;

impl Game<'_> {
    /// `isEyeInFluid(WATER)`: the eye inside the water of its block column.
    pub(super) fn eye_in_water(&self, x: f64, eye_y: f64, z: f64) -> bool {
        let (bx, by, bz) = (x.floor() as i32, eye_y.floor() as i32, z.floor() as i32);
        let f = self.res.facts().get(self.world.block(bx, by, bz));
        if f.fluid != Fluid::Water {
            return false;
        }
        let above = self.res.facts().get(self.world.block(bx, by + 1, bz));
        let height = if above.fluid == Fluid::Water {
            1.0
        } else {
            f64::from(f.fluid_level) / 9.0
        };
        eye_y <= f64::from(by) + height
    }
}
