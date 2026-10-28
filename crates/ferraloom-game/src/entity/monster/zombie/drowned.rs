//! Drowned's single amphibious navigation and constructor-only subclass fields.
use super::Zombie;
use crate::{
    entity::{
        EntityData,
        ai::{AiOperation, MoveControlKind, NavigationKind, Unported, pathfinder::PathType},
    },
    level::{Level, LevelError},
};

/// Port of Drowned. Searching and ranged uncertainty are transient, not saved.
#[derive(Debug)]
pub struct Drowned {
    pub parent: Zombie,
    pub searching_for_land: bool,
    pub ranged_attack_uncertainty: f32,
}
impl Drowned {
    /// Port of Drowned#Drowned and its virtual createNavigation selection.
    /// 26.3 has one Amphibious navigation, without a ground/water swap pair.
    pub fn with_level(parent: EntityData, level: &Level) -> Result<Self, LevelError> {
        let mut parent = Zombie::with_level(parent, level)?;
        let mob = parent.as_mob_mut();
        mob.ai.navigation = Unported::new(AiOperation::Navigation(NavigationKind::Amphibious));
        mob.ai.move_control = Unported::new(AiOperation::MoveControl(MoveControlKind::Drowned));
        mob.pathfinding_malus[PathType::Water.id() as usize] = Some(0.0);
        Ok(Self {
            parent,
            searching_for_land: false,
            ranged_attack_uncertainty: -1.0,
        })
    }
}
