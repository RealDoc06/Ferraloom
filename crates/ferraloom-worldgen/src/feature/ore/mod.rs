//! Ore replacement geometry and air-exposure rejection, from AbstractOreFeature.
use super::{Feature, FeatureContext, FeatureLoader};
use crate::{
    Error,
    codec::*,
    generator::is_air,
    placement::predicate::move_direction,
    region::Heightmap,
    structure::rule::{BlockTags, RuleTest},
};
use ferraloom_data::{BlockState, datapack::decode_block_state};
use ferraloom_util::{Direction, mth, random::RandomSource};
use serde_json::Value;
use std::sync::Arc;

/// Port of BlockReplacement, shared by ore and ReplaceBlockFeature.
#[derive(Debug)]
pub(crate) struct BlockReplacement {
    pub target: RuleTest,
    pub state: BlockState,
}
pub(crate) fn replacements(v: &Value) -> Result<Vec<BlockReplacement>, Error> {
    list(field(v, "targets")?)?
        .iter()
        .map(|v| {
            Ok(BlockReplacement {
                target: RuleTest::decode(field(v, "target")?)?,
                state: decode_block_state(field(v, "state")?)?,
            })
        })
        .collect()
}

/// Port of OreFeature and ScatteredOreFeature. Both share AbstractOreFeature's
/// target predicates but deliberately use different block-write paths.
#[derive(Debug)]
struct OreFeature {
    targets: Vec<BlockReplacement>,
    tags: BlockTags,
    size: i32,
    discard: f32,
    scattered: bool,
}
pub fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("ore".parse()?, |v, l| decode(v, l, false))?;
    loader.register("scattered_ore".parse()?, |v, l| decode(v, l, true))
}
fn decode(
    v: &Value,
    loader: &mut FeatureLoader<'_>,
    scattered: bool,
) -> Result<Arc<dyn Feature>, Error> {
    Ok(Arc::new(OreFeature {
        targets: replacements(v)?,
        tags: BlockTags(loader.data.block_tags.clone()),
        size: int_range(field(v, "size")?, 0, 64)?,
        discard: float_range(field(v, "discard_chance_on_air_exposure")?, 0.0, 1.0)?,
        scattered,
    }))
}
impl OreFeature {
    fn replacement(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        p: [i32; 3],
    ) -> Result<Option<BlockState>, Error> {
        let old = ctx.region.block(p)?;
        for target in &self.targets {
            if !target.target.test(old, p, &self.tags, ctx.random) {
                continue;
            }
            if self.discard <= 0.0
                || (self.discard < 1.0 && ctx.random.next_float() >= self.discard)
            {
                return Ok(Some(target.state));
            }
            let mut exposed = false;
            for direction in Direction::VALUES {
                if is_air(ctx.region.block(move_direction(p, direction))?) {
                    exposed = true;
                    break;
                }
            }
            if !exposed {
                return Ok(Some(target.state));
            }
        }
        Ok(None)
    }
    fn scattered(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let tries = ctx.random.next_int_bound(self.size + 1);
        for i in 0..tries {
            let distance = i.min(7) as f32;
            let p = std::array::from_fn(|axis| {
                let offset = (ctx.random.next_float() - ctx.random.next_float()) * distance;
                ctx.origin[axis].wrapping_add((offset + 0.5).floor() as i32)
            });
            if let Some(state) = self.replacement(ctx, p)? {
                ctx.region.set_block(p, state, 2)?;
            }
        }
        Ok(true)
    }
    fn vein(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let dir = ctx.random.next_float() * std::f32::consts::PI;
        let spread = self.size as f32 / 8.0;
        let max_radius = ((self.size as f32 / 16.0 * 2.0 + 1.0) / 2.0).ceil() as i32;
        let x = [
            ctx.origin[0] as f64 + (dir as f64).sin() * spread as f64,
            ctx.origin[0] as f64 - (dir as f64).sin() * spread as f64,
        ];
        let z = [
            ctx.origin[2] as f64 + (dir as f64).cos() * spread as f64,
            ctx.origin[2] as f64 - (dir as f64).cos() * spread as f64,
        ];
        let y = [
            (ctx.origin[1] + ctx.random.next_int_bound(3) - 2) as f64,
            (ctx.origin[1] + ctx.random.next_int_bound(3) - 2) as f64,
        ];
        let start = [
            ctx.origin[0] - spread.ceil() as i32 - max_radius,
            ctx.origin[1] - 2 - max_radius,
            ctx.origin[2] - spread.ceil() as i32 - max_radius,
        ];
        let size_xz = 2 * (spread.ceil() as i32 + max_radius);
        let size_y = 2 * (2 + max_radius);
        let mut below_surface = false;
        'probe: for px in start[0]..=start[0] + size_xz {
            for pz in start[2]..=start[2] + size_xz {
                if start[1] <= ctx.region.height_at(Heightmap::OceanFloorWg, px, pz)? {
                    below_surface = true;
                    break 'probe;
                }
            }
        }
        if !below_surface {
            return Ok(false);
        }
        let mut spheres = Vec::with_capacity(self.size as usize);
        for i in 0..self.size {
            let step = (i as f32 / self.size as f32) as f64;
            let scale = ctx.random.next_double() * self.size as f64 / 16.0;
            let radius = ((mth::sin((std::f32::consts::PI * step as f32) as f64) + 1.0) as f64
                * scale
                + 1.0)
                / 2.0;
            spheres.push([
                mth::lerp(step, x[0], x[1]),
                mth::lerp(step, y[0], y[1]),
                mth::lerp(step, z[0], z[1]),
                radius,
            ]);
        }
        for i in 0..spheres.len().saturating_sub(1) {
            if spheres[i][3] <= 0.0 {
                continue;
            }
            for j in i + 1..spheres.len() {
                if spheres[j][3] <= 0.0 {
                    continue;
                }
                let a = spheres[i];
                let b = spheres[j];
                let dr = a[3] - b[3];
                let distance =
                    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
                if dr * dr > distance {
                    spheres[if dr > 0.0 { j } else { i }][3] = -1.0;
                }
            }
        }
        // Vanilla's BitSet grows if necessary. Its linear index is significant:
        // a coordinate set would not preserve possible boundary index aliases.
        let mut tested = std::collections::BTreeSet::new();
        let mut placed = false;
        for [x, y, z, radius] in spheres {
            if radius < 0.0 {
                continue;
            }
            let center = [x, y, z];
            let min: [i32; 3] =
                std::array::from_fn(|i| mth::floor(center[i] - radius).max(start[i]));
            let max: [i32; 3] = std::array::from_fn(|i| mth::floor(center[i] + radius).max(min[i]));
            for px in min[0]..=max[0] {
                let dx = (px as f64 + 0.5 - x) / radius;
                if dx * dx >= 1.0 {
                    continue;
                }
                for py in min[1]..=max[1] {
                    let dy = (py as f64 + 0.5 - y) / radius;
                    if dx * dx + dy * dy >= 1.0 {
                        continue;
                    }
                    for pz in min[2]..=max[2] {
                        let dz = (pz as f64 + 0.5 - z) / radius;
                        if dx * dx + dy * dy + dz * dz >= 1.0
                            || py < ctx.region.min_y()
                            || py >= ctx.region.min_y() + ctx.region.height()
                        {
                            continue;
                        }
                        let index = px - start[0]
                            + (py - start[1]) * size_xz
                            + (pz - start[2]) * size_xz * size_y;
                        if !tested.insert(index) {
                            continue;
                        }
                        let p = [px, py, pz];
                        if ctx.region.ensure_can_write(p)
                            && let Some(state) = self.replacement(ctx, p)?
                        {
                            ctx.region.set_block_in_section(p, state)?;
                            placed = true;
                        }
                    }
                }
            }
        }
        Ok(placed)
    }
}
impl Feature for OreFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        if self.scattered {
            self.scattered(ctx)
        } else {
            self.vein(ctx)
        }
    }
}
