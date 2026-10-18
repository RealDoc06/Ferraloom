//! Ocean monument room graph, room fitting and geometry.
pub mod graph;
use crate::structure::{BoundingBox, piece::StructurePieceData};
use ferraloom_util::{Direction, random::RandomSource};
use graph::{RoomGraph, RoomKind};

#[derive(Clone, Debug)]
pub struct MonumentRoom {
    pub data: StructurePieceData,
    pub room: Option<usize>,
    pub kind: RoomKind,
}
/// Port of OceanMonumentPieces.MonumentBuilding's start-local room graph and
/// ordered child assembly. Child rooms are regenerated from seed after loading.
#[derive(Clone, Debug)]
pub struct MonumentBuilding {
    pub data: StructurePieceData,
    pub graph: RoomGraph,
    pub children: Vec<MonumentRoom>,
}
impl MonumentBuilding {
    pub fn new(
        random: &mut impl RandomSource,
        west: i32,
        north: i32,
        direction: Direction,
    ) -> Self {
        let data = StructurePieceData::new(
            BoundingBox::new([west, 39, north], [west + 57, 61, north + 57]),
            0,
            Some(direction),
        );
        let mut graph = RoomGraph::new(random);
        graph.room_mut(2).claimed = true;
        let mut children = vec![
            room_piece(direction, 2, RoomKind::Entry),
            room_piece(direction, graph.core, RoomKind::Core),
        ];
        for i in 0..graph.order.len() {
            let id = graph.order[i];
            if graph.room(id).claimed || graph.room(id).index >= 75 {
                continue;
            }
            let kind = graph.fit(id, random);
            children.push(room_piece(direction, id, kind));
        }
        let offset = data.world_position([9, 0, 22]);
        for child in &mut children {
            child.data.bounds = child.data.bounds.moved(offset);
        }
        let wing = random.next_int();
        for (kind, a, b) in [
            (RoomKind::Wing { design: wing & 1 }, [1, 1, 1], [23, 8, 21]),
            (
                RoomKind::Wing {
                    design: wing.wrapping_add(1) & 1,
                },
                [34, 1, 1],
                [56, 8, 21],
            ),
            (RoomKind::Penthouse, [22, 13, 22], [35, 17, 35]),
        ] {
            children.push(MonumentRoom {
                data: StructurePieceData::new(
                    BoundingBox::new(data.world_position(a), data.world_position(b)),
                    1,
                    Some(direction),
                ),
                room: None,
                kind,
            });
        }
        Self {
            data,
            graph,
            children,
        }
    }
}
fn room_piece(direction: Direction, id: usize, kind: RoomKind) -> MonumentRoom {
    let [w, h, d] = kind.size().map(|v| v as i32);
    let (x, y, z) = ((id % 5) as i32, (id / 25) as i32, ((id / 5) % 5) as i32);
    let size = if matches!(direction, Direction::North | Direction::South) {
        [w * 8, h * 4, d * 8]
    } else {
        [d * 8, h * 4, w * 8]
    };
    let offset = match direction {
        Direction::North => [x * 8, y * 4, -(z + d) * 8 + 1],
        Direction::South => [x * 8, y * 4, z * 8],
        Direction::West => [-(z + d) * 8 + 1, y * 4, x * 8],
        _ => [z * 8, y * 4, x * 8],
    };
    MonumentRoom {
        data: StructurePieceData::new(
            BoundingBox::new([0; 3], size.map(|v| v - 1)).moved(offset),
            1,
            Some(direction),
        ),
        room: Some(id),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_util::random::LegacyRandomSource;
    use std::io::Read;
    #[test]
    fn monument_room_graph_source_scans_fit_order_child_bounds_and_rng_match_vanilla() {
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../testdata/monument_pieces.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let direction =
                Direction::from_2d_data_value(case["direction"].as_i64().unwrap() as i32);
            let label = format!("seed={seed} direction={direction:?}");
            let mut random = LegacyRandomSource::new(seed);
            let piece = MonumentBuilding::new(&mut random, -46, 8, direction);
            assert_eq!(
                piece.data.to_nbt(&"minecraft:omb".parse().unwrap()),
                ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap(),
                "{label}"
            );
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "{label}"
            );
            assert_eq!(
                piece.graph.core,
                case["core"].as_u64().unwrap() as usize,
                "{label}"
            );
            assert_eq!(
                piece.graph.rooms.iter().flatten().count(),
                case["rooms"].as_array().unwrap().len(),
                "{label}"
            );
            for expected in case["rooms"].as_array().unwrap() {
                let index = expected["index"].as_i64().unwrap() as i32;
                let room = piece
                    .graph
                    .rooms
                    .iter()
                    .flatten()
                    .find(|r| r.index == index)
                    .unwrap();
                assert_eq!(
                    room.claimed,
                    expected["claimed"].as_bool().unwrap(),
                    "{label} room={index}"
                );
                assert_eq!(
                    room.source,
                    expected["source"].as_bool().unwrap(),
                    "{label} room={index}"
                );
                assert_eq!(
                    room.scan_index,
                    expected["scan"].as_i64().unwrap() as i32,
                    "{label} room={index}"
                );
                for face in 0..6 {
                    assert_eq!(
                        room.openings[face],
                        expected["openings"][face].as_bool().unwrap(),
                        "{label} room={index} face={face}"
                    );
                    assert_eq!(
                        room.connections[face].map_or(-1, |id| piece.graph.room(id).index),
                        expected["connections"][face].as_i64().unwrap() as i32,
                        "{label} room={index} face={face}"
                    );
                }
            }
            let children = case["children"].as_array().unwrap();
            assert_eq!(piece.children.len(), children.len(), "{label}");
            for (i, (child, expected)) in piece.children.iter().zip(children).enumerate() {
                assert_eq!(
                    child.data.to_nbt(&child.kind.piece_id().parse().unwrap()),
                    ferraloom_nbt::parse_compound(expected["nbt"].as_str().unwrap()).unwrap(),
                    "{label} child={i}"
                );
                assert_eq!(
                    child.room.map_or(-1, |id| piece.graph.room(id).index),
                    expected["room"].as_i64().unwrap() as i32,
                    "{label} child={i}"
                );
                if let Some(design) = expected.get("design") {
                    assert!(
                        matches!(child.kind,RoomKind::Simple{design:d}|RoomKind::Wing{design:d} if d==design.as_i64().unwrap() as i32),
                        "{label} child={i}"
                    );
                }
            }
        }
    }
}
