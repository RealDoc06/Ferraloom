use crate::entity::boss::ender_dragon::EnderDragonEntity;
use pumpkin_util::math::vector3::Vector3;
use std::sync::Arc;

mod charging;
mod circling;
mod dying;
mod hovering;
mod landing;
mod landing_approach;
mod sit_attacking;
mod sit_breathing;
mod strafing;
mod taking_off;

pub use charging::ChargingPhase;
pub use circling::CirclingPhase;
pub use dying::DyingPhase;
pub use hovering::HoveringPhase;
pub use landing::LandingPhase;
pub use landing_approach::LandingApproachPhase;
pub use sit_attacking::SitAttackingPhase;
pub use sit_breathing::SitBreathingPhase;
pub use strafing::StrafingPhase;
pub use taking_off::TakingOffPhase;

pub trait Phase: Send + Sync {
    fn get_type(&self) -> EnderDragonPhase;
    fn tick(&self, dragon: &EnderDragonEntity);
    fn begin(&self, _dragon: &EnderDragonEntity) {}
    fn end(&self, _dragon: &EnderDragonEntity) {}
    fn is_sitting(&self) -> bool {
        self.get_type().is_sitting()
    }
    fn on_hurt(&self, direct: Option<&dyn crate::entity::EntityBase>, amount: f32) -> f32 {
        use crate::entity::projectile::{
            arrow::ArrowEntity, trident::TridentEntity, wind_charge::WindChargeEntity,
        };
        if self.get_type().rejects_sitting_projectile()
            && let Some(direct) = direct
            && (direct.cast_any().is::<ArrowEntity>()
                || direct.cast_any().is::<TridentEntity>()
                || direct
                    .cast_any()
                    .downcast_ref::<WindChargeEntity>()
                    .is_some_and(WindChargeEntity::is_player_wind_charge))
        {
            direct.set_on_fire_for_ticks(20);
            direct.get_entity().set_frozen_ticks(0);
            return 0.0;
        }
        amount
    }
    fn get_fly_speed(&self) -> f32 {
        0.6
    }
    fn get_turn_speed(&self) -> f32 {
        0.1
    }
    fn get_fly_target_location(&self) -> Option<Vector3<f64>> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EnderDragonPhase {
    Circling,
    Strafing,
    LandingApproach,
    Landing,
    TakingOff,
    SitBreathing,
    SitScanning,
    SitAttacking,
    Charging,
    Dying,
    #[default]
    Hovering,
}

impl EnderDragonPhase {
    /// The exact26.3 registry order. The old extra FlyToPortal state was an
    /// unregistered approach helper; its caller now enters LandingApproach.
    pub const ALL: [Self; 11] = [
        Self::Circling,
        Self::Strafing,
        Self::LandingApproach,
        Self::Landing,
        Self::TakingOff,
        Self::SitBreathing,
        Self::SitScanning,
        Self::SitAttacking,
        Self::Charging,
        Self::Dying,
        Self::Hovering,
    ];

    #[must_use]
    pub const fn is_sitting(self) -> bool {
        matches!(
            self,
            Self::SitBreathing | Self::SitScanning | Self::SitAttacking | Self::Hovering
        )
    }

    #[must_use]
    pub fn network_id(self) -> i32 {
        Self::ALL
            .iter()
            .position(|phase| *phase == self)
            .expect("registered dragon phase") as i32
    }

    pub fn from_network_id(id: i32) -> Self {
        usize::try_from(id)
            .ok()
            .and_then(|id| Self::ALL.get(id))
            .copied()
            .unwrap_or(Self::Circling)
    }

    pub const fn rejects_sitting_projectile(self) -> bool {
        matches!(
            self,
            Self::SitBreathing | Self::SitScanning | Self::SitAttacking
        )
    }
}

/// Only the registered state and combat policy are ported for Scanning. Keep a
/// loaded ID 6 as ID 6, without pretending the existing attacking AI is scanning.
/// Target selection / scanning transitions require a separate flight-AI port.
struct ScanningState;

impl Phase for ScanningState {
    fn get_type(&self) -> EnderDragonPhase {
        EnderDragonPhase::SitScanning
    }
    fn tick(&self, _dragon: &EnderDragonEntity) {}
    fn begin(&self, dragon: &EnderDragonEntity) {
        *dragon
            .target_location
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

pub struct PhaseManager {
    phases: [Arc<dyn Phase>; 11],
}

impl Default for PhaseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PhaseManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            phases: std::array::from_fn(|id| -> Arc<dyn Phase> {
                match EnderDragonPhase::ALL[id] {
                    EnderDragonPhase::Circling => Arc::new(CirclingPhase),
                    EnderDragonPhase::Strafing => Arc::new(StrafingPhase),
                    EnderDragonPhase::LandingApproach => Arc::new(LandingApproachPhase),
                    EnderDragonPhase::Landing => Arc::new(LandingPhase),
                    EnderDragonPhase::TakingOff => Arc::new(TakingOffPhase),
                    EnderDragonPhase::SitBreathing => Arc::new(SitBreathingPhase),
                    EnderDragonPhase::SitScanning => Arc::new(ScanningState),
                    EnderDragonPhase::SitAttacking => Arc::new(SitAttackingPhase),
                    EnderDragonPhase::Charging => Arc::new(ChargingPhase),
                    EnderDragonPhase::Dying => Arc::new(DyingPhase::default()),
                    EnderDragonPhase::Hovering => Arc::new(HoveringPhase),
                }
            }),
        }
    }

    #[must_use]
    pub fn get_phase(&self, phase_type: EnderDragonPhase) -> Arc<dyn Phase> {
        self.phases[phase_type.network_id() as usize].clone()
    }
}
