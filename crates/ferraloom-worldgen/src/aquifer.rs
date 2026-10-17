//! Port of Aquifer's disabled and noise-based implementations.
use crate::{Error, density::*, settings::AquiferConfig};
use ferraloom_data::{Block, BlockState};
use ferraloom_util::{
    BlockPos, mth,
    random::{PositionalRandomFactory, RandomSource},
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FluidStatus {
    pub level: i32,
    pub state: BlockState,
}
impl FluidStatus {
    pub fn at(self, y: i32) -> BlockState {
        if y < self.level {
            self.state
        } else {
            Block::AIR.default_state()
        }
    }
}
/// Port of NoiseBasedChunkGenerator#createFluidPicker.
#[derive(Clone, Copy, Debug)]
pub struct FluidPicker {
    pub sea_level: i32,
    pub default_fluid: BlockState,
}
impl FluidPicker {
    pub fn compute(self, y: i32) -> FluidStatus {
        if y < (-54).min(self.sea_level) {
            FluidStatus {
                level: -54,
                state: Block::LAVA.default_state(),
            }
        } else {
            FluidStatus {
                level: self.sea_level,
                state: self.default_fluid,
            }
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CompiledAquifer {
    functions: [NodeId; 6],
    random: PositionalRandomFactory,
}
impl AquiferConfig {
    pub fn compile(
        &self,
        compiler: &mut DensityFunctionCompiler,
    ) -> Result<CompiledAquifer, Error> {
        let functions = self
            .functions
            .iter()
            .map(|f| compiler.compile(f))
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .unwrap();
        Ok(CompiledAquifer {
            functions,
            random: compiler.positional_random(&"aquifer".parse().unwrap()),
        })
    }
}
/// One aquifer per chunk. All locations, statuses, surface samples and update
/// decisions are local to this value, never stored in shared RandomState.
#[derive(Debug)]
pub struct Aquifer {
    config: Option<CompiledAquifer>,
    picker: FluidPicker,
    locations: HashMap<[i32; 3], [i32; 3]>,
    statuses: HashMap<[i32; 3], FluidStatus>,
    surface: HashMap<[i32; 2], i32>,
    skip_above: i32,
    should_update: bool,
}
impl Aquifer {
    pub fn new(
        config: Option<CompiledAquifer>,
        picker: FluidPicker,
        compiler: &DensityFunctionCompiler,
        context: &mut SamplerContext,
        volume: DensityVolume,
    ) -> Self {
        let mut aquifer = Self {
            config,
            picker,
            locations: HashMap::new(),
            statuses: HashMap::new(),
            surface: HashMap::new(),
            skip_above: i32::MAX,
            should_update: false,
        };
        if let Some(config) = config {
            let min_x = ((volume.min[0] - 5) >> 4) * 16;
            let min_z = ((volume.min[2] - 5) >> 4) * 16;
            let max_x = (((volume.min[0] + (volume.size[0] as i32 - 1) * volume.step[0] - 5) >> 4)
                + 1)
                * 16
                + 9;
            let max_z = (((volume.min[2] + (volume.size[2] as i32 - 1) * volume.step[2] - 5) >> 4)
                + 1)
                * 16
                + 9;
            let min = [min_x >> 2, min_z >> 2];
            let max = [max_x >> 2, max_z >> 2];
            let grid = DensityVolume {
                size: [
                    (max[0] - min[0] + 1) as usize,
                    1,
                    (max[1] - min[1] + 1) as usize,
                ],
                min: [min[0] * 4, 0, min[1] * 4],
                step: [4, 1, 4],
            };
            let buffer = compiler.sample_volume(config.functions[5], context, grid);
            let mut highest = i32::MIN;
            for (i, v) in buffer.into_iter().enumerate() {
                let [x, _, z] = grid.position(i);
                let y = mth::floor(f64::from(v));
                aquifer.surface.insert([x, z], y);
                highest = highest.max(y);
            }
            aquifer.skip_above = ((highest + 8 + 12).div_euclid(12) + 1) * 12 + 10;
        }
        aquifer
    }
    pub fn should_schedule_fluid_update(&self) -> bool {
        self.should_update
    }
    pub fn compute_substance(
        &mut self,
        compiler: &DensityFunctionCompiler,
        context: &mut SamplerContext,
        p: [i32; 3],
        density: f64,
    ) -> Option<BlockState> {
        self.should_update = false;
        if density > 0.0 {
            return None;
        }
        let global = self.picker.compute(p[1]);
        let Some(config) = self.config else {
            return Some(global.at(p[1]));
        };
        if p[1] > self.skip_above {
            return Some(global.at(p[1]));
        }
        if global.at(p[1]).block() == Block::LAVA {
            return Some(Block::LAVA.default_state());
        }
        let anchor = [(p[0] - 5) >> 4, (p[1] + 1).div_euclid(12), (p[2] - 5) >> 4];
        let mut nearest = [(i32::MAX, [0; 3]); 4];
        for dx in 0..=1 {
            for dy in -1..=1 {
                for dz in 0..=1 {
                    let key = [anchor[0] + dx, anchor[1] + dy, anchor[2] + dz];
                    let location = *self.locations.entry(key).or_insert_with(|| {
                        let mut r = config.random.at(key[0], key[1], key[2]);
                        let p = BlockPos::new(
                            key[0] * 16 + r.next_int_bound(10),
                            key[1] * 12 + r.next_int_bound(9),
                            key[2] * 16 + r.next_int_bound(10),
                        );
                        let p = BlockPos::of(p.as_long());
                        [p.x, p.y, p.z]
                    });
                    let distance = location
                        .into_iter()
                        .zip(p)
                        .map(|(a, b)| a.wrapping_sub(b).wrapping_mul(a.wrapping_sub(b)))
                        .fold(0i32, i32::wrapping_add);
                    if let Some(index) = nearest.iter().position(|(d, _)| *d >= distance) {
                        for j in (index + 1..4).rev() {
                            nearest[j] = nearest[j - 1];
                        }
                        nearest[index] = (distance, key);
                    }
                }
            }
        }
        let s1 = self.status(compiler, context, nearest[0].1);
        let sim12 = similarity(nearest[0].0, nearest[1].0);
        let fluid = s1.at(p[1]);
        const FLOW: f64 = 1.0 - (144.0 - 100.0) / 25.0;
        if sim12 <= 0.0 {
            if sim12 >= FLOW {
                self.should_update = s1 != self.status(compiler, context, nearest[1].1);
            }
            return Some(fluid);
        }
        if fluid.block() == Block::WATER
            && self.picker.compute(p[1] - 1).at(p[1] - 1).block() == Block::LAVA
        {
            self.should_update = true;
            return Some(fluid);
        }
        let mut barrier = f64::NAN;
        let s2 = self.status(compiler, context, nearest[1].1);
        if density + sim12 * self.pressure(compiler, context, p, &mut barrier, s1, s2) > 0.0 {
            return None;
        }
        let s3 = self.status(compiler, context, nearest[2].1);
        let sim13 = similarity(nearest[0].0, nearest[2].0);
        if sim13 > 0.0
            && density + sim12 * sim13 * self.pressure(compiler, context, p, &mut barrier, s1, s3)
                > 0.0
        {
            return None;
        }
        let sim23 = similarity(nearest[1].0, nearest[2].0);
        if sim23 > 0.0
            && density + sim12 * sim23 * self.pressure(compiler, context, p, &mut barrier, s2, s3)
                > 0.0
        {
            return None;
        }
        self.should_update = s1 != s2 || (sim23 >= FLOW && s2 != s3) || (sim13 >= FLOW && s1 != s3);
        if !self.should_update {
            self.should_update = sim13 >= FLOW
                && similarity(nearest[0].0, nearest[3].0) >= FLOW
                && s1 != self.status(compiler, context, nearest[3].1);
        }
        Some(fluid)
    }
    fn sample(
        &self,
        compiler: &DensityFunctionCompiler,
        context: &mut SamplerContext,
        index: usize,
        p: [i32; 3],
    ) -> f32 {
        compiler.sample_value(self.config.unwrap().functions[index], context, p)
    }
    fn pressure(
        &self,
        compiler: &DensityFunctionCompiler,
        context: &mut SamplerContext,
        p: [i32; 3],
        barrier: &mut f64,
        a: FluidStatus,
        b: FluidStatus,
    ) -> f64 {
        let ta = a.at(p[1]).block();
        let tb = b.at(p[1]).block();
        if (ta == Block::LAVA && tb == Block::WATER) || (ta == Block::WATER && tb == Block::LAVA) {
            return 2.0;
        }
        let diff = (a.level - b.level).abs();
        if diff == 0 {
            return 0.0;
        }
        let above = f64::from(p[1]) + 0.5 - 0.5 * f64::from(a.level + b.level);
        let distance = f64::from(diff) / 2.0 - above.abs();
        let gradient = if above > 0.0 {
            if distance > 0.0 {
                distance / 1.5
            } else {
                distance / 2.5
            }
        } else {
            let center = 3.0 + distance;
            if center > 0.0 {
                center / 3.0
            } else {
                center / 10.0
            }
        };
        let noise = if (-2.0..=2.0).contains(&gradient) {
            if barrier.is_nan() {
                *barrier = f64::from(self.sample(compiler, context, 0, p));
            }
            *barrier
        } else {
            0.0
        };
        2.0 * (noise + gradient)
    }
    fn surface_level(
        &mut self,
        compiler: &DensityFunctionCompiler,
        context: &mut SamplerContext,
        x: i32,
        z: i32,
    ) -> i32 {
        let key = [(x >> 2) << 2, (z >> 2) << 2];
        if let Some(v) = self.surface.get(&key) {
            *v
        } else {
            let value = mth::floor(f64::from(self.sample(
                compiler,
                context,
                5,
                [key[0], 0, key[1]],
            )));
            self.surface.insert(key, value);
            value
        }
    }
    fn status(
        &mut self,
        compiler: &DensityFunctionCompiler,
        context: &mut SamplerContext,
        key: [i32; 3],
    ) -> FluidStatus {
        if let Some(status) = self.statuses.get(&key) {
            return *status;
        }
        let p = self.locations[&key];
        let status = self.compute_fluid(compiler, context, p);
        self.statuses.insert(key, status);
        status
    }
    fn compute_fluid(
        &mut self,
        compiler: &DensityFunctionCompiler,
        context: &mut SamplerContext,
        p: [i32; 3],
    ) -> FluidStatus {
        let global = self.picker.compute(p[1]);
        let mut lowest = i32::MAX;
        let mut underwater = false;
        const OFFSETS: [[i32; 2]; 13] = [
            [0, 0],
            [-2, -1],
            [-1, -1],
            [0, -1],
            [1, -1],
            [-3, 0],
            [-2, 0],
            [-1, 0],
            [1, 0],
            [-2, 1],
            [-1, 1],
            [0, 1],
            [1, 1],
        ];
        for [dx, dz] in OFFSETS {
            let x = p[0] + dx * 16;
            let z = p[2] + dz * 16;
            let surface = self.surface_level(compiler, context, x, z);
            let adjusted = surface + 8;
            let center = dx == 0 && dz == 0;
            if center && p[1] - 12 > adjusted {
                return global;
            }
            let pokes = p[1] + 12 > adjusted;
            if pokes || center {
                let at_surface = self.picker.compute(adjusted);
                if at_surface.at(adjusted).block() != Block::AIR {
                    if center {
                        underwater = true;
                    }
                    if pokes {
                        return at_surface;
                    }
                }
            }
            lowest = lowest.min(surface);
        }
        let level = if self.sample(compiler, context, 4, p) > 0.0 {
            -32512
        } else {
            let factor = if underwater {
                mth::clamped_map(f64::from(lowest + 8 - p[1]), 0.0, 64.0, 1.0, 0.0)
            } else {
                0.0
            };
            let noise = f64::from(self.sample(compiler, context, 1, p)).clamp(-1.0, 1.0);
            let full = mth::map(factor, 1.0, 0.0, -0.3, 0.8);
            let partial = mth::map(factor, 1.0, 0.0, -0.8, 0.4);
            if noise - full > 0.0 {
                global.level
            } else if noise - partial > 0.0 {
                let cell = [
                    p[0].div_euclid(16),
                    p[1].div_euclid(40),
                    p[2].div_euclid(16),
                ];
                let spread = f64::from(self.sample(compiler, context, 2, cell) * 10.0);
                lowest.min(cell[1] * 40 + 20 + mth::quantize(spread, 3))
            } else {
                -32512
            }
        };
        let mut state = global.state;
        if level <= -10
            && level != -32512
            && state != Block::LAVA.default_state()
            && f64::from(self.sample(
                compiler,
                context,
                3,
                [
                    p[0].div_euclid(64),
                    p[1].div_euclid(40),
                    p[2].div_euclid(64),
                ],
            ))
            .abs()
                > 0.3
        {
            state = Block::LAVA.default_state();
        }
        FluidStatus { level, state }
    }
}
fn similarity(a: i32, b: i32) -> f64 {
    1.0 - f64::from(b - a) / 25.0
}
