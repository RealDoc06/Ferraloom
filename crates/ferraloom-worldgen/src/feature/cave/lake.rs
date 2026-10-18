use super::*;

/// Port of LakeFeature, including its deprecated lava-lake geometry and ticks.
#[derive(Debug)]
struct LakeFeature {
    fluid: Arc<BlockStateProvider>,
    barrier: Arc<BlockStateProvider>,
    can_place: BlockPredicate,
    replace_fluid: BlockPredicate,
    replace_barrier: BlockPredicate,
    water: Vec<fluid::Id>,
}
pub(super) fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("lake".parse()?, |v, l| {
        Ok(Arc::new(LakeFeature {
            fluid: l.state_provider(field(v, "fluid")?)?,
            water: l.data.fluids(&Value::String("#minecraft:water".into()))?,
            barrier: l.state_provider(field(v, "barrier")?)?,
            can_place: BlockPredicate::decode(field(v, "can_place_feature")?, &l.data)?,
            replace_fluid: BlockPredicate::decode(
                field(v, "can_replace_with_air_or_fluid")?,
                &l.data,
            )?,
            replace_barrier: BlockPredicate::decode(
                field(v, "can_replace_with_barrier")?,
                &l.data,
            )?,
        }))
    })
}
fn index(x: usize, y: usize, z: usize) -> usize {
    (x * 16 + z) * 8 + y
}
fn boundary(grid: &[bool; 2048], x: usize, y: usize, z: usize) -> bool {
    !grid[index(x, y, z)]
        && (x < 15 && grid[index(x + 1, y, z)]
            || x > 0 && grid[index(x - 1, y, z)]
            || z < 15 && grid[index(x, y, z + 1)]
            || z > 0 && grid[index(x, y, z - 1)]
            || y < 7 && grid[index(x, y + 1, z)]
            || y > 0 && grid[index(x, y - 1, z)])
}
impl Feature for LakeFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if ctx.origin[1] <= ctx.region.min_y() + 4 {
            return Ok(false);
        }
        let origin = offset(ctx.origin, [-8, -4, -8]);
        let mut grid = [false; 2048];
        let spots = ctx.random.next_int_bound(4) + 4;
        for _ in 0..spots {
            let xr = ctx.random.next_double() * 6.0 + 3.0;
            let yr = ctx.random.next_double() * 4.0 + 2.0;
            let zr = ctx.random.next_double() * 6.0 + 3.0;
            let xp = ctx.random.next_double() * (16.0 - xr - 2.0) + 1.0 + xr / 2.0;
            let yp = ctx.random.next_double() * (8.0 - yr - 4.0) + 2.0 + yr / 2.0;
            let zp = ctx.random.next_double() * (16.0 - zr - 2.0) + 1.0 + zr / 2.0;
            for x in 1..15 {
                for z in 1..15 {
                    for y in 1..7 {
                        let dx = (x as f64 - xp) / (xr / 2.0);
                        let dy = (y as f64 - yp) / (yr / 2.0);
                        let dz = (z as f64 - zp) / (zr / 2.0);
                        if dx * dx + dy * dy + dz * dz < 1.0 {
                            grid[index(x, y, z)] = true;
                        }
                    }
                }
            }
        }
        let fluid = self.fluid.get_state(ctx, origin)?;
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..8 {
                    if !boundary(&grid, x, y, z) {
                        continue;
                    }
                    let p = offset(origin, [x as i32, y as i32, z as i32]);
                    let old = ctx.region.block(p)?;
                    if y >= 4 && matches!(old.block(), Block::WATER | Block::LAVA) {
                        return Ok(false);
                    }
                    if y < 4 && !ctx.region.is_solid(p)? && old != fluid {
                        return Ok(false);
                    }
                    if !self.can_place.test(ctx.region, p, ctx.generator)? {
                        return Ok(false);
                    }
                }
            }
        }
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..8 {
                    let p = offset(origin, [x as i32, y as i32, z as i32]);
                    if grid[index(x, y, z)]
                        && self.replace_fluid.test(ctx.region, p, ctx.generator)?
                    {
                        let air = y >= 4;
                        ctx.region.set_block(
                            p,
                            if air {
                                Block::CAVE_AIR.default_state()
                            } else {
                                fluid
                            },
                            2,
                        )?;
                        if air {
                            ctx.region.schedule_block_tick(SavedTick {
                                position: p,
                                kind: Block::CAVE_AIR,
                                priority: 0,
                            })?;
                            mark_above(ctx.region, p)?;
                        }
                    }
                }
            }
        }
        let barrier = self.barrier.get_state(ctx, origin)?;
        if !is_air(barrier) {
            for x in 0..16 {
                for z in 0..16 {
                    for y in 0..8 {
                        if boundary(&grid, x, y, z) && (y < 4 || ctx.random.next_int_bound(2) != 0)
                        {
                            let p = offset(origin, [x as i32, y as i32, z as i32]);
                            if ctx.region.is_solid(p)?
                                && self.replace_barrier.test(ctx.region, p, ctx.generator)?
                            {
                                ctx.region.set_block(p, barrier, 2)?;
                                mark_above(ctx.region, p)?;
                            }
                        }
                    }
                }
            }
        }
        if self.water.contains(&fluid.data().fluid.fluid_type) {
            for x in 0..16 {
                for z in 0..16 {
                    let p = offset(origin, [x, 4, z]);
                    let quart = crate::biome_manager::zoom_quart(
                        crate::biome_manager::obfuscate_seed(ctx.region.seed),
                        p,
                    );
                    let biome = ctx.region.noise_biome(quart)?;
                    let biome = &ctx.generator.data().biome_names[biome.0 as usize];
                    if ctx
                        .generator
                        .data()
                        .temperatures
                        .should_freeze(biome, ctx.region, p, false)?
                        && self.replace_fluid.test(ctx.region, p, ctx.generator)?
                    {
                        ctx.region.set_block(p, Block::ICE.default_state(), 2)?;
                    }
                }
            }
        }
        Ok(true)
    }
}
