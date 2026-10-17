use super::*;
use ferraloom_data::properties::{self, Facing};
use ferraloom_util::{
    mth,
    noise::{Noise, NormalNoise},
    random::{LegacyRandomSource, WorldgenRandom},
};

/// Port of GeodeFeature and its three settings records.
#[derive(Debug)]
struct GeodeFeature {
    providers: [Arc<BlockStateProvider>; 5],
    placements: Vec<BlockState>,
    cannot_replace: Vec<Block>,
    invalid: Vec<Block>,
    layers: [f64; 4],
    crack_chance: f64,
    crack_size: f64,
    crack_offset: i32,
    placement_chance: f64,
    alternate_chance: f64,
    require_alternate: bool,
    outer_distance: IntProvider,
    distribution: IntProvider,
    point_offset: IntProvider,
    min: i32,
    max: i32,
    noise_multiplier: f64,
    invalid_threshold: i32,
}
pub(super) fn register(loader: &mut FeatureLoader<'_>) -> Result<(), Error> {
    loader.register("geode".parse()?, |v, l| {
        let blocks = field(v, "blocks")?;
        let layers = field(v, "layers")?;
        let crack = field(v, "crack")?;
        let placements = list(field(blocks, "inner_placements")?)?
            .iter()
            .map(|v| decode_block_state(v).map_err(Error::from))
            .collect::<Result<Vec<_>, _>>()?;
        if placements.is_empty() {
            return Err(Error("inner_placements must be nonempty".into()));
        }
        Ok(Arc::new(GeodeFeature {
            providers: [
                l.state_provider(field(blocks, "filling_provider")?)?,
                l.state_provider(field(blocks, "inner_layer_provider")?)?,
                l.state_provider(field(blocks, "alternate_inner_layer_provider")?)?,
                l.state_provider(field(blocks, "middle_layer_provider")?)?,
                l.state_provider(field(blocks, "outer_layer_provider")?)?,
            ],
            placements,
            cannot_replace: l.data.blocks(field(blocks, "cannot_replace")?)?,
            invalid: l.data.blocks(field(blocks, "invalid_blocks")?)?,
            layers: [
                optional(layers, "filling", 1.7, |v| bounded(v, 0.01, 50.0))?,
                optional(layers, "inner_layer", 2.2, |v| bounded(v, 0.01, 50.0))?,
                optional(layers, "middle_layer", 3.2, |v| bounded(v, 0.01, 50.0))?,
                optional(layers, "outer_layer", 4.2, |v| bounded(v, 0.01, 50.0))?,
            ],
            crack_chance: optional(crack, "generate_crack_chance", 1.0, |v| {
                bounded(v, 0.0, 1.0)
            })?,
            crack_size: optional(crack, "base_crack_size", 2.0, |v| bounded(v, 0.0, 5.0))?,
            crack_offset: optional(crack, "crack_point_offset", 2, |v| int_range(v, 0, 10))?,
            placement_chance: optional(v, "use_potential_placements_chance", 0.35, |v| {
                bounded(v, 0.0, 1.0)
            })?,
            alternate_chance: optional(v, "use_alternate_layer0_chance", 0.0, |v| {
                bounded(v, 0.0, 1.0)
            })?,
            require_alternate: optional(v, "placements_require_layer0_alternate", true, boolean)?,
            outer_distance: optional(v, "outer_wall_distance", IntProvider::Uniform(4, 5), |v| {
                IntProvider::decode_bounded(v, 1, 20)
            })?,
            distribution: optional(v, "distribution_points", IntProvider::Uniform(3, 4), |v| {
                IntProvider::decode_bounded(v, 1, 20)
            })?,
            point_offset: optional(v, "point_offset", IntProvider::Uniform(1, 2), |v| {
                IntProvider::decode_bounded(v, 0, 10)
            })?,
            min: optional(v, "min_gen_offset", -16, integer)?,
            max: optional(v, "max_gen_offset", 16, integer)?,
            noise_multiplier: optional(v, "noise_multiplier", 0.05, |v| bounded(v, 0.0, 1.0))?,
            invalid_threshold: integer(field(v, "invalid_blocks_threshold")?)?,
        }))
    })
}
impl GeodeFeature {
    fn safe_set(
        &self,
        ctx: &mut FeatureContext<'_, '_>,
        p: [i32; 3],
        state: BlockState,
    ) -> Result<(), Error> {
        if !self.cannot_replace.contains(&ctx.region.block(p)?.block()) {
            ctx.region.set_block(p, state, 2)?;
        }
        Ok(())
    }
}
impl Feature for GeodeFeature {
    fn place(&self, ctx: &mut FeatureContext<'_, '_>) -> Result<bool, Error> {
        let count = self.distribution.sample(ctx.random);
        let noise = NormalNoise::create_parity(-4, vec![1.0]).create(&mut WorldgenRandom::new(
            LegacyRandomSource::new(ctx.region.seed),
        ));
        let adjustment = count as f64 / self.outer_distance.range().1 as f64;
        let thresholds = [
            mth::inv_sqrt(self.layers[0]),
            mth::inv_sqrt(self.layers[1] + adjustment),
            mth::inv_sqrt(self.layers[2] + adjustment),
            mth::inv_sqrt(self.layers[3] + adjustment),
        ];
        let crack_size = mth::inv_sqrt(
            self.crack_size
                + ctx.random.next_double() / 2.0
                + if count > 3 { adjustment } else { 0.0 },
        );
        let crack = (ctx.random.next_float() as f64) < self.crack_chance;
        let mut points = Vec::new();
        let mut invalid = 0;
        for _ in 0..count {
            let p = offset(
                ctx.origin,
                [
                    self.outer_distance.sample(ctx.random),
                    self.outer_distance.sample(ctx.random),
                    self.outer_distance.sample(ctx.random),
                ],
            );
            let state = ctx.region.block(p)?;
            if is_air(state) || self.invalid.contains(&state.block()) {
                invalid += 1;
                if invalid > self.invalid_threshold {
                    return Ok(false);
                }
            }
            points.push((p, self.point_offset.sample(ctx.random)));
        }
        let mut cracks = Vec::new();
        if crack {
            let index = ctx.random.next_int_bound(4);
            let d = count * 2 + 1;
            let x = if index == 0 || index == 2 { d } else { 0 };
            let z = if index == 1 || index == 2 { d } else { 0 };
            cracks.extend([7, 5, 1].map(|y| offset(ctx.origin, [x, y, z])));
        }
        let mut potential = Vec::new();
        let min = self.min.min(self.max);
        let max = self.min.max(self.max);
        for z in min..=max {
            for y in min..=max {
                for x in min..=max {
                    let p = offset(ctx.origin, [x, y, z]);
                    let noise_offset = noise.get(p[0] as f64, p[1] as f64, p[2] as f64) as f64
                        * self.noise_multiplier;
                    let distance = |q: [i32; 3]| {
                        (p[0] as f64 - q[0] as f64).powi(2)
                            + (p[1] as f64 - q[1] as f64).powi(2)
                            + (p[2] as f64 - q[2] as f64).powi(2)
                    };
                    let shell = points
                        .iter()
                        .map(|(q, o)| mth::inv_sqrt(distance(*q) + *o as f64) + noise_offset)
                        .sum::<f64>();
                    if shell < thresholds[3] {
                        continue;
                    }
                    if shell >= thresholds[0] {
                        let state = self.providers[0].get_state(ctx, p)?;
                        self.safe_set(ctx, p, state)?;
                    } else {
                        let crack_sum = cracks
                            .iter()
                            .map(|q| {
                                mth::inv_sqrt(distance(*q) + self.crack_offset as f64)
                                    + noise_offset
                            })
                            .sum::<f64>();
                        if crack && crack_sum >= crack_size {
                            self.safe_set(ctx, p, Block::AIR.default_state())?;
                            for d in Direction::VALUES {
                                let adjacent = move_direction(p, d);
                                let fluid = ctx.region.fluid(adjacent)?;
                                if !fluid.empty {
                                    ctx.region.schedule_fluid_tick(SavedTick {
                                        position: adjacent,
                                        kind: fluid.fluid_type,
                                        priority: 0,
                                    })?;
                                }
                            }
                        } else if shell >= thresholds[1] {
                            let alternate =
                                (ctx.random.next_float() as f64) < self.alternate_chance;
                            let state =
                                self.providers[if alternate { 2 } else { 1 }].get_state(ctx, p)?;
                            self.safe_set(ctx, p, state)?;
                            if (!self.require_alternate || alternate)
                                && (ctx.random.next_float() as f64) < self.placement_chance
                            {
                                potential.push(p);
                            }
                        } else {
                            let state = self.providers[if shell >= thresholds[2] { 3 } else { 4 }]
                                .get_state(ctx, p)?;
                            self.safe_set(ctx, p, state)?;
                        }
                    }
                }
            }
        }
        for p in potential {
            let mut state =
                self.placements[ctx.random.next_int_bound(self.placements.len() as i32) as usize];
            for d in Direction::VALUES {
                let facing = match d {
                    Direction::Down => Facing::Down,
                    Direction::Up => Facing::Up,
                    Direction::North => Facing::North,
                    Direction::South => Facing::South,
                    Direction::West => Facing::West,
                    Direction::East => Facing::East,
                };
                state = state.with(properties::FACING, facing).unwrap_or(state);
                let q = move_direction(p, d);
                let old = ctx.region.block(q)?;
                state = state
                    .with(properties::WATERLOGGED, old.data().fluid.source)
                    .unwrap_or(state);
                // BuddingAmethystBlock#canClusterGrowAtState is state-only.
                if is_air(old)
                    || old.block() == Block::WATER && old.get(properties::LEVEL) == Some(0)
                {
                    self.safe_set(ctx, q, state)?;
                    break;
                }
            }
        }
        Ok(true)
    }
}
