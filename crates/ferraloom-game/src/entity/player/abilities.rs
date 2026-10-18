//! Canonical Player abilities and the strict optional-field Packed codec.
use super::GameType;
use ferraloom_nbt::Tag;
use ferraloom_protocol::{
    Error, Result,
    component::{Persistent, PersistentValue},
};
use ferraloom_util::java::JavaF32;

/// Port of Abilities. Each ability remains mutable independently of game mode.
#[derive(Clone, Debug)]
pub struct Abilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub mayfly: bool,
    pub instabuild: bool,
    pub may_build: bool,
    flying_speed: f32,
    walking_speed: f32,
}
impl Default for Abilities {
    fn default() -> Self {
        Self {
            invulnerable: false,
            flying: false,
            mayfly: false,
            instabuild: false,
            may_build: true,
            flying_speed: 0.05,
            walking_speed: 0.1,
        }
    }
}
impl Abilities {
    pub fn flying_speed(&self) -> f32 {
        self.flying_speed
    }
    pub fn set_flying_speed(&mut self, value: f32) {
        self.flying_speed = value;
    }
    pub fn walking_speed(&self) -> f32 {
        self.walking_speed
    }
    pub fn set_walking_speed(&mut self, value: f32) {
        self.walking_speed = value;
    }
    /// Port of Abilities#pack, capturing the seven scalar values together.
    pub fn pack(&self) -> PackedAbilities {
        PackedAbilities {
            invulnerable: self.invulnerable,
            flying: self.flying,
            mayfly: self.mayfly,
            instabuild: self.instabuild,
            may_build: self.may_build,
            flying_speed: self.flying_speed,
            walking_speed: self.walking_speed,
        }
    }
    pub fn apply(&mut self, packed: PackedAbilities) {
        *self = Self {
            invulnerable: packed.invulnerable,
            flying: packed.flying,
            mayfly: packed.mayfly,
            instabuild: packed.instabuild,
            may_build: packed.may_build,
            flying_speed: packed.flying_speed,
            walking_speed: packed.walking_speed,
        };
    }
    /// Port of GameType#updatePlayerAbilities, preserving Creative's flying flag.
    pub fn update_game_mode(&mut self, mode: GameType) {
        match mode {
            GameType::Creative => {
                self.mayfly = true;
                self.instabuild = true;
                self.invulnerable = true;
            }
            GameType::Spectator => {
                self.mayfly = true;
                self.instabuild = false;
                self.invulnerable = true;
                self.flying = true;
            }
            GameType::Survival | GameType::Adventure => {
                self.mayfly = false;
                self.instabuild = false;
                self.invulnerable = false;
                self.flying = false;
            }
        }
        self.may_build = matches!(mode, GameType::Survival | GameType::Creative);
    }
}
/// Port of Abilities.Packed, the immutable codec payload. Missing fields take
/// defaults, while a malformed present field rejects the entire packed value.
#[derive(Clone, Copy, Debug)]
pub struct PackedAbilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub mayfly: bool,
    pub instabuild: bool,
    pub may_build: bool,
    pub flying_speed: f32,
    pub walking_speed: f32,
}
impl Persistent for PackedAbilities {
    fn from_json(value: &serde_json::Value) -> Result<Self> {
        let value = value
            .as_object()
            .ok_or_else(|| Error::invalid("abilities", "expected object"))?;
        let defaults = Abilities::default().pack();
        macro_rules! read {
            ($key:literal,$default:expr,$ty:ty) => {
                value
                    .get($key)
                    .map(<$ty>::from_json)
                    .transpose()?
                    .unwrap_or($default)
            };
        }
        Ok(Self {
            invulnerable: read!("invulnerable", defaults.invulnerable, bool),
            flying: read!("flying", defaults.flying, bool),
            mayfly: read!("mayfly", defaults.mayfly, bool),
            instabuild: read!("instabuild", defaults.instabuild, bool),
            may_build: read!("mayBuild", defaults.may_build, bool),
            flying_speed: read!("flySpeed", defaults.flying_speed, f32),
            walking_speed: read!("walkSpeed", defaults.walking_speed, f32),
        })
    }
    fn from_nbt(value: &Tag) -> Result<Self> {
        let value = value
            .as_compound()
            .ok_or_else(|| Error::invalid("abilities", "expected compound"))?;
        let defaults = Abilities::default().pack();
        macro_rules! read {
            ($key:literal,$default:expr,$ty:ty) => {
                value
                    .get($key)
                    .map(<$ty>::from_nbt)
                    .transpose()?
                    .unwrap_or($default)
            };
        }
        Ok(Self {
            invulnerable: read!("invulnerable", defaults.invulnerable, bool),
            flying: read!("flying", defaults.flying, bool),
            mayfly: read!("mayfly", defaults.mayfly, bool),
            instabuild: read!("instabuild", defaults.instabuild, bool),
            may_build: read!("mayBuild", defaults.may_build, bool),
            flying_speed: read!("flySpeed", defaults.flying_speed, f32),
            walking_speed: read!("walkSpeed", defaults.walking_speed, f32),
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(vec![
            (
                "invulnerable".into(),
                PersistentValue::Boolean(self.invulnerable),
            ),
            ("flying".into(), PersistentValue::Boolean(self.flying)),
            ("mayfly".into(), PersistentValue::Boolean(self.mayfly)),
            (
                "instabuild".into(),
                PersistentValue::Boolean(self.instabuild),
            ),
            ("mayBuild".into(), PersistentValue::Boolean(self.may_build)),
            (
                "flySpeed".into(),
                PersistentValue::Float(JavaF32(self.flying_speed)),
            ),
            (
                "walkSpeed".into(),
                PersistentValue::Float(JavaF32(self.walking_speed)),
            ),
        ]))
    }
}
#[cfg(test)]
mod tests;
