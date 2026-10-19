/// Port of PathType: wire IDs and default costs are explicit vanilla data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PathType {
    Blocked = 0,
    Open = 1,
    Walkable = 2,
    WalkableDoor = 3,
    Trapdoor = 4,
    PowderSnow = 5,
    OnTopOfPowderSnow = 6,
    Fence = 7,
    Lava = 8,
    Water = 9,
    WaterBorder = 10,
    Rail = 11,
    UnpassableRail = 12,
    FireInNeighbor = 13,
    Fire = 14,
    DamagingInNeighbor = 15,
    Damaging = 16,
    DoorOpen = 17,
    DoorWoodClosed = 18,
    DoorIronClosed = 19,
    Breach = 20,
    Leaves = 21,
    StickyHoney = 22,
    Cocoa = 23,
    DamageCautious = 24,
    OnTopOfTrapdoor = 25,
    BigMobsCloseToDanger = 26,
}
impl PathType {
    pub const ALL: [Self; 27] = [
        Self::Blocked,
        Self::Open,
        Self::Walkable,
        Self::WalkableDoor,
        Self::Trapdoor,
        Self::PowderSnow,
        Self::OnTopOfPowderSnow,
        Self::Fence,
        Self::Lava,
        Self::Water,
        Self::WaterBorder,
        Self::Rail,
        Self::UnpassableRail,
        Self::FireInNeighbor,
        Self::Fire,
        Self::DamagingInNeighbor,
        Self::Damaging,
        Self::DoorOpen,
        Self::DoorWoodClosed,
        Self::DoorIronClosed,
        Self::Breach,
        Self::Leaves,
        Self::StickyHoney,
        Self::Cocoa,
        Self::DamageCautious,
        Self::OnTopOfTrapdoor,
        Self::BigMobsCloseToDanger,
    ];
    pub fn id(self) -> u8 {
        self as u8
    }
    /// Port of ByIdMap.OutOfBoundsStrategy.ZERO.
    pub fn by_id(id: i32) -> Self {
        Self::ALL.get(id as usize).copied().unwrap_or(Self::Blocked)
    }
    pub fn malus(self) -> f32 {
        match self {
            Self::Blocked => -1.0,
            Self::Open => 0.0,
            Self::Walkable => 0.0,
            Self::WalkableDoor => 0.0,
            Self::Trapdoor => 0.0,
            Self::PowderSnow => -1.0,
            Self::OnTopOfPowderSnow => 0.0,
            Self::Fence => -1.0,
            Self::Lava => -1.0,
            Self::Water => 8.0,
            Self::WaterBorder => 8.0,
            Self::Rail => 0.0,
            Self::UnpassableRail => -1.0,
            Self::FireInNeighbor => 8.0,
            Self::Fire => 16.0,
            Self::DamagingInNeighbor => 8.0,
            Self::Damaging => -1.0,
            Self::DoorOpen => 0.0,
            Self::DoorWoodClosed => -1.0,
            Self::DoorIronClosed => -1.0,
            Self::Breach => 4.0,
            Self::Leaves => -1.0,
            Self::StickyHoney => 8.0,
            Self::Cocoa => 0.0,
            Self::DamageCautious => 0.0,
            Self::OnTopOfTrapdoor => 0.0,
            Self::BigMobsCloseToDanger => 4.0,
        }
    }
}
