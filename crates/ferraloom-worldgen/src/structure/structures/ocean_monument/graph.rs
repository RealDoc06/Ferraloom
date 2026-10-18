use Direction::{East, North, South, Up};
use ferraloom_util::{Direction, random::RandomSource};

/// Port of OceanMonumentPieces.RoomDefinition. Connections use stable indices
/// into one start's arena, preserving recursive source searches without sharing
/// mutable room objects across generation jobs.
#[derive(Clone, Debug)]
pub struct RoomDefinition {
    pub index: i32,
    pub connections: [Option<usize>; 6],
    pub openings: [bool; 6],
    pub claimed: bool,
    pub source: bool,
    pub scan_index: i32,
}
impl RoomDefinition {
    fn new(index: i32) -> Self {
        Self {
            index,
            connections: [None; 6],
            openings: [false; 6],
            claimed: false,
            source: false,
            scan_index: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct RoomGraph {
    pub rooms: [Option<RoomDefinition>; 78],
    pub order: Vec<usize>,
    pub core: usize,
}
impl RoomGraph {
    pub fn room(&self, id: usize) -> &RoomDefinition {
        self.rooms[id].as_ref().expect("connected monument room")
    }
    pub fn room_mut(&mut self, id: usize) -> &mut RoomDefinition {
        self.rooms[id].as_mut().expect("connected monument room")
    }
    pub fn neighbor(&self, id: usize, direction: Direction) -> usize {
        self.room(id).connections[direction as usize].expect("monument room connection")
    }
    fn connect(&mut self, id: usize, direction: Direction, neighbor: usize) {
        self.room_mut(id).connections[direction as usize] = Some(neighbor);
        self.room_mut(neighbor).connections[direction.opposite() as usize] = Some(id);
    }
    fn find_source(&mut self, id: usize, scan: i32) -> bool {
        if self.room(id).source {
            return true;
        }
        self.room_mut(id).scan_index = scan;
        for direction in 0..6 {
            let room = self.room(id);
            if room.openings[direction]
                && let Some(neighbor) = room.connections[direction]
                && self.room(neighbor).scan_index != scan
                && self.find_source(neighbor, scan)
            {
                return true;
            }
        }
        false
    }
    pub fn new(random: &mut impl RandomSource) -> Self {
        let mut graph = Self {
            rooms: std::array::from_fn(|_| None),
            order: Vec::new(),
            core: 0,
        };
        for y in 0..2 {
            for x in 0..5 {
                for z in 0..4 {
                    let id = y * 25 + z * 5 + x;
                    graph.rooms[id] = Some(RoomDefinition::new(id as i32));
                }
            }
        }
        for x in 1..4 {
            for z in 0..2 {
                let id = 50 + z * 5 + x;
                graph.rooms[id] = Some(RoomDefinition::new(id as i32));
            }
        }
        for x in 0..5 {
            for z in 0..5 {
                for y in 0..3 {
                    let id = y * 25 + z * 5 + x;
                    if graph.rooms[id].is_none() {
                        continue;
                    }
                    for direction in Direction::VALUES {
                        let p = direction.step();
                        let (nx, ny, nz) = (x as i32 + p.x, y as i32 + p.y, z as i32 + p.z);
                        if (0..5).contains(&nx) && (0..3).contains(&ny) && (0..5).contains(&nz) {
                            let neighbor = (ny * 25 + nz * 5 + nx) as usize;
                            if graph.rooms[neighbor].is_some() {
                                graph.connect(
                                    id,
                                    if nz == z as i32 {
                                        direction
                                    } else {
                                        direction.opposite()
                                    },
                                    neighbor,
                                );
                            }
                        }
                    }
                }
            }
        }
        graph.rooms[75] = Some(RoomDefinition::new(1003));
        graph.rooms[76] = Some(RoomDefinition::new(1001));
        graph.rooms[77] = Some(RoomDefinition::new(1002));
        graph.connect(52, Up, 75);
        graph.connect(25, South, 76);
        graph.connect(29, South, 77);
        for id in 75..78 {
            graph.room_mut(id).claimed = true;
        }
        graph.room_mut(2).source = true;
        graph.core = 10 + random.next_int_bound(4) as usize;
        for dx in 0..2 {
            for dz in 0..2 {
                for dy in 0..2 {
                    graph.room_mut(graph.core + dx + 5 * dz + 25 * dy).claimed = true;
                }
            }
        }
        for id in 0..75 {
            if let Some(room) = &mut graph.rooms[id] {
                room.openings = room.connections.map(|n| n.is_some());
                graph.order.push(id);
            }
        }
        let roof = graph.room_mut(75);
        roof.openings = roof.connections.map(|n| n.is_some());
        for i in (2..=graph.order.len()).rev() {
            let j = random.next_int_bound(i as i32) as usize;
            graph.order.swap(i - 1, j);
        }
        let mut scan = 1;
        for index in 0..graph.order.len() {
            let id = graph.order[index];
            let mut closed = 0;
            for _ in 0..5 {
                if closed == 2 {
                    break;
                }
                let face = random.next_int_bound(6) as usize;
                if !graph.room(id).openings[face] {
                    continue;
                }
                let opposite = Direction::from_3d_data_value(face as i32).opposite() as usize;
                let neighbor = graph.room(id).connections[face].expect("open connection");
                graph.room_mut(id).openings[face] = false;
                graph.room_mut(neighbor).openings[opposite] = false;
                let a = graph.find_source(id, scan);
                scan += 1;
                let b = if a {
                    let b = graph.find_source(neighbor, scan);
                    scan += 1;
                    b
                } else {
                    false
                };
                if a && b {
                    closed += 1;
                } else {
                    graph.room_mut(id).openings[face] = true;
                    graph.room_mut(neighbor).openings[opposite] = true;
                }
            }
        }
        graph.order.extend([75, 76, 77]);
        graph
    }
    fn unclaimed_open_neighbor(&self, id: usize, direction: Direction) -> bool {
        self.room(id).openings[direction as usize]
            && !self.room(self.neighbor(id, direction)).claimed
    }
    /// First matching fitter wins. Claim all constituent rooms before appending
    /// the piece; only the simple-room fitter consumes an extra random draw.
    pub fn fit(&mut self, id: usize, random: &mut impl RandomSource) -> RoomKind {
        let east = self.unclaimed_open_neighbor(id, East);
        let north = self.unclaimed_open_neighbor(id, North);
        let up = self.unclaimed_open_neighbor(id, Up);
        let kind = if east && up && self.unclaimed_open_neighbor(self.neighbor(id, East), Up) {
            RoomKind::DoubleXY
        } else if north && up && self.unclaimed_open_neighbor(self.neighbor(id, North), Up) {
            RoomKind::DoubleYZ
        } else if north {
            RoomKind::DoubleZ
        } else if east {
            RoomKind::DoubleX
        } else if up {
            RoomKind::DoubleY
        } else if [Direction::West, East, North, South, Up]
            .into_iter()
            .all(|d| !self.room(id).openings[d as usize])
        {
            RoomKind::SimpleTop
        } else {
            RoomKind::Simple {
                design: random.next_int_bound(3),
            }
        };
        let [w, h, d] = kind.size();
        for dy in 0..h {
            for dz in 0..d {
                for dx in 0..w {
                    self.room_mut(id + dx + 25 * dy + 5 * dz).claimed = true;
                }
            }
        }
        kind
    }
}
/// Typed monument child-room variants. Room dimensions count 8x4x8 cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind {
    Entry,
    Core,
    DoubleXY,
    DoubleYZ,
    DoubleZ,
    DoubleX,
    DoubleY,
    SimpleTop,
    Simple { design: i32 },
    Wing { design: i32 },
    Penthouse,
}
impl RoomKind {
    pub fn size(self) -> [usize; 3] {
        match self {
            Self::Core => [2, 2, 2],
            Self::DoubleXY => [2, 2, 1],
            Self::DoubleYZ => [1, 2, 2],
            Self::DoubleX => [2, 1, 1],
            Self::DoubleY => [1, 2, 1],
            Self::DoubleZ => [1, 1, 2],
            _ => [1; 3],
        }
    }
    pub fn piece_id(self) -> &'static str {
        match self {
            Self::Entry => "minecraft:omentry",
            Self::Core => "minecraft:omcr",
            Self::DoubleXY => "minecraft:omdxyr",
            Self::DoubleYZ => "minecraft:omdyzr",
            Self::DoubleZ => "minecraft:omdzr",
            Self::DoubleX => "minecraft:omdxr",
            Self::DoubleY => "minecraft:omdyr",
            Self::SimpleTop => "minecraft:omsimplet",
            Self::Simple { .. } => "minecraft:omsimple",
            Self::Wing { .. } => "minecraft:omwr",
            Self::Penthouse => "minecraft:ompenthouse",
        }
    }
}
