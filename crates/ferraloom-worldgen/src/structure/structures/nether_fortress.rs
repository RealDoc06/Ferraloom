use crate::{
    Error,
    structure::{BoundingBox, piece::StructurePieceData},
};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::{Direction, Plane, random::RandomSource};

/// Port of NetherFortressPieces' concrete piece variants and persisted fields.
#[derive(Clone, Debug)]
pub enum FortressPieceKind {
    Start,
    BridgeCrossing,
    BridgeStraight,
    BridgeEndFiller { seed: i32 },
    RoomCrossing,
    StairsRoom,
    MonsterThrone { placed_spawner: bool },
    CastleEntrance,
    CastleSmallCorridor,
    CastleCrossing,
    CastleRightTurn { needs_chest: bool },
    CastleLeftTurn { needs_chest: bool },
    CastleStairs,
    CastleBalcony,
    CastleStalkRoom,
}
#[derive(Clone, Debug)]
pub struct FortressPiece {
    pub data: StructurePieceData,
    pub kind: FortressPieceKind,
}
impl FortressPiece {
    pub fn to_nbt(&self) -> CompoundTag {
        let id = match self.kind {
            FortressPieceKind::Start => "nestart",
            FortressPieceKind::BridgeCrossing => "nebcr",
            FortressPieceKind::BridgeStraight => "nebs",
            FortressPieceKind::BridgeEndFiller { .. } => "nebef",
            FortressPieceKind::RoomCrossing => "nerc",
            FortressPieceKind::StairsRoom => "nesr",
            FortressPieceKind::MonsterThrone { .. } => "nemt",
            FortressPieceKind::CastleEntrance => "nece",
            FortressPieceKind::CastleSmallCorridor => "nesc",
            FortressPieceKind::CastleCrossing => "nescsc",
            FortressPieceKind::CastleRightTurn { .. } => "nescrt",
            FortressPieceKind::CastleLeftTurn { .. } => "nesclt",
            FortressPieceKind::CastleStairs => "neccs",
            FortressPieceKind::CastleBalcony => "nectb",
            FortressPieceKind::CastleStalkRoom => "necsr",
        };
        let mut tag = self
            .data
            .to_nbt(&format!("minecraft:{id}").parse().expect("builtin piece"));
        match self.kind {
            FortressPieceKind::BridgeEndFiller { seed } => {
                tag.insert("Seed", Tag::Int(seed));
            }
            FortressPieceKind::MonsterThrone { placed_spawner } => {
                tag.insert("Mob", Tag::Byte(i8::from(placed_spawner)));
            }
            FortressPieceKind::CastleRightTurn { needs_chest }
            | FortressPieceKind::CastleLeftTurn { needs_chest } => {
                tag.insert("Chest", Tag::Byte(i8::from(needs_chest)));
            }
            _ => {}
        }
        tag
    }
    pub fn from_nbt(id: &str, tag: &CompoundTag) -> Result<Self, Error> {
        let kind = match id {
            "minecraft:nestart" => FortressPieceKind::Start,
            "minecraft:nebcr" => FortressPieceKind::BridgeCrossing,
            "minecraft:nebs" => FortressPieceKind::BridgeStraight,
            "minecraft:nebef" => FortressPieceKind::BridgeEndFiller {
                seed: tag.get_int("Seed").unwrap_or(0),
            },
            "minecraft:nerc" => FortressPieceKind::RoomCrossing,
            "minecraft:nesr" => FortressPieceKind::StairsRoom,
            "minecraft:nemt" => FortressPieceKind::MonsterThrone {
                placed_spawner: tag.get_byte("Mob").unwrap_or(0) != 0,
            },
            "minecraft:nece" => FortressPieceKind::CastleEntrance,
            "minecraft:nesc" => FortressPieceKind::CastleSmallCorridor,
            "minecraft:nescsc" => FortressPieceKind::CastleCrossing,
            "minecraft:nescrt" => FortressPieceKind::CastleRightTurn {
                needs_chest: tag.get_byte("Chest").unwrap_or(0) != 0,
            },
            "minecraft:nesclt" => FortressPieceKind::CastleLeftTurn {
                needs_chest: tag.get_byte("Chest").unwrap_or(0) != 0,
            },
            "minecraft:neccs" => FortressPieceKind::CastleStairs,
            "minecraft:nectb" => FortressPieceKind::CastleBalcony,
            "minecraft:necsr" => FortressPieceKind::CastleStalkRoom,
            _ => return Err(Error(format!("unknown fortress piece {id}"))),
        };
        Ok(Self {
            data: StructurePieceData::from_nbt(tag)?,
            kind,
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PieceType {
    BridgeStraight,
    BridgeCrossing,
    RoomCrossing,
    StairsRoom,
    MonsterThrone,
    CastleEntrance,
    CastleSmallCorridor,
    CastleCrossing,
    CastleRightTurn,
    CastleLeftTurn,
    CastleStairs,
    CastleBalcony,
    CastleStalkRoom,
    BridgeEndFiller,
}
#[derive(Clone, Copy)]
struct Weight {
    kind: PieceType,
    weight: i32,
    max: i32,
    count: i32,
    allow_in_row: bool,
}
impl Weight {
    const fn new(kind: PieceType, weight: i32, max: i32, allow_in_row: bool) -> Self {
        Self {
            kind,
            weight,
            max,
            count: 0,
            allow_in_row,
        }
    }
}
const BRIDGE: [Weight; 6] = [
    Weight::new(PieceType::BridgeStraight, 30, 0, true),
    Weight::new(PieceType::BridgeCrossing, 10, 4, false),
    Weight::new(PieceType::RoomCrossing, 10, 4, false),
    Weight::new(PieceType::StairsRoom, 10, 3, false),
    Weight::new(PieceType::MonsterThrone, 5, 2, false),
    Weight::new(PieceType::CastleEntrance, 5, 1, false),
];
const CASTLE: [Weight; 7] = [
    Weight::new(PieceType::CastleSmallCorridor, 25, 0, true),
    Weight::new(PieceType::CastleCrossing, 15, 5, false),
    Weight::new(PieceType::CastleRightTurn, 5, 10, false),
    Weight::new(PieceType::CastleLeftTurn, 5, 10, false),
    Weight::new(PieceType::CastleStairs, 10, 3, true),
    Weight::new(PieceType::CastleBalcony, 7, 2, false),
    Weight::new(PieceType::CastleStalkRoom, 5, 2, false),
];
struct Builder {
    pieces: Vec<FortressPiece>,
    pending: Vec<usize>,
    weights: [Vec<Weight>; 2],
    previous: Option<PieceType>,
}
/// Port of NetherFortressStructure#generatePieces. Mutable weights, pending
/// children and previous-piece identity belong exclusively to this start.
pub fn generate(random: &mut impl RandomSource, chunk: [i32; 2]) -> Vec<FortressPiece> {
    let direction = Plane::Horizontal.faces()[random.next_int_bound(4) as usize];
    let min = [chunk[0] * 16 + 2, 64, chunk[1] * 16 + 2];
    let max = [min[0] + 18, 73, min[2] + 18];
    let start = FortressPiece {
        data: StructurePieceData::new(BoundingBox::new(min, max), 0, Some(direction)),
        kind: FortressPieceKind::BridgeCrossing,
    };
    let mut builder = Builder {
        pieces: vec![start],
        pending: Vec::new(),
        weights: [BRIDGE.to_vec(), CASTLE.to_vec()],
        previous: None,
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
        .expect("start piece");
    let span = 70 - 48 + 1 - (bounds.max[1] - bounds.min[1] + 1);
    let bottom = if span > 1 {
        48 + random.next_int_bound(span)
    } else {
        48
    };
    for piece in &mut builder.pieces {
        piece.data.bounds = piece.data.bounds.moved([0, bottom - bounds.min[1], 0]);
    }
    builder.pieces
}
impl Builder {
    fn factory(
        &self,
        kind: PieceType,
        random: &mut impl RandomSource,
        foot: [i32; 3],
        direction: Direction,
        depth: i32,
    ) -> Option<FortressPiece> {
        let (offset, size) = match kind {
            PieceType::BridgeStraight => ([-1, -3, 0], [5, 10, 19]),
            PieceType::BridgeCrossing => ([-8, -3, 0], [19, 10, 19]),
            PieceType::RoomCrossing => ([-2, 0, 0], [7, 9, 7]),
            PieceType::StairsRoom => ([-2, 0, 0], [7, 11, 7]),
            PieceType::MonsterThrone => ([-2, 0, 0], [7, 8, 9]),
            PieceType::CastleEntrance | PieceType::CastleStalkRoom => ([-5, -3, 0], [13, 14, 13]),
            PieceType::CastleSmallCorridor
            | PieceType::CastleCrossing
            | PieceType::CastleRightTurn
            | PieceType::CastleLeftTurn => ([-1, 0, 0], [5, 7, 5]),
            PieceType::CastleStairs => ([-1, -7, 0], [5, 14, 10]),
            PieceType::CastleBalcony => ([-3, 0, 0], [9, 7, 9]),
            PieceType::BridgeEndFiller => ([-1, -3, 0], [5, 10, 8]),
        };
        let bounds = BoundingBox::orient(foot, offset, size, direction);
        if bounds.min[1] <= 10 || self.pieces.iter().any(|p| p.data.bounds.intersects(bounds)) {
            return None;
        }
        let kind = match kind {
            PieceType::BridgeStraight => FortressPieceKind::BridgeStraight,
            PieceType::BridgeCrossing => FortressPieceKind::BridgeCrossing,
            PieceType::RoomCrossing => FortressPieceKind::RoomCrossing,
            PieceType::StairsRoom => FortressPieceKind::StairsRoom,
            PieceType::MonsterThrone => FortressPieceKind::MonsterThrone {
                placed_spawner: false,
            },
            PieceType::CastleEntrance => FortressPieceKind::CastleEntrance,
            PieceType::CastleSmallCorridor => FortressPieceKind::CastleSmallCorridor,
            PieceType::CastleCrossing => FortressPieceKind::CastleCrossing,
            PieceType::CastleRightTurn => FortressPieceKind::CastleRightTurn {
                needs_chest: random.next_int_bound(3) == 0,
            },
            PieceType::CastleLeftTurn => FortressPieceKind::CastleLeftTurn {
                needs_chest: random.next_int_bound(3) == 0,
            },
            PieceType::CastleStairs => FortressPieceKind::CastleStairs,
            PieceType::CastleBalcony => FortressPieceKind::CastleBalcony,
            PieceType::CastleStalkRoom => FortressPieceKind::CastleStalkRoom,
            PieceType::BridgeEndFiller => FortressPieceKind::BridgeEndFiller {
                seed: random.next_int(),
            },
        };
        Some(FortressPiece {
            data: StructurePieceData::new(bounds, depth, Some(direction)),
            kind,
        })
    }
    fn select(
        &mut self,
        random: &mut impl RandomSource,
        foot: [i32; 3],
        direction: Direction,
        depth: i32,
        castle: bool,
    ) -> Option<FortressPiece> {
        let group = usize::from(castle);
        let weights = &self.weights[group];
        let total = if weights.iter().any(|w| w.max > 0 && w.count < w.max) {
            weights.iter().map(|w| w.weight).sum()
        } else {
            -1
        };
        if total > 0 && depth <= 30 {
            for _ in 0..5 {
                let mut choice = random.next_int_bound(total);
                for i in 0..self.weights[group].len() {
                    let w = self.weights[group][i];
                    choice -= w.weight;
                    if choice < 0 {
                        if (w.max != 0 && w.count >= w.max)
                            || (self.previous == Some(w.kind) && !w.allow_in_row)
                        {
                            break;
                        }
                        if let Some(piece) = self.factory(w.kind, random, foot, direction, depth) {
                            self.weights[group][i].count += 1;
                            self.previous = Some(w.kind);
                            if w.max != 0 && self.weights[group][i].count >= w.max {
                                self.weights[group].remove(i);
                            }
                            return Some(piece);
                        }
                    }
                }
            }
        }
        self.factory(PieceType::BridgeEndFiller, random, foot, direction, depth)
    }
    fn add(
        &mut self,
        random: &mut impl RandomSource,
        foot: [i32; 3],
        direction: Direction,
        depth: i32,
        castle: bool,
    ) {
        let b = self.pieces[0].data.bounds;
        if (foot[0] - b.min[0]).abs() > 112 || (foot[2] - b.min[2]).abs() > 112 {
            // Vanilla constructs an unretained filler outside the radius. Its
            // self-seed draw still advances the shared generation stream.
            let _ = self.factory(PieceType::BridgeEndFiller, random, foot, direction, depth);
            return;
        }
        if let Some(piece) = self.select(random, foot, direction, depth + 1, castle) {
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
        castle: bool,
    ) {
        let data = &self.pieces[index].data;
        let b = data.bounds;
        let Some(direction) = data.orientation else {
            return;
        };
        let (foot, d) = match side {
            Side::Forward => match direction {
                Direction::North => ([b.min[0] + offset, b.min[1] + y, b.min[2] - 1], direction),
                Direction::South => ([b.min[0] + offset, b.min[1] + y, b.max[2] + 1], direction),
                Direction::West => ([b.min[0] - 1, b.min[1] + y, b.min[2] + offset], direction),
                Direction::East => ([b.max[0] + 1, b.min[1] + y, b.min[2] + offset], direction),
                _ => unreachable!("horizontal fortress"),
            },
            Side::Left => {
                if matches!(direction, Direction::North | Direction::South) {
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
                if matches!(direction, Direction::North | Direction::South) {
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
        self.add(random, foot, d, data.gen_depth, castle);
    }
    fn children(&mut self, index: usize, random: &mut impl RandomSource) {
        use Side::{Forward, Left, Right};
        let children = match self.pieces[index].kind {
            FortressPieceKind::Start | FortressPieceKind::BridgeCrossing => vec![
                (Forward, 8, 3, false),
                (Left, 8, 3, false),
                (Right, 8, 3, false),
            ],
            FortressPieceKind::BridgeStraight => vec![(Forward, 1, 3, false)],
            FortressPieceKind::RoomCrossing => vec![
                (Forward, 2, 0, false),
                (Left, 2, 0, false),
                (Right, 2, 0, false),
            ],
            FortressPieceKind::StairsRoom => vec![(Right, 2, 6, false)],
            FortressPieceKind::CastleEntrance => vec![(Forward, 5, 3, true)],
            FortressPieceKind::CastleSmallCorridor | FortressPieceKind::CastleStairs => {
                vec![(Forward, 1, 0, true)]
            }
            FortressPieceKind::CastleCrossing => vec![
                (Forward, 1, 0, true),
                (Left, 1, 0, true),
                (Right, 1, 0, true),
            ],
            FortressPieceKind::CastleRightTurn { .. } => vec![(Right, 1, 0, true)],
            FortressPieceKind::CastleLeftTurn { .. } => vec![(Left, 1, 0, true)],
            FortressPieceKind::CastleStalkRoom => {
                vec![(Forward, 5, 3, true), (Forward, 5, 11, true)]
            }
            FortressPieceKind::CastleBalcony => {
                let offset = if matches!(
                    self.pieces[index].data.orientation,
                    Some(Direction::West | Direction::North)
                ) {
                    5
                } else {
                    1
                };
                let castle = random.next_int_bound(8) > 0;
                self.child(index, random, Left, offset, 0, castle);
                let castle = random.next_int_bound(8) > 0;
                self.child(index, random, Right, offset, 0, castle);
                return;
            }
            FortressPieceKind::MonsterThrone { .. } | FortressPieceKind::BridgeEndFiller { .. } => {
                Vec::new()
            }
        };
        for (side, offset, y, castle) in children {
            self.child(index, random, side, offset, y, castle);
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
    use ferraloom_nbt::ListTag;
    use ferraloom_util::random::{Algorithm, WorldgenRandom};
    use serde_json::Value;
    use std::io::Read;
    #[test]
    fn fortress_weighted_retries_pending_order_bounds_and_rng_match_vanilla() {
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../testdata/nether_fortress_pieces.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let chunk = std::array::from_fn(|i| case["chunk"][i].as_i64().unwrap() as i32);
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(0));
            random.set_large_feature_seed(seed, chunk[0], chunk[1]);
            let pieces = generate(&mut random, chunk);
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
                    Tag::Compound(FortressPiece::from_nbt(&id, &tag).unwrap().to_nbt())
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
