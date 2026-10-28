use super::{PoiTypeId, PoiTypes};
use ferraloom_util::BlockPos;

/// Port of PoiRecord. Equality and hashing in native sets use only the position.
#[derive(Clone, Debug)]
pub struct PoiRecord {
    pub pos: BlockPos,
    pub poi_type: PoiTypeId,
    pub free_tickets: i32,
}
impl PoiRecord {
    pub fn new(pos: BlockPos, poi_type: PoiTypeId) -> Self {
        Self {
            pos,
            poi_type,
            free_tickets: PoiTypes::builtin().get(poi_type).max_tickets,
        }
    }
    pub fn has_space(&self) -> bool {
        self.free_tickets > 0
    }
    pub fn is_occupied(&self) -> bool {
        self.free_tickets != PoiTypes::builtin().get(self.poi_type).max_tickets
    }
    pub fn acquire_ticket(&mut self) -> bool {
        if self.free_tickets <= 0 {
            return false;
        }
        self.free_tickets -= 1;
        true
    }
    pub fn release_ticket(&mut self) -> bool {
        if self.free_tickets >= PoiTypes::builtin().get(self.poi_type).max_tickets {
            return false;
        }
        self.free_tickets += 1;
        true
    }
}

/// Port of PoiManager.Occupancy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Occupancy {
    HasSpace,
    IsOccupied,
    Any,
}
impl Occupancy {
    pub fn test(self, record: &PoiRecord) -> bool {
        match self {
            Self::HasSpace => record.has_space(),
            Self::IsOccupied => record.is_occupied(),
            Self::Any => true,
        }
    }
}

impl PartialEq for PoiRecord {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}
impl Eq for PoiRecord {}
