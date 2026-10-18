use crate::{
    Error,
    generator::ChunkGeneratorAccess,
    structure::{BoundingBox, piece::StructurePieceData},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{
    Direction, Plane,
    random::{RandomSource, WorldgenRandom},
};

/// Port of StrongholdPiece.SmallDoorType's legacy persisted names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SmallDoorType {
    Opening,
    WoodDoor,
    Grates,
    IronDoor,
}
impl SmallDoorType {
    fn name(self) -> &'static str {
        match self {
            Self::Opening => "OPENING",
            Self::WoodDoor => "WOOD_DOOR",
            Self::Grates => "GRATES",
            Self::IronDoor => "IRON_DOOR",
        }
    }
    fn random(random: &mut impl RandomSource) -> Self {
        match random.next_int_bound(5) {
            2 => Self::WoodDoor,
            3 => Self::Grates,
            4 => Self::IronDoor,
            _ => Self::Opening,
        }
    }
    fn from_nbt(tag: &CompoundTag) -> Result<Self, Error> {
        match tag
            .get_string("EntryDoor")
            .map(|s| s.to_string_lossy())
            .as_deref()
        {
            Some("OPENING") => Ok(Self::Opening),
            Some("WOOD_DOOR") => Ok(Self::WoodDoor),
            Some("GRATES") => Ok(Self::Grates),
            Some("IRON_DOOR") => Ok(Self::IronDoor),
            _ => Err(Error("invalid stronghold entry door".into())),
        }
    }
}
/// Port of the stronghold's typed pieces and persisted subclass state.
#[derive(Clone, Debug)]
pub enum StrongholdPieceKind {
    Start {
        source: bool,
    },
    StairsDown {
        source: bool,
    },
    Straight {
        left: bool,
        right: bool,
    },
    PrisonHall,
    LeftTurn,
    RightTurn,
    RoomCrossing {
        style: i32,
    },
    StraightStairs,
    FiveCrossing {
        left_low: bool,
        left_high: bool,
        right_low: bool,
        right_high: bool,
    },
    ChestCorridor {
        placed_chest: bool,
    },
    Library {
        tall: bool,
    },
    PortalRoom {
        placed_spawner: bool,
    },
    FillerCorridor {
        steps: i32,
    },
}
#[derive(Clone, Debug)]
pub struct StrongholdPiece {
    pub data: StructurePieceData,
    pub door: SmallDoorType,
    pub kind: StrongholdPieceKind,
}
impl StrongholdPiece {
    pub fn to_nbt(&self) -> CompoundTag {
        let id = match self.kind {
            StrongholdPieceKind::Start { .. } => "shstart",
            StrongholdPieceKind::StairsDown { .. } => "shsd",
            StrongholdPieceKind::Straight { .. } => "shs",
            StrongholdPieceKind::PrisonHall => "shph",
            StrongholdPieceKind::LeftTurn => "shlt",
            StrongholdPieceKind::RightTurn => "shrt",
            StrongholdPieceKind::RoomCrossing { .. } => "shrc",
            StrongholdPieceKind::StraightStairs => "shssd",
            StrongholdPieceKind::FiveCrossing { .. } => "sh5c",
            StrongholdPieceKind::ChestCorridor { .. } => "shcc",
            StrongholdPieceKind::Library { .. } => "shli",
            StrongholdPieceKind::PortalRoom { .. } => "shpr",
            StrongholdPieceKind::FillerCorridor { .. } => "shfc",
        };
        let mut tag = self
            .data
            .to_nbt(&format!("minecraft:{id}").parse().expect("builtin piece"));
        tag.insert("EntryDoor", Tag::String(self.door.name().into()));
        let mut boolean = |name, value| {
            tag.insert(name, Tag::Byte(i8::from(value)));
        };
        match self.kind {
            StrongholdPieceKind::Start { source } | StrongholdPieceKind::StairsDown { source } => {
                boolean("Source", source)
            }
            StrongholdPieceKind::Straight { left, right } => {
                boolean("Left", left);
                boolean("Right", right);
            }
            StrongholdPieceKind::FiveCrossing {
                left_low,
                left_high,
                right_low,
                right_high,
            } => {
                boolean("leftLow", left_low);
                boolean("leftHigh", left_high);
                boolean("rightLow", right_low);
                boolean("rightHigh", right_high);
            }
            StrongholdPieceKind::ChestCorridor { placed_chest } => boolean("Chest", placed_chest),
            StrongholdPieceKind::Library { tall } => boolean("Tall", tall),
            StrongholdPieceKind::PortalRoom { placed_spawner } => boolean("Mob", placed_spawner),
            StrongholdPieceKind::RoomCrossing { style } => {
                tag.insert("Type", Tag::Int(style));
            }
            StrongholdPieceKind::FillerCorridor { steps } => {
                tag.insert("Steps", Tag::Int(steps));
            }
            _ => {}
        }
        tag
    }
    pub fn from_nbt(id: &str, tag: &CompoundTag) -> Result<Self, Error> {
        let b = |name| tag.get_byte(name).unwrap_or(0) != 0;
        let kind = match id {
            "minecraft:shstart" => StrongholdPieceKind::Start {
                source: b("Source"),
            },
            "minecraft:shsd" => StrongholdPieceKind::StairsDown {
                source: b("Source"),
            },
            "minecraft:shs" => StrongholdPieceKind::Straight {
                left: b("Left"),
                right: b("Right"),
            },
            "minecraft:shph" => StrongholdPieceKind::PrisonHall,
            "minecraft:shlt" => StrongholdPieceKind::LeftTurn,
            "minecraft:shrt" => StrongholdPieceKind::RightTurn,
            "minecraft:shrc" => StrongholdPieceKind::RoomCrossing {
                style: tag.get_int("Type").unwrap_or(0),
            },
            "minecraft:shssd" => StrongholdPieceKind::StraightStairs,
            "minecraft:sh5c" => StrongholdPieceKind::FiveCrossing {
                left_low: b("leftLow"),
                left_high: b("leftHigh"),
                right_low: b("rightLow"),
                right_high: b("rightHigh"),
            },
            "minecraft:shcc" => StrongholdPieceKind::ChestCorridor {
                placed_chest: b("Chest"),
            },
            "minecraft:shli" => StrongholdPieceKind::Library { tall: b("Tall") },
            "minecraft:shpr" => StrongholdPieceKind::PortalRoom {
                placed_spawner: b("Mob"),
            },
            "minecraft:shfc" => StrongholdPieceKind::FillerCorridor {
                steps: tag.get_int("Steps").unwrap_or(0),
            },
            _ => return Err(Error(format!("unknown stronghold piece {id}"))),
        };
        Ok(Self {
            data: StructurePieceData::from_nbt(tag)?,
            door: SmallDoorType::from_nbt(tag)?,
            kind,
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PieceType {
    Straight,
    PrisonHall,
    LeftTurn,
    RightTurn,
    RoomCrossing,
    StraightStairs,
    StairsDown,
    FiveCrossing,
    ChestCorridor,
    Library,
    PortalRoom,
}
#[derive(Clone, Copy)]
struct Weight {
    kind: PieceType,
    weight: i32,
    max: i32,
    count: i32,
}
impl Weight {
    const fn new(kind: PieceType, weight: i32, max: i32) -> Self {
        Self {
            kind,
            weight,
            max,
            count: 0,
        }
    }
}
const WEIGHTS: [Weight; 11] = [
    Weight::new(PieceType::Straight, 40, 0),
    Weight::new(PieceType::PrisonHall, 5, 5),
    Weight::new(PieceType::LeftTurn, 20, 0),
    Weight::new(PieceType::RightTurn, 20, 0),
    Weight::new(PieceType::RoomCrossing, 10, 6),
    Weight::new(PieceType::StraightStairs, 5, 5),
    Weight::new(PieceType::StairsDown, 5, 5),
    Weight::new(PieceType::FiveCrossing, 5, 4),
    Weight::new(PieceType::ChestCorridor, 5, 4),
    Weight::new(PieceType::Library, 10, 2),
    Weight::new(PieceType::PortalRoom, 20, 1),
];
struct Builder {
    pieces: Vec<StrongholdPiece>,
    pending: Vec<usize>,
    weights: Vec<Weight>,
    previous: Option<PieceType>,
    imposed: Option<PieceType>,
    portal: bool,
}
/// Port of StrongholdStructure#generatePieces. Retry seeds, weight counts and
/// the imposed first crossing are local to one start, with no static mutation.
pub fn generate(
    seed: i64,
    chunk: [i32; 2],
    random: &mut WorldgenRandom,
    generator: &dyn ChunkGeneratorAccess,
) -> Vec<StrongholdPiece> {
    let mut tries = 0_i32;
    loop {
        random.set_large_feature_seed(seed.wrapping_add(i64::from(tries)), chunk[0], chunk[1]);
        tries = tries.wrapping_add(1);
        let direction = Plane::Horizontal.faces()[random.next_int_bound(4) as usize];
        let min = [chunk[0] * 16 + 2, 64, chunk[1] * 16 + 2];
        let max = [min[0] + 4, 74, min[2] + 4];
        let start = StrongholdPiece {
            data: StructurePieceData::new(BoundingBox::new(min, max), 0, Some(direction)),
            door: SmallDoorType::Opening,
            kind: StrongholdPieceKind::Start { source: true },
        };
        let mut builder = Builder {
            pieces: vec![start],
            pending: Vec::new(),
            weights: WEIGHTS.to_vec(),
            previous: None,
            imposed: None,
            portal: false,
        };
        builder.children(0, random);
        while !builder.pending.is_empty() {
            let i = random.next_int_bound(builder.pending.len() as i32) as usize;
            let piece = builder.pending.remove(i);
            builder.children(piece, random);
        }
        let bounds = builder
            .pieces
            .iter()
            .map(|p| p.data.bounds)
            .reduce(BoundingBox::encapsulating)
            .expect("start room");
        let cap = generator.sea_level() - 10;
        let mut top = bounds.max[1] - bounds.min[1] + 1 + generator.min_y() + 1;
        if top < cap {
            top += random.next_int_bound(cap - top);
        }
        for piece in &mut builder.pieces {
            piece.data.bounds = piece.data.bounds.moved([0, top - bounds.max[1], 0]);
        }
        if builder.portal {
            return builder.pieces;
        }
    }
}
impl Builder {
    fn collision(&self, bounds: BoundingBox) -> Option<&StrongholdPiece> {
        self.pieces
            .iter()
            .find(|p| p.data.bounds.intersects(bounds))
    }
    fn factory(
        &self,
        kind: PieceType,
        random: &mut impl RandomSource,
        foot: [i32; 3],
        direction: Direction,
        depth: i32,
    ) -> Option<StrongholdPiece> {
        let (offset, size) = match kind {
            PieceType::Straight | PieceType::ChestCorridor => ([-1, -1, 0], [5, 5, 7]),
            PieceType::PrisonHall => ([-1, -1, 0], [9, 5, 11]),
            PieceType::LeftTurn | PieceType::RightTurn => ([-1, -1, 0], [5, 5, 5]),
            PieceType::RoomCrossing => ([-4, -1, 0], [11, 7, 11]),
            PieceType::StraightStairs => ([-1, -7, 0], [5, 11, 8]),
            PieceType::StairsDown => ([-1, -7, 0], [5, 11, 5]),
            PieceType::FiveCrossing => ([-4, -3, 0], [10, 9, 11]),
            PieceType::Library => ([-4, -1, 0], [14, 11, 15]),
            PieceType::PortalRoom => ([-4, -1, 0], [11, 8, 16]),
        };
        let mut bounds = BoundingBox::orient(foot, offset, size, direction);
        if bounds.min[1] <= 10 || self.collision(bounds).is_some() {
            if kind != PieceType::Library {
                return None;
            }
            bounds = BoundingBox::orient(foot, offset, [14, 6, 15], direction);
            if bounds.min[1] <= 10 || self.collision(bounds).is_some() {
                return None;
            }
        }
        let door = if kind == PieceType::PortalRoom {
            SmallDoorType::Opening
        } else {
            SmallDoorType::random(random)
        };
        let kind = match kind {
            PieceType::Straight => StrongholdPieceKind::Straight {
                left: random.next_int_bound(2) == 0,
                right: random.next_int_bound(2) == 0,
            },
            PieceType::PrisonHall => StrongholdPieceKind::PrisonHall,
            PieceType::LeftTurn => StrongholdPieceKind::LeftTurn,
            PieceType::RightTurn => StrongholdPieceKind::RightTurn,
            PieceType::RoomCrossing => StrongholdPieceKind::RoomCrossing {
                style: random.next_int_bound(5),
            },
            PieceType::StraightStairs => StrongholdPieceKind::StraightStairs,
            PieceType::StairsDown => StrongholdPieceKind::StairsDown { source: false },
            PieceType::FiveCrossing => StrongholdPieceKind::FiveCrossing {
                left_low: random.next_boolean(),
                left_high: random.next_boolean(),
                right_low: random.next_boolean(),
                right_high: random.next_int_bound(3) > 0,
            },
            PieceType::ChestCorridor => StrongholdPieceKind::ChestCorridor {
                placed_chest: false,
            },
            PieceType::Library => StrongholdPieceKind::Library {
                tall: bounds.max[1] - bounds.min[1] + 1 > 6,
            },
            PieceType::PortalRoom => StrongholdPieceKind::PortalRoom {
                placed_spawner: false,
            },
        };
        Some(StrongholdPiece {
            data: StructurePieceData::new(bounds, depth, Some(direction)),
            door,
            kind,
        })
    }
    fn select(
        &mut self,
        random: &mut impl RandomSource,
        foot: [i32; 3],
        direction: Direction,
        depth: i32,
    ) -> Option<StrongholdPiece> {
        if !self.weights.iter().any(|w| w.max > 0 && w.count < w.max) {
            return None;
        }
        let total = self.weights.iter().map(|w| w.weight).sum();
        if let Some(kind) = self.imposed.take() {
            if let Some(piece) = self.factory(kind, random, foot, direction, depth) {
                return Some(piece);
            }
        }
        for _ in 0..5 {
            let mut choice = random.next_int_bound(total);
            for i in 0..self.weights.len() {
                let w = self.weights[i];
                choice -= w.weight;
                if choice < 0 {
                    let allowed = (w.max == 0 || w.count < w.max)
                        && (w.kind != PieceType::Library || depth > 4)
                        && (w.kind != PieceType::PortalRoom || depth > 5);
                    if !allowed || self.previous == Some(w.kind) {
                        break;
                    }
                    if let Some(piece) = self.factory(w.kind, random, foot, direction, depth) {
                        self.weights[i].count += 1;
                        self.previous = Some(w.kind);
                        if w.max != 0 && self.weights[i].count >= w.max {
                            self.weights.remove(i);
                        }
                        return Some(piece);
                    }
                }
            }
        }
        let large = BoundingBox::orient(foot, [-1, -1, 0], [5, 5, 4], direction);
        let collision = self.collision(large)?;
        if collision.data.bounds.min[1] != large.min[1] {
            return None;
        }
        for size in (1..=2).rev() {
            let bounds = BoundingBox::orient(foot, [-1, -1, 0], [5, 5, size], direction);
            if !collision.data.bounds.intersects(bounds) {
                let bounds = BoundingBox::orient(foot, [-1, -1, 0], [5, 5, size + 1], direction);
                if bounds.min[1] <= 1 {
                    return None;
                }
                let axis = if matches!(direction, Direction::North | Direction::South) {
                    2
                } else {
                    0
                };
                return Some(StrongholdPiece {
                    data: StructurePieceData::new(bounds, depth, Some(direction)),
                    door: SmallDoorType::Opening,
                    kind: StrongholdPieceKind::FillerCorridor {
                        steps: bounds.max[axis] - bounds.min[axis] + 1,
                    },
                });
            }
        }
        None
    }
    fn add(
        &mut self,
        random: &mut impl RandomSource,
        foot: [i32; 3],
        direction: Direction,
        depth: i32,
    ) {
        let b = self.pieces[0].data.bounds;
        if depth > 50 || (foot[0] - b.min[0]).abs() > 112 || (foot[2] - b.min[2]).abs() > 112 {
            return;
        }
        if let Some(piece) = self.select(random, foot, direction, depth + 1) {
            self.pending.push(self.pieces.len());
            self.pieces.push(piece);
        }
    }
    fn child(
        &mut self,
        index: usize,
        random: &mut impl RandomSource,
        side: Side,
        offset: i32,
        y: i32,
    ) {
        let data = &self.pieces[index].data;
        let b = data.bounds;
        let Some(d) = data.orientation else {
            return;
        };
        let (foot, direction) = match side {
            Side::Forward => match d {
                Direction::North => ([b.min[0] + offset, b.min[1] + y, b.min[2] - 1], d),
                Direction::South => ([b.min[0] + offset, b.min[1] + y, b.max[2] + 1], d),
                Direction::West => ([b.min[0] - 1, b.min[1] + y, b.min[2] + offset], d),
                Direction::East => ([b.max[0] + 1, b.min[1] + y, b.min[2] + offset], d),
                _ => unreachable!("horizontal stronghold"),
            },
            Side::Left => {
                if matches!(d, Direction::North | Direction::South) {
                    (
                        [b.min[0] - 1, b.min[1] + y, b.min[2] + offset],
                        Direction::West,
                    )
                } else {
                    (
                        [b.min[0] + offset, b.min[1] + y, b.min[2] - 1],
                        Direction::North,
                    )
                }
            }
            Side::Right => {
                if matches!(d, Direction::North | Direction::South) {
                    (
                        [b.max[0] + 1, b.min[1] + y, b.min[2] + offset],
                        Direction::East,
                    )
                } else {
                    (
                        [b.min[0] + offset, b.min[1] + y, b.max[2] + 1],
                        Direction::South,
                    )
                }
            }
        };
        self.add(random, foot, direction, data.gen_depth);
    }
    fn children(&mut self, index: usize, random: &mut impl RandomSource) {
        use Side::{Forward, Left, Right};
        let mut children = Vec::new();
        match self.pieces[index].kind {
            StrongholdPieceKind::Start { source } | StrongholdPieceKind::StairsDown { source } => {
                if source {
                    self.imposed = Some(PieceType::FiveCrossing);
                }
                children.push((Forward, 1, 1));
            }
            StrongholdPieceKind::Straight { left, right } => {
                children.push((Forward, 1, 1));
                if left {
                    children.push((Left, 2, 1));
                }
                if right {
                    children.push((Right, 2, 1));
                }
            }
            StrongholdPieceKind::PrisonHall
            | StrongholdPieceKind::StraightStairs
            | StrongholdPieceKind::ChestCorridor { .. } => children.push((Forward, 1, 1)),
            StrongholdPieceKind::LeftTurn => children.push((
                if matches!(
                    self.pieces[index].data.orientation,
                    Some(Direction::North | Direction::East)
                ) {
                    Left
                } else {
                    Right
                },
                1,
                1,
            )),
            StrongholdPieceKind::RightTurn => children.push((
                if matches!(
                    self.pieces[index].data.orientation,
                    Some(Direction::North | Direction::East)
                ) {
                    Right
                } else {
                    Left
                },
                1,
                1,
            )),
            StrongholdPieceKind::RoomCrossing { .. } => {
                children.extend([(Forward, 4, 1), (Left, 4, 1), (Right, 4, 1)])
            }
            StrongholdPieceKind::FiveCrossing {
                left_low,
                left_high,
                right_low,
                right_high,
            } => {
                let (a, b) = if matches!(
                    self.pieces[index].data.orientation,
                    Some(Direction::West | Direction::North)
                ) {
                    (5, 3)
                } else {
                    (3, 5)
                };
                children.push((Forward, 5, 1));
                if left_low {
                    children.push((Left, 1, a));
                }
                if left_high {
                    children.push((Left, 7, b));
                }
                if right_low {
                    children.push((Right, 1, a));
                }
                if right_high {
                    children.push((Right, 7, b));
                }
            }
            StrongholdPieceKind::PortalRoom { .. } => self.portal = true,
            StrongholdPieceKind::Library { .. } | StrongholdPieceKind::FillerCorridor { .. } => {}
        }
        for (side, offset, y) in children {
            self.child(index, random, side, offset, y);
        }
    }
}
#[derive(Clone, Copy)]
enum Side {
    Forward,
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generator::flat::{FlatLevelGeneratorSettings, FlatLevelSource},
        structure::processor::tests::generator,
    };
    use ferraloom_nbt::ListTag;
    use ferraloom_util::random::Algorithm;
    use serde_json::Value;
    use std::io::Read;
    #[test]
    fn stronghold_retry_seeds_imposed_crossing_pending_order_and_rng_match_vanilla() {
        let (_, noise) = generator();
        let flat = FlatLevelSource::new(
            noise.data.clone(),
            FlatLevelGeneratorSettings::default_settings(),
        )
        .unwrap();
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../testdata/stronghold_pieces.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let chunk = std::array::from_fn(|i| case["chunk"][i].as_i64().unwrap() as i32);
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(0));
            let pieces = generate(seed, chunk, &mut random, &flat);
            let expected = ferraloom_nbt::parse(case["nbt"].as_str().unwrap()).unwrap();
            let expected = expected.as_list().unwrap();
            assert_eq!(pieces.len(), expected.len(), "seed={seed} chunk={chunk:?}");
            for (i, (actual, expected)) in pieces.iter().zip(expected.iter()).enumerate() {
                assert_eq!(
                    Tag::Compound(actual.to_nbt()),
                    *expected,
                    "seed={seed} chunk={chunk:?} piece={i}"
                );
            }
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "seed={seed} chunk={chunk:?}"
            );
            let loaded = pieces
                .iter()
                .map(|p| {
                    let tag = p.to_nbt();
                    let id = tag.get_string("id").unwrap().to_string_lossy();
                    Tag::Compound(StrongholdPiece::from_nbt(&id, &tag).unwrap().to_nbt())
                })
                .collect::<Vec<_>>();
            assert_eq!(
                Tag::List(ListTag(loaded)),
                ferraloom_nbt::parse(case["loaded"].as_str().unwrap()).unwrap(),
                "seed={seed} chunk={chunk:?}"
            );
        }
    }
}
