//! Surface feature records and their vanilla geometry.
use super::{Feature, FeatureContext, FeatureLoader, vegetation::relative};
use crate::{
    Error,
    codec::*,
    placement::predicate::{BlockPredicate, offset},
    region::Heightmap,
};
use ferraloom_data::{Block, BlockState, datapack::decode_block_state, properties};
use ferraloom_util::{Direction, random::RandomSource};
use std::sync::Arc;
mod iceberg;

pub fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("fill_layer".parse()?, |v, _| {
        Ok(fill_layer(
            int_range(field(v, "height")?, 0, 4064)?,
            decode_block_state(field(v, "state")?)?,
        ))
    })?;
    loader.register("block_blob".parse()?, |v, l| {
        Ok(Arc::new(BlockBlobFeature {
            state: decode_block_state(field(v, "state")?)?,
            can_place_on: BlockPredicate::decode(field(v, "can_place_on")?, &l.data)?,
        }))
    })?;
    loader.register("spike".parse()?, |v, l| {
        Ok(Arc::new(SpikeFeature {
            state: decode_block_state(field(v, "state")?)?,
            can_place_on: BlockPredicate::decode(field(v, "can_place_on")?, &l.data)?,
            can_replace: BlockPredicate::decode(field(v, "can_replace")?, &l.data)?,
        }))
    })?;
    loader.register("freeze_top_layer".parse()?, |_, _| {
        Ok(Arc::new(SnowAndFreezeFeature))
    })?;
    loader.register("iceberg".parse()?, |v, _| {
        Ok(Arc::new(iceberg::IcebergFeature {
            state: decode_block_state(field(v, "state")?)?,
        }))
    })?;
    Ok(())
}
/// Construct the inline feature used for FlatLevelSource's non-motion-blocking layers.
pub(crate) fn fill_layer(height: i32, state: BlockState) -> Arc<dyn Feature> {
    Arc::new(FillLayerFeature { height, state })
}

/// Port of FillLayerFeature.
#[derive(Debug)]
struct FillLayerFeature {
    height: i32,
    state: BlockState,
}
impl Feature for FillLayerFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        for x in 0..16 {
            for z in 0..16 {
                let p = [
                    ctx.origin[0] + x,
                    ctx.level.min_y() + self.height,
                    ctx.origin[2] + z,
                ];
                if ctx.level.block(p)?.is_air() {
                    ctx.level.set_block(p, self.state, 2)?;
                }
            }
        }
        Ok(true)
    }
}
/// Port of BlockBlobFeature.
#[derive(Debug)]
struct BlockBlobFeature {
    state: BlockState,
    can_place_on: BlockPredicate,
}
impl Feature for BlockBlobFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut p = ctx.origin;
        while p[1] > ctx.level.min_y() + 3
            && !self.can_place_on.test(
                &mut ctx.level,
                relative(p, Direction::Down),
                ctx.generator,
            )?
        {
            p[1] -= 1;
        }
        if p[1] <= ctx.level.min_y() + 3 {
            return Ok(false);
        }
        for _ in 0..3 {
            let xr = ctx.random()?.next_int_bound(2);
            let yr = ctx.random()?.next_int_bound(2);
            let zr = ctx.random()?.next_int_bound(2);
            let radius = (xr + yr + zr) as f32 * 0.333 + 0.5;
            for z in -zr..=zr {
                for y in -yr..=yr {
                    for x in -xr..=xr {
                        if f64::from(x * x + y * y + z * z) <= f64::from(radius * radius) {
                            ctx.level.set_block(offset(p, [x, y, z]), self.state, 3)?;
                        }
                    }
                }
            }
            p = offset(
                p,
                [
                    -1 + ctx.random()?.next_int_bound(2),
                    -ctx.random()?.next_int_bound(2),
                    -1 + ctx.random()?.next_int_bound(2),
                ],
            );
        }
        Ok(true)
    }
}
/// Port of SpikeFeature, including the fixed y=50 root cutoff.
#[derive(Debug)]
struct SpikeFeature {
    state: BlockState,
    can_place_on: BlockPredicate,
    can_replace: BlockPredicate,
}
impl Feature for SpikeFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let mut origin = ctx.origin;
        while ctx.level.block(origin)?.is_air() && origin[1] > ctx.level.min_y() + 2 {
            origin[1] -= 1;
        }
        if !self
            .can_place_on
            .test(&mut ctx.level, origin, ctx.generator)?
        {
            return Ok(false);
        }
        origin[1] += ctx.random()?.next_int_bound(4);
        let height = ctx.random()?.next_int_bound(4) + 7;
        let width = height / 4 + ctx.random()?.next_int_bound(2);
        if width > 1 && ctx.random()?.next_int_bound(60) == 0 {
            origin[1] += 10 + ctx.random()?.next_int_bound(30);
        }
        for y in 0..height {
            let scale = (1.0 - y as f32 / height as f32) * width as f32;
            let radius = scale.ceil() as i32;
            for x in -radius..=radius {
                let dx = x.abs() as f32 - 0.25;
                for z in -radius..=radius {
                    let dz = z.abs() as f32 - 0.25;
                    if (x == 0 && z == 0 || dx * dx + dz * dz <= scale * scale)
                        && (x != -radius && x != radius && z != -radius && z != radius
                            || ctx.random()?.next_float() <= 0.75)
                    {
                        let p = offset(origin, [x, y, z]);
                        if ctx.level.block(p)?.is_air()
                            || self.can_replace.test(&mut ctx.level, p, ctx.generator)?
                        {
                            ctx.level.set_block(p, self.state, 3)?;
                        }
                        if y != 0 && radius > 1 {
                            let p = offset(origin, [x, -y, z]);
                            if ctx.level.block(p)?.is_air()
                                || self.can_replace.test(&mut ctx.level, p, ctx.generator)?
                            {
                                ctx.level.set_block(p, self.state, 3)?;
                            }
                        }
                    }
                }
            }
        }
        let radius = (width - 1).clamp(0, 1);
        for x in -radius..=radius {
            for z in -radius..=radius {
                let mut p = offset(origin, [x, -1, z]);
                let mut run = if x.abs() == 1 && z.abs() == 1 {
                    ctx.random()?.next_int_bound(5)
                } else {
                    50
                };
                while p[1] > 50 {
                    let state = ctx.level.block(p)?;
                    if !state.is_air()
                        && !self.can_replace.test(&mut ctx.level, p, ctx.generator)?
                        && state != self.state
                    {
                        break;
                    }
                    ctx.level.set_block(p, self.state, 3)?;
                    p[1] -= 1;
                    run -= 1;
                    if run <= 0 {
                        p[1] -= ctx.random()?.next_int_bound(5) + 1;
                        run = ctx.random()?.next_int_bound(5);
                    }
                }
            }
        }
        Ok(true)
    }
}
/// Port of SnowAndFreezeFeature. Biome logic is shared with worldgen weather.
#[derive(Debug)]
struct SnowAndFreezeFeature;
impl Feature for SnowAndFreezeFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        for x in 0..16 {
            for z in 0..16 {
                let x = ctx.origin[0] + x;
                let z = ctx.origin[2] + z;
                let top = [x, ctx.level.height_at(Heightmap::MotionBlocking, x, z)?, z];
                let below = relative(top, Direction::Down);
                let quart = crate::biome_manager::zoom_quart(
                    crate::biome_manager::obfuscate_seed(ctx.level.seed()),
                    top,
                );
                let id =
                    &ctx.generator.data().biome_names[ctx.level.noise_biome(quart)?.0 as usize];
                let climate = &ctx.generator.data().temperatures;
                if climate.should_freeze(id, &mut ctx.level, below, false)? {
                    ctx.level.set_block(below, Block::ICE.default_state(), 2)?;
                }
                if climate.should_snow(id, &mut ctx.level, top)? {
                    ctx.level.set_block(top, Block::SNOW.default_state(), 2)?;
                    let state = ctx.level.block(below)?;
                    if let Some(state) = state.with(properties::SNOWY, true) {
                        ctx.level.set_block(below, state, 2)?;
                    }
                }
            }
        }
        Ok(true)
    }
}
