//! Canonical mob-effect instances; world callbacks stay at their native call sites.
use ferraloom_data::registries::mob_effect::Id as MobEffect;
use ferraloom_protocol::{Error, Result, codec::VarInt, component};
use ferraloom_util::mth;

mod active;
pub use active::ActiveEffects;

/// Port of MobEffectInstance's server state, including its hidden-effect chain.
#[derive(Debug)]
pub struct MobEffectInstance {
    effect: MobEffect,
    duration: i32,
    amplifier: i32,
    ambient: bool,
    visible: bool,
    show_icon: bool,
    hidden_effect: Option<Box<Self>>,
}
impl MobEffectInstance {
    pub const INFINITE_DURATION: i32 = -1;

    /// Decode the protocol/persistent DTO into the actual built-in holder. A
    /// missing registry entry is rejected rather than retained as a name.
    pub fn from_component(value: component::MobEffectInstance) -> Result<Self> {
        let effect = MobEffect::from_name(value.effect.0.as_str()).ok_or_else(|| {
            Error::invalid("mob effect registry entry", value.effect.0.to_string())
        })?;
        Ok(Self::from_details(effect, value.details))
    }
    fn from_details(effect: MobEffect, value: component::MobEffectDetails) -> Self {
        Self::new(
            effect,
            value.duration.0,
            value.amplifier.0,
            value.ambient,
            value.show_particles,
            value.show_icon,
            value
                .hidden_effect
                .map(|hidden| Box::new(Self::from_details(effect, *hidden))),
        )
    }
    pub fn to_component(&self) -> component::MobEffectInstance {
        component::MobEffectInstance {
            effect: component::MobEffect(
                self.effect
                    .name()
                    .parse()
                    .expect("native effect identifier"),
            ),
            details: self.to_details(),
        }
    }
    fn to_details(&self) -> component::MobEffectDetails {
        component::MobEffectDetails {
            amplifier: VarInt(self.amplifier),
            duration: VarInt(self.duration),
            ambient: self.ambient,
            show_particles: self.visible,
            show_icon: self.show_icon,
            hidden_effect: self
                .hidden_effect
                .as_ref()
                .map(|hidden| Box::new(hidden.to_details())),
        }
    }

    pub fn new(
        effect: MobEffect,
        duration: i32,
        amplifier: i32,
        ambient: bool,
        visible: bool,
        show_icon: bool,
        hidden_effect: Option<Box<Self>>,
    ) -> Self {
        Self {
            effect,
            duration,
            amplifier: amplifier.clamp(0, 255),
            ambient,
            visible,
            show_icon,
            hidden_effect,
        }
    }
    pub fn effect(&self) -> MobEffect {
        self.effect
    }
    pub fn duration(&self) -> i32 {
        self.duration
    }
    pub fn amplifier(&self) -> i32 {
        self.amplifier
    }
    pub fn is_ambient(&self) -> bool {
        self.ambient
    }
    pub fn is_visible(&self) -> bool {
        self.visible
    }
    pub fn show_icon(&self) -> bool {
        self.show_icon
    }
    pub fn hidden_effect(&self) -> Option<&Self> {
        self.hidden_effect.as_deref()
    }
    pub fn is_infinite_duration(&self) -> bool {
        self.duration == Self::INFINITE_DURATION
    }
    pub fn ends_within(&self, ticks: i32) -> bool {
        !self.is_infinite_duration() && self.duration <= ticks
    }
    pub fn has_remaining_duration(&self) -> bool {
        self.is_infinite_duration() || self.duration > 0
    }
    fn is_shorter_duration_than(&self, other: &Self) -> bool {
        !self.is_infinite_duration()
            && (self.duration < other.duration || other.is_infinite_duration())
    }
    fn set_details_from(&mut self, other: &Self) {
        self.duration = other.duration;
        self.amplifier = other.amplifier;
        self.ambient = other.ambient;
        self.visible = other.visible;
        self.show_icon = other.show_icon;
    }
    /// Port of MobEffectInstance#update. Hidden-only changes intentionally return
    /// false; the return value controls the outer LivingEntity update callback.
    pub fn update(&mut self, takeover: &Self) -> bool {
        if self.effect != takeover.effect {
            eprintln!("MobEffectInstance::update should only be called for matching effects");
        }
        let mut changed = false;
        if takeover.amplifier > self.amplifier {
            if takeover.is_shorter_duration_than(self) {
                let mut hidden = Box::new(self.clone());
                hidden.hidden_effect = self.hidden_effect.take();
                self.hidden_effect = Some(hidden);
            }
            self.amplifier = takeover.amplifier;
            self.duration = takeover.duration;
            changed = true;
        } else if self.is_shorter_duration_than(takeover) {
            if takeover.amplifier == self.amplifier {
                self.duration = takeover.duration;
                changed = true;
            } else if let Some(hidden) = &mut self.hidden_effect {
                hidden.update(takeover);
            } else {
                self.hidden_effect = Some(Box::new(takeover.clone()));
            }
        }
        if !takeover.ambient && self.ambient || changed {
            self.ambient = takeover.ambient;
            changed = true;
        }
        if takeover.visible != self.visible {
            self.visible = takeover.visible;
            changed = true;
        }
        if takeover.show_icon != self.show_icon {
            self.show_icon = takeover.show_icon;
            changed = true;
        }
        changed
    }
    pub fn map_duration(&self, mapper: impl FnOnce(i32) -> i32) -> i32 {
        if !self.is_infinite_duration() && self.duration != 0 {
            mapper(self.duration)
        } else {
            self.duration
        }
    }
    pub fn with_scaled_duration(&self, scale: f32) -> Self {
        let mut copy = self.clone();
        copy.duration =
            copy.map_duration(|duration| mth::floor(f64::from(duration as f32 * scale)).max(1));
        copy
    }
    /// Port of MobEffectInstance#tickDownDuration. Called only after the native
    /// effect tick callback; hidden effects count down before their parent.
    pub fn tick_down_duration(&mut self) {
        if let Some(hidden) = &mut self.hidden_effect {
            hidden.tick_down_duration();
        }
        self.duration = self.map_duration(|duration| duration.wrapping_sub(1));
    }
    /// Port of MobEffectInstance#downgradeToHiddenEffect. The caller invokes
    /// LivingEntity's update callback if this returns true.
    pub fn downgrade_to_hidden_effect(&mut self) -> bool {
        if self.duration != 0 {
            return false;
        }
        let Some(mut hidden) = self.hidden_effect.take() else {
            return false;
        };
        self.set_details_from(&hidden);
        self.hidden_effect = hidden.hidden_effect.take();
        true
    }
}
impl Clone for MobEffectInstance {
    /// Native's copy constructor copies details, not the hidden chain or blend
    /// state. update explicitly reconnects the old hidden chain when required.
    fn clone(&self) -> Self {
        Self::new(
            self.effect,
            self.duration,
            self.amplifier,
            self.ambient,
            self.visible,
            self.show_icon,
            None,
        )
    }
}
impl PartialEq for MobEffectInstance {
    /// Native equals ignores hidden effects and client blend state.
    fn eq(&self, other: &Self) -> bool {
        self.effect == other.effect
            && self.duration == other.duration
            && self.amplifier == other.amplifier
            && self.ambient == other.ambient
            && self.visible == other.visible
            && self.show_icon == other.show_icon
    }
}
impl Eq for MobEffectInstance {}

#[cfg(test)]
mod tests;
