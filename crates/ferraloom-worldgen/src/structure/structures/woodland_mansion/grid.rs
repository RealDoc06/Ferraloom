use ferraloom_util::{Direction, random::RandomSource};

pub const ROOM_1X1: i32 = 65536;
pub const ROOM_1X2: i32 = 131072;
pub const ROOM_2X2: i32 = 262144;
pub const ORIGIN: i32 = 1048576;
pub const DOOR: i32 = 2097152;
pub const STAIRS: i32 = 4194304;
pub const CORRIDOR_FLAG: i32 = 8388608;
pub const TYPE_MASK: i32 = 983040;
pub const ID_MASK: i32 = 65535;
const HORIZONTAL: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];

/// Port of WoodlandMansionPieces.SimpleGrid. Out-of-bounds cells are blocked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimpleGrid(pub [i32; 121]);
impl Default for SimpleGrid {
    fn default() -> Self {
        Self([0; 121])
    }
}
impl SimpleGrid {
    pub fn get(&self, x: i32, y: i32) -> i32 {
        if (0..11).contains(&x) && (0..11).contains(&y) {
            self.0[(y * 11 + x) as usize]
        } else {
            5
        }
    }
    fn set(&mut self, x: i32, y: i32, value: i32) {
        if (0..11).contains(&x) && (0..11).contains(&y) {
            self.0[(y * 11 + x) as usize] = value;
        }
    }
    fn rectangle(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, value: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.set(x, y, value);
            }
        }
    }
    fn set_if(&mut self, x: i32, y: i32, old: i32, value: i32) {
        if self.get(x, y) == old {
            self.set(x, y, value);
        }
    }
    pub fn is_house(&self, x: i32, y: i32) -> bool {
        matches!(self.get(x, y), 1..=4)
    }
    fn edges_to_corridor(&self, x: i32, y: i32) -> bool {
        [(x - 1, y), (x + 1, y), (x, y + 1), (x, y - 1)]
            .into_iter()
            .any(|(x, y)| self.get(x, y) == 1)
    }
}

/// Port of WoodlandMansionPieces.MansionGrid. All mutable grid and random state
/// belongs to one structure start, including the third-floor staircase fallback.
#[derive(Clone, Debug)]
pub struct MansionGrid {
    pub base: SimpleGrid,
    pub third: SimpleGrid,
    pub rooms: [SimpleGrid; 3],
}
impl MansionGrid {
    pub fn new(random: &mut impl RandomSource) -> Self {
        let mut base = SimpleGrid::default();
        base.rectangle(7, 4, 8, 5, 3);
        base.rectangle(6, 4, 6, 5, 2);
        base.rectangle(9, 2, 10, 7, 5);
        base.rectangle(8, 2, 8, 3, 1);
        base.rectangle(8, 6, 8, 7, 1);
        base.set(6, 3, 1);
        base.set(6, 6, 1);
        base.rectangle(0, 0, 11, 1, 5);
        base.rectangle(0, 9, 11, 11, 5);
        for (x, y, depth) in [(7, 2, 6), (7, 7, 6), (5, 3, 3), (5, 6, 3)] {
            corridor(&mut base, random, x, y, Direction::West, depth);
        }
        while clean_edges(&mut base) {}
        let mut rooms = std::array::from_fn(|_| SimpleGrid::default());
        identify_rooms(&base, &mut rooms[0], random);
        identify_rooms(&base, &mut rooms[1], random);
        rooms[0].rectangle(8, 4, 8, 5, CORRIDOR_FLAG);
        rooms[1].rectangle(8, 4, 8, 5, CORRIDOR_FLAG);
        let mut result = Self {
            base,
            third: SimpleGrid::default(),
            rooms,
        };
        result.setup_third_floor(random);
        identify_rooms(&result.third, &mut result.rooms[2], random);
        result
    }
    pub fn is_room_id(&self, x: i32, y: i32, floor: usize, id: i32) -> bool {
        self.rooms[floor].get(x, y) & ID_MASK == id
    }
    pub fn room_direction(&self, x: i32, y: i32, floor: usize, id: i32) -> Option<Direction> {
        HORIZONTAL.into_iter().find(|d| {
            let p = d.step();
            self.is_room_id(x + p.x, y + p.z, floor, id)
        })
    }
    fn setup_third_floor(&mut self, random: &mut impl RandomSource) {
        let mut candidates = Vec::new();
        for y in 0..11 {
            for x in 0..11 {
                let data = self.rooms[1].get(x, y);
                if data & TYPE_MASK == ROOM_1X2 && data & DOOR == DOOR {
                    candidates.push((x, y));
                }
            }
        }
        if candidates.is_empty() {
            self.third.0.fill(5);
            return;
        }
        let (x, y) = candidates[random.next_int_bound(candidates.len() as i32) as usize];
        let data = self.rooms[1].get(x, y);
        self.rooms[1].set(x, y, data | STAIRS);
        let direction = self
            .room_direction(x, y, 1, data & ID_MASK)
            .expect("two-cell mansion room");
        let step = direction.step();
        let end = [x + step.x, y + step.z];
        for gy in 0..11 {
            for gx in 0..11 {
                if !self.base.is_house(gx, gy) {
                    self.third.set(gx, gy, 5);
                } else if [gx, gy] == [x, y] {
                    self.third.set(gx, gy, 3);
                } else if [gx, gy] == end {
                    self.third.set(gx, gy, 3);
                    self.rooms[2].set(gx, gy, CORRIDOR_FLAG);
                }
            }
        }
        let exits = HORIZONTAL
            .into_iter()
            .filter(|d| {
                let p = d.step();
                self.third.get(end[0] + p.x, end[1] + p.z) == 0
            })
            .collect::<Vec<_>>();
        if exits.is_empty() {
            self.third.0.fill(5);
            self.rooms[1].set(x, y, data);
        } else {
            let direction = exits[random.next_int_bound(exits.len() as i32) as usize];
            let p = direction.step();
            corridor(
                &mut self.third,
                random,
                end[0] + p.x,
                end[1] + p.z,
                direction,
                4,
            );
            while clean_edges(&mut self.third) {}
        }
    }
}
fn corridor(
    grid: &mut SimpleGrid,
    random: &mut impl RandomSource,
    x: i32,
    y: i32,
    heading: Direction,
    depth: i32,
) {
    if depth <= 0 {
        return;
    }
    let h = heading.step();
    grid.set(x, y, 1);
    grid.set_if(x + h.x, y + h.z, 0, 1);
    for _ in 0..8 {
        let direction = Direction::from_2d_data_value(random.next_int_bound(4));
        if direction != heading.opposite()
            && (direction != Direction::East || !random.next_boolean())
        {
            let p = direction.step();
            let nx = x + h.x;
            let ny = y + h.z;
            if grid.get(nx + p.x, ny + p.z) == 0 && grid.get(nx + 2 * p.x, ny + 2 * p.z) == 0 {
                corridor(grid, random, nx + p.x, ny + p.z, direction, depth - 1);
                break;
            }
        }
    }
    let cw = heading.clockwise().step();
    let ccw = heading.counter_clockwise().step();
    for (dx, dy) in [
        (cw.x, cw.z),
        (ccw.x, ccw.z),
        (h.x + cw.x, h.z + cw.z),
        (h.x + ccw.x, h.z + ccw.z),
        (2 * h.x, 2 * h.z),
        (2 * cw.x, 2 * cw.z),
        (2 * ccw.x, 2 * ccw.z),
    ] {
        grid.set_if(x + dx, y + dy, 0, 2);
    }
}
fn clean_edges(grid: &mut SimpleGrid) -> bool {
    let mut changed = false;
    for y in 0..11 {
        for x in 0..11 {
            if grid.get(x, y) != 0 {
                continue;
            }
            let count = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
                .into_iter()
                .filter(|&(x, y)| grid.is_house(x, y))
                .count();
            let fill = count >= 3
                || count == 2
                    && [
                        (x + 1, y + 1),
                        (x - 1, y + 1),
                        (x + 1, y - 1),
                        (x - 1, y - 1),
                    ]
                    .into_iter()
                    .filter(|&(x, y)| grid.is_house(x, y))
                    .count()
                        <= 1;
            if fill {
                grid.set(x, y, 2);
                changed = true;
            }
        }
    }
    changed
}
fn identify_rooms(source: &SimpleGrid, rooms: &mut SimpleGrid, random: &mut impl RandomSource) {
    let mut positions = Vec::new();
    for y in 0..11 {
        for x in 0..11 {
            if source.get(x, y) == 2 {
                positions.push((x, y));
            }
        }
    }
    for i in (2..=positions.len()).rev() {
        let j = random.next_int_bound(i as i32) as usize;
        positions.swap(i - 1, j);
    }
    let mut id = 10;
    for (x, y) in positions {
        if rooms.get(x, y) != 0 {
            continue;
        }
        let (mut x0, mut x1, mut y0, mut y1) = (x, x, y, y);
        let mut kind = ROOM_1X1;
        let free = |x, y| rooms.get(x, y) == 0 && source.get(x, y) == 2;
        if free(x + 1, y) && free(x, y + 1) && free(x + 1, y + 1) {
            x1 += 1;
            y1 += 1;
            kind = ROOM_2X2;
        } else if free(x - 1, y) && free(x, y + 1) && free(x - 1, y + 1) {
            x0 -= 1;
            y1 += 1;
            kind = ROOM_2X2;
        } else if free(x - 1, y) && free(x, y - 1) && free(x - 1, y - 1) {
            x0 -= 1;
            y0 -= 1;
            kind = ROOM_2X2;
        } else if free(x + 1, y) {
            x1 += 1;
            kind = ROOM_1X2;
        } else if free(x, y + 1) {
            y1 += 1;
            kind = ROOM_1X2;
        } else if free(x - 1, y) {
            x0 -= 1;
            kind = ROOM_1X2;
        } else if free(x, y - 1) {
            y0 -= 1;
            kind = ROOM_1X2;
        }
        let mut door_x = if random.next_boolean() { x0 } else { x1 };
        let mut door_y = if random.next_boolean() { y0 } else { y1 };
        let mut flag = DOOR;
        if !source.edges_to_corridor(door_x, door_y) {
            door_x = if door_x == x0 { x1 } else { x0 };
            door_y = if door_y == y0 { y1 } else { y0 };
            if !source.edges_to_corridor(door_x, door_y) {
                door_y = if door_y == y0 { y1 } else { y0 };
                if !source.edges_to_corridor(door_x, door_y) {
                    door_x = if door_x == x0 { x1 } else { x0 };
                    door_y = if door_y == y0 { y1 } else { y0 };
                    if !source.edges_to_corridor(door_x, door_y) {
                        flag = 0;
                        door_x = x0;
                        door_y = y0;
                    }
                }
            }
        }
        for ry in y0..=y1 {
            for rx in x0..=x1 {
                rooms.set(
                    rx,
                    ry,
                    kind | id
                        | if rx == door_x && ry == door_y {
                            ORIGIN | flag
                        } else {
                            0
                        },
                );
            }
        }
        id += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_util::random::LegacyRandomSource;
    use std::io::Read;
    #[test]
    fn mansion_corridors_room_partition_staircase_and_rng_match_vanilla() {
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../../testdata/mansion_grids.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let mut random = LegacyRandomSource::new(seed);
            let grid = MansionGrid::new(&mut random);
            for (name, actual) in [
                ("base", &grid.base),
                ("third", &grid.third),
                ("rooms0", &grid.rooms[0]),
                ("rooms1", &grid.rooms[1]),
                ("rooms2", &grid.rooms[2]),
            ] {
                let expected = case[name]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_i64().unwrap() as i32)
                    .collect::<Vec<_>>();
                assert_eq!(actual.0.as_slice(), expected, "seed={seed} {name}");
            }
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "seed={seed}"
            );
        }
    }
}
