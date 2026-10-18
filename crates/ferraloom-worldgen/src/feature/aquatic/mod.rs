//! Aquatic feature geometry. 26.3 kelp/seagrass/pickles use general features.
use super::{Feature, FeatureContext, FeatureLoader, PlacedFeature, vegetation::relative};
use crate::{Error, codec::*, placement::predicate::offset};
use ferraloom_data::Block;
use ferraloom_util::{Direction, core::Plane, random::RandomSource};
use std::sync::Arc;

pub fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("blue_ice".parse()?, |_, _| Ok(Arc::new(BlueIceFeature)))?;
    loader.register("coral_claw".parse()?, |v, l| {
        Ok(Arc::new(CoralClawFeature {
            feature: l.placed(field(v, "feature")?)?,
        }))
    })?;
    loader.register("coral_tree".parse()?, |v, l| {
        Ok(Arc::new(CoralTreeFeature {
            feature: l.placed(field(v, "feature")?)?,
        }))
    })?;
    Ok(())
}
/// Port of BlueIceFeature.
#[derive(Debug)]
struct BlueIceFeature;
impl Feature for BlueIceFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let p = ctx.origin;
        if p[1] > ctx.level.sea_level() - 1 {
            return Ok(false);
        }
        if ctx.level.block(p)?.block() != Block::WATER
            && ctx.level.block(relative(p, Direction::Down))?.block() != Block::WATER
        {
            return Ok(false);
        }
        let mut found = false;
        for d in Direction::VALUES {
            if d != Direction::Down && ctx.level.block(relative(p, d))?.block() == Block::PACKED_ICE
            {
                found = true;
                break;
            }
        }
        if !found {
            return Ok(false);
        }
        ctx.level.set_block(p, Block::BLUE_ICE.default_state(), 2)?;
        for _ in 0..200 {
            let dy = ctx.random.next_int_bound(5) - ctx.random.next_int_bound(6);
            let width = 3 + if dy < 2 { dy / 2 } else { 0 };
            if width >= 1 {
                let q = offset(
                    p,
                    [
                        ctx.random.next_int_bound(width) - ctx.random.next_int_bound(width),
                        dy,
                        ctx.random.next_int_bound(width) - ctx.random.next_int_bound(width),
                    ],
                );
                let state = ctx.level.block(q)?;
                if state.is_air()
                    || matches!(state.block(), Block::WATER | Block::PACKED_ICE | Block::ICE)
                {
                    for d in Direction::VALUES {
                        if ctx.level.block(relative(q, d))?.block() == Block::BLUE_ICE {
                            ctx.level.set_block(q, Block::BLUE_ICE.default_state(), 2)?;
                            break;
                        }
                    }
                }
            }
        }
        Ok(true)
    }
}
fn place(
    feature: &PlacedFeature,
    ctx: &mut FeatureContext<'_, '_>,
    p: [i32; 3],
) -> Result<bool, Error> {
    let origin = ctx.origin;
    ctx.origin = p;
    let result = feature.place(ctx, false);
    ctx.origin = origin;
    result
}
/// Port of CoralClawFeature. Each segment runs its configured placed feature.
#[derive(Debug)]
struct CoralClawFeature {
    feature: Arc<PlacedFeature>,
}
impl Feature for CoralClawFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let origin = ctx.origin;
        if !place(&self.feature, ctx, origin)? {
            return Ok(false);
        }
        let claw = Plane::Horizontal.random_direction(&mut ctx.random);
        let count = ctx.random.next_int_bound(2) + 2;
        let mut directions = [claw, claw.clockwise(), claw.counter_clockwise()];
        for i in (2..=directions.len()).rev() {
            directions.swap(i - 1, ctx.random.next_int_bound(i as i32) as usize);
        }
        for branch in directions.into_iter().take(count as usize) {
            let sideways = ctx.random.next_int_bound(2) + 1;
            let mut p = relative(origin, branch);
            let (segment, inward) = if branch == claw {
                (claw, ctx.random.next_int_bound(3) + 2)
            } else {
                p = relative(p, Direction::Up);
                (
                    [branch, Direction::Up][ctx.random.next_int_bound(2) as usize],
                    ctx.random.next_int_bound(3) + 3,
                )
            };
            for _ in 0..sideways {
                if !place(&self.feature, ctx, p)? {
                    break;
                }
                p = relative(p, segment);
            }
            p = relative(relative(p, segment.opposite()), Direction::Up);
            for _ in 0..inward {
                p = relative(p, claw);
                if !place(&self.feature, ctx, p)? {
                    break;
                }
                if ctx.random.next_float() < 0.25 {
                    p = relative(p, Direction::Up);
                }
            }
        }
        Ok(true)
    }
}
/// Port of CoralTreeFeature, including successful early trunk termination.
#[derive(Debug)]
struct CoralTreeFeature {
    feature: Arc<PlacedFeature>,
}
impl Feature for CoralTreeFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut p = ctx.origin;
        let height = ctx.random.next_int_bound(3) + 1;
        for _ in 0..height {
            if !place(&self.feature, ctx, p)? {
                return Ok(true);
            }
            p = relative(p, Direction::Up);
        }
        let top = p;
        let count = ctx.random.next_int_bound(3) + 2;
        let directions = Plane::Horizontal.shuffled_copy(&mut ctx.random);
        for branch in directions.into_iter().take(count as usize) {
            p = relative(top, branch);
            let height = ctx.random.next_int_bound(5) + 2;
            let mut length = 0;
            for j in 0..height {
                if !place(&self.feature, ctx, p)? {
                    break;
                }
                length += 1;
                p = relative(p, Direction::Up);
                if j == 0 || length >= 2 && ctx.random.next_float() < 0.25 {
                    p = relative(p, branch);
                    length = 0;
                }
            }
        }
        Ok(true)
    }
}
