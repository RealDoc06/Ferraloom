mod placement;
use crate::{
    Error, codec,
    generator::ChunkGeneratorAccess,
    region::Heightmap,
    structure::{BoundingBox, piece::StructurePieceData},
};
use ferraloom_data::Block;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_util::{Direction, random::RandomSource};
use serde_json::Value;

/// Port of MineshaftStructure.Type, including the persisted ordinal fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MineshaftType {
    Normal,
    Mesa,
}
impl MineshaftType {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        match codec::string(codec::field(v, "mineshaft_type")?)? {
            "normal" => Ok(Self::Normal),
            "mesa" => Ok(Self::Mesa),
            _ => Err(Error("invalid mineshaft type".into())),
        }
    }
    pub fn wood(self) -> Block {
        match self {
            Self::Normal => Block::OAK_LOG,
            Self::Mesa => Block::DARK_OAK_LOG,
        }
    }
    pub fn planks(self) -> Block {
        match self {
            Self::Normal => Block::OAK_PLANKS,
            Self::Mesa => Block::DARK_OAK_PLANKS,
        }
    }
    pub fn fence(self) -> Block {
        match self {
            Self::Normal => Block::OAK_FENCE,
            Self::Mesa => Block::DARK_OAK_FENCE,
        }
    }
}
/// Port of the four MineshaftPieces subclasses. Room entrances remain ordered
/// and move with their piece; crossing direction is independent of orientation.
#[derive(Clone, Debug)]
pub enum MineshaftPieceKind {
    Room {
        entrances: Vec<BoundingBox>,
    },
    Corridor {
        rails: bool,
        spiders: bool,
        placed_spider: bool,
        sections: i32,
    },
    Crossing {
        direction: Direction,
        two_floored: bool,
    },
    Stairs,
}
#[derive(Clone, Debug)]
pub struct MineshaftPiece {
    pub data: StructurePieceData,
    pub mine_type: MineshaftType,
    pub kind: MineshaftPieceKind,
}
impl MineshaftPiece {
    pub fn to_nbt(&self) -> CompoundTag {
        let id = match self.kind {
            MineshaftPieceKind::Room { .. } => "minecraft:msroom",
            MineshaftPieceKind::Corridor { .. } => "minecraft:mscorridor",
            MineshaftPieceKind::Crossing { .. } => "minecraft:mscrossing",
            MineshaftPieceKind::Stairs => "minecraft:msstairs",
        };
        let mut tag = self.data.to_nbt(&id.parse().expect("builtin piece"));
        tag.insert(
            "MST",
            Tag::Int(i32::from(self.mine_type == MineshaftType::Mesa)),
        );
        match &self.kind {
            MineshaftPieceKind::Room { entrances } => {
                tag.insert(
                    "Entrances",
                    Tag::List(ListTag(
                        entrances
                            .iter()
                            .map(|b| Tag::IntArray(b.min.into_iter().chain(b.max).collect()))
                            .collect(),
                    )),
                );
            }
            MineshaftPieceKind::Corridor {
                rails,
                spiders,
                placed_spider,
                sections,
            } => {
                for (name, value) in [("hr", *rails), ("sc", *spiders), ("hps", *placed_spider)] {
                    tag.insert(name, Tag::Byte(i8::from(value)));
                }
                tag.insert("Num", Tag::Int(*sections));
            }
            MineshaftPieceKind::Crossing {
                direction,
                two_floored,
            } => {
                tag.insert("D", Tag::Byte(direction.data_2d() as i8));
                tag.insert("tf", Tag::Byte(i8::from(*two_floored)));
            }
            MineshaftPieceKind::Stairs => {}
        }
        tag
    }
    pub fn from_nbt(id: &str, tag: &CompoundTag) -> Result<Self, Error> {
        let boolean = |name| tag.get_byte(name).unwrap_or(0) != 0;
        let kind = match id {
            "minecraft:msroom" => MineshaftPieceKind::Room {
                entrances: tag
                    .get_list("Entrances")
                    .and_then(|list| {
                        list.iter()
                            .map(|v| {
                                let mut wrapper = CompoundTag::new();
                                wrapper.insert("BB", v.clone());
                                StructurePieceData::from_nbt(&wrapper).map(|d| d.bounds)
                            })
                            .collect::<Result<Vec<_>, _>>()
                            .ok()
                    })
                    .unwrap_or_default(),
            },
            "minecraft:mscorridor" => MineshaftPieceKind::Corridor {
                rails: boolean("hr"),
                spiders: boolean("sc"),
                placed_spider: boolean("hps"),
                sections: tag.get_int("Num").unwrap_or(0),
            },
            "minecraft:mscrossing" => MineshaftPieceKind::Crossing {
                direction: Direction::from_2d_data_value(tag.get_int("D").unwrap_or(0)),
                two_floored: boolean("tf"),
            },
            "minecraft:msstairs" => MineshaftPieceKind::Stairs,
            _ => return Err(Error(format!("unknown mineshaft piece {id}"))),
        };
        Ok(Self {
            data: StructurePieceData::from_nbt(tag)?,
            mine_type: if tag.get_int("MST") == Some(1) {
                MineshaftType::Mesa
            } else {
                MineshaftType::Normal
            },
            kind,
        })
    }
    fn moved(&mut self, dy: i32) {
        self.data.bounds = self.data.bounds.moved([0, dy, 0]);
        if let MineshaftPieceKind::Room { entrances } = &mut self.kind {
            for b in entrances {
                *b = b.moved([0, dy, 0]);
            }
        }
    }
}

/// Port of MineshaftStructure#findGenerationPoint and its job-local recursive
/// builder. Collision probes include every previously accepted piece.
pub fn generate(
    mine_type: MineshaftType,
    random: &mut impl RandomSource,
    generator: &dyn ChunkGeneratorAccess,
    chunk: [i32; 2],
    min_y: i32,
    height: i32,
) -> Result<([i32; 3], Vec<MineshaftPiece>), Error> {
    random.next_double();
    let west = chunk[0] * 16 + 2;
    let north = chunk[1] * 16 + 2;
    let room = MineshaftPiece {
        data: StructurePieceData::new(
            BoundingBox::new(
                [west, 50, north],
                [
                    west + 7 + random.next_int_bound(6),
                    54 + random.next_int_bound(6),
                    north + 7 + random.next_int_bound(6),
                ],
            ),
            0,
            None,
        ),
        mine_type,
        kind: MineshaftPieceKind::Room {
            entrances: Vec::new(),
        },
    };
    let mut pieces = vec![room];
    add_children(0, &mut pieces, random);
    let bounds = pieces
        .iter()
        .map(|p| p.data.bounds)
        .reduce(BoundingBox::encapsulating)
        .expect("start room");
    let sea = generator.sea_level();
    let dy = if mine_type == MineshaftType::Mesa {
        let center = bounds.center();
        let surface = generator.get_base_height(
            center[0],
            center[2],
            Heightmap::WorldSurfaceWg,
            min_y,
            height,
        )?;
        let target = if surface <= sea {
            sea
        } else {
            sea + random.next_int_bound(surface - sea + 1)
        };
        target - center[1]
    } else {
        let max_y = sea - 10;
        let mut top = bounds.max[1] - bounds.min[1] + 1 + generator.min_y() + 1;
        if top < max_y {
            top += random.next_int_bound(max_y - top);
        }
        top - bounds.max[1]
    };
    for piece in &mut pieces {
        piece.moved(dy);
    }
    Ok(([chunk[0] * 16 + 8, 50 + dy, chunk[1] * 16], pieces))
}
fn create_and_add(
    pieces: &mut Vec<MineshaftPiece>,
    random: &mut impl RandomSource,
    foot: [i32; 3],
    direction: Direction,
    depth: i32,
) -> Option<usize> {
    let start = &pieces[0];
    if depth > 8
        || (foot[0] - start.data.bounds.min[0]).abs() > 80
        || (foot[2] - start.data.bounds.min[2]).abs() > 80
    {
        return None;
    }
    let mine_type = start.mine_type;
    let selection = random.next_int_bound(100);
    let (bounds, kind, orientation) = if selection >= 80 {
        let top = if random.next_int_bound(4) == 0 { 6 } else { 2 };
        let (min, max) = match direction {
            Direction::South => ([-1, 0, 0], [3, top, 4]),
            Direction::West => ([-4, 0, -1], [0, top, 3]),
            Direction::East => ([0, 0, -1], [4, top, 3]),
            _ => ([-1, 0, -4], [3, top, 0]),
        };
        let b = BoundingBox::new(min, max).moved(foot);
        if collides(pieces, b) {
            return None;
        }
        (
            b,
            MineshaftPieceKind::Crossing {
                direction,
                two_floored: top > 2,
            },
            None,
        )
    } else if selection >= 70 {
        let (min, max) = match direction {
            Direction::South => ([0, -5, 0], [2, 2, 8]),
            Direction::West => ([-8, -5, 0], [0, 2, 2]),
            Direction::East => ([0, -5, 0], [8, 2, 2]),
            _ => ([0, -5, -8], [2, 2, 0]),
        };
        let b = BoundingBox::new(min, max).moved(foot);
        if collides(pieces, b) {
            return None;
        }
        (b, MineshaftPieceKind::Stairs, Some(direction))
    } else {
        let mut chosen = None;
        for sections in (1..=(random.next_int_bound(3) + 2)).rev() {
            let length = sections * 5;
            let (min, max) = match direction {
                Direction::South => ([0, 0, 0], [2, 2, length - 1]),
                Direction::West => ([-(length - 1), 0, 0], [0, 2, 2]),
                Direction::East => ([0, 0, 0], [length - 1, 2, 2]),
                _ => ([0, 0, -(length - 1)], [2, 2, 0]),
            };
            let b = BoundingBox::new(min, max).moved(foot);
            if !collides(pieces, b) {
                chosen = Some((b, sections));
                break;
            }
        }
        let (b, sections) = chosen?;
        let rails = random.next_int_bound(3) == 0;
        let spiders = !rails && random.next_int_bound(23) == 0;
        (
            b,
            MineshaftPieceKind::Corridor {
                rails,
                spiders,
                placed_spider: false,
                sections,
            },
            Some(direction),
        )
    };
    let index = pieces.len();
    pieces.push(MineshaftPiece {
        data: StructurePieceData::new(bounds, depth + 1, orientation),
        mine_type,
        kind,
    });
    add_children(index, pieces, random);
    Some(index)
}
fn collides(pieces: &[MineshaftPiece], bounds: BoundingBox) -> bool {
    pieces.iter().any(|p| p.data.bounds.intersects(bounds))
}
fn add_children(index: usize, pieces: &mut Vec<MineshaftPiece>, random: &mut impl RandomSource) {
    let piece = pieces[index].clone();
    let b = piece.data.bounds;
    let depth = piece.data.gen_depth;
    match piece.kind {
        MineshaftPieceKind::Room { .. } => {
            let height = (b.max[1] - b.min[1] + 1 - 4).max(1);
            for direction in [
                Direction::North,
                Direction::South,
                Direction::West,
                Direction::East,
            ] {
                let axis = if matches!(direction, Direction::North | Direction::South) {
                    0
                } else {
                    2
                };
                let span = b.max[axis] - b.min[axis] + 1;
                let mut pos = 0;
                while pos < span {
                    pos += random.next_int_bound(span);
                    if pos + 3 > span {
                        break;
                    }
                    let y = b.min[1] + random.next_int_bound(height) + 1;
                    let foot = match direction {
                        Direction::North => [b.min[0] + pos, y, b.min[2] - 1],
                        Direction::South => [b.min[0] + pos, y, b.max[2] + 1],
                        Direction::West => [b.min[0] - 1, y, b.min[2] + pos],
                        Direction::East => [b.max[0] + 1, y, b.min[2] + pos],
                        _ => unreachable!(),
                    };
                    if let Some(child) = create_and_add(pieces, random, foot, direction, depth) {
                        let c = pieces[child].data.bounds;
                        let entrance = match direction {
                            Direction::North => BoundingBox::new(
                                [c.min[0], c.min[1], b.min[2]],
                                [c.max[0], c.max[1], b.min[2] + 1],
                            ),
                            Direction::South => BoundingBox::new(
                                [c.min[0], c.min[1], b.max[2] - 1],
                                [c.max[0], c.max[1], b.max[2]],
                            ),
                            Direction::West => BoundingBox::new(
                                [b.min[0], c.min[1], c.min[2]],
                                [b.min[0] + 1, c.max[1], c.max[2]],
                            ),
                            Direction::East => BoundingBox::new(
                                [b.max[0] - 1, c.min[1], c.min[2]],
                                [b.max[0], c.max[1], c.max[2]],
                            ),
                            _ => unreachable!(),
                        };
                        if let MineshaftPieceKind::Room { entrances } = &mut pieces[index].kind {
                            entrances.push(entrance);
                        }
                    }
                    pos += 4;
                }
            }
        }
        MineshaftPieceKind::Corridor { .. } => {
            let selection = random.next_int_bound(4);
            let direction = piece.data.orientation;
            if let Some(d) = direction {
                let y = b.min[1] - 1 + random.next_int_bound(3);
                let (foot, next) = match (d, selection) {
                    (Direction::North, 0 | 1) => ([b.min[0], y, b.min[2] - 1], d),
                    (Direction::North, 2) => ([b.min[0] - 1, y, b.min[2]], Direction::West),
                    (Direction::North, _) => ([b.max[0] + 1, y, b.min[2]], Direction::East),
                    (Direction::South, 0 | 1) => ([b.min[0], y, b.max[2] + 1], d),
                    (Direction::South, 2) => ([b.min[0] - 1, y, b.max[2] - 3], Direction::West),
                    (Direction::South, _) => ([b.max[0] + 1, y, b.max[2] - 3], Direction::East),
                    (Direction::West, 0 | 1) => ([b.min[0] - 1, y, b.min[2]], d),
                    (Direction::West, 2) => ([b.min[0], y, b.min[2] - 1], Direction::North),
                    (Direction::West, _) => ([b.min[0], y, b.max[2] + 1], Direction::South),
                    (Direction::East, 0 | 1) => ([b.max[0] + 1, y, b.min[2]], d),
                    (Direction::East, 2) => ([b.max[0] - 3, y, b.min[2] - 1], Direction::North),
                    (Direction::East, _) => ([b.max[0] - 3, y, b.max[2] + 1], Direction::South),
                    _ => unreachable!("horizontal corridor"),
                };
                let _ = create_and_add(pieces, random, foot, next, depth);
            }
            if depth < 8 {
                let z_axis = matches!(direction, Some(Direction::North | Direction::South));
                let axis = if z_axis { 2 } else { 0 };
                let mut pos = b.min[axis] + 3;
                while pos + 3 <= b.max[axis] {
                    let selection = random.next_int_bound(5);
                    let branch = match (z_axis, selection) {
                        (true, 0) => Some(([b.min[0] - 1, b.min[1], pos], Direction::West)),
                        (true, 1) => Some(([b.max[0] + 1, b.min[1], pos], Direction::East)),
                        (false, 0) => Some(([pos, b.min[1], b.min[2] - 1], Direction::North)),
                        (false, 1) => Some(([pos, b.min[1], b.max[2] + 1], Direction::South)),
                        _ => None,
                    };
                    if let Some((foot, d)) = branch {
                        let _ = create_and_add(pieces, random, foot, d, depth + 1);
                    }
                    pos += 5;
                }
            }
        }
        MineshaftPieceKind::Crossing {
            direction,
            two_floored,
        } => {
            let north = ([b.min[0] + 1, b.min[1], b.min[2] - 1], Direction::North);
            let south = ([b.min[0] + 1, b.min[1], b.max[2] + 1], Direction::South);
            let west = ([b.min[0] - 1, b.min[1], b.min[2] + 1], Direction::West);
            let east = ([b.max[0] + 1, b.min[1], b.min[2] + 1], Direction::East);
            let branches = match direction {
                Direction::South => [south, west, east],
                Direction::West => [north, south, west],
                Direction::East => [north, south, east],
                _ => [north, west, east],
            };
            for (foot, d) in branches {
                let _ = create_and_add(pieces, random, foot, d, depth);
            }
            if two_floored {
                for (mut foot, d) in [north, west, east, south] {
                    if random.next_boolean() {
                        foot[1] += 4;
                        let _ = create_and_add(pieces, random, foot, d, depth);
                    }
                }
            }
        }
        MineshaftPieceKind::Stairs => {
            if let Some(d) = piece.data.orientation {
                let foot = match d {
                    Direction::North => [b.min[0], b.min[1], b.min[2] - 1],
                    Direction::South => [b.min[0], b.min[1], b.max[2] + 1],
                    Direction::West => [b.min[0] - 1, b.min[1], b.min[2]],
                    Direction::East => [b.max[0] + 1, b.min[1], b.min[2]],
                    _ => unreachable!("horizontal stairs"),
                };
                let _ = create_and_add(pieces, random, foot, d, depth);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generator::flat::{FlatLevelGeneratorSettings, FlatLevelSource},
        structure::processor::tests::generator,
    };
    use ferraloom_util::random::{Algorithm, WorldgenRandom};
    use std::io::Read;
    #[test]
    fn mineshaft_recursion_collisions_entrances_vertical_adjustment_and_rng_match_vanilla() {
        let (_, noise) = generator();
        let flat = FlatLevelSource::new(
            noise.data.clone(),
            FlatLevelGeneratorSettings::default_settings(),
        )
        .unwrap();
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../testdata/mineshaft_pieces.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let chunk = std::array::from_fn(|i| case["chunk"][i].as_i64().unwrap() as i32);
            let mine_type = if case["id"] == "minecraft:mineshaft_mesa" {
                MineshaftType::Mesa
            } else {
                MineshaftType::Normal
            };
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(0));
            random.set_large_feature_seed(seed, chunk[0], chunk[1]);
            let (position, pieces) =
                generate(mine_type, &mut random, &flat, chunk, -64, 384).unwrap();
            assert_eq!(
                position,
                std::array::from_fn(|i| case["position"][i].as_i64().unwrap() as i32),
                "{mine_type:?} {seed} {chunk:?}"
            );
            let actual = Tag::List(ListTag(
                pieces.iter().map(|p| Tag::Compound(p.to_nbt())).collect(),
            ));
            let expected = ferraloom_nbt::parse(case["nbt"].as_str().unwrap()).unwrap();
            assert_eq!(
                actual.as_list().unwrap().len(),
                expected.as_list().unwrap().len(),
                "{mine_type:?} {seed} {chunk:?}"
            );
            for (i, (actual, expected)) in actual
                .as_list()
                .unwrap()
                .iter()
                .zip(expected.as_list().unwrap().iter())
                .enumerate()
            {
                assert_eq!(actual, expected, "{mine_type:?} {seed} {chunk:?} piece={i}");
            }
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "{mine_type:?} {seed} {chunk:?}"
            );
            let loaded = pieces
                .iter()
                .map(|p| {
                    let tag = p.to_nbt();
                    let id = tag.get_string("id").unwrap().to_string_lossy();
                    Tag::Compound(MineshaftPiece::from_nbt(&id, &tag).unwrap().to_nbt())
                })
                .collect::<Vec<_>>();
            assert_eq!(
                Tag::List(ListTag(loaded)),
                ferraloom_nbt::parse(case["loaded"].as_str().unwrap()).unwrap(),
                "{mine_type:?} {seed} {chunk:?}"
            );
        }
    }
}
