mod building;
use super::{
    MonumentRoom,
    graph::{RoomGraph, RoomKind},
};
use crate::{
    Error,
    feature::FeatureContext,
    structure::{BoundingBox, piece::StructurePieceData},
};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{Direction, random::RandomSource};
const GRAY: Block = Block::PRISMARINE;
const LIGHT: Block = Block::PRISMARINE_BRICKS;
const BLACK: Block = Block::DARK_PRISMARINE;
const LAMP: Block = Block::SEA_LANTERN;
struct RoomPlacer<'a, 'b, 'c, 'd> {
    data: &'a StructurePieceData,
    context: &'b mut FeatureContext<'c, 'd>,
    bounds: BoundingBox,
}
impl RoomPlacer<'_, '_, '_, '_> {
    fn block(&self, p: [i32; 3]) -> Result<BlockState, Error> {
        let p = self.data.world_position(p);
        if self.bounds.contains(p) {
            self.context.region.block(p)
        } else {
            Ok(Block::AIR.default_state())
        }
    }
    fn put(&mut self, p: [i32; 3], block: Block) -> Result<(), Error> {
        self.data
            .place_block(self.context.region, block.default_state(), p, self.bounds)
    }
    fn fill(&mut self, a: [i32; 3], b: [i32; 3], block: Block) -> Result<(), Error> {
        self.data.generate_box(
            self.context.region,
            self.bounds,
            a,
            b,
            block.default_state(),
            block.default_state(),
            false,
        )
    }
    fn boxes(&mut self, boxes: &[([i32; 3], [i32; 3])], block: Block) -> Result<(), Error> {
        for &(a, b) in boxes {
            self.fill(a, b, block)?;
        }
        Ok(())
    }
    fn fill_water_only(&mut self, a: [i32; 3], b: [i32; 3], block: Block) -> Result<(), Error> {
        for y in a[1]..=b[1] {
            for x in a[0]..=b[0] {
                for z in a[2]..=b[2] {
                    let p = [x, y, z];
                    if self.block(p)? == Block::WATER.default_state() {
                        self.put(p, block)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn water(&mut self, a: [i32; 3], b: [i32; 3]) -> Result<(), Error> {
        for y in a[1]..=b[1] {
            for x in a[0]..=b[0] {
                for z in a[2]..=b[2] {
                    let p = [x, y, z];
                    let state = self.block(p)?;
                    if !matches!(
                        state.block(),
                        Block::ICE | Block::PACKED_ICE | Block::BLUE_ICE | Block::WATER
                    ) {
                        let block = if self.data.world_position(p)[1]
                            >= self.context.region.sea_level
                            && state != Block::WATER.default_state()
                        {
                            Block::AIR
                        } else {
                            Block::WATER
                        };
                        self.put(p, block)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn floor(&mut self, x: i32, z: i32, down: bool) -> Result<(), Error> {
        if down {
            for (a, b) in [
                ([0, 0, 0], [2, 0, 7]),
                ([5, 0, 0], [7, 0, 7]),
                ([3, 0, 0], [4, 0, 2]),
                ([3, 0, 5], [4, 0, 7]),
            ] {
                self.fill([a[0] + x, 0, a[2] + z], [b[0] + x, 0, b[2] + z], GRAY)?;
            }
            for (a, b) in [
                ([3, 0, 2], [4, 0, 2]),
                ([3, 0, 5], [4, 0, 5]),
                ([2, 0, 3], [2, 0, 4]),
                ([5, 0, 3], [5, 0, 4]),
            ] {
                self.fill([a[0] + x, 0, a[2] + z], [b[0] + x, 0, b[2] + z], LIGHT)?;
            }
        } else {
            self.fill([x, 0, z], [x + 7, 0, z + 7], GRAY)?;
        }
        Ok(())
    }
    fn elder(&mut self, local: [i32; 3]) -> Result<(), Error> {
        let p = self.data.world_position(local);
        if self.bounds.contains(p) {
            self.context.region.spawn_monument_elder([
                f64::from(p[0]) + 0.5,
                f64::from(p[1]),
                f64::from(p[2]) + 0.5,
            ])?;
        }
        Ok(())
    }
    fn penthouse(&mut self) -> Result<(), Error> {
        self.boxes(&[([2, -1, 2], [11, -1, 11])], LIGHT)?;
        self.boxes(
            &[
                ([0, -1, 0], [1, -1, 11]),
                ([12, -1, 0], [13, -1, 11]),
                ([2, -1, 0], [11, -1, 1]),
                ([2, -1, 12], [11, -1, 13]),
            ],
            GRAY,
        )?;
        self.boxes(
            &[
                ([0, 0, 0], [0, 0, 13]),
                ([13, 0, 0], [13, 0, 13]),
                ([1, 0, 0], [12, 0, 0]),
                ([1, 0, 13], [12, 0, 13]),
            ],
            LIGHT,
        )?;
        for i in (2..=11).step_by(3) {
            self.put([0, 0, i], LAMP)?;
            self.put([13, 0, i], LAMP)?;
            self.put([i, 0, 0], LAMP)?;
        }
        self.boxes(
            &[
                ([2, 0, 3], [4, 0, 9]),
                ([9, 0, 3], [11, 0, 9]),
                ([4, 0, 9], [9, 0, 11]),
                ([5, 0, 8], [5, 0, 8]),
                ([8, 0, 8], [8, 0, 8]),
                ([10, 0, 10], [10, 0, 10]),
                ([3, 0, 10], [3, 0, 10]),
            ],
            LIGHT,
        )?;
        self.boxes(
            &[
                ([3, 0, 3], [3, 0, 7]),
                ([10, 0, 3], [10, 0, 7]),
                ([6, 0, 10], [7, 0, 10]),
            ],
            BLACK,
        )?;
        for x in [3, 10] {
            for z in (2..=8).step_by(3) {
                self.fill([x, 0, z], [x, 2, z], LIGHT)?;
            }
        }
        self.boxes(&[([5, 0, 10], [5, 2, 10]), ([8, 0, 10], [8, 2, 10])], LIGHT)?;
        self.fill([6, -1, 7], [7, -1, 8], BLACK)?;
        self.water([6, -1, 3], [7, -1, 4])?;
        self.elder([6, 1, 6])
    }
    fn wing(&mut self, design: i32) -> Result<(), Error> {
        if design == 0 {
            for i in 0..4 {
                self.fill([10 - i, 3 - i, 20 - i], [12 + i, 3 - i, 20], LIGHT)?;
            }
            self.boxes(
                &[
                    ([7, 0, 6], [15, 0, 16]),
                    ([6, 0, 6], [6, 3, 20]),
                    ([16, 0, 6], [16, 3, 20]),
                    ([7, 1, 7], [7, 1, 20]),
                    ([15, 1, 7], [15, 1, 20]),
                    ([7, 1, 6], [9, 3, 6]),
                    ([13, 1, 6], [15, 3, 6]),
                    ([8, 1, 7], [9, 1, 7]),
                    ([13, 1, 7], [14, 1, 7]),
                    ([9, 0, 5], [13, 0, 5]),
                ],
                LIGHT,
            )?;
            self.boxes(
                &[
                    ([10, 0, 7], [12, 0, 7]),
                    ([8, 0, 10], [8, 0, 12]),
                    ([14, 0, 10], [14, 0, 12]),
                ],
                BLACK,
            )?;
            for z in [18, 15, 12, 9] {
                self.put([6, 3, z], LAMP)?;
                self.put([16, 3, z], LAMP)?;
            }
            for p in [
                [10, 0, 10],
                [12, 0, 10],
                [10, 0, 12],
                [12, 0, 12],
                [8, 3, 6],
                [14, 3, 6],
            ] {
                self.put(p, LAMP)?;
            }
            for p in [[4, 2, 4], [18, 2, 4], [4, 2, 18], [18, 2, 18]] {
                self.put(p, LIGHT)?;
                self.put([p[0], 1, p[2]], LAMP)?;
                self.put([p[0], 0, p[2]], LIGHT)?;
            }
            for p in [[9, 7, 20], [13, 7, 20]] {
                self.put(p, LIGHT)?;
            }
            self.boxes(
                &[([6, 0, 21], [7, 4, 21]), ([15, 0, 21], [16, 4, 21])],
                LIGHT,
            )?;
            self.elder([11, 2, 16])?;
        } else if design == 1 {
            self.boxes(
                &[
                    ([9, 3, 18], [13, 3, 20]),
                    ([9, 0, 18], [9, 2, 18]),
                    ([13, 0, 18], [13, 2, 18]),
                ],
                LIGHT,
            )?;
            for x in [9, 13] {
                self.put([x, 6, 20], LIGHT)?;
                self.put([x, 5, 20], LAMP)?;
                self.put([x, 4, 20], LIGHT)?;
            }
            self.fill([7, 3, 7], [15, 3, 14], LIGHT)?;
            for x in [10, 12] {
                self.fill([x, 0, 10], [x, 6, 10], LIGHT)?;
                self.fill([x, 0, 12], [x, 6, 12], LIGHT)?;
                for p in [[x, 0, 10], [x, 0, 12], [x, 4, 10], [x, 4, 12]] {
                    self.put(p, LAMP)?;
                }
            }
            for x in [8, 14] {
                self.fill([x, 0, 7], [x, 2, 7], LIGHT)?;
                self.fill([x, 0, 14], [x, 2, 14], LIGHT)?;
            }
            self.boxes(&[([8, 3, 8], [8, 3, 13]), ([14, 3, 8], [14, 3, 13])], BLACK)?;
            self.elder([11, 5, 13])?;
        }
        Ok(())
    }
    fn entry(&mut self, openings: [bool; 6]) -> Result<(), Error> {
        self.boxes(
            &[
                ([0, 3, 0], [2, 3, 7]),
                ([5, 3, 0], [7, 3, 7]),
                ([0, 2, 0], [1, 2, 7]),
                ([6, 2, 0], [7, 2, 7]),
                ([0, 1, 0], [0, 1, 7]),
                ([7, 1, 0], [7, 1, 7]),
                ([0, 1, 7], [7, 3, 7]),
                ([1, 1, 0], [2, 3, 0]),
                ([5, 1, 0], [6, 3, 0]),
            ],
            LIGHT,
        )?;
        for (direction, a, b) in [
            (Direction::North, [3, 1, 7], [4, 2, 7]),
            (Direction::West, [0, 1, 3], [1, 2, 4]),
            (Direction::East, [6, 1, 3], [7, 2, 4]),
        ] {
            if openings[direction as usize] {
                self.water(a, b)?;
            }
        }
        Ok(())
    }
    fn core(&mut self) -> Result<(), Error> {
        self.fill_water_only([1, 8, 0], [14, 8, 14], GRAY)?;
        self.boxes(
            &[
                ([0, 7, 0], [0, 7, 15]),
                ([15, 7, 0], [15, 7, 15]),
                ([1, 7, 0], [15, 7, 0]),
                ([1, 7, 15], [14, 7, 15]),
            ],
            LIGHT,
        )?;
        for y in 1..=6 {
            let block = if y == 2 || y == 6 { GRAY } else { LIGHT };
            for x in [0, 15] {
                for (z0, z1) in [(0, 1), (6, 9), (14, 15)] {
                    self.fill([x, y, z0], [x, y, z1], block)?;
                }
            }
            for (x0, z0, x1, z1) in [(1, 0, 1, 0), (6, 0, 9, 0), (14, 0, 14, 0), (1, 15, 14, 15)] {
                self.fill([x0, y, z0], [x1, y, z1], block)?;
            }
        }
        self.fill([6, 3, 6], [9, 6, 9], BLACK)?;
        self.fill([7, 4, 7], [8, 5, 8], Block::GOLD_BLOCK)?;
        for y in [3, 6] {
            for x in [6, 9] {
                self.put([x, y, 6], LAMP)?;
                self.put([x, y, 9], LAMP)?;
            }
        }
        self.boxes(
            &[
                ([5, 1, 6], [5, 2, 6]),
                ([5, 1, 9], [5, 2, 9]),
                ([10, 1, 6], [10, 2, 6]),
                ([10, 1, 9], [10, 2, 9]),
                ([6, 1, 5], [6, 2, 5]),
                ([9, 1, 5], [9, 2, 5]),
                ([6, 1, 10], [6, 2, 10]),
                ([9, 1, 10], [9, 2, 10]),
                ([5, 2, 5], [5, 6, 5]),
                ([5, 2, 10], [5, 6, 10]),
                ([10, 2, 5], [10, 6, 5]),
                ([10, 2, 10], [10, 6, 10]),
                ([5, 7, 1], [5, 7, 6]),
                ([10, 7, 1], [10, 7, 6]),
                ([5, 7, 9], [5, 7, 14]),
                ([10, 7, 9], [10, 7, 14]),
                ([1, 7, 5], [6, 7, 5]),
                ([1, 7, 10], [6, 7, 10]),
                ([9, 7, 5], [14, 7, 5]),
                ([9, 7, 10], [14, 7, 10]),
                ([2, 1, 2], [2, 1, 3]),
                ([3, 1, 2], [3, 1, 2]),
                ([13, 1, 2], [13, 1, 3]),
                ([12, 1, 2], [12, 1, 2]),
                ([2, 1, 12], [2, 1, 13]),
                ([3, 1, 13], [3, 1, 13]),
                ([13, 1, 12], [13, 1, 13]),
                ([12, 1, 13], [12, 1, 13]),
            ],
            LIGHT,
        )
    }
    fn double_horizontal(
        &mut self,
        graph: &RoomGraph,
        id: usize,
        along_x: bool,
    ) -> Result<(), Error> {
        let other = graph.neighbor(
            id,
            if along_x {
                Direction::East
            } else {
                Direction::North
            },
        );
        let room = graph.room(id);
        let neighbor = graph.room(other);
        let [width, depth] = if along_x { [16, 8] } else { [8, 16] };
        if room.index / 25 > 0 {
            self.floor(
                if along_x { 8 } else { 0 },
                if along_x { 0 } else { 8 },
                neighbor.openings[Direction::Down as usize],
            )?;
            self.floor(0, 0, room.openings[Direction::Down as usize])?;
        }
        if room.connections[Direction::Up as usize].is_none() {
            self.fill_water_only([1, 4, 1], if along_x { [7, 4, 6] } else { [6, 4, 7] }, GRAY)?;
        }
        if neighbor.connections[Direction::Up as usize].is_none() {
            self.fill_water_only(
                if along_x { [8, 4, 1] } else { [1, 4, 8] },
                if along_x { [14, 4, 6] } else { [6, 4, 14] },
                GRAY,
            )?;
        }
        for y in (1..=3).rev() {
            self.boxes(
                &[
                    ([0, y, 0], [0, y, depth - 1]),
                    ([width - 1, y, 0], [width - 1, y, depth - 1]),
                    ([1, y, 0], [width - 1, y, 0]),
                    ([1, y, depth - 1], [width - 2, y, depth - 1]),
                ],
                if y == 2 { GRAY } else { LIGHT },
            )?;
        }
        if along_x {
            self.fill([5, 1, 0], [10, 1, 4], LIGHT)?;
            self.fill([6, 2, 0], [9, 2, 3], GRAY)?;
            self.fill([5, 3, 0], [10, 3, 4], LIGHT)?;
            self.put([6, 2, 3], LAMP)?;
            self.put([9, 2, 3], LAMP)?;
            for (room, direction, a, b) in [
                (room, Direction::South, [3, 1, 0], [4, 2, 0]),
                (room, Direction::North, [3, 1, 7], [4, 2, 7]),
                (room, Direction::West, [0, 1, 3], [0, 2, 4]),
                (neighbor, Direction::South, [11, 1, 0], [12, 2, 0]),
                (neighbor, Direction::North, [11, 1, 7], [12, 2, 7]),
                (neighbor, Direction::East, [15, 1, 3], [15, 2, 4]),
            ] {
                if room.openings[direction as usize] {
                    self.water(a, b)?;
                }
            }
        } else {
            self.boxes(
                &[
                    ([1, 1, 1], [1, 1, 2]),
                    ([6, 1, 1], [6, 1, 2]),
                    ([1, 3, 1], [1, 3, 2]),
                    ([6, 3, 1], [6, 3, 2]),
                    ([1, 1, 13], [1, 1, 14]),
                    ([6, 1, 13], [6, 1, 14]),
                    ([1, 3, 13], [1, 3, 14]),
                    ([6, 3, 13], [6, 3, 14]),
                    ([2, 1, 6], [2, 3, 6]),
                    ([5, 1, 6], [5, 3, 6]),
                    ([2, 1, 9], [2, 3, 9]),
                    ([5, 1, 9], [5, 3, 9]),
                    ([3, 2, 6], [4, 2, 6]),
                    ([3, 2, 9], [4, 2, 9]),
                    ([2, 2, 7], [2, 2, 8]),
                    ([5, 2, 7], [5, 2, 8]),
                ],
                LIGHT,
            )?;
            for p in [[2, 2, 5], [5, 2, 5], [2, 2, 10], [5, 2, 10]] {
                self.put(p, LAMP)?;
            }
            for p in [[2, 3, 5], [5, 3, 5], [2, 3, 10], [5, 3, 10]] {
                self.put(p, LIGHT)?;
            }
            for (room, direction, a, b) in [
                (room, Direction::South, [3, 1, 0], [4, 2, 0]),
                (room, Direction::East, [7, 1, 3], [7, 2, 4]),
                (room, Direction::West, [0, 1, 3], [0, 2, 4]),
                (neighbor, Direction::North, [3, 1, 15], [4, 2, 15]),
                (neighbor, Direction::West, [0, 1, 11], [0, 2, 12]),
                (neighbor, Direction::East, [7, 1, 11], [7, 2, 12]),
            ] {
                if room.openings[direction as usize] {
                    self.water(a, b)?;
                }
            }
        }
        Ok(())
    }
    fn double_tall(&mut self, graph: &RoomGraph, id: usize, along_x: bool) -> Result<(), Error> {
        let other = graph.neighbor(
            id,
            if along_x {
                Direction::East
            } else {
                Direction::North
            },
        );
        let room = graph.room(id);
        let neighbor = graph.room(other);
        let upper = graph.room(graph.neighbor(id, Direction::Up));
        let other_upper = graph.room(graph.neighbor(other, Direction::Up));
        if room.index / 25 > 0 {
            self.floor(
                if along_x { 8 } else { 0 },
                if along_x { 0 } else { 8 },
                neighbor.openings[Direction::Down as usize],
            )?;
            self.floor(0, 0, room.openings[Direction::Down as usize])?;
        }
        if upper.connections[Direction::Up as usize].is_none() {
            self.fill_water_only([1, 8, 1], if along_x { [7, 8, 6] } else { [6, 8, 7] }, GRAY)?;
        }
        if other_upper.connections[Direction::Up as usize].is_none() {
            self.fill_water_only(
                if along_x { [8, 8, 1] } else { [1, 8, 8] },
                if along_x { [14, 8, 6] } else { [6, 8, 14] },
                GRAY,
            )?;
        }
        let [w, d] = if along_x { [16, 8] } else { [8, 16] };
        for y in 1..=7 {
            self.boxes(
                &[
                    ([0, y, 0], [0, y, d - 1]),
                    ([w - 1, y, 0], [w - 1, y, d - 1]),
                    ([1, y, 0], [if along_x { 15 } else { 6 }, y, 0]),
                    ([1, y, d - 1], [w - 2, y, d - 1]),
                ],
                if y == 2 || y == 6 { GRAY } else { LIGHT },
            )?;
        }
        if along_x {
            self.boxes(
                &[
                    ([2, 1, 3], [2, 7, 4]),
                    ([3, 1, 2], [4, 7, 2]),
                    ([3, 1, 5], [4, 7, 5]),
                    ([13, 1, 3], [13, 7, 4]),
                    ([11, 1, 2], [12, 7, 2]),
                    ([11, 1, 5], [12, 7, 5]),
                    ([5, 1, 3], [5, 3, 4]),
                    ([10, 1, 3], [10, 3, 4]),
                    ([5, 7, 2], [10, 7, 5]),
                    ([5, 5, 2], [5, 7, 2]),
                    ([10, 5, 2], [10, 7, 2]),
                    ([5, 5, 5], [5, 7, 5]),
                    ([10, 5, 5], [10, 7, 5]),
                ],
                LIGHT,
            )?;
            for p in [[6, 6, 2], [9, 6, 2], [6, 6, 5], [9, 6, 5]] {
                self.put(p, LIGHT)?;
            }
            self.boxes(&[([5, 4, 3], [6, 4, 4]), ([9, 4, 3], [10, 4, 4])], LIGHT)?;
            for p in [[5, 4, 2], [5, 4, 5], [10, 4, 2], [10, 4, 5]] {
                self.put(p, LAMP)?;
            }
        } else {
            for y in 1..=7 {
                self.fill(
                    [3, y, 7],
                    [4, y, 8],
                    if y == 2 || y == 6 { LAMP } else { BLACK },
                )?;
            }
        }
        for (near, far, y) in [(room, neighbor, 1), (upper, other_upper, 5)] {
            if along_x {
                for (room, direction, a, b) in [
                    (near, Direction::South, [3, y, 0], [4, y + 1, 0]),
                    (near, Direction::North, [3, y, 7], [4, y + 1, 7]),
                    (near, Direction::West, [0, y, 3], [0, y + 1, 4]),
                    (far, Direction::South, [11, y, 0], [12, y + 1, 0]),
                    (far, Direction::North, [11, y, 7], [12, y + 1, 7]),
                    (far, Direction::East, [15, y, 3], [15, y + 1, 4]),
                ] {
                    if room.openings[direction as usize] {
                        self.water(a, b)?;
                    }
                }
            } else {
                for (room, direction, a, b) in [
                    (near, Direction::South, [3, y, 0], [4, y + 1, 0]),
                    (near, Direction::East, [7, y, 3], [7, y + 1, 4]),
                    (near, Direction::West, [0, y, 3], [0, y + 1, 4]),
                    (far, Direction::North, [3, y, 15], [4, y + 1, 15]),
                    (far, Direction::West, [0, y, 11], [0, y + 1, 12]),
                    (far, Direction::East, [7, y, 11], [7, y + 1, 12]),
                ] {
                    if !room.openings[direction as usize] {
                        continue;
                    }
                    self.water(a, b)?;
                    if y == 5 && matches!(direction, Direction::West | Direction::East) {
                        let z = if a[2] == 3 { 2 } else { 10 };
                        let (x0, x1) = if direction == Direction::East {
                            (5, 6)
                        } else {
                            (1, 2)
                        };
                        let pillar_x = if direction == Direction::East { 6 } else { 1 };
                        self.fill([x0, 4, z], [x1, 4, z + 3], LIGHT)?;
                        self.fill([pillar_x, 1, z], [pillar_x, 3, z], LIGHT)?;
                        self.fill([pillar_x, 1, z + 3], [pillar_x, 3, z + 3], LIGHT)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn double_y(&mut self, graph: &RoomGraph, id: usize) -> Result<(), Error> {
        let room = graph.room(id);
        let above = graph.room(graph.neighbor(id, Direction::Up));
        if room.index / 25 > 0 {
            self.floor(0, 0, room.openings[Direction::Down as usize])?;
        }
        if above.connections[Direction::Up as usize].is_none() {
            self.fill_water_only([1, 8, 1], [6, 8, 6], GRAY)?;
        }
        self.boxes(
            &[
                ([0, 4, 0], [0, 4, 7]),
                ([7, 4, 0], [7, 4, 7]),
                ([1, 4, 0], [6, 4, 0]),
                ([1, 4, 7], [6, 4, 7]),
                ([2, 4, 1], [2, 4, 2]),
                ([1, 4, 2], [1, 4, 2]),
                ([5, 4, 1], [5, 4, 2]),
                ([6, 4, 2], [6, 4, 2]),
                ([2, 4, 5], [2, 4, 6]),
                ([1, 4, 5], [1, 4, 5]),
                ([5, 4, 5], [5, 4, 6]),
                ([6, 4, 5], [6, 4, 5]),
            ],
            LIGHT,
        )?;
        for (room, y) in [(room, 1), (above, 5)] {
            for (direction, axis, side) in [
                (Direction::South, false, 0),
                (Direction::North, false, 7),
                (Direction::West, true, 0),
                (Direction::East, true, 7),
            ] {
                let pos = |n, level| {
                    if axis {
                        [side, level, n]
                    } else {
                        [n, level, side]
                    }
                };
                if room.openings[direction as usize] {
                    self.fill(pos(2, y), pos(2, y + 2), LIGHT)?;
                    self.fill(pos(5, y), pos(5, y + 2), LIGHT)?;
                    self.fill(pos(3, y + 2), pos(4, y + 2), LIGHT)?;
                } else {
                    self.fill(pos(0, y), pos(7, y + 2), LIGHT)?;
                    self.fill(pos(0, y + 1), pos(7, y + 1), GRAY)?;
                }
            }
        }
        Ok(())
    }
    fn simple(&mut self, graph: &RoomGraph, id: usize, design: i32) -> Result<(), Error> {
        let room = graph.room(id);
        if room.index / 25 > 0 {
            self.floor(0, 0, room.openings[Direction::Down as usize])?;
        }
        if room.connections[Direction::Up as usize].is_none() {
            self.fill_water_only([1, 4, 1], [6, 4, 6], GRAY)?;
        }
        let pillar = design != 0
            && self.context.random.next_boolean()
            && !room.openings[Direction::Down as usize]
            && !room.openings[Direction::Up as usize]
            && room.openings.iter().filter(|&&v| v).count() > 1;
        match design {
            0 => {
                for (x, z) in [(0, 0), (5, 0), (0, 5), (5, 5)] {
                    self.fill([x, 1, z], [x + 2, 1, z + 2], LIGHT)?;
                    self.fill([x, 3, z], [x + 2, 3, z + 2], LIGHT)?;
                    let side_x = if x == 0 { 0 } else { 7 };
                    let side_z = if z == 0 { 0 } else { 7 };
                    self.fill([side_x, 2, z], [side_x, 2, z + 2], GRAY)?;
                    self.fill(
                        [if x == 0 { 1 } else { 5 }, 2, side_z],
                        [if x == 0 { 2 } else { 6 }, 2, side_z],
                        GRAY,
                    )?;
                    self.put(
                        [if x == 0 { 1 } else { 6 }, 2, if z == 0 { 1 } else { 6 }],
                        LAMP,
                    )?;
                }
                for (direction, a, b, closed_a, closed_b, gray_a, gray_b) in [
                    (
                        Direction::South,
                        [3, 3, 0],
                        [4, 3, 0],
                        [3, 3, 0],
                        [4, 3, 1],
                        [3, 2, 0],
                        [4, 2, 0],
                    ),
                    (
                        Direction::North,
                        [3, 3, 7],
                        [4, 3, 7],
                        [3, 3, 6],
                        [4, 3, 7],
                        [3, 2, 7],
                        [4, 2, 7],
                    ),
                    (
                        Direction::West,
                        [0, 3, 3],
                        [0, 3, 4],
                        [0, 3, 3],
                        [1, 3, 4],
                        [0, 2, 3],
                        [0, 2, 4],
                    ),
                    (
                        Direction::East,
                        [7, 3, 3],
                        [7, 3, 4],
                        [6, 3, 3],
                        [7, 3, 4],
                        [7, 2, 3],
                        [7, 2, 4],
                    ),
                ] {
                    if room.openings[direction as usize] {
                        self.fill(a, b, LIGHT)?;
                    } else {
                        self.fill(closed_a, closed_b, LIGHT)?;
                        self.fill(gray_a, gray_b, GRAY)?;
                        self.fill(
                            [closed_a[0], 1, closed_a[2]],
                            [closed_b[0], 1, closed_b[2]],
                            LIGHT,
                        )?;
                    }
                }
            }
            1 => {
                for (x, z) in [(2, 2), (2, 5), (5, 5), (5, 2)] {
                    self.fill([x, 1, z], [x, 3, z], LIGHT)?;
                }
                for (x, z) in [(2, 2), (2, 5), (5, 5), (5, 2)] {
                    self.put([x, 2, z], LAMP)?;
                }
                self.boxes(
                    &[
                        ([0, 1, 0], [1, 3, 0]),
                        ([0, 1, 1], [0, 3, 1]),
                        ([0, 1, 7], [1, 3, 7]),
                        ([0, 1, 6], [0, 3, 6]),
                        ([6, 1, 7], [7, 3, 7]),
                        ([7, 1, 6], [7, 3, 6]),
                        ([6, 1, 0], [7, 3, 0]),
                        ([7, 1, 1], [7, 3, 1]),
                    ],
                    LIGHT,
                )?;
                for p in [
                    [1, 2, 0],
                    [0, 2, 1],
                    [1, 2, 7],
                    [0, 2, 6],
                    [6, 2, 7],
                    [7, 2, 6],
                    [6, 2, 0],
                    [7, 2, 1],
                ] {
                    self.put(p, GRAY)?;
                }
                for (direction, a, b) in [
                    (Direction::South, [1, 0, 0], [6, 0, 0]),
                    (Direction::North, [1, 0, 7], [6, 0, 7]),
                    (Direction::West, [0, 0, 1], [0, 0, 6]),
                    (Direction::East, [7, 0, 1], [7, 0, 6]),
                ] {
                    if !room.openings[direction as usize] {
                        for y in (1..=3).rev() {
                            self.fill(
                                [a[0], y, a[2]],
                                [b[0], y, b[2]],
                                if y == 2 { GRAY } else { LIGHT },
                            )?;
                        }
                    }
                }
            }
            2 => {
                for y in 1..=3 {
                    self.boxes(
                        &[
                            ([0, y, 0], [0, y, 7]),
                            ([7, y, 0], [7, y, 7]),
                            ([1, y, 0], [6, y, 0]),
                            ([1, y, 7], [6, y, 7]),
                        ],
                        if y == 2 { BLACK } else { LIGHT },
                    )?;
                }
                self.boxes(
                    &[
                        ([0, 1, 3], [0, 2, 4]),
                        ([7, 1, 3], [7, 2, 4]),
                        ([3, 1, 0], [4, 2, 0]),
                        ([3, 1, 7], [4, 2, 7]),
                    ],
                    BLACK,
                )?;
                for (direction, a, b) in [
                    (Direction::South, [3, 1, 0], [4, 2, 0]),
                    (Direction::North, [3, 1, 7], [4, 2, 7]),
                    (Direction::West, [0, 1, 3], [0, 2, 4]),
                    (Direction::East, [7, 1, 3], [7, 2, 4]),
                ] {
                    if room.openings[direction as usize] {
                        self.water(a, b)?;
                    }
                }
            }
            _ => return Err(Error("invalid monument simple-room design".into())),
        }
        if pillar {
            for y in 1..=3 {
                self.fill([3, y, 3], [4, y, 4], if y == 2 { GRAY } else { LIGHT })?;
            }
        }
        Ok(())
    }
    fn simple_top(&mut self, graph: &RoomGraph, id: usize) -> Result<(), Error> {
        let room = graph.room(id);
        if room.index / 25 > 0 {
            self.floor(0, 0, room.openings[Direction::Down as usize])?;
        }
        if room.connections[Direction::Up as usize].is_none() {
            self.fill_water_only([1, 4, 1], [6, 4, 6], GRAY)?;
        }
        for x in 1..=6 {
            for z in 1..=6 {
                if self.context.random.next_int_bound(3) != 0 {
                    let y = 2 + if self.context.random.next_int_bound(4) == 0 {
                        0
                    } else {
                        1
                    };
                    self.fill([x, y, z], [x, 3, z], Block::WET_SPONGE)?;
                }
            }
        }
        for y in 1..=3 {
            let block = if y == 2 { BLACK } else { LIGHT };
            self.boxes(
                &[
                    ([0, y, 0], [0, y, 7]),
                    ([7, y, 0], [7, y, 7]),
                    ([1, y, 0], [6, y, 0]),
                    ([1, y, 7], [6, y, 7]),
                ],
                block,
            )?;
        }
        self.boxes(
            &[
                ([0, 1, 3], [0, 2, 4]),
                ([7, 1, 3], [7, 2, 4]),
                ([3, 1, 0], [4, 2, 0]),
                ([3, 1, 7], [4, 2, 7]),
            ],
            BLACK,
        )?;
        if room.openings[Direction::South as usize] {
            self.water([3, 1, 0], [4, 2, 0])?;
        }
        Ok(())
    }
}
impl MonumentRoom {
    /// Port of every monument child-room postProcess implementation. Elder
    /// creation remains an explicit game-adapter operation.
    pub fn post_process(
        &self,
        context: &mut FeatureContext<'_, '_>,
        graph: &RoomGraph,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let mut placer = RoomPlacer {
            data: &self.data,
            context,
            bounds,
        };
        match self.kind {
            RoomKind::Entry => {
                placer.entry(graph.room(self.room.expect("entry definition")).openings)
            }
            RoomKind::Core => placer.core(),
            RoomKind::DoubleXY => {
                placer.double_tall(graph, self.room.expect("double XY definition"), true)
            }
            RoomKind::DoubleYZ => {
                placer.double_tall(graph, self.room.expect("double YZ definition"), false)
            }
            RoomKind::DoubleX => {
                placer.double_horizontal(graph, self.room.expect("double X definition"), true)
            }
            RoomKind::DoubleZ => {
                placer.double_horizontal(graph, self.room.expect("double Z definition"), false)
            }
            RoomKind::DoubleY => placer.double_y(graph, self.room.expect("double Y definition")),
            RoomKind::Simple { design } => {
                placer.simple(graph, self.room.expect("simple definition"), design)
            }
            RoomKind::SimpleTop => placer.simple_top(graph, self.room.expect("sponge definition")),
            RoomKind::Wing { design } => placer.wing(design),
            RoomKind::Penthouse => placer.penthouse(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        feature::FeatureRegistry,
        region::{ChunkStatus, RegionChunk, WorldGenRegion},
        structure::{
            processor::tests::{StaticBlocks, generator},
            structures::ocean_monument::MonumentBuilding,
        },
    };
    use ferraloom_util::random::{Algorithm, LegacyRandomSource, WorldgenRandom};
    use std::io::Read;
    fn initial(mode: i32, [x, y, z]: [i32; 3]) -> BlockState {
        if mode == 0 {
            return if y < 80 { Block::WATER } else { Block::AIR }.default_state();
        }
        match (x * 3 + y * 5 + z).rem_euclid(8) {
            0 => Block::WATER.default_state(),
            1 => Block::WATER
                .default_state()
                .with(ferraloom_data::properties::LEVEL, 3)
                .unwrap(),
            2 => Block::ICE.default_state(),
            3 => Block::PACKED_ICE.default_state(),
            4 => Block::BLUE_ICE.default_state(),
            5 => Block::STONE.default_state(),
            _ => Block::AIR.default_state(),
        }
    }
    #[test]
    fn monument_single_core_and_sponge_geometry_clipping_water_and_rng_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../testdata/monument_room_blocks.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let seed = case["seed"].as_i64().unwrap();
            let direction =
                Direction::from_2d_data_value(case["direction"].as_i64().unwrap() as i32);
            let mode = case["mode"].as_i64().unwrap() as i32;
            let building =
                MonumentBuilding::new(&mut LegacyRandomSource::new(seed), -46, 8, direction);
            let mut child = building.children[case["child"].as_u64().unwrap() as usize].clone();
            let min = child.data.bounds.min;
            child.data.bounds = child.data.bounds.moved([-min[0], 64 - min[1], -min[2]]);
            assert_eq!(
                child.data.to_nbt(&child.kind.piece_id().parse().unwrap()),
                ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap()
            );
            let mut chunk = generator.new_chunk([0, 0]);
            for x in 0..16 {
                for z in 0..16 {
                    for y in 63..=80 {
                        chunk.set_block(x as u8, y, z as u8, initial(mode, [x, y, z]));
                    }
                }
            }
            let mut chunks = [RegionChunk::new(chunk, ChunkStatus::Terrain)];
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [0, 0],
                &[ChunkStatus::Terrain],
                0,
                None,
                &StaticBlocks,
                0,
                67,
            )
            .unwrap();
            let b = &case["bounds"];
            let bounds = BoundingBox::new(
                std::array::from_fn(|i| b[i].as_i64().unwrap() as i32),
                std::array::from_fn(|i| b[i + 3].as_i64().unwrap() as i32),
            );
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(17));
            child
                .post_process(
                    &mut FeatureContext {
                        region: &mut region,
                        random: &mut random,
                        origin: [0; 3],
                        generator: &generator,
                        registry: &features,
                    },
                    &building.graph,
                    bounds,
                )
                .unwrap();
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "seed={seed} direction={direction:?} kind={:?} mode={mode}",
                child.kind
            );
            let actual = region
                .chunk([0, 0], ChunkStatus::Terrain)
                .unwrap()
                .poi_changes
                .iter()
                .map(|(p, _, state)| (*p, state.id()))
                .collect::<Vec<_>>();
            let expected = case["writes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| {
                    (
                        std::array::from_fn(|i| w["pos"][i].as_i64().unwrap() as i32),
                        w["state"].as_u64().unwrap() as u16,
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual, expected,
                "seed={seed} direction={direction:?} kind={:?} mode={mode} bounds={bounds:?}",
                child.kind
            );
        }
    }
}
