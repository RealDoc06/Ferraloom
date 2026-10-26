pub mod attributes;
pub mod goal;
pub mod pathfinder;

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
    GoalSelectorMutation,
    Navigation(NavigationKind),
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
            Self::GoalSelectorMutation => "GoalSelector structural mutation during a goal callback",
            Self::Navigation(NavigationKind::Ground) => "GroundPathNavigation#tick",
            Self::Navigation(NavigationKind::Water) => "WaterBoundPathNavigation#tick",
            Self::Navigation(NavigationKind::Flying) => "FlyingPathNavigation#tick",
            Self::Navigation(NavigationKind::WallClimber) => "WallClimberNavigation#tick",
            Self::Navigation(NavigationKind::Amphibious) => "AmphibiousPathNavigation#tick",
            Self::MoveControl(MoveControlKind::Ground) => "MoveControl#tick",
            Self::MoveControl(MoveControlKind::Drowned) => "DrownedMoveControl#tick",
            Self::MoveControl(MoveControlKind::AbstractCube) => {
                "AbstractCubeMob.CubeMobMoveControl#tick"
            }
            Self::LookControl => "LookControl#tick",
            Self::JumpControl => "JumpControl#tick",
            Self::BodyRotationControl => "BodyRotationControl#clientTick",
            Self::Sensing => "Sensing#hasLineOfSight",
        }
    }
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
    pub move_control: Unported,
    pub look_control: Unported,
    pub jump_control: Unported,
    pub body_rotation_control: Unported,
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
            move_control: Unported::new(AiOperation::MoveControl(MoveControlKind::Ground)),
            look_control: Unported::new(AiOperation::LookControl),
            jump_control: Unported::new(AiOperation::JumpControl),
            body_rotation_control: Unported::new(AiOperation::BodyRotationControl),
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
