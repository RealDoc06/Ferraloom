//! Nether terrain features, preserving vanilla traversal and provider order.
use super::{Feature, FeatureContext, FeatureLoader};
use crate::{
    Error,
    codec::*,
    generator::is_air,
    placement::{
        predicate::{BlockPredicate, move_direction, offset},
        providers::IntProvider,
    },
};
use ferraloom_data::{Block, BlockState, datapack::decode_block_state};
use ferraloom_util::{Direction, random::RandomSource};
use std::sync::Arc;

pub fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("delta_feature".parse()?, |v, _| {
        Ok(Arc::new(DeltaFeature {
            contents: decode_block_state(field(v, "contents")?)?,
            rim: decode_block_state(field(v, "rim")?)?,
            size: IntProvider::decode_bounded(field(v, "size")?, 0, 16)?,
            rim_size: IntProvider::decode_bounded(field(v, "rim_size")?, 0, 16)?,
        }))
    })?;
    loader.register("netherrack_replace_blobs".parse()?, |v, _| {
        Ok(Arc::new(ReplaceBlobsFeature {
            target: decode_block_state(field(v, "target")?)?.block(),
            state: decode_block_state(field(v, "state")?)?,
            radius: IntProvider::decode_bounded(field(v, "radius")?, 0, 12)?,
        }))
    })?;
    loader.register("stepped_column_cluster".parse()?, |v, l| {
        Ok(Arc::new(SteppedColumnClusterFeature {
            block: l.state_provider(field(v, "block")?)?,
            continue_through: BlockPredicate::decode(field(v, "continue_through")?, &l.data)?,
            can_replace: BlockPredicate::decode(field(v, "can_replace")?, &l.data)?,
            cannot_place_on: l.data.blocks(field(v, "cannot_place_on")?)?,
            cluster_reach: IntProvider::decode_bounded(field(v, "cluster_reach")?, 0, 13)?,
            column_count: IntProvider::decode_bounded(field(v, "column_count")?, 1, 150)?,
            column_reach: IntProvider::decode_bounded(field(v, "column_reach")?, 0, 3)?,
            height: IntProvider::decode_bounded(field(v, "height")?, 1, 10)?,
        }))
    })?;
    Ok(())
}
/// Port of BlockPos#withinBoxByManhattanDistance, truncated at `depth`.
fn manhattan(origin: [i32; 3], reach: [i32; 3], depth: i32) -> impl Iterator<Item = [i32; 3]> {
    (0..=depth).flat_map(move |d| {
        let max_x = reach[0].min(d);
        (-max_x..=max_x).flat_map(move |x| {
            let max_y = reach[1].min(d - x.abs());
            (-max_y..=max_y).flat_map(move |y| {
                let z = d - x.abs() - y.abs();
                let positive = (z <= reach[2]).then(|| offset(origin, [x, y, z]));
                let negative = (z > 0 && z <= reach[2]).then(|| offset(origin, [x, y, -z]));
                positive.into_iter().chain(negative)
            })
        })
    })
}
/// Port of DeltaFeature.
#[derive(Debug)]
struct DeltaFeature {
    contents: BlockState,
    rim: BlockState,
    size: IntProvider,
    rim_size: IntProvider,
}
impl DeltaFeature {
    fn is_clear(&self, ctx: &FeatureContext<'_, '_>, p: [i32; 3]) -> Result<bool, Error> {
        let block = ctx.region.block(p)?.block();
        if block == self.contents.block()
            || [
                Block::BEDROCK,
                Block::NETHER_BRICKS,
                Block::NETHER_BRICK_FENCE,
                Block::NETHER_BRICK_STAIRS,
                Block::NETHER_WART,
                Block::CHEST,
                Block::SPAWNER,
            ]
            .contains(&block)
        {
            return Ok(false);
        }
        for d in [
            Direction::Down,
            Direction::Up,
            Direction::North,
            Direction::South,
            Direction::West,
            Direction::East,
        ] {
            let air = is_air(ctx.region.block(move_direction(p, d))?);
            if air != (d == Direction::Up) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
impl Feature for DeltaFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let spawn_rim = ctx.random.next_double() < 0.9;
        let rx = if spawn_rim {
            self.rim_size.sample(ctx.random)
        } else {
            0
        };
        let rz = if spawn_rim {
            self.rim_size.sample(ctx.random)
        } else {
            0
        };
        let sx = self.size.sample(ctx.random);
        let sz = self.size.sample(ctx.random);
        let mut placed = false;
        for p in manhattan(ctx.origin, [sx, 0, sz], sx.max(sz)) {
            if self.is_clear(ctx, p)? {
                if spawn_rim && rx != 0 && rz != 0 {
                    ctx.region.set_block(p, self.rim, 2)?;
                    placed = true;
                }
                let p = offset(p, [rx, 0, rz]);
                if self.is_clear(ctx, p)? {
                    ctx.region.set_block(p, self.contents, 2)?;
                    placed = true;
                }
            }
        }
        Ok(placed)
    }
}
/// Port of ReplaceBlobsFeature.
#[derive(Debug)]
struct ReplaceBlobsFeature {
    target: Block,
    state: BlockState,
    radius: IntProvider,
}
impl Feature for ReplaceBlobsFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut p = ctx.origin;
        p[1] = p[1].clamp(
            ctx.region.min_y() + 1,
            ctx.region.min_y() + ctx.region.height() - 1,
        );
        while p[1] > ctx.region.min_y() + 1 && ctx.region.block(p)?.block() != self.target {
            p[1] -= 1;
        }
        if p[1] <= ctx.region.min_y() + 1 {
            return Ok(false);
        }
        let radii = std::array::from_fn(|_| self.radius.sample(ctx.random));
        let mut placed = false;
        for p in manhattan(p, radii, *radii.iter().max().unwrap()) {
            if ctx.region.block(p)?.block() == self.target {
                ctx.region.set_block(p, self.state, 2)?;
                placed = true;
            }
        }
        Ok(placed)
    }
}
/// Port of SteppedColumnClusterFeature (26.3's basalt column implementation).
#[derive(Debug)]
struct SteppedColumnClusterFeature {
    block: Arc<super::state_provider::BlockStateProvider>,
    continue_through: BlockPredicate,
    can_replace: BlockPredicate,
    cannot_place_on: Vec<Block>,
    cluster_reach: IntProvider,
    column_count: IntProvider,
    column_reach: IntProvider,
    height: IntProvider,
}
impl SteppedColumnClusterFeature {
    fn can_place_at(&self, ctx: &mut FeatureContext<'_, '_>, p: [i32; 3]) -> Result<bool, Error> {
        if !self.can_replace.test(ctx.region, p, ctx.generator)? {
            return Ok(false);
        }
        let below = ctx.region.block(offset(p, [0, -1, 0]))?;
        Ok(!is_air(below) && !self.cannot_place_on.contains(&below.block()))
    }
    fn find_surface(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        mut p: [i32; 3],
        mut limit: i32,
    ) -> Result<Option<[i32; 3]>, Error> {
        while p[1] > ctx.region.min_y() + 1 && limit > 0 {
            limit -= 1;
            if self.can_place_at(ctx, p)? {
                return Ok(Some(p));
            }
            p[1] -= 1;
        }
        Ok(None)
    }
    fn find_air(
        &self,
        ctx: &FeatureContext<'_, '_>,
        mut p: [i32; 3],
        mut limit: i32,
    ) -> Result<Option<[i32; 3]>, Error> {
        while p[1] < ctx.region.min_y() + ctx.region.height() && limit > 0 {
            limit -= 1;
            let state = ctx.region.block(p)?;
            if self.cannot_place_on.contains(&state.block()) {
                return Ok(None);
            }
            if is_air(state) {
                return Ok(Some(p));
            }
            p[1] += 1;
        }
        Ok(None)
    }
    fn place_column(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        origin: [i32; 3],
        height: i32,
        reach: i32,
    ) -> Result<bool, Error> {
        let mut placed = false;
        for z in -reach..=reach {
            for x in -reach..=reach {
                let p = offset(origin, [x, 0, z]);
                let steps = x.abs() + z.abs();
                let start = if self.can_replace.test(ctx.region, p, ctx.generator)? {
                    self.find_surface(ctx, p, steps)?
                } else {
                    self.find_air(ctx, p, steps)?
                };
                if let Some(mut cursor) = start {
                    for _ in 0..=height - steps / 2 {
                        if self.can_replace.test(ctx.region, cursor, ctx.generator)? {
                            let state = self.block.get_state(ctx, cursor)?;
                            ctx.region.set_block(cursor, state, 2)?;
                            placed = true;
                        } else if !self
                            .continue_through
                            .test(ctx.region, cursor, ctx.generator)?
                        {
                            break;
                        }
                        cursor[1] += 1;
                    }
                }
            }
        }
        Ok(placed)
    }
}
impl Feature for SteppedColumnClusterFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if !self.can_place_at(ctx, ctx.origin)? {
            return Ok(false);
        }
        let height = self.height.sample(ctx.random);
        let reach = height.min(self.cluster_reach.sample(ctx.random));
        let count = self.column_count.sample(ctx.random);
        let mut placed = false;
        for _ in 0..count {
            let x = ctx.origin[0]
                .wrapping_sub(reach)
                .wrapping_add(ctx.random.next_int_bound(2 * reach + 1));
            let y = ctx.origin[1] + ctx.random.next_int_bound(1);
            let z = ctx.origin[2]
                .wrapping_sub(reach)
                .wrapping_add(ctx.random.next_int_bound(2 * reach + 1));
            let h = height
                - x.wrapping_sub(ctx.origin[0]).wrapping_abs()
                - z.wrapping_sub(ctx.origin[2]).wrapping_abs();
            if h >= 0 {
                let r = self.column_reach.sample(ctx.random);
                placed |= self.place_column(ctx, [x, y, z], h, r)?;
            }
        }
        Ok(placed)
    }
}
