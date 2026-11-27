/// One clip-space vertex of a screen quad.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ScreenVertex {
    pub pos: [f32; 2],
    /// Texture coordinates in sprite units (repeat outside 0..1).
    pub uv: [f32; 2],
    /// Atlas rectangle (u0, v0, u1, v1) of the sprite.
    pub rect: [f32; 4],
    pub color: [f32; 4],
}

/// Effect state advanced every tick.
#[derive(Clone, Copy, Debug)]
pub struct ScreenEffects {
    /// `Hud.vignetteBrightness`.
    pub vignette_brightness: f32,
    pub portal: f32,
    pub o_portal: f32,
    /// Nausea's `MobEffectInstance.BlendState` factor and the previous tick's.
    pub nausea: f32,
    pub o_nausea: f32,
    /// `LocalPlayer.spinningEffectTime` and `spinningEffectSpeed` (degrees).
    pub spinning_time: f32,
    pub spinning_speed: f32,
}
impl Default for ScreenEffects {
    fn default() -> Self {
        Self {
            vignette_brightness: 1.0,
            portal: 0.0,
            o_portal: 0.0,
            nausea: 0.0,
            o_nausea: 0.0,
            spinning_time: 0.0,
            spinning_speed: 0.0,
        }
    }
}
/// `Lightmap.getBrightness`.
pub fn lightmap_brightness(ambient_light: f32, level: u8) -> f32 {
    let v = f32::from(level) / 15.0;
    let curved = v / (4.0 - 3.0 * v);
    curved + (1.0 - curved) * ambient_light
}
impl ScreenEffects {
    /// `Hud.updateVignetteBrightness` and `LocalPlayer.handlePortalTransitionEffect`.
    pub fn tick(&mut self, eye_brightness: f32, inside_portal: bool) {
        let target = (1.0 - eye_brightness).clamp(0.0, 1.0);
        self.vignette_brightness += (target - self.vignette_brightness) * 0.01;
        self.o_portal = self.portal;
        let step = if inside_portal {
            0.0125
        } else if self.portal > 0.0 {
            -0.05
        } else {
            0.0
        };
        self.portal = (self.portal + step).clamp(0.0, 1.0);
    }
    /// `LocalPlayer.tickSpinningEffect` and Nausea's blend. `nausea` is `None`
    /// without the effect, else whether it is past its blend-out point
    /// (`!endsWithin(60)`, `MobEffects.NAUSEA`).
    pub fn tick_spinning(&mut self, nausea: Option<bool>) {
        self.o_nausea = self.nausea;
        match nausea {
            // BlendState.tick: fades in over 150 ticks and out over 20.
            Some(active) => {
                let (target, duration) = if active { (1.0, 150.0) } else { (0.0, 20.0) };
                self.nausea += (target - self.nausea).clamp(-1.0 / duration, 1.0 / duration);
            }
            // The blend state belongs to the instance, so a new one starts at 0.
            None => self.nausea = 0.0,
        }
        if self.portal > 0.0 || self.nausea > 0.0 {
            self.spinning_speed =
                (self.portal * 20.0 + self.nausea * 7.0) / (self.portal + self.nausea);
            self.spinning_time += self.spinning_speed;
        } else {
            self.spinning_speed = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lightmap_brightness_curve() {
        assert_eq!(lightmap_brightness(0.0, 15), 1.0);
        assert_eq!(lightmap_brightness(0.0, 0), 0.0);
        assert!((lightmap_brightness(0.1, 0) - 0.1).abs() < 1e-6);
    }
    #[test]
    fn portal_intensity_ramps() {
        let mut e = ScreenEffects::default();
        for _ in 0..80 {
            e.tick(1.0, true);
        }
        assert!((e.portal - 1.0).abs() < 1e-5);
        e.tick(1.0, false);
        assert!((e.portal - 0.95).abs() < 1e-5);
        // Vignette fades towards 1 - brightness at 1% per tick.
        assert!(e.vignette_brightness < 1.0);
    }
}
