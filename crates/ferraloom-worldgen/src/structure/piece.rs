//! Common StructurePiece state and world-position helpers.
use super::{BoundingBox, nbt::string};
use crate::{Error, Identifier};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::Direction;

/// Port of StructurePiece's persisted base fields.
#[derive(Clone, Debug)]
pub struct StructurePieceData {
    pub bounds: BoundingBox,
    pub gen_depth: i32,
    pub orientation: Option<Direction>,
}
impl StructurePieceData {
    pub fn new(bounds: BoundingBox, gen_depth: i32, orientation: Option<Direction>) -> Self {
        Self {
            bounds,
            gen_depth,
            orientation,
        }
    }
    pub fn to_nbt(&self, id: &Identifier) -> CompoundTag {
        [
            ("id", string(id)),
            (
                "BB",
                Tag::IntArray(self.bounds.min.into_iter().chain(self.bounds.max).collect()),
            ),
            ("GD", Tag::Int(self.gen_depth)),
            (
                "O",
                Tag::Int(self.orientation.map_or(-1, Direction::data_2d)),
            ),
        ]
        .into_iter()
        .collect()
    }
    pub fn from_nbt(tag: &CompoundTag) -> Result<Self, Error> {
        // BoundingBox.CODEC consumes an int stream, including homogeneous lists.
        let bounds = match tag.get("BB") {
            Some(Tag::IntArray(a)) => a.clone(),
            Some(Tag::ByteArray(a)) => a.iter().copied().map(i32::from).collect(),
            Some(Tag::LongArray(a)) => a.iter().map(|v| *v as i32).collect(),
            Some(Tag::List(a)) => a
                .iter()
                .map(|v| super::nbt::ops_int(v).ok_or_else(|| Error("invalid piece BB".into())))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(Error("missing piece BB".into())),
        };
        if bounds.len() != 6 {
            return Err(Error("piece BB requires six integers".into()));
        }
        let orientation = tag.get_int("O").unwrap_or(0);
        Ok(Self {
            bounds: BoundingBox::new(
                [bounds[0], bounds[1], bounds[2]],
                [bounds[3], bounds[4], bounds[5]],
            ),
            gen_depth: tag.get_int("GD").unwrap_or(0),
            orientation: (orientation != -1).then(|| Direction::from_2d_data_value(orientation)),
        })
    }
}

impl StructurePieceData {
    /// Port of StructurePiece#getWorldPos.
    pub fn world_position(&self, [x, y, z]: [i32; 3]) -> [i32; 3] {
        let Some(direction) = self.orientation else {
            return [x, y, z];
        };
        let b = self.bounds;
        let wx = match direction {
            Direction::North | Direction::South => b.min[0] + x,
            Direction::West => b.max[0] - z,
            Direction::East => b.min[0] + z,
            _ => x,
        };
        let wz = match direction {
            Direction::North => b.max[2] - z,
            Direction::South => b.min[2] + z,
            Direction::West | Direction::East => b.min[2] + x,
            _ => z,
        };
        [wx, b.min[1] + y, wz]
    }
    pub fn transformed_state(
        &self,
        state: ferraloom_data::BlockState,
    ) -> ferraloom_data::BlockState {
        use ferraloom_util::{Mirror, Rotation};
        let (mirror, rotation) = match self.orientation {
            Some(Direction::South) => (Mirror::LeftRight, Rotation::None),
            Some(Direction::West) => (Mirror::LeftRight, Rotation::Clockwise90),
            Some(Direction::East) => (Mirror::None, Rotation::Clockwise90),
            _ => (Mirror::None, Rotation::None),
        };
        state.mirror(mirror).rotate(rotation)
    }
    /// StructurePiece#placeBlock for pieces inheriting canBeReplaced=true.
    pub fn place_block(
        &self,
        region: &mut crate::region::WorldGenRegion<'_>,
        state: ferraloom_data::BlockState,
        local: [i32; 3],
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        use ferraloom_data::Block;
        let p = self.world_position(local);
        if !bounds.contains(p) {
            return Ok(());
        }
        let state = self.transformed_state(state);
        region.set_block(p, state, 2)?;
        let fluid = region.fluid(p)?;
        if !fluid.empty {
            region.schedule_fluid_tick(crate::region::SavedTick {
                position: p,
                kind: fluid.fluid_type,
                priority: 0,
            })?;
        }
        if matches!(
            state.block(),
            Block::NETHER_BRICK_FENCE
                | Block::TORCH
                | Block::WALL_TORCH
                | Block::OAK_FENCE
                | Block::SPRUCE_FENCE
                | Block::DARK_OAK_FENCE
                | Block::PALE_OAK_FENCE
                | Block::ACACIA_FENCE
                | Block::BIRCH_FENCE
                | Block::JUNGLE_FENCE
                | Block::LADDER
                | Block::IRON_BARS
        ) {
            region.mark_post_processing(p)?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn generate_box(
        &self,
        region: &mut crate::region::WorldGenRegion<'_>,
        bounds: BoundingBox,
        min: [i32; 3],
        max: [i32; 3],
        edge: ferraloom_data::BlockState,
        fill: ferraloom_data::BlockState,
        skip_air: bool,
    ) -> Result<(), Error> {
        for y in min[1]..=max[1] {
            for x in min[0]..=max[0] {
                for z in min[2]..=max[2] {
                    let local = [x, y, z];
                    let p = self.world_position(local);
                    if skip_air && (!bounds.contains(p) || region.block(p)?.is_air()) {
                        continue;
                    }
                    let inner = (0..3).all(|i| local[i] != min[i] && local[i] != max[i]);
                    self.place_block(region, if inner { fill } else { edge }, local, bounds)?;
                }
            }
        }
        Ok(())
    }
    /// Port of StructurePiece#fillColumnDown. The initial position alone is
    /// clipped; subsequent downward writes continue below the supplied box.
    pub fn fill_column_down(
        &self,
        region: &mut crate::region::WorldGenRegion<'_>,
        state: ferraloom_data::BlockState,
        local: [i32; 3],
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        use ferraloom_data::Block;
        let mut p = self.world_position(local);
        if !bounds.contains(p) {
            return Ok(());
        }
        loop {
            let old = region.block(p)?;
            let replaceable = old.is_air()
                || matches!(
                    old.block(),
                    Block::WATER
                        | Block::LAVA
                        | Block::GLOW_LICHEN
                        | Block::SEAGRASS
                        | Block::TALL_SEAGRASS
                );
            if !replaceable || p[1] <= region.min_y() + 1 {
                break;
            }
            region.set_block(p, state, 2)?;
            p[1] -= 1;
        }
        Ok(())
    }
}

impl StructurePieceData {
    /// Port of StructurePiece#createChest at a transformed local position.
    pub fn create_chest(
        &self,
        context: &mut crate::structure::StructurePlacementContext<'_, '_>,
        local: [i32; 3],
        bounds: BoundingBox,
        loot: &Identifier,
    ) -> Result<bool, Error> {
        Self::create_chest_at(context, self.world_position(local), bounds, loot, None)
    }
    /// Port of StructurePiece#createChest with world coordinates and an optional
    /// explicit state. Explicit template-marker facings bypass chest reorientation.
    pub fn create_chest_at(
        context: &mut crate::structure::StructurePlacementContext<'_, '_>,
        p: [i32; 3],
        bounds: BoundingBox,
        loot: &Identifier,
        state: Option<ferraloom_data::BlockState>,
    ) -> Result<bool, Error> {
        use ferraloom_data::Block;
        if !bounds.contains(p) || context.region.block(p)?.block() == Block::CHEST {
            return Ok(false);
        }
        let state = match state {
            Some(state) => state,
            None => super::structures::buried_treasure::reorient_chest(context.region, p)?,
        };
        context.region.set_block(p, state, 2)?;
        context.region.set_loot_table(p, loot, context.random)?;
        Ok(true)
    }
}

impl StructurePieceData {
    /// Port of StructurePiece#createDispenser. The adapter consumes a loot seed
    /// only when the placed block entity is a dispenser.
    pub fn create_dispenser(
        &self,
        context: &mut crate::structure::StructurePlacementContext<'_, '_>,
        local: [i32; 3],
        facing: ferraloom_data::properties::Facing,
        bounds: BoundingBox,
        loot: &Identifier,
    ) -> Result<bool, Error> {
        use ferraloom_data::{Block, properties};
        let p = self.world_position(local);
        if !bounds.contains(p) || context.region.block(p)?.block() == Block::DISPENSER {
            return Ok(false);
        }
        self.place_block(
            context.region,
            Block::DISPENSER
                .default_state()
                .with(properties::FACING, facing)
                .expect("dispenser facing"),
            local,
            bounds,
        )?;
        context.region.set_loot_table(p, loot, context.random)?;
        Ok(true)
    }
}
