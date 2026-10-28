pub mod attributes;
pub mod control;
pub mod goal;
pub mod navigation;
pub mod pathfinder;
pub mod sensing;

/// Port of Mob#createNavigation's family-selected concrete implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum NavigationKind {
    Ground,
    Water,
    Flying,
    WallClimber,
    Amphibious,
}
/// Port of Mob#createMoveControl and subclass constructor replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum MoveControlKind {
    Ground,
    Drowned,
    AbstractCube,
}
/// Typed required boundaries while each native AI subsystem is being ported.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AiOperation {
    Goal(&'static str),
    TickSelector(goal::SelectorKind),
    TickRunningGoals(goal::SelectorKind),
    GoalSelectorMutation,
    NavigationTick,
    NavigationIsDone,
    NavigationPathType,
    Navigation(NavigationKind),
    MoveControlTick,
    MoveControl(MoveControlKind),
    LookControl,
    JumpControl,
    BodyRotationControl,
    Sensing,
}
impl AiOperation {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Goal(name) => name,
            Self::TickSelector(_) => "GoalSelector#tick",
            Self::TickRunningGoals(_) => "GoalSelector#tickRunningGoals",
            Self::GoalSelectorMutation => "GoalSelector structural mutation during a goal callback",
            Self::NavigationIsDone => "PathNavigation#isDone",
            Self::NavigationPathType => "NodeEvaluator#getPathType",
            Self::NavigationTick => "PathNavigation#tick",
            Self::Navigation(NavigationKind::Ground) => "GroundPathNavigation#tick",
            Self::Navigation(NavigationKind::Water) => "WaterBoundPathNavigation#tick",
            Self::Navigation(NavigationKind::Flying) => "FlyingPathNavigation#tick",
            Self::Navigation(NavigationKind::WallClimber) => "WallClimberNavigation#tick",
            Self::Navigation(NavigationKind::Amphibious) => "AmphibiousPathNavigation#tick",
            Self::MoveControl(MoveControlKind::Ground) => "MoveControl#tick",
            Self::MoveControlTick => "MoveControl#tick",
            Self::MoveControl(MoveControlKind::Drowned) => "DrownedMoveControl#tick",
            Self::MoveControl(MoveControlKind::AbstractCube) => {
                "AbstractCubeMob.CubeMobMoveControl#tick"
            }
            Self::LookControl => "LookControl#tick",
            Self::JumpControl => "JumpControl#tick",
            Self::BodyRotationControl => "BodyRotationControl#clientTick",
            Self::Sensing => "Sensing#tick",
        }
    }
}

/// Required dispatch until the live Mob-owned subsystem adapter is released.
/// Capture the receiver's type now, so the error can outlive the Level.
pub(crate) fn pending(
    entity: crate::entity::EntityRef,
    operation: AiOperation,
    level: &crate::level::Level,
) -> Result<(), crate::level::LevelError> {
    let entity_type = level.entity_state(entity)?.entity_type();
    Err(crate::level::LevelError::UnsupportedMobAi {
        entity,
        entity_type,
        operation,
    })
}
/// An unavailable native implementation, never an empty successful subsystem.
#[derive(Debug)]
pub struct Unported {
    operation: AiOperation,
}
impl Unported {
    pub const fn new(operation: AiOperation) -> Self {
        Self { operation }
    }
    pub const fn require(&self) -> Result<(), AiOperation> {
        Err(self.operation)
    }
}
/// Canonical Goal object state. Concrete variants are added with their ports;
/// constructor registration retains an explicit required boundary meanwhile.
#[derive(Debug)]
pub enum GoalState {
    Unported {
        native_name: &'static str,
        flags: goal::Flags,
    },
}
/// Mob-local retained Goal object identities, allocated before publication.
#[derive(Debug, Default)]
pub struct GoalArena {
    goals: Vec<GoalState>,
}
impl GoalArena {
    pub fn add(&mut self, state: GoalState) -> goal::GoalId {
        let id = goal::GoalId(self.goals.len());
        self.goals.push(state);
        id
    }
    pub fn get(&self, id: goal::GoalId) -> Option<&GoalState> {
        self.goals.get(id.0)
    }
    pub fn get_mut(&mut self, id: goal::GoalId) -> Option<&mut GoalState> {
        self.goals.get_mut(id.0)
    }
}
/// Port of Mob's single owned AI state. Native empty selectors are available;
/// unported controls/navigation/sensing remain typed required boundaries.
#[derive(Debug)]
pub struct MobAi {
    pub goal_selector: goal::GoalSelector,
    pub target_selector: goal::GoalSelector,
    pub goals: GoalArena,
    pub navigation: Unported,
    pub move_control: control::MoveControl,
    pub look_control: control::LookControl,
    pub jump_control: control::JumpControl,
    pub body_rotation_control: control::BodyRotationControl,
    pub sensing: Unported,
}
impl MobAi {
    /// Port of Mob's constructor order; family goal registration follows creation.
    pub fn new(navigation: NavigationKind) -> Self {
        Self {
            goal_selector: Default::default(),
            target_selector: Default::default(),
            goals: Default::default(),
            navigation: Unported::new(AiOperation::Navigation(navigation)),
            move_control: Default::default(),
            look_control: Default::default(),
            jump_control: Default::default(),
            body_rotation_control: Default::default(),
            sensing: Unported::new(AiOperation::Sensing),
        }
    }
}

impl Default for MobAi {
    /// Port of Mob constructor defaults; unavailable native bodies stay required.
    fn default() -> Self {
        Self::new(NavigationKind::Ground)
    }
}

#[cfg(test)]
mod pending_tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::{EntityRef, EntityState, animal::Cow},
        level::{Level, LevelContext, LevelError, Region},
    };

    #[test]
    fn pending_entries_preserve_receiver_type_and_display_without_a_live_level() {
        let mut level = Level::new(Region::new(&Behaviors::EMPTY), LevelContext::fixture(0, 63));
        let entity = level.next_entity_ref();
        let mut cow = Cow::default();
        cow.as_living_mut().parent.network_id = 9;
        cow.as_living_mut().parent.network_id_allocated = true;
        level
            .add_entity(entity, EntityState::Cow(Box::new(cow)))
            .unwrap();
        type Entry = fn(EntityRef, &mut Level) -> Result<(), LevelError>;
        let entries: [(AiOperation, Entry); 6] = [
            (AiOperation::Sensing, sensing::tick),
            (
                AiOperation::TickSelector(goal::SelectorKind::Goal),
                |id, l| goal::tick_selector(id, goal::SelectorKind::Goal, l),
            ),
            (
                AiOperation::TickSelector(goal::SelectorKind::Target),
                |id, l| goal::tick_selector(id, goal::SelectorKind::Target, l),
            ),
            (
                AiOperation::TickRunningGoals(goal::SelectorKind::Goal),
                |id, l| goal::tick_running_goals(id, goal::SelectorKind::Goal, true, l),
            ),
            (
                AiOperation::TickRunningGoals(goal::SelectorKind::Target),
                |id, l| goal::tick_running_goals(id, goal::SelectorKind::Target, false, l),
            ),
            (AiOperation::NavigationTick, navigation::tick),
        ];
        let mut captured = Vec::new();
        for (operation, entry) in entries {
            let error = entry(entity, &mut level).unwrap_err();
            assert!(
                matches!(error,LevelError::UnsupportedMobAi{entity:actual,entity_type,operation:actual_operation}
                if actual==entity && entity_type==ferraloom_data::EntityType::COW && actual_operation==operation)
            );
            captured.push((operation, error));
        }
        drop(level);
        for (operation, error) in captured {
            assert_eq!(
                error.to_string(),
                format!(
                    "unsupported mob AI {} for minecraft:cow ({entity:?})",
                    operation.name()
                )
            );
        }
    }
}
