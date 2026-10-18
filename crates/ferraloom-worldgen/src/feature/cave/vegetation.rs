use super::super::PlacedFeature;
use super::*;
use ferraloom_data::properties;
use ferraloom_util::{BlockPos, collections::JavaHashSet};

/// Port of VegetationPatchFeature and WaterloggedVegetationPatchFeature.
#[derive(Debug)]
struct VegetationPatchFeature {
    replaceable: Vec<Block>,
    ground: Arc<BlockStateProvider>,
    vegetation: Arc<PlacedFeature>,
    direction: Direction,
    depth: IntProvider,
    extra_bottom: f32,
    range: i32,
    chance: f32,
    radius: IntProvider,
    extra_edge: f32,
    waterlogged: bool,
}
pub(super) fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("vegetation_patch".parse()?, |v, l| decode(v, l, false))?;
    loader.register("waterlogged_vegetation_patch".parse()?, |v, l| {
        decode(v, l, true)
    })
}
fn decode(
    v: &Value,
    l: &mut FeatureLoader<'_>,
    waterlogged: bool,
) -> Result<Arc<dyn Feature>, Error> {
    let direction = match string(field(v, "surface")?)? {
        "floor" => Direction::Down,
        "ceiling" => Direction::Up,
        _ => return Err(Error("unknown cave surface".into())),
    };
    Ok(Arc::new(VegetationPatchFeature {
        replaceable: l.data.blocks(field(v, "replaceable")?)?,
        ground: l.state_provider(field(v, "ground_state")?)?,
        vegetation: l.placed(field(v, "vegetation_feature")?)?,
        direction,
        depth: IntProvider::decode_bounded(field(v, "depth")?, 1, 128)?,
        extra_bottom: float_range(field(v, "extra_bottom_block_chance")?, 0.0, 1.0)?,
        range: int_range(field(v, "vertical_range")?, 1, 256)?,
        chance: float_range(field(v, "vegetation_chance")?, 0.0, 1.0)?,
        radius: IntProvider::decode(field(v, "xz_radius")?)?,
        extra_edge: float_range(field(v, "extra_edge_column_chance")?, 0.0, 1.0)?,
        waterlogged,
    }))
}
// Vanilla draws RNG while iterating HashSet<BlockPos>. The shared collection
// preserves its deterministic order; identical-hash tree ties are not stable
// even in vanilla and are excluded from parity fixtures.
impl VegetationPatchFeature {
    fn ground(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        mut p: [i32; 3],
        depth: i32,
    ) -> Result<bool, Error> {
        for i in 0..depth {
            let state = self.ground.get_state(ctx, p)?;
            let old = ctx.region.block(p)?;
            if state.block() == old.block() {
                continue;
            }
            if !self.replaceable.contains(&old.block()) {
                return Ok(i != 0);
            }
            ctx.region.set_block(p, state, 2)?;
            p = move_direction(p, self.direction);
        }
        Ok(true)
    }
}
impl Feature for VegetationPatchFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let xr = self.radius.sample(ctx.random) + 1;
        let zr = self.radius.sample(ctx.random) + 1;
        let mut surface = JavaHashSet::<BlockPos>::new();
        for dx in -xr..=xr {
            for dz in -zr..=zr {
                let xe = dx == -xr || dx == xr;
                let ze = dz == -zr || dz == zr;
                if xe && ze
                    || (xe || ze)
                        && (self.extra_edge == 0.0 || ctx.random.next_float() > self.extra_edge)
                {
                    continue;
                }
                let mut p = offset(ctx.origin, [dx, 0, dz]);
                let mut n = 0;
                while is_air(ctx.region.block(p)?) && n < self.range {
                    p = move_direction(p, self.direction);
                    n += 1;
                }
                let mut n = 0;
                while !is_air(ctx.region.block(p)?) && n < self.range {
                    p = move_direction(p, self.direction.opposite());
                    n += 1;
                }
                let ground = move_direction(p, self.direction);
                if is_air(ctx.region.block(p)?)
                    && ctx
                        .region
                        .has_sturdy_face(ground, self.direction.opposite())?
                {
                    let depth = self.depth.sample(ctx.random)
                        + i32::from(
                            self.extra_bottom > 0.0 && ctx.random.next_float() < self.extra_bottom,
                        );
                    if self.ground(ctx, ground, depth)? {
                        surface.insert(BlockPos::new(ground[0], ground[1], ground[2]));
                    }
                }
            }
        }
        if self.waterlogged {
            let mut water = JavaHashSet::<BlockPos>::new();
            for p in surface.iter() {
                let p = [p.x, p.y, p.z];
                let mut exposed = false;
                for d in HORIZONTAL.into_iter().chain([Direction::Down]) {
                    if !ctx
                        .region
                        .has_sturdy_face(move_direction(p, d), d.opposite())?
                    {
                        exposed = true;
                        break;
                    }
                }
                if !exposed {
                    water.insert(BlockPos::new(p[0], p[1], p[2]));
                }
            }
            for p in water.iter() {
                let p = [p.x, p.y, p.z];
                ctx.region.set_block(p, Block::WATER.default_state(), 2)?;
            }
            surface = water;
        }
        for p in surface.iter() {
            let p = [p.x, p.y, p.z];
            if self.chance > 0.0 && ctx.random.next_float() < self.chance {
                let origin = ctx.origin;
                let vegetation_p = if self.waterlogged {
                    move_direction(p, Direction::Down)
                } else {
                    p
                };
                ctx.origin = move_direction(vegetation_p, self.direction.opposite());
                let result = self.vegetation.place(ctx, false);
                ctx.origin = origin;
                if result? && self.waterlogged {
                    let state = ctx.region.block(p)?;
                    if state.get(properties::WATERLOGGED) == Some(false) {
                        ctx.region.set_block(
                            p,
                            state.with(properties::WATERLOGGED, true).unwrap(),
                            2,
                        )?;
                    }
                }
            }
        }
        Ok(!surface.is_empty())
    }
}
