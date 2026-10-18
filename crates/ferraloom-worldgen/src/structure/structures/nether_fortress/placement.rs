use super::{FortressPiece, FortressPieceKind};
use crate::{
    Error, region::SavedTick, structure::BoundingBox, structure::StructurePlacementContext,
};
use ferraloom_data::{
    Block, BlockState,
    properties::{self, East, Facing, North, South, West},
};
use ferraloom_util::random::{LegacyRandomSource, RandomSource};

impl FortressPiece {
    /// Port of the fortress pieces' postProcess methods. Persisted chest and
    /// spawner flags change before synchronous game-backed entity access.
    pub fn post_process(
        &mut self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let bricks = Block::NETHER_BRICKS.default_state();
        let air = Block::AIR.default_state();
        let ns = fence(true, true, false, false);
        let ew = fence(false, false, true, true);
        match self.kind.clone() {
            FortressPieceKind::BridgeEndFiller { seed } => {
                let mut own = LegacyRandomSource::new(i64::from(seed));
                for x in 0..=4 {
                    for y in 3..=4 {
                        self.rect(
                            context,
                            bounds,
                            [x, y, 0, x, y, own.next_int_bound(8)],
                            bricks,
                        )?;
                    }
                }
                for x in [0, 4] {
                    self.rect(
                        context,
                        bounds,
                        [x, 5, 0, x, 5, own.next_int_bound(8)],
                        bricks,
                    )?;
                }
                for x in 0..=4 {
                    self.rect(
                        context,
                        bounds,
                        [x, 2, 0, x, 2, own.next_int_bound(5)],
                        bricks,
                    )?;
                }
                for x in 0..=4 {
                    for y in 0..=1 {
                        self.rect(
                            context,
                            bounds,
                            [x, y, 0, x, y, own.next_int_bound(3)],
                            bricks,
                        )?;
                    }
                }
            }
            FortressPieceKind::BridgeStraight => {
                self.rects(
                    context,
                    bounds,
                    [
                        (bricks, [0, 3, 0, 4, 4, 18]),
                        (air, [1, 5, 0, 3, 7, 18]),
                        (bricks, [0, 5, 0, 0, 5, 18]),
                        (bricks, [4, 5, 0, 4, 5, 18]),
                        (bricks, [0, 2, 0, 4, 2, 5]),
                        (bricks, [0, 2, 13, 4, 2, 18]),
                        (bricks, [0, 0, 0, 4, 1, 3]),
                        (bricks, [0, 0, 15, 4, 1, 18]),
                    ],
                )?;
                for x in 0..=4 {
                    for z in 0..=2 {
                        for z in [z, 18 - z] {
                            self.foundation(context, bounds, [x, -1, z])?;
                        }
                    }
                }
                for (x, state) in [
                    (0, fence(true, true, true, false)),
                    (4, fence(true, true, false, true)),
                ] {
                    for [bottom, z] in [[1, 1], [3, 4], [3, 14], [1, 17]] {
                        self.rect(context, bounds, [x, bottom, z, x, 4, z], state)?;
                    }
                }
            }
            FortressPieceKind::Start | FortressPieceKind::BridgeCrossing => {
                self.rects(
                    context,
                    bounds,
                    [
                        (bricks, [7, 3, 0, 11, 4, 18]),
                        (bricks, [0, 3, 7, 18, 4, 11]),
                        (air, [8, 5, 0, 10, 7, 18]),
                        (air, [0, 5, 8, 18, 7, 10]),
                    ],
                )?;
                for rect in [
                    [7, 5, 0, 7, 5, 7],
                    [7, 5, 11, 7, 5, 18],
                    [11, 5, 0, 11, 5, 7],
                    [11, 5, 11, 11, 5, 18],
                    [0, 5, 7, 7, 5, 7],
                    [11, 5, 7, 18, 5, 7],
                    [0, 5, 11, 7, 5, 11],
                    [11, 5, 11, 18, 5, 11],
                    [7, 2, 0, 11, 2, 5],
                    [7, 2, 13, 11, 2, 18],
                    [7, 0, 0, 11, 1, 3],
                    [7, 0, 15, 11, 1, 18],
                ] {
                    self.rect(context, bounds, rect, bricks)?;
                }
                for x in 7..=11 {
                    for z in 0..=2 {
                        for z in [z, 18 - z] {
                            self.foundation(context, bounds, [x, -1, z])?;
                        }
                    }
                }
                for rect in [
                    [0, 2, 7, 5, 2, 11],
                    [13, 2, 7, 18, 2, 11],
                    [0, 0, 7, 3, 1, 11],
                    [15, 0, 7, 18, 1, 11],
                ] {
                    self.rect(context, bounds, rect, bricks)?;
                }
                for x in 0..=2 {
                    for z in 7..=11 {
                        for x in [x, 18 - x] {
                            self.foundation(context, bounds, [x, -1, z])?;
                        }
                    }
                }
            }
            FortressPieceKind::CastleSmallCorridor
            | FortressPieceKind::CastleCrossing
            | FortressPieceKind::CastleLeftTurn { .. }
            | FortressPieceKind::CastleRightTurn { .. } => {
                self.rects(
                    context,
                    bounds,
                    [(bricks, [0, 0, 0, 4, 1, 4]), (air, [0, 2, 0, 4, 5, 4])],
                )?;
                match self.kind.clone() {
                    FortressPieceKind::CastleCrossing => {
                        for rect in [
                            [0, 2, 0, 0, 5, 0],
                            [4, 2, 0, 4, 5, 0],
                            [0, 2, 4, 0, 5, 4],
                            [4, 2, 4, 4, 5, 4],
                        ] {
                            self.rect(context, bounds, rect, bricks)?;
                        }
                    }
                    FortressPieceKind::CastleSmallCorridor => {
                        for x in [0, 4] {
                            self.rect(context, bounds, [x, 2, 0, x, 5, 4], bricks)?;
                        }
                        for x in [0, 4] {
                            for z in [1, 3] {
                                self.rect(context, bounds, [x, 3, z, x, 4, z], ns)?;
                            }
                        }
                    }
                    FortressPieceKind::CastleLeftTurn { needs_chest }
                    | FortressPieceKind::CastleRightTurn { needs_chest } => {
                        let left = matches!(self.kind, FortressPieceKind::CastleLeftTurn { .. });
                        let wall = if left { 4 } else { 0 };
                        let open = 4 - wall;
                        self.rect(context, bounds, [wall, 2, 0, wall, 5, 4], bricks)?;
                        for z in [1, 3] {
                            self.rect(context, bounds, [wall, 3, z, wall, 4, z], ns)?;
                        }
                        self.rect(context, bounds, [open, 2, 0, open, 5, 0], bricks)?;
                        self.rect(
                            context,
                            bounds,
                            [
                                if left { 0 } else { 1 },
                                2,
                                4,
                                if left { 3 } else { 4 },
                                5,
                                4,
                            ],
                            bricks,
                        )?;
                        for x in [1, 3] {
                            self.rect(context, bounds, [x, 3, 4, x, 4, 4], ew)?;
                        }
                        let p = [if left { 3 } else { 1 }, 2, 3];
                        if needs_chest && bounds.contains(self.data.world_position(p)) {
                            match &mut self.kind {
                                FortressPieceKind::CastleLeftTurn { needs_chest }
                                | FortressPieceKind::CastleRightTurn { needs_chest } => {
                                    *needs_chest = false
                                }
                                _ => unreachable!(),
                            }
                            self.data.create_chest(
                                context,
                                p,
                                bounds,
                                &"minecraft:chests/nether_bridge".parse()?,
                            )?;
                        }
                    }
                    _ => unreachable!("small corridor"),
                }
                self.rect(context, bounds, [0, 6, 0, 4, 6, 4], bricks)?;
                self.foundation_rectangle(context, bounds, [0, 0], [4, 4])?;
            }
            FortressPieceKind::CastleStairs => {
                let stairs = stairs(Facing::South);
                for z in 0..=9 {
                    let floor = 1.max(7 - z);
                    let roof = (floor + 5).max(14 - z).min(13);
                    self.rects(
                        context,
                        bounds,
                        [
                            (bricks, [0, 0, z, 4, floor, z]),
                            (air, [1, floor + 1, z, 3, roof - 1, z]),
                        ],
                    )?;
                    if z <= 6 {
                        for x in 1..=3 {
                            self.data.place_block(
                                context.region,
                                stairs,
                                [x, floor + 1, z],
                                bounds,
                            )?;
                        }
                    }
                    self.rects(
                        context,
                        bounds,
                        [
                            (bricks, [0, roof, z, 4, roof, z]),
                            (bricks, [0, floor + 1, z, 0, roof - 1, z]),
                            (bricks, [4, floor + 1, z, 4, roof - 1, z]),
                        ],
                    )?;
                    if z & 1 == 0 {
                        for x in [0, 4] {
                            self.rect(context, bounds, [x, floor + 2, z, x, floor + 3, z], ns)?;
                        }
                    }
                    for x in 0..=4 {
                        self.foundation(context, bounds, [x, -1, z])?;
                    }
                }
            }
            FortressPieceKind::CastleBalcony => {
                self.rects(
                    context,
                    bounds,
                    [
                        (bricks, [0, 0, 0, 8, 1, 8]),
                        (air, [0, 2, 0, 8, 5, 8]),
                        (bricks, [0, 6, 0, 8, 6, 5]),
                        (bricks, [0, 2, 0, 2, 5, 0]),
                        (bricks, [6, 2, 0, 8, 5, 0]),
                        (ew, [1, 3, 0, 1, 4, 0]),
                        (ew, [7, 3, 0, 7, 4, 0]),
                        (bricks, [0, 2, 4, 8, 2, 8]),
                        (air, [1, 1, 4, 2, 2, 4]),
                        (air, [6, 1, 4, 7, 2, 4]),
                        (ew, [1, 3, 8, 7, 3, 8]),
                    ],
                )?;
                for (state, p) in [
                    (fence(false, true, true, false), [0, 3, 8]),
                    (fence(false, true, false, true), [8, 3, 8]),
                ] {
                    self.data.place_block(context.region, state, p, bounds)?;
                }
                self.rects(
                    context,
                    bounds,
                    [
                        (ns, [0, 3, 6, 0, 3, 7]),
                        (ns, [8, 3, 6, 8, 3, 7]),
                        (bricks, [0, 3, 4, 0, 5, 5]),
                        (bricks, [8, 3, 4, 8, 5, 5]),
                        (bricks, [1, 3, 5, 2, 5, 5]),
                        (bricks, [6, 3, 5, 7, 5, 5]),
                        (ew, [1, 4, 5, 1, 5, 5]),
                        (ew, [7, 4, 5, 7, 5, 5]),
                    ],
                )?;
                for z in 0..=5 {
                    for x in 0..=8 {
                        self.foundation(context, bounds, [x, -1, z])?;
                    }
                }
            }
            FortressPieceKind::RoomCrossing => {
                self.rects(
                    context,
                    bounds,
                    [(bricks, [0, 0, 0, 6, 1, 6]), (air, [0, 2, 0, 6, 7, 6])],
                )?;
                for rect in [
                    [0, 2, 0, 1, 6, 0],
                    [0, 2, 6, 1, 6, 6],
                    [5, 2, 0, 6, 6, 0],
                    [5, 2, 6, 6, 6, 6],
                    [0, 2, 0, 0, 6, 1],
                    [0, 2, 5, 0, 6, 6],
                    [6, 2, 0, 6, 6, 1],
                    [6, 2, 5, 6, 6, 6],
                ] {
                    self.rect(context, bounds, rect, bricks)?;
                }
                self.rects(
                    context,
                    bounds,
                    [
                        (bricks, [2, 6, 0, 4, 6, 0]),
                        (ew, [2, 5, 0, 4, 5, 0]),
                        (bricks, [2, 6, 6, 4, 6, 6]),
                        (ew, [2, 5, 6, 4, 5, 6]),
                        (bricks, [0, 6, 2, 0, 6, 4]),
                        (ns, [0, 5, 2, 0, 5, 4]),
                        (bricks, [6, 6, 2, 6, 6, 4]),
                        (ns, [6, 5, 2, 6, 5, 4]),
                    ],
                )?;
                self.foundation_rectangle(context, bounds, [0, 0], [6, 6])?;
            }
            FortressPieceKind::StairsRoom => {
                self.rects(
                    context,
                    bounds,
                    [
                        (bricks, [0, 0, 0, 6, 1, 6]),
                        (air, [0, 2, 0, 6, 10, 6]),
                        (bricks, [0, 2, 0, 1, 8, 0]),
                        (bricks, [5, 2, 0, 6, 8, 0]),
                        (bricks, [0, 2, 1, 0, 8, 6]),
                        (bricks, [6, 2, 1, 6, 8, 6]),
                        (bricks, [1, 2, 6, 5, 8, 6]),
                        (ns, [0, 3, 2, 0, 5, 4]),
                        (ns, [6, 3, 2, 6, 5, 2]),
                        (ns, [6, 3, 4, 6, 5, 4]),
                    ],
                )?;
                self.data
                    .place_block(context.region, bricks, [5, 2, 5], bounds)?;
                for (x, top) in [(4, 3), (3, 4), (2, 5), (1, 6)] {
                    self.rect(context, bounds, [x, 2, 5, x, top, 5], bricks)?;
                }
                self.rects(
                    context,
                    bounds,
                    [
                        (bricks, [1, 7, 1, 5, 7, 4]),
                        (air, [6, 8, 2, 6, 8, 4]),
                        (bricks, [2, 6, 0, 4, 8, 0]),
                        (ew, [2, 5, 0, 4, 5, 0]),
                    ],
                )?;
                self.foundation_rectangle(context, bounds, [0, 0], [6, 6])?;
            }
            FortressPieceKind::MonsterThrone { placed_spawner } => {
                self.rect(context, bounds, [0, 2, 0, 6, 7, 7], air)?;
                for rect in [
                    [1, 0, 0, 5, 1, 7],
                    [1, 2, 1, 5, 2, 7],
                    [1, 3, 2, 5, 3, 7],
                    [1, 4, 3, 5, 4, 7],
                    [1, 2, 0, 1, 4, 2],
                    [5, 2, 0, 5, 4, 2],
                    [1, 5, 2, 1, 5, 3],
                    [5, 5, 2, 5, 5, 3],
                    [0, 5, 3, 0, 5, 8],
                    [6, 5, 3, 6, 5, 8],
                    [1, 5, 8, 5, 5, 8],
                ] {
                    self.rect(context, bounds, rect, bricks)?;
                }
                for (state, p) in [
                    (fence(false, false, false, true), [1, 6, 3]),
                    (fence(false, false, true, false), [5, 6, 3]),
                    (fence(true, false, true, false), [0, 6, 3]),
                    (fence(true, false, false, true), [6, 6, 3]),
                ] {
                    self.data.place_block(context.region, state, p, bounds)?;
                }
                self.rects(
                    context,
                    bounds,
                    [(ns, [0, 6, 4, 0, 6, 7]), (ns, [6, 6, 4, 6, 6, 7])],
                )?;
                for (state, p) in [
                    (fence(false, true, true, false), [0, 6, 8]),
                    (fence(false, true, false, true), [6, 6, 8]),
                ] {
                    self.data.place_block(context.region, state, p, bounds)?;
                }
                self.rect(context, bounds, [1, 6, 8, 5, 6, 8], ew)?;
                self.data.place_block(
                    context.region,
                    fence(false, false, true, false),
                    [1, 7, 8],
                    bounds,
                )?;
                self.rect(context, bounds, [2, 7, 8, 4, 7, 8], ew)?;
                for (state, p) in [
                    (fence(false, false, false, true), [5, 7, 8]),
                    (fence(false, false, true, false), [2, 8, 8]),
                    (ew, [3, 8, 8]),
                    (fence(false, false, false, true), [4, 8, 8]),
                ] {
                    self.data.place_block(context.region, state, p, bounds)?;
                }
                let p = self.data.world_position([3, 5, 5]);
                if !placed_spawner && bounds.contains(p) {
                    if let FortressPieceKind::MonsterThrone { placed_spawner } = &mut self.kind {
                        *placed_spawner = true;
                    }
                    context
                        .region
                        .set_block(p, Block::SPAWNER.default_state(), 2)?;
                    context.region.set_spawner_entity(
                        p,
                        ferraloom_data::EntityType::BLAZE,
                        context.random,
                    )?;
                }
                self.foundation_rectangle(context, bounds, [0, 0], [6, 6])?;
            }
            FortressPieceKind::CastleEntrance | FortressPieceKind::CastleStalkRoom => self
                .large_room(
                    context,
                    bounds,
                    matches!(self.kind, FortressPieceKind::CastleStalkRoom),
                )?,
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
    fn rects(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        rects: impl IntoIterator<Item = (BlockState, [i32; 6])>,
    ) -> Result<(), Error> {
        for (state, rect) in rects {
            self.rect(context, bounds, rect, state)?;
        }
        Ok(())
    }
    fn foundation(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        p: [i32; 3],
    ) -> Result<(), Error> {
        self.data.fill_column_down(
            context.region,
            Block::NETHER_BRICKS.default_state(),
            p,
            bounds,
        )
    }
    fn foundation_rectangle(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        min: [i32; 2],
        max: [i32; 2],
    ) -> Result<(), Error> {
        for x in min[0]..=max[0] {
            for z in min[1]..=max[1] {
                self.foundation(context, bounds, [x, -1, z])?;
            }
        }
        Ok(())
    }
    fn large_room(
        &self,
        context: &mut StructurePlacementContext<'_, '_>,
        bounds: BoundingBox,
        stalk: bool,
    ) -> Result<(), Error> {
        let bricks = Block::NETHER_BRICKS.default_state();
        let air = Block::AIR.default_state();
        let ns = fence(true, true, false, false);
        let ew = fence(false, false, true, true);
        self.rects(
            context,
            bounds,
            [(bricks, [0, 3, 0, 12, 4, 12]), (air, [0, 5, 0, 12, 13, 12])],
        )?;
        for rect in [
            [0, 5, 0, 1, 12, 12],
            [11, 5, 0, 12, 12, 12],
            [2, 5, 11, 4, 12, 12],
            [8, 5, 11, 10, 12, 12],
            [5, 9, 11, 7, 12, 12],
            [2, 5, 0, 4, 12, 1],
            [8, 5, 0, 10, 12, 1],
            [5, 9, 0, 7, 12, 1],
            [2, 11, 2, 10, 12, 10],
        ] {
            self.rect(context, bounds, rect, bricks)?;
        }
        if !stalk {
            self.rect(
                context,
                bounds,
                [5, 8, 0, 7, 8, 0],
                Block::NETHER_BRICK_FENCE.default_state(),
            )?;
        }
        for i in (1..=11).step_by(2) {
            self.rects(
                context,
                bounds,
                [
                    (ew, [i, 10, 0, i, 11, 0]),
                    (ew, [i, 10, 12, i, 11, 12]),
                    (ns, [0, 10, i, 0, 11, i]),
                    (ns, [12, 10, i, 12, 11, i]),
                ],
            )?;
            for p in [[i, 13, 0], [i, 13, 12], [0, 13, i], [12, 13, i]] {
                self.data.place_block(context.region, bricks, p, bounds)?;
            }
            if i != 11 {
                for (state, p) in [
                    (ew, [i + 1, 13, 0]),
                    (ew, [i + 1, 13, 12]),
                    (ns, [0, 13, i + 1]),
                    (ns, [12, 13, i + 1]),
                ] {
                    self.data.place_block(context.region, state, p, bounds)?;
                }
            }
        }
        for (state, p) in [
            (fence(true, false, true, false), [0, 13, 0]),
            (fence(false, true, true, false), [0, 13, 12]),
            (fence(false, true, false, true), [12, 13, 12]),
            (fence(true, false, false, true), [12, 13, 0]),
        ] {
            self.data.place_block(context.region, state, p, bounds)?;
        }
        let nsw = fence(true, true, false, true);
        let nse = fence(true, true, true, false);
        for z in (3..=9).step_by(2) {
            self.rects(
                context,
                bounds,
                [(nsw, [1, 7, z, 1, 8, z]), (nse, [11, 7, z, 11, 8, z])],
            )?;
        }
        if stalk {
            for i in 0..=6 {
                let z = i + 4;
                for x in 5..=7 {
                    self.data.place_block(
                        context.region,
                        stairs(Facing::North),
                        [x, 5 + i, z],
                        bounds,
                    )?;
                }
                if (5..=8).contains(&z) {
                    self.rect(context, bounds, [5, 5, z, 7, i + 4, z], bricks)?;
                } else if (9..=10).contains(&z) {
                    self.rect(context, bounds, [5, 8, z, 7, i + 4, z], bricks)?;
                }
                if i >= 1 {
                    self.rect(context, bounds, [5, 6 + i, z, 7, 9 + i, z], air)?;
                }
            }
            for x in 5..=7 {
                self.data.place_block(
                    context.region,
                    stairs(Facing::North),
                    [x, 12, 11],
                    bounds,
                )?;
            }
            self.rects(
                context,
                bounds,
                [
                    (nse, [5, 6, 7, 5, 7, 7]),
                    (nsw, [7, 6, 7, 7, 7, 7]),
                    (air, [5, 13, 12, 7, 13, 12]),
                ],
            )?;
            for rect in [
                [2, 5, 2, 3, 5, 3],
                [2, 5, 9, 3, 5, 10],
                [2, 5, 4, 2, 5, 8],
                [9, 5, 2, 10, 5, 3],
                [9, 5, 9, 10, 5, 10],
                [10, 5, 4, 10, 5, 8],
            ] {
                self.rect(context, bounds, rect, bricks)?;
            }
            for (x, facing) in [(4, Facing::West), (8, Facing::East)] {
                for z in [2, 3, 9, 10] {
                    self.data
                        .place_block(context.region, stairs(facing), [x, 5, z], bounds)?;
                }
            }
            self.rects(
                context,
                bounds,
                [
                    (Block::SOUL_SAND.default_state(), [3, 4, 4, 4, 4, 8]),
                    (Block::SOUL_SAND.default_state(), [8, 4, 4, 9, 4, 8]),
                    (Block::NETHER_WART.default_state(), [3, 5, 4, 4, 5, 8]),
                    (Block::NETHER_WART.default_state(), [8, 5, 4, 9, 5, 8]),
                ],
            )?;
        }
        for rect in [
            [4, 2, 0, 8, 2, 12],
            [0, 2, 4, 12, 2, 8],
            [4, 0, 0, 8, 1, 3],
            [4, 0, 9, 8, 1, 12],
            [0, 0, 4, 3, 1, 8],
            [9, 0, 4, 12, 1, 8],
        ] {
            self.rect(context, bounds, rect, bricks)?;
        }
        for x in 4..=8 {
            for z in 0..=2 {
                for z in [z, 12 - z] {
                    self.foundation(context, bounds, [x, -1, z])?;
                }
            }
        }
        for x in 0..=2 {
            for z in 4..=8 {
                for x in [x, 12 - x] {
                    self.foundation(context, bounds, [x, -1, z])?;
                }
            }
        }
        if !stalk {
            self.rects(
                context,
                bounds,
                [(bricks, [5, 5, 5, 7, 5, 7]), (air, [6, 1, 6, 6, 4, 6])],
            )?;
            self.data
                .place_block(context.region, bricks, [6, 0, 6], bounds)?;
            let lava = Block::LAVA.default_state();
            self.data
                .place_block(context.region, lava, [6, 5, 6], bounds)?;
            let p = self.data.world_position([6, 5, 6]);
            if bounds.contains(p) {
                context.region.schedule_fluid_tick(SavedTick {
                    position: p,
                    kind: lava.data().fluid.fluid_type,
                    priority: 0,
                })?;
            }
        }
        Ok(())
    }
}
fn fence(north: bool, south: bool, east: bool, west: bool) -> BlockState {
    Block::NETHER_BRICK_FENCE
        .default_state()
        .with(
            properties::NORTH,
            if north { North::True } else { North::False },
        )
        .expect("fence north")
        .with(
            properties::SOUTH,
            if south { South::True } else { South::False },
        )
        .expect("fence south")
        .with(
            properties::EAST,
            if east { East::True } else { East::False },
        )
        .expect("fence east")
        .with(
            properties::WEST,
            if west { West::True } else { West::False },
        )
        .expect("fence west")
}
fn stairs(facing: Facing) -> BlockState {
    Block::NETHER_BRICK_STAIRS
        .default_state()
        .with(properties::FACING, facing)
        .expect("stairs facing")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        feature::FeatureRegistry,
        region::{ChunkStatus, RegionChunk, WorldGenRegion},
        structure::processor::tests::{StaticBlocks, generator},
    };
    use ferraloom_util::random::{Algorithm, WorldgenRandom};
    use serde_json::Value;
    use std::io::Read;
    fn point(v: &Value) -> [i32; 3] {
        std::array::from_fn(|i| v[i].as_i64().unwrap() as i32)
    }
    #[test]
    fn fortress_saved_piece_geometry_foundations_clipping_and_ticks_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../testdata/nether_fortress_blocks.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let center = std::array::from_fn(|i| case["center"][i].as_i64().unwrap() as i32);
            let mode = case["mode"].as_i64().unwrap();
            let initial = ferraloom_nbt::parse_compound(case["initial"].as_str().unwrap()).unwrap();
            let mut piece =
                FortressPiece::from_nbt(case["id"].as_str().unwrap(), &initial).unwrap();
            let mut chunk = generator.new_chunk(center);
            for z in 0..16 {
                for x in 0..16 {
                    let ground = if mode == 0 {
                        60
                    } else {
                        58 + ((center[0] * 16 + x) * 3 + (center[1] * 16 + z) * 5) % 7
                    };
                    for y in -64..=ground {
                        chunk.set_block(x as u8, y, z as u8, Block::STONE.default_state());
                    }
                }
            }
            let mut chunks = [RegionChunk::new(chunk, ChunkStatus::Terrain)];
            let mut region = WorldGenRegion::new(
                &mut chunks,
                center,
                &[ChunkStatus::Terrain],
                0,
                None,
                &StaticBlocks,
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
                "{initial:?}"
            );
            let chunk = region.chunk(center, ChunkStatus::Terrain).unwrap();
            let actual = chunk
                .poi_changes
                .iter()
                .map(|(p, _, s)| (*p, s.id()))
                .collect::<Vec<_>>();
            let expected = case["writes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (point(&v["pos"]), v["state"].as_u64().unwrap() as u16))
                .collect::<Vec<_>>();
            assert_eq!(
                actual.len(),
                expected.len(),
                "{initial:?} mode={mode} {bounds:?}"
            );
            for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(a, e, "{initial:?} mode={mode} {bounds:?} write={i}");
            }
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
}
