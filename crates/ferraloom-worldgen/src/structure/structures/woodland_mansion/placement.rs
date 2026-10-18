use super::{WoodlandMansionPiece, grid::*};
use crate::structure::template::{StructureTemplateManager, transform_block};
use Direction::{East, North, South, Up, West};
use Mirror::{FrontBack, LeftRight, None as NoMirror};
use Rotation::{Clockwise90 as R90, Clockwise180 as R180, CounterClockwise90 as R270, None as R0};
use ferraloom_util::{Direction, Mirror, Rotation, random::RandomSource};

fn offset(p: [i32; 3], rotation: Rotation, v: [i32; 3]) -> [i32; 3] {
    let delta = transform_block(v, NoMirror, rotation, [0; 3]);
    std::array::from_fn(|i| p[i].wrapping_add(delta[i]))
}
#[derive(Clone, Copy)]
struct Wall {
    position: [i32; 3],
    rotation: Rotation,
    kind: &'static str,
}
struct Placer<'a, R> {
    templates: &'a StructureTemplateManager,
    random: &'a mut R,
    pieces: Vec<WoodlandMansionPiece>,
}
impl<R: RandomSource> Placer<'_, R> {
    fn add(&mut self, name: &str, position: [i32; 3], rotation: Rotation, mirror: Mirror) {
        self.pieces.push(WoodlandMansionPiece::new(
            self.templates,
            name,
            position,
            rotation,
            mirror,
        ));
    }
    fn wall(&mut self, data: &mut Wall) {
        self.add(
            data.kind,
            offset(data.position, data.rotation, [7, 0, 0]),
            data.rotation,
            NoMirror,
        );
        data.position = offset(data.position, data.rotation, [0, 0, 8]);
    }
    fn outer_walls(&mut self, data: &mut Wall, grid: &SimpleGrid, start: [i32; 2], end: [i32; 2]) {
        let [mut x, mut y] = start;
        let mut direction = South;
        loop {
            let h = direction.step();
            let ccw = direction.counter_clockwise().step();
            if !grid.is_house(x + h.x, y + h.z) {
                data.position = offset(data.position, data.rotation, [0, 0, -1]);
                self.add("wall_corner", data.position, data.rotation, NoMirror);
                data.position = offset(data.position, data.rotation, [6, 0, -7]);
                data.rotation = data.rotation.rotated(R90);
                direction = direction.clockwise();
                if [x, y] != end || direction != South {
                    self.wall(data);
                }
            } else if grid.is_house(x + h.x + ccw.x, y + h.z + ccw.z) {
                data.position = offset(data.position, data.rotation, [8, 0, 6]);
                data.rotation = data.rotation.rotated(R270);
                x += h.x;
                y += h.z;
                direction = direction.counter_clockwise();
            } else {
                x += h.x;
                y += h.z;
                if [x, y] != end || direction != South {
                    self.wall(data);
                }
            }
            if [x, y] == end && direction == South {
                break;
            }
        }
    }
    fn roof(
        &mut self,
        origin: [i32; 3],
        rotation: Rotation,
        grid: &SimpleGrid,
        above: Option<&SimpleGrid>,
    ) {
        for pass in 0..3 {
            for y in 0..11 {
                for x in 0..11 {
                    if !grid.is_house(x, y) {
                        continue;
                    }
                    let covered = above.is_some_and(|g| g.is_house(x, y));
                    if (pass == 1) != covered {
                        continue;
                    }
                    let p = offset(origin, rotation, [(x - 8) * 8, 0, 8 + (y - 5) * 8]);
                    let (e, w, n, s) = (
                        !grid.is_house(x + 1, y),
                        !grid.is_house(x - 1, y),
                        !grid.is_house(x, y - 1),
                        !grid.is_house(x, y + 1),
                    );
                    let mut put = |name: &str, v, extra| {
                        self.add(
                            name,
                            offset(p, rotation, v),
                            rotation.rotated(extra),
                            NoMirror,
                        )
                    };
                    match pass {
                        0 => {
                            put("roof", [0, 3, 0], R0);
                            for (condition, v, rot) in [
                                (e, [6, 0, 0], R0),
                                (w, [0, 0, 7], R180),
                                (n, [-1, 0, 0], R270),
                                (s, [6, 0, 6], R90),
                            ] {
                                if condition {
                                    put("roof_front", v, rot);
                                }
                            }
                        }
                        1 => {
                            for (condition, v, rot) in [
                                (e, [7, 0, 0], R0),
                                (w, [-1, 0, 6], R180),
                                (n, [0, 0, -1], R270),
                                (s, [6, 0, 7], R90),
                            ] {
                                if condition {
                                    put("small_wall", v, rot);
                                }
                            }
                            for (condition, v, rot) in [
                                (e && n, [7, 0, -2], R0),
                                (e && s, [8, 0, 7], R90),
                                (w && n, [-2, 0, -1], R270),
                                (w && s, [-1, 0, 8], R180),
                            ] {
                                if condition {
                                    put("small_wall_corner", v, rot);
                                }
                            }
                        }
                        _ => {
                            if e {
                                if s {
                                    put("roof_corner", [6, 0, 6], R0);
                                } else if grid.is_house(x + 1, y + 1) {
                                    put("roof_inner_corner", [6, 0, 5], R0);
                                }
                                if n {
                                    put("roof_corner", [6, 0, 0], R270);
                                } else if grid.is_house(x + 1, y - 1) {
                                    put("roof_inner_corner", [9, 0, -2], R90);
                                }
                            }
                            if w {
                                if s {
                                    put("roof_corner", [0, 0, 6], R90);
                                } else if grid.is_house(x - 1, y + 1) {
                                    put("roof_inner_corner", [-3, 0, 8], R270);
                                }
                                if n {
                                    put("roof_corner", [0, 0, 0], R180);
                                } else if grid.is_house(x - 1, y - 1) {
                                    put("roof_inner_corner", [0, 0, 1], R180);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    fn room_1x1(&mut self, p: [i32; 3], rotation: Rotation, door: Option<Direction>, upper: bool) {
        let mut name = format!(
            "1x1_{}{}",
            if upper { "b" } else { "a" },
            self.random.next_int_bound(5) + 1
        );
        let extra = match door {
            Some(East) => R0,
            Some(North) => R270,
            Some(West) => R180,
            Some(South) => R90,
            _ => {
                name = format!("1x1_as{}", self.random.next_int_bound(4) + 1);
                R0
            }
        };
        // getZeroPositionWithTransform([1,0,0], NONE, extra, 7, 7).
        let v = match extra {
            R0 => [1, 0, 0],
            R90 => [7, 0, 0],
            R180 => [7, 0, 6],
            R270 => [1, 0, 6],
        };
        self.add(
            &name,
            offset(p, rotation, v),
            extra.rotated(rotation),
            NoMirror,
        );
    }
    fn room_1x2(
        &mut self,
        p: [i32; 3],
        rotation: Rotation,
        room: Direction,
        door: Direction,
        upper: bool,
        stairs: bool,
    ) {
        let (v, extra, mirror, kind) = match (door, room) {
            (East, South) => ([1, 0, 0], R0, NoMirror, 0),
            (East, North) => ([1, 0, 6], R0, LeftRight, 0),
            (West, North) => ([7, 0, 6], R180, NoMirror, 0),
            (West, South) => ([7, 0, 0], R0, FrontBack, 0),
            (South, East) => ([1, 0, 0], R90, LeftRight, 0),
            (South, West) => ([7, 0, 0], R90, NoMirror, 0),
            (North, West) => ([7, 0, 6], R90, FrontBack, 0),
            (North, East) => ([1, 0, 6], R270, NoMirror, 0),
            (South, North) => ([1, 0, -8], R0, NoMirror, 1),
            (North, South) => ([7, 0, 14], R180, NoMirror, 1),
            (West, East) => ([15, 0, 0], R90, NoMirror, 1),
            (East, West) => ([-7, 0, 6], R270, NoMirror, 1),
            (Up, East) => ([15, 0, 0], R90, NoMirror, 2),
            (Up, South) => ([1, 0, 0], R0, NoMirror, 2),
            _ => return,
        };
        let name = match (kind, upper, stairs) {
            (0, true, true) => "1x2_c_stairs".into(),
            (1, true, true) => "1x2_d_stairs".into(),
            (0, true, false) => format!("1x2_c{}", self.random.next_int_bound(4) + 1),
            (0, false, _) => format!("1x2_a{}", self.random.next_int_bound(9) + 1),
            (1, true, false) => format!("1x2_d{}", self.random.next_int_bound(5) + 1),
            (1, false, _) => format!("1x2_b{}", self.random.next_int_bound(5) + 1),
            (_, true, _) => format!("1x2_se{}", self.random.next_int_bound(1) + 1),
            (_, false, _) => format!("1x2_s{}", self.random.next_int_bound(2) + 1),
        };
        self.add(
            &name,
            offset(p, rotation, v),
            rotation.rotated(extra),
            mirror,
        );
    }
    fn room_2x2(
        &mut self,
        p: [i32; 3],
        rotation: Rotation,
        room: Direction,
        door: Direction,
        upper: bool,
    ) {
        let (v, extra, mirror) = match (door, room) {
            (East, South) => ([-7, 0, 0], R0, NoMirror),
            (East, North) => ([-7, 0, 6], R0, LeftRight),
            (North, East) => ([1, 0, 14], R270, NoMirror),
            (North, West) => ([7, 0, 14], R270, LeftRight),
            (South, West) => ([7, 0, -8], R90, NoMirror),
            (South, East) => ([1, 0, -8], R90, LeftRight),
            (West, North) => ([15, 0, 6], R180, NoMirror),
            (West, South) => ([15, 0, 0], R0, FrontBack),
            _ => ([0; 3], R0, NoMirror),
        };
        let name = format!(
            "2x2_{}{}",
            if upper { "b" } else { "a" },
            self.random.next_int_bound(if upper { 5 } else { 4 }) + 1
        );
        self.add(
            &name,
            offset(p, rotation, v),
            rotation.rotated(extra),
            mirror,
        );
    }
    fn create(&mut self, origin: [i32; 3], rotation: Rotation, mansion: &MansionGrid) {
        let mut data = Wall {
            position: origin,
            rotation,
            kind: "wall_flat",
        };
        self.add(
            "entrance",
            offset(origin, rotation, [-9, 0, 0]),
            rotation,
            NoMirror,
        );
        data.position = offset(origin, rotation, [0, 0, 16]);
        let mut second = Wall {
            position: offset(data.position, R0, [0, 8, 0]),
            kind: "wall_window",
            ..data
        };
        self.outer_walls(&mut data, &mansion.base, [8, 5], [8, 4]);
        self.outer_walls(&mut second, &mansion.base, [8, 5], [8, 4]);
        let mut third = Wall {
            position: offset(data.position, R0, [0, 19, 0]),
            rotation: data.rotation,
            kind: "wall_window",
        };
        'third: for y in 0..11 {
            for x in (0..11).rev() {
                if mansion.third.is_house(x, y) {
                    third.position =
                        offset(third.position, rotation, [(x - 8) * 8, 0, 8 + (y - 5) * 8]);
                    self.wall(&mut third);
                    self.outer_walls(&mut third, &mansion.third, [x, y], [x, y]);
                    break 'third;
                }
            }
        }
        self.roof(
            offset(origin, R0, [0, 16, 0]),
            rotation,
            &mansion.base,
            Some(&mansion.third),
        );
        self.roof(
            offset(origin, R0, [0, 27, 0]),
            rotation,
            &mansion.third,
            None,
        );
        for floor in 0..3 {
            let floor_origin = offset(
                origin,
                R0,
                [0, 8 * floor as i32 + if floor == 2 { 3 } else { 0 }, 0],
            );
            let grid = if floor == 2 {
                &mansion.third
            } else {
                &mansion.base
            };
            let rooms = &mansion.rooms[floor];
            let upper = floor != 0;
            for y in 0..11 {
                for x in 0..11 {
                    if grid.get(x, y) == 1 {
                        let p = offset(floor_origin, rotation, [(x - 8) * 8, 0, 8 + (y - 5) * 8]);
                        self.add("corridor_floor", p, rotation, NoMirror);
                        for (dx, dy, name, v) in [
                            (0, -1, "carpet_north", [1, 1, 0]),
                            (1, 0, "carpet_east", [5, 1, 1]),
                            (
                                0,
                                1,
                                if upper {
                                    "carpet_south_2"
                                } else {
                                    "carpet_south_1"
                                },
                                [-1, 0, 5],
                            ),
                            (
                                -1,
                                0,
                                if upper {
                                    "carpet_west_2"
                                } else {
                                    "carpet_west_1"
                                },
                                [-1, 0, -1],
                            ),
                        ] {
                            if grid.get(x + dx, y + dy) == 1
                                || rooms.get(x + dx, y + dy) & CORRIDOR_FLAG == CORRIDOR_FLAG
                            {
                                self.add(name, offset(p, rotation, v), rotation, NoMirror);
                            }
                        }
                    }
                }
            }
            for y in 0..11 {
                for x in 0..11 {
                    let third_start = floor == 2 && grid.get(x, y) == 3;
                    if grid.get(x, y) != 2 && !third_start {
                        continue;
                    }
                    let data = rooms.get(x, y);
                    let id = data & ID_MASK;
                    let third_start = third_start && data & CORRIDOR_FLAG == CORRIDOR_FLAG;
                    let doors = if data & DOOR == DOOR {
                        [North, East, South, West]
                            .into_iter()
                            .filter(|d| {
                                let p = d.step();
                                grid.get(x + p.x, y + p.z) == 1
                            })
                            .collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };
                    let door = if !doors.is_empty() {
                        Some(doors[self.random.next_int_bound(doors.len() as i32) as usize])
                    } else if data & ORIGIN == ORIGIN {
                        Some(Up)
                    } else {
                        None
                    };
                    let p = offset(
                        floor_origin,
                        rotation,
                        [-1 + (x - 8) * 8, 0, 8 + (y - 5) * 8],
                    );
                    for (condition, side, v, extra) in [
                        (
                            grid.is_house(x - 1, y) && !mansion.is_room_id(x - 1, y, floor, id),
                            West,
                            [0, 0, 0],
                            R0,
                        ),
                        (grid.get(x + 1, y) == 1 && !third_start, East, [8, 0, 0], R0),
                        (
                            grid.is_house(x, y + 1) && !mansion.is_room_id(x, y + 1, floor, id),
                            South,
                            [7, 0, 7],
                            R90,
                        ),
                        (
                            grid.get(x, y - 1) == 1 && !third_start,
                            North,
                            [7, 0, -1],
                            R90,
                        ),
                    ] {
                        if condition {
                            let name = match (door == Some(side), upper) {
                                (true, true) => "indoors_door_2",
                                (true, false) => "indoors_door_1",
                                (false, true) => "indoors_wall_2",
                                (false, false) => "indoors_wall_1",
                            };
                            self.add(
                                name,
                                offset(p, rotation, v),
                                rotation.rotated(extra),
                                NoMirror,
                            );
                        }
                    }
                    match data & TYPE_MASK {
                        ROOM_1X1 => self.room_1x1(p, rotation, door, upper),
                        ROOM_1X2 => {
                            if let Some(door) = door {
                                let room = mansion
                                    .room_direction(x, y, floor, id)
                                    .expect("two-cell room");
                                self.room_1x2(
                                    p,
                                    rotation,
                                    room,
                                    door,
                                    upper,
                                    data & STAIRS == STAIRS,
                                );
                            }
                        }
                        ROOM_2X2 => {
                            if let Some(door) = door {
                                if door == Up {
                                    self.add(
                                        "2x2_s1",
                                        offset(p, rotation, [1, 0, 0]),
                                        rotation,
                                        NoMirror,
                                    );
                                } else {
                                    let mut room = door.clockwise();
                                    let step = room.step();
                                    if !mansion.is_room_id(x + step.x, y + step.z, floor, id) {
                                        room = room.opposite();
                                    }
                                    self.room_2x2(p, rotation, room, door, upper);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
/// Port of WoodlandMansionPieces#generateMansion. The floor planner and room
/// template choices consume one caller-owned stream in vanilla's traversal order.
pub fn generate(
    templates: &StructureTemplateManager,
    origin: [i32; 3],
    rotation: Rotation,
    random: &mut impl RandomSource,
) -> Vec<WoodlandMansionPiece> {
    let mansion = MansionGrid::new(random);
    let mut placer = Placer {
        templates,
        random,
        pieces: Vec::new(),
    };
    placer.create(origin, rotation, &mansion);
    placer.pieces
}
