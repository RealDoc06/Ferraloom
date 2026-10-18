use super::{SmallDoorType, StrongholdPiece, StrongholdPieceKind};
use crate::{
    Error, region::WorldGenRegion, structure::BoundingBox, structure::StructurePlacementContext,
};
use ferraloom_data::{
    Block, BlockState,
    properties::{self, East, Facing, Half, North, South, Type, West},
};
use ferraloom_util::{Direction, random::RandomSource};

impl StrongholdPiece {
    /// Port of StrongholdPieces' synchronous postProcess dispatch. Smooth-stone
    /// selection observes live air and consumes RNG before write clipping.
    pub fn post_process(
        &mut self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let brick = Block::STONE_BRICKS.default_state();
        let air = Block::CAVE_AIR.default_state();
        let slab = Block::SMOOTH_STONE_SLAB.default_state();
        match self.kind.clone() {
            StrongholdPieceKind::FillerCorridor { steps } => {
                for z in 0..steps {
                    for x in 0..=4 {
                        self.data
                            .place_block(context.region, brick, [x, 0, z], bounds)?;
                    }
                    for y in 1..=3 {
                        for (state, x) in [(brick, 0), (air, 1), (air, 2), (air, 3), (brick, 4)] {
                            self.data
                                .place_block(context.region, state, [x, y, z], bounds)?;
                        }
                    }
                    for x in 0..=4 {
                        self.data
                            .place_block(context.region, brick, [x, 4, z], bounds)?;
                    }
                }
            }
            StrongholdPieceKind::Start { .. } | StrongholdPieceKind::StairsDown { .. } => {
                self.smooth(context, bounds, [0, 0, 0, 4, 10, 4], true)?;
                self.small_door(context, bounds, self.door, [1, 7, 0])?;
                self.small_door(context, bounds, SmallDoorType::Opening, [1, 1, 4])?;
                for (state, p) in [
                    (brick, [2, 6, 1]),
                    (brick, [1, 5, 1]),
                    (slab, [1, 6, 1]),
                    (brick, [1, 5, 2]),
                    (brick, [1, 4, 3]),
                    (slab, [1, 5, 3]),
                    (brick, [2, 4, 3]),
                    (brick, [3, 3, 3]),
                    (slab, [3, 4, 3]),
                    (brick, [3, 3, 2]),
                    (brick, [3, 2, 1]),
                    (slab, [3, 3, 1]),
                    (brick, [2, 2, 1]),
                    (brick, [1, 1, 1]),
                    (slab, [1, 2, 1]),
                    (brick, [1, 1, 2]),
                    (slab, [1, 1, 3]),
                ] {
                    self.data.place_block(context.region, state, p, bounds)?;
                }
            }
            StrongholdPieceKind::Straight { left, right } => {
                self.smooth(context, bounds, [0, 0, 0, 4, 4, 6], true)?;
                self.small_door(context, bounds, self.door, [1, 1, 0])?;
                self.small_door(context, bounds, SmallDoorType::Opening, [1, 1, 6])?;
                for (p, facing) in [
                    ([1, 2, 1], Facing::East),
                    ([3, 2, 1], Facing::West),
                    ([1, 2, 5], Facing::East),
                    ([3, 2, 5], Facing::West),
                ] {
                    if context.random.next_float() < 0.1 {
                        self.data
                            .place_block(context.region, torch(facing), p, bounds)?;
                    }
                }
                if left {
                    self.rect(context, bounds, [0, 1, 2, 0, 3, 4], air)?;
                }
                if right {
                    self.rect(context, bounds, [4, 1, 2, 4, 3, 4], air)?;
                }
            }
            StrongholdPieceKind::StraightStairs => {
                self.smooth(context, bounds, [0, 0, 0, 4, 10, 7], true)?;
                self.small_door(context, bounds, self.door, [1, 7, 0])?;
                self.small_door(context, bounds, SmallDoorType::Opening, [1, 1, 7])?;
                let stairs = Block::COBBLESTONE_STAIRS
                    .default_state()
                    .with(properties::FACING, Facing::South)
                    .expect("stair facing");
                for i in 0..6 {
                    for x in 1..=3 {
                        self.data
                            .place_block(context.region, stairs, [x, 6 - i, 1 + i], bounds)?;
                    }
                    if i < 5 {
                        for x in 1..=3 {
                            self.data.place_block(
                                context.region,
                                brick,
                                [x, 5 - i, 1 + i],
                                bounds,
                            )?;
                        }
                    }
                }
            }
            StrongholdPieceKind::LeftTurn | StrongholdPieceKind::RightTurn => {
                self.smooth(context, bounds, [0, 0, 0, 4, 4, 4], true)?;
                self.small_door(context, bounds, self.door, [1, 1, 0])?;
                let north_east = matches!(
                    self.data.orientation,
                    Some(Direction::North | Direction::East)
                );
                let x = if north_east == matches!(self.kind, StrongholdPieceKind::LeftTurn) {
                    0
                } else {
                    4
                };
                self.rect(context, bounds, [x, 1, 1, x, 3, 3], air)?;
            }
            StrongholdPieceKind::ChestCorridor { placed_chest } => {
                self.smooth(context, bounds, [0, 0, 0, 4, 4, 6], true)?;
                self.small_door(context, bounds, self.door, [1, 1, 0])?;
                self.small_door(context, bounds, SmallDoorType::Opening, [1, 1, 6])?;
                self.rect(context, bounds, [3, 1, 2, 3, 1, 4], brick)?;
                let stone_slab = Block::STONE_BRICK_SLAB.default_state();
                for p in [
                    [3, 1, 1],
                    [3, 1, 5],
                    [3, 2, 2],
                    [3, 2, 4],
                    [2, 1, 2],
                    [2, 1, 3],
                    [2, 1, 4],
                ] {
                    self.data
                        .place_block(context.region, stone_slab, p, bounds)?;
                }
                let p = [3, 2, 3];
                if !placed_chest && bounds.contains(self.data.world_position(p)) {
                    if let StrongholdPieceKind::ChestCorridor { placed_chest } = &mut self.kind {
                        *placed_chest = true;
                    }
                    self.data.create_chest(
                        context,
                        p,
                        bounds,
                        &"minecraft:chests/stronghold_corridor".parse()?,
                    )?;
                }
            }
            StrongholdPieceKind::FiveCrossing {
                left_low,
                left_high,
                right_low,
                right_high,
            } => {
                self.smooth(context, bounds, [0, 0, 0, 9, 8, 10], true)?;
                self.small_door(context, bounds, self.door, [4, 3, 0])?;
                for (present, r) in [
                    (left_low, [0, 3, 1, 0, 5, 3]),
                    (right_low, [9, 3, 1, 9, 5, 3]),
                    (left_high, [0, 5, 7, 0, 7, 9]),
                    (right_high, [9, 5, 7, 9, 7, 9]),
                ] {
                    if present {
                        self.rect(context, bounds, r, air)?;
                    }
                }
                self.rect(context, bounds, [5, 1, 10, 7, 3, 10], air)?;
                for r in [
                    [1, 2, 1, 8, 2, 6],
                    [4, 1, 5, 4, 4, 9],
                    [8, 1, 5, 8, 4, 9],
                    [1, 4, 7, 3, 4, 9],
                    [1, 3, 5, 3, 3, 6],
                ] {
                    self.smooth(context, bounds, r, false)?;
                }
                self.rect(context, bounds, [1, 3, 4, 3, 3, 4], slab)?;
                self.rect(context, bounds, [1, 4, 6, 3, 4, 6], slab)?;
                self.smooth(context, bounds, [5, 1, 7, 7, 1, 8], false)?;
                for r in [
                    [5, 1, 9, 7, 1, 9],
                    [5, 2, 7, 7, 2, 7],
                    [4, 5, 7, 4, 5, 9],
                    [8, 5, 7, 8, 5, 9],
                ] {
                    self.rect(context, bounds, r, slab)?;
                }
                self.rect(
                    context,
                    bounds,
                    [5, 5, 7, 7, 5, 9],
                    slab.with(properties::TYPE_, Type::Double)
                        .expect("slab type"),
                )?;
                self.data
                    .place_block(context.region, torch(Facing::South), [6, 5, 6], bounds)?;
            }
            StrongholdPieceKind::PrisonHall => self.prison(context, bounds)?,
            StrongholdPieceKind::RoomCrossing { style } => {
                self.room_crossing(context, bounds, style)?
            }
            StrongholdPieceKind::Library { tall } => self.library(context, bounds, tall)?,
            StrongholdPieceKind::PortalRoom { placed_spawner } => {
                self.portal_room(context, bounds, placed_spawner)?
            }
        }
        Ok(())
    }
    fn block(
        &self,
        region: &WorldGenRegion<'_>,
        p: [i32; 3],
        bounds: BoundingBox,
    ) -> Result<BlockState, Error> {
        let world = self.data.world_position(p);
        if bounds.contains(world) {
            region.block(world)
        } else {
            Ok(Block::AIR.default_state())
        }
    }
    fn smooth(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        r: [i32; 6],
        skip_air: bool,
    ) -> Result<(), Error> {
        let [x0, y0, z0, x1, y1, z1] = r;
        for y in y0..=y1 {
            for x in x0..=x1 {
                for z in z0..=z1 {
                    if skip_air && self.block(context.region, [x, y, z], bounds)?.is_air() {
                        continue;
                    }
                    let edge = x == x0 || x == x1 || y == y0 || y == y1 || z == z0 || z == z1;
                    let block = if edge {
                        let choice = context.random.next_float();
                        if choice < 0.2 {
                            Block::CRACKED_STONE_BRICKS
                        } else if choice < 0.5 {
                            Block::MOSSY_STONE_BRICKS
                        } else if choice < 0.55 {
                            Block::INFESTED_STONE_BRICKS
                        } else {
                            Block::STONE_BRICKS
                        }
                    } else {
                        Block::CAVE_AIR
                    };
                    self.data.place_block(
                        context.region,
                        block.default_state(),
                        [x, y, z],
                        bounds,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn rect(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        r: [i32; 6],
        state: BlockState,
    ) -> Result<(), Error> {
        self.data.generate_box(
            context.region,
            bounds,
            [r[0], r[1], r[2]],
            [r[3], r[4], r[5]],
            state,
            state,
            false,
        )
    }
    fn small_door(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        kind: SmallDoorType,
        [x, y, z]: [i32; 3],
    ) -> Result<(), Error> {
        match kind {
            SmallDoorType::Opening => self.rect(
                context,
                bounds,
                [x, y, z, x + 2, y + 2, z],
                Block::CAVE_AIR.default_state(),
            )?,
            SmallDoorType::WoodDoor | SmallDoorType::IronDoor => {
                for p in [
                    [x, y, z],
                    [x, y + 1, z],
                    [x, y + 2, z],
                    [x + 1, y + 2, z],
                    [x + 2, y + 2, z],
                    [x + 2, y + 1, z],
                    [x + 2, y, z],
                ] {
                    self.data.place_block(
                        context.region,
                        Block::STONE_BRICKS.default_state(),
                        p,
                        bounds,
                    )?;
                }
                let block = if kind == SmallDoorType::WoodDoor {
                    Block::OAK_DOOR
                } else {
                    Block::IRON_DOOR
                };
                self.data.place_block(
                    context.region,
                    block.default_state(),
                    [x + 1, y, z],
                    bounds,
                )?;
                self.data.place_block(
                    context.region,
                    block
                        .default_state()
                        .with(properties::HALF, Half::Upper)
                        .expect("door half"),
                    [x + 1, y + 1, z],
                    bounds,
                )?;
                if kind == SmallDoorType::IronDoor {
                    for (facing, p) in [
                        (Facing::North, [x + 2, y + 1, z + 1]),
                        (Facing::South, [x + 2, y + 1, z - 1]),
                    ] {
                        self.data.place_block(
                            context.region,
                            Block::STONE_BUTTON
                                .default_state()
                                .with(properties::FACING, facing)
                                .expect("button facing"),
                            p,
                            bounds,
                        )?;
                    }
                }
            }
            SmallDoorType::Grates => {
                let bars = Block::IRON_BARS.default_state();
                let west = bars.with(properties::WEST, West::True).expect("bars west");
                let east = bars.with(properties::EAST, East::True).expect("bars east");
                let ew = east.with(properties::WEST, West::True).expect("bars west");
                for (state, p) in [
                    (Block::CAVE_AIR.default_state(), [x + 1, y, z]),
                    (Block::CAVE_AIR.default_state(), [x + 1, y + 1, z]),
                    (west, [x, y, z]),
                    (west, [x, y + 1, z]),
                    (ew, [x, y + 2, z]),
                    (ew, [x + 1, y + 2, z]),
                    (ew, [x + 2, y + 2, z]),
                    (east, [x + 2, y + 1, z]),
                    (east, [x + 2, y, z]),
                ] {
                    self.data.place_block(context.region, state, p, bounds)?;
                }
            }
        }
        Ok(())
    }
    fn prison(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        self.smooth(context, bounds, [0, 0, 0, 8, 4, 10], true)?;
        self.small_door(context, bounds, self.door, [1, 1, 0])?;
        self.rect(
            context,
            bounds,
            [1, 1, 10, 3, 3, 10],
            Block::CAVE_AIR.default_state(),
        )?;
        for z in [1, 3, 7, 9] {
            self.smooth(context, bounds, [4, 1, z, 4, 3, z], false)?;
        }
        let ns = connections(Block::IRON_BARS, true, true, false, false);
        let nse = connections(Block::IRON_BARS, true, true, true, false);
        let ew = connections(Block::IRON_BARS, false, false, true, true);
        for y in 1..=3 {
            for (state, p) in [
                (ns, [4, y, 4]),
                (nse, [4, y, 5]),
                (ns, [4, y, 6]),
                (ew, [5, y, 5]),
                (ew, [6, y, 5]),
                (ew, [7, y, 5]),
            ] {
                self.data.place_block(context.region, state, p, bounds)?;
            }
        }
        for z in [2, 8] {
            self.data
                .place_block(context.region, ns, [4, 3, z], bounds)?;
        }
        let door = Block::IRON_DOOR
            .default_state()
            .with(properties::FACING, Facing::West)
            .expect("door facing");
        let upper = door.with(properties::HALF, Half::Upper).expect("door half");
        for z in [2, 8] {
            self.data
                .place_block(context.region, door, [4, 1, z], bounds)?;
            self.data
                .place_block(context.region, upper, [4, 2, z], bounds)?;
        }
        Ok(())
    }
    fn room_crossing(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        style: i32,
    ) -> Result<(), Error> {
        self.smooth(context, bounds, [0, 0, 0, 10, 6, 10], true)?;
        self.small_door(context, bounds, self.door, [4, 1, 0])?;
        let air = Block::CAVE_AIR.default_state();
        let brick = Block::STONE_BRICKS.default_state();
        for r in [
            [4, 1, 10, 6, 3, 10],
            [0, 1, 4, 0, 3, 6],
            [10, 1, 4, 10, 3, 6],
        ] {
            self.rect(context, bounds, r, air)?;
        }
        match style {
            0 => {
                for y in 1..=3 {
                    self.data
                        .place_block(context.region, brick, [5, y, 5], bounds)?;
                }
                for (facing, p) in [
                    (Facing::West, [4, 3, 5]),
                    (Facing::East, [6, 3, 5]),
                    (Facing::South, [5, 3, 4]),
                    (Facing::North, [5, 3, 6]),
                ] {
                    self.data
                        .place_block(context.region, torch(facing), p, bounds)?;
                }
                for p in [
                    [4, 1, 4],
                    [4, 1, 5],
                    [4, 1, 6],
                    [6, 1, 4],
                    [6, 1, 5],
                    [6, 1, 6],
                    [5, 1, 4],
                    [5, 1, 6],
                ] {
                    self.data.place_block(
                        context.region,
                        Block::SMOOTH_STONE_SLAB.default_state(),
                        p,
                        bounds,
                    )?;
                }
            }
            1 => {
                for i in 0..5 {
                    for p in [[3, 1, 3 + i], [7, 1, 3 + i], [3 + i, 1, 3], [3 + i, 1, 7]] {
                        self.data.place_block(context.region, brick, p, bounds)?;
                    }
                }
                for y in 1..=3 {
                    self.data
                        .place_block(context.region, brick, [5, y, 5], bounds)?;
                }
                self.data.place_block(
                    context.region,
                    Block::WATER.default_state(),
                    [5, 4, 5],
                    bounds,
                )?;
            }
            2 => {
                let cobble = Block::COBBLESTONE.default_state();
                for z in 1..=9 {
                    for x in [1, 9] {
                        self.data
                            .place_block(context.region, cobble, [x, 3, z], bounds)?;
                    }
                }
                for x in 1..=9 {
                    for z in [1, 9] {
                        self.data
                            .place_block(context.region, cobble, [x, 3, z], bounds)?;
                    }
                }
                for p in [
                    [5, 1, 4],
                    [5, 1, 6],
                    [5, 3, 4],
                    [5, 3, 6],
                    [4, 1, 5],
                    [6, 1, 5],
                    [4, 3, 5],
                    [6, 3, 5],
                ] {
                    self.data.place_block(context.region, cobble, p, bounds)?;
                }
                for y in 1..=3 {
                    for p in [[4, y, 4], [6, y, 4], [4, y, 6], [6, y, 6]] {
                        self.data.place_block(context.region, cobble, p, bounds)?;
                    }
                }
                self.data.place_block(
                    context.region,
                    Block::WALL_TORCH.default_state(),
                    [5, 3, 5],
                    bounds,
                )?;
                for z in 2..=8 {
                    let planks = Block::OAK_PLANKS.default_state();
                    for x in [2, 3] {
                        self.data
                            .place_block(context.region, planks, [x, 3, z], bounds)?;
                    }
                    if z <= 3 || z >= 7 {
                        for x in 4..=6 {
                            self.data
                                .place_block(context.region, planks, [x, 3, z], bounds)?;
                        }
                    }
                    for x in [7, 8] {
                        self.data
                            .place_block(context.region, planks, [x, 3, z], bounds)?;
                    }
                }
                let ladder = Block::LADDER
                    .default_state()
                    .with(properties::FACING, Facing::West)
                    .expect("ladder facing");
                for y in 1..=3 {
                    self.data
                        .place_block(context.region, ladder, [9, y, 3], bounds)?;
                }
                self.data.create_chest(
                    context,
                    [3, 4, 8],
                    bounds,
                    &"minecraft:chests/stronghold_crossing".parse()?,
                )?;
            }
            _ => {}
        }
        Ok(())
    }
    fn library(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        tall: bool,
    ) -> Result<(), Error> {
        self.smooth(
            context,
            bounds,
            [0, 0, 0, 13, if tall { 10 } else { 5 }, 14],
            true,
        )?;
        self.small_door(context, bounds, self.door, [4, 1, 0])?;
        for y in 1..=4 {
            for x in 2..=11 {
                for z in 1..=13 {
                    if context.random.next_float() <= 0.07 {
                        self.data.place_block(
                            context.region,
                            Block::COBWEB.default_state(),
                            [x, y, z],
                            bounds,
                        )?;
                    }
                }
            }
        }
        let planks = Block::OAK_PLANKS.default_state();
        let books = Block::BOOKSHELF.default_state();
        for z in 1..=13 {
            let pillar = (z - 1) % 4 == 0;
            let block = if pillar { planks } else { books };
            for x in [1, 12] {
                self.rect(context, bounds, [x, 1, z, x, 4, z], block)?;
            }
            if pillar {
                for (x, facing) in [(2, Facing::East), (11, Facing::West)] {
                    self.data
                        .place_block(context.region, torch(facing), [x, 3, z], bounds)?;
                }
            }
            if tall {
                for x in [1, 12] {
                    self.rect(context, bounds, [x, 6, z, x, 9, z], block)?;
                }
            }
        }
        for z in (3..12).step_by(2) {
            for x in [3, 6, 9] {
                self.rect(context, bounds, [x, 1, z, x + 1, 3, z], books)?;
            }
        }
        if tall {
            for r in [
                [1, 5, 1, 3, 5, 13],
                [10, 5, 1, 12, 5, 13],
                [4, 5, 1, 9, 5, 2],
                [4, 5, 12, 9, 5, 13],
            ] {
                self.rect(context, bounds, r, planks)?;
            }
            for p in [[9, 5, 11], [8, 5, 11], [9, 5, 10]] {
                self.data.place_block(context.region, planks, p, bounds)?;
            }
            let ns = connections(Block::OAK_FENCE, true, true, false, false);
            let ew = connections(Block::OAK_FENCE, false, false, true, true);
            for (state, r) in [
                (ns, [3, 6, 3, 3, 6, 11]),
                (ns, [10, 6, 3, 10, 6, 9]),
                (ew, [4, 6, 2, 9, 6, 2]),
                (ew, [4, 6, 12, 7, 6, 12]),
            ] {
                self.rect(context, bounds, r, state)?;
            }
            for (state, p) in [
                (
                    connections(Block::OAK_FENCE, true, false, true, false),
                    [3, 6, 2],
                ),
                (
                    connections(Block::OAK_FENCE, false, true, true, false),
                    [3, 6, 12],
                ),
                (
                    connections(Block::OAK_FENCE, true, false, false, true),
                    [10, 6, 2],
                ),
            ] {
                self.data.place_block(context.region, state, p, bounds)?;
            }
            for i in 0..=2 {
                self.data.place_block(
                    context.region,
                    connections(Block::OAK_FENCE, false, true, false, true),
                    [8 + i, 6, 12 - i],
                    bounds,
                )?;
                if i != 2 {
                    self.data.place_block(
                        context.region,
                        connections(Block::OAK_FENCE, true, false, true, false),
                        [8 + i, 6, 11 - i],
                        bounds,
                    )?;
                }
            }
            let ladder = Block::LADDER
                .default_state()
                .with(properties::FACING, Facing::South)
                .expect("ladder facing");
            for y in 1..=7 {
                self.data
                    .place_block(context.region, ladder, [10, y, 13], bounds)?;
            }
            let east = connections(Block::OAK_FENCE, false, false, true, false);
            let west = connections(Block::OAK_FENCE, false, false, false, true);
            let all = connections(Block::OAK_FENCE, true, true, true, true);
            for (state, p) in [
                (east, [6, 9, 7]),
                (west, [7, 9, 7]),
                (east, [6, 8, 7]),
                (west, [7, 8, 7]),
                (all, [6, 7, 7]),
                (all, [7, 7, 7]),
                (east, [5, 7, 7]),
                (west, [8, 7, 7]),
                (
                    connections(Block::OAK_FENCE, true, false, true, false),
                    [6, 7, 6],
                ),
                (
                    connections(Block::OAK_FENCE, false, true, true, false),
                    [6, 7, 8],
                ),
                (
                    connections(Block::OAK_FENCE, true, false, false, true),
                    [7, 7, 6],
                ),
                (
                    connections(Block::OAK_FENCE, false, true, false, true),
                    [7, 7, 8],
                ),
            ] {
                self.data.place_block(context.region, state, p, bounds)?;
            }
            for p in [
                [5, 8, 7],
                [8, 8, 7],
                [6, 8, 6],
                [6, 8, 8],
                [7, 8, 6],
                [7, 8, 8],
            ] {
                self.data
                    .place_block(context.region, Block::TORCH.default_state(), p, bounds)?;
            }
        }
        let loot = "minecraft:chests/stronghold_library".parse()?;
        self.data.create_chest(context, [3, 3, 5], bounds, &loot)?;
        if tall {
            self.data.place_block(
                context.region,
                Block::CAVE_AIR.default_state(),
                [12, 9, 1],
                bounds,
            )?;
            self.data.create_chest(context, [12, 8, 1], bounds, &loot)?;
        }
        Ok(())
    }
    fn portal_room(
        &mut self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        placed_spawner: bool,
    ) -> Result<(), Error> {
        self.smooth(context, bounds, [0, 0, 0, 10, 7, 15], false)?;
        self.small_door(context, bounds, SmallDoorType::Grates, [4, 1, 0])?;
        for r in [
            [1, 6, 1, 1, 6, 14],
            [9, 6, 1, 9, 6, 14],
            [2, 6, 1, 8, 6, 2],
            [2, 6, 14, 8, 6, 14],
            [1, 1, 1, 2, 1, 4],
            [8, 1, 1, 9, 1, 4],
        ] {
            self.smooth(context, bounds, r, false)?;
        }
        for r in [[1, 1, 1, 1, 1, 3], [9, 1, 1, 9, 1, 3]] {
            self.rect(context, bounds, r, Block::LAVA.default_state())?;
        }
        self.smooth(context, bounds, [3, 1, 8, 7, 1, 12], false)?;
        self.rect(
            context,
            bounds,
            [4, 1, 9, 6, 1, 11],
            Block::LAVA.default_state(),
        )?;
        let ns = connections(Block::IRON_BARS, true, true, false, false);
        let ew = connections(Block::IRON_BARS, false, false, true, true);
        for z in (3..14).step_by(2) {
            for x in [0, 10] {
                self.rect(context, bounds, [x, 3, z, x, 4, z], ns)?;
            }
        }
        for x in (2..9).step_by(2) {
            self.rect(context, bounds, [x, 3, 15, x, 4, 15], ew)?;
        }
        for r in [[4, 1, 5, 6, 1, 7], [4, 2, 6, 6, 2, 7], [4, 3, 7, 6, 3, 7]] {
            self.smooth(context, bounds, r, false)?;
        }
        let stairs = Block::STONE_BRICK_STAIRS
            .default_state()
            .with(properties::FACING, Facing::North)
            .expect("stair facing");
        for x in 4..=6 {
            for (y, z) in [(1, 4), (2, 5), (3, 6)] {
                self.data
                    .place_block(context.region, stairs, [x, y, z], bounds)?;
            }
        }
        let eyes = std::array::from_fn::<_, 12, _>(|_| context.random.next_float() > 0.9);
        self.portal_frames(context, bounds, eyes)?;
        let p = self.data.world_position([5, 3, 6]);
        if !placed_spawner && bounds.contains(p) {
            if let StrongholdPieceKind::PortalRoom { placed_spawner } = &mut self.kind {
                *placed_spawner = true;
            }
            context
                .region
                .set_block(p, Block::SPAWNER.default_state(), 2)?;
            context.region.set_spawner_entity(
                p,
                ferraloom_data::EntityType::SILVERFISH,
                context.random,
            )?;
        }
        Ok(())
    }
    fn portal_frames(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        eyes: [bool; 12],
    ) -> Result<(), Error> {
        let positions = [
            (Facing::North, [4, 3, 8]),
            (Facing::North, [5, 3, 8]),
            (Facing::North, [6, 3, 8]),
            (Facing::South, [4, 3, 12]),
            (Facing::South, [5, 3, 12]),
            (Facing::South, [6, 3, 12]),
            (Facing::East, [3, 3, 9]),
            (Facing::East, [3, 3, 10]),
            (Facing::East, [3, 3, 11]),
            (Facing::West, [7, 3, 9]),
            (Facing::West, [7, 3, 10]),
            (Facing::West, [7, 3, 11]),
        ];
        for ((facing, p), eye) in positions.into_iter().zip(eyes) {
            let state = Block::END_PORTAL_FRAME
                .default_state()
                .with(properties::FACING, facing)
                .expect("frame facing")
                .with(properties::EYE, eye)
                .expect("frame eye");
            self.data.place_block(context.region, state, p, bounds)?;
        }
        if eyes.into_iter().all(|eye| eye) {
            for z in 9..=11 {
                for x in 4..=6 {
                    self.data.place_block(
                        context.region,
                        Block::END_PORTAL.default_state(),
                        [x, 3, z],
                        bounds,
                    )?;
                }
            }
        }
        Ok(())
    }
}
fn torch(facing: Facing) -> BlockState {
    Block::WALL_TORCH
        .default_state()
        .with(properties::FACING, facing)
        .expect("torch facing")
}
fn connections(block: Block, north: bool, south: bool, east: bool, west: bool) -> BlockState {
    block
        .default_state()
        .with(
            properties::NORTH,
            if north { North::True } else { North::False },
        )
        .expect("north connection")
        .with(
            properties::SOUTH,
            if south { South::True } else { South::False },
        )
        .expect("south connection")
        .with(
            properties::EAST,
            if east { East::True } else { East::False },
        )
        .expect("east connection")
        .with(
            properties::WEST,
            if west { West::True } else { West::False },
        )
        .expect("west connection")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        feature::FeatureRegistry,
        region::{ChunkStatus, RegionChunk, WorldgenBlockBehavior},
        structure::processor::tests::generator,
    };
    use ferraloom_util::random::{Algorithm, WorldgenRandom};
    use serde_json::Value;
    use std::io::Read;
    // This matches the Java world's explicit null block-entity accessor. Pending
    // chunk NBT does not instantiate a container in either controlled fixture.
    struct NoContainers;
    impl WorldgenBlockBehavior for NoContainers {
        fn has_block_entity(&self, _state: BlockState) -> bool {
            false
        }
        fn post_process_pos(
            &self,
            _state: BlockState,
            _region: &WorldGenRegion<'_>,
            _p: [i32; 3],
        ) -> Result<Option<[i32; 3]>, Error> {
            Ok(None)
        }
        fn set_loot_table(
            &self,
            _region: &mut WorldGenRegion<'_>,
            _p: [i32; 3],
            _table: &crate::Identifier,
            _random: &mut WorldgenRandom,
        ) -> Result<(), Error> {
            Ok(())
        }
    }
    fn point(v: &Value) -> [i32; 3] {
        std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
    }
    fn golden(bytes: &[u8]) -> Value {
        let mut text = String::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_string(&mut text)
            .unwrap();
        serde_json::from_str(&text).unwrap()
    }
    #[test]
    fn stronghold_saved_piece_doors_stone_selection_layout_clipping_and_rng_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let cases = golden(include_bytes!("../../testdata/stronghold_blocks.json.gz"));
        for case in cases.as_array().unwrap() {
            let initial = ferraloom_nbt::parse_compound(case["initial"].as_str().unwrap()).unwrap();
            let mode = case["mode"].as_i64().unwrap();
            let mut piece =
                StrongholdPiece::from_nbt(case["id"].as_str().unwrap(), &initial).unwrap();
            let mut chunk = generator.new_chunk([0, 0]);
            for z in 0..16 {
                for x in 0..16 {
                    for y in -64..=100 {
                        let v: i32 = x * 3 + y * 5 + z * 7;
                        let block = if mode == 1 && (50..=65).contains(&y) && v.rem_euclid(11) == 0
                        {
                            Block::AIR
                        } else if mode == 1 && (50..=65).contains(&y) && v.rem_euclid(17) == 0 {
                            Block::WATER
                        } else {
                            Block::STONE
                        };
                        chunk.set_block(x as u8, y, z as u8, block.default_state());
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
                &NoContainers,
                0,
                63,
            )
            .unwrap();
            let b = &case["bounds"];
            let bounds = BoundingBox::new(
                point(b),
                std::array::from_fn(|i| b[i + 3].as_i64().unwrap() as i32),
            );
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(17));
            piece
                .post_process(
                    &mut StructurePlacementContext {
                        region: &mut region,
                        random: &mut random,
                        origin: [0; 3],
                        generator: &generator,
                        registry: &features,
                    },
                    bounds,
                )
                .unwrap();
            assert_eq!(
                piece.to_nbt(),
                ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap(),
                "{initial:?}"
            );
            assert_eq!(
                random.next_int(),
                case["next"].as_i64().unwrap() as i32,
                "{initial:?} mode={mode} {bounds:?}"
            );
            let chunk = region.chunk([0, 0], ChunkStatus::Terrain).unwrap();
            assert_writes(
                &chunk.poi_changes,
                case,
                &format!("{initial:?} mode={mode} {bounds:?}"),
            );
            let mut post = chunk.chunk.post_processing.clone();
            post.sort_by_key(|p| p[1] >> 4);
            let expected = case["post"]
                .as_array()
                .unwrap()
                .iter()
                .map(point)
                .collect::<Vec<_>>();
            assert_eq!(post, expected, "{initial:?} mode={mode} {bounds:?}");
            let actual = chunk
                .fluid_ticks
                .ticks()
                .iter()
                .map(|t| {
                    serde_json::json!([
                        t.position[0],
                        t.position[1],
                        t.position[2],
                        t.kind.protocol_id(),
                        t.priority
                    ])
                })
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                *case["fluid_ticks"].as_array().unwrap(),
                "{initial:?} mode={mode} {bounds:?}"
            );
        }
    }
    fn assert_writes(actual: &[([i32; 3], BlockState, BlockState)], case: &Value, label: &str) {
        let expected = case["writes"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "{label}");
        for (i, ((p, _, state), expected)) in actual.iter().zip(expected).enumerate() {
            assert_eq!(
                (*p, state.id()),
                (
                    point(&expected["pos"]),
                    expected["state"].as_u64().unwrap() as u16
                ),
                "{label} write={i}"
            );
        }
    }
    #[test]
    fn portal_frame_orientations_and_all_eyes_fill_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let cases = golden(include_bytes!(
            "../../testdata/stronghold_portal_frames.json.gz"
        ));
        for case in cases.as_array().unwrap() {
            let initial = ferraloom_nbt::parse_compound(case["initial"].as_str().unwrap()).unwrap();
            let piece = StrongholdPiece::from_nbt(case["id"].as_str().unwrap(), &initial).unwrap();
            let mut chunks = [RegionChunk::new(
                generator.new_chunk([0, 0]),
                ChunkStatus::Terrain,
            )];
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [0, 0],
                &[ChunkStatus::Terrain],
                0,
                None,
                &NoContainers,
                0,
                63,
            )
            .unwrap();
            let b = &case["bounds"];
            let bounds = BoundingBox::new(
                point(b),
                std::array::from_fn(|i| b[i + 3].as_i64().unwrap() as i32),
            );
            let mut random = WorldgenRandom::new(Algorithm::Legacy.new_instance(17));
            piece
                .portal_frames(
                    &mut StructurePlacementContext {
                        region: &mut region,
                        random: &mut random,
                        origin: [0; 3],
                        generator: &generator,
                        registry: &features,
                    },
                    bounds,
                    [true; 12],
                )
                .unwrap();
            assert_writes(
                &region
                    .chunk([0, 0], ChunkStatus::Terrain)
                    .unwrap()
                    .poi_changes,
                case,
                &format!("{initial:?}"),
            );
        }
    }
}
