//! Ports of the 26.3 `levelgen.synth` primitives. This version uses float
//! gradients and sums; the former ImprovedNoise/PerlinSimplexNoise classes no
//! longer exist. Density-volume integration belongs to the worldgen crate.
use crate::{java, mth, random::RandomSource};

const GRADIENT: [[i32; 3]; 16] = [
    [1, 1, 0],
    [-1, 1, 0],
    [1, -1, 0],
    [-1, -1, 0],
    [1, 0, 1],
    [-1, 0, 1],
    [1, 0, -1],
    [-1, 0, -1],
    [0, 1, 1],
    [0, -1, 1],
    [0, 1, -1],
    [0, -1, -1],
    [1, 1, 0],
    [0, -1, 1],
    [-1, 1, 0],
    [0, -1, -1],
];

/// Port of GradientNoise's shared permutation and random offsets.
#[derive(Clone, Debug)]
pub struct GradientNoise {
    perms: [u8; 256],
    pub offset_x: f64,
    pub offset_y: f64,
    pub offset_z: f64,
}
impl GradientNoise {
    pub fn new(random: &mut impl RandomSource, offset_scale: f64) -> Self {
        let offset_x = random.next_double() * offset_scale;
        let offset_y = random.next_double() * offset_scale;
        let offset_z = random.next_double() * offset_scale;
        let mut perms = std::array::from_fn(|i| i as u8);
        for i in 0..256 {
            let offset = random.next_int_bound((256 - i) as i32) as usize;
            perms.swap(i, i + offset);
        }
        Self {
            perms,
            offset_x,
            offset_y,
            offset_z,
        }
    }
    #[inline]
    fn permute(&self, x: i32) -> i32 {
        i32::from(self.perms[(x & 255) as usize])
    }
    #[inline]
    fn gradient(&self, x: i32) -> [i32; 3] {
        GRADIENT[(self.permute(x) & 15) as usize]
    }
    /// Port of GradientNoise#wrap with the default DEBUG_ENABLE_FARLANDS=false.
    #[inline]
    pub fn wrap(x: f64) -> f64 {
        const HALF: f64 = f64::from_bits(0x416fffffffffffff); // Math.nextDown(16777216.0)
        if (-HALF..HALF).contains(&x) {
            x
        } else {
            x - (x / 33554432.0 + 0.5).floor() * 33554432.0
        }
    }
}
#[inline]
fn dot_f32(g: [i32; 3], x: f32, y: f32, z: f32) -> f32 {
    g[0] as f32 * x + g[1] as f32 * y + g[2] as f32 * z
}
#[inline]
fn dot(g: [i32; 3], x: f64, y: f64, z: f64) -> f64 {
    f64::from(g[0]) * x + f64::from(g[1]) * y + f64::from(g[2]) * z
}
#[inline]
fn lerp_corners(x: f32, y: f32, z: f32, d: [f32; 8]) -> f32 {
    mth::lerp3_f32(x, y, z, d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7])
}

/// Coordinate layout used by Noise#addToVolume: Y contiguous, then X, then Z.
/// Port of the coordinate-only part of DensityVolume.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NoiseVolume {
    pub size: [usize; 3],
    pub min: [i32; 3],
    pub step: [i32; 3],
}
impl NoiseVolume {
    pub fn len(self) -> usize {
        self.size.iter().product()
    }
    pub fn is_empty(self) -> bool {
        self.size.contains(&0)
    }
    pub fn index(self, x: usize, y: usize, z: usize) -> usize {
        (z * self.size[0] + x) * self.size[1] + y
    }
    pub fn position(self, index: usize) -> [i32; 3] {
        let coords = [
            index / self.size[1] % self.size[0],
            index % self.size[1],
            index / (self.size[0] * self.size[1]),
        ];
        std::array::from_fn(|i| {
            self.min[i].wrapping_add((coords[i] as i32).wrapping_mul(self.step[i]))
        })
    }
}

/// Port of the Noise interface. Range bounds are inclusive floats.
pub trait Noise {
    fn get_2d(&self, x: f64, y: f64) -> f32;
    fn get(&self, x: f64, y: f64, z: f64) -> f32;
    fn range(&self) -> [f32; 2];
    fn add_to_volume(
        &self,
        buffer: &mut [f32],
        volume: NoiseVolume,
        xz_scale: f64,
        y_scale: f64,
        amplitude: f32,
    ) {
        assert_eq!(buffer.len(), volume.len());
        for (index, value) in buffer.iter_mut().enumerate() {
            let [x, y, z] = volume.position(index);
            *value += amplitude
                * self.get(
                    f64::from(x) * xz_scale,
                    f64::from(y) * y_scale,
                    f64::from(z) * xz_scale,
                );
        }
    }
}
/// Port of PerlinNoise, the single-octave primitive in 26.3.
#[derive(Clone, Debug)]
pub struct PerlinNoise {
    pub gradient: GradientNoise,
}
impl PerlinNoise {
    pub const STANDARD_DEVIATION: f64 = 0.2702247831245211;
    pub fn new(random: &mut impl RandomSource) -> Self {
        Self {
            gradient: GradientNoise::new(random, 256.0),
        }
    }
    fn coordinates(&self, x: f64, y: f64, z: f64) -> ([i32; 3], [f32; 3]) {
        let x = GradientNoise::wrap(x) + self.gradient.offset_x;
        let y = GradientNoise::wrap(y) + self.gradient.offset_y;
        let z = GradientNoise::wrap(z) + self.gradient.offset_z;
        let ix = mth::floor(x);
        let iy = mth::floor(y);
        let iz = mth::floor(z);
        (
            [ix, iy, iz],
            [
                (x - f64::from(ix)) as f32,
                (y - f64::from(iy)) as f32,
                (z - f64::from(iz)) as f32,
            ],
        )
    }
    fn corners(&self, [x, y, z]: [i32; 3], [xr, yr, zr]: [f32; 3]) -> ([[i32; 3]; 8], [f32; 8]) {
        let p = &self.gradient;
        let x0 = p.permute(x);
        let x1 = p.permute(x.wrapping_add(1));
        let xy00 = p.permute(x0.wrapping_add(y));
        let xy01 = p.permute(x0.wrapping_add(y).wrapping_add(1));
        let xy10 = p.permute(x1.wrapping_add(y));
        let xy11 = p.permute(x1.wrapping_add(y).wrapping_add(1));
        let gradients = [
            p.gradient(xy00.wrapping_add(z)),
            p.gradient(xy10.wrapping_add(z)),
            p.gradient(xy01.wrapping_add(z)),
            p.gradient(xy11.wrapping_add(z)),
            p.gradient(xy00.wrapping_add(z).wrapping_add(1)),
            p.gradient(xy10.wrapping_add(z).wrapping_add(1)),
            p.gradient(xy01.wrapping_add(z).wrapping_add(1)),
            p.gradient(xy11.wrapping_add(z).wrapping_add(1)),
        ];
        let values = std::array::from_fn(|i| {
            dot_f32(
                gradients[i],
                xr - (i & 1) as f32,
                yr - ((i >> 1) & 1) as f32,
                zr - ((i >> 2) & 1) as f32,
            )
        });
        (gradients, values)
    }
    fn sample_fudged(&self, x: f64, y: f64, z: f64, fudge: Option<f64>, volume: bool) -> f32 {
        let (cell, [xr, _, zr]) = self.coordinates(x, y, z);
        let absolute_y = GradientNoise::wrap(y) + self.gradient.offset_y;
        let relative_y = absolute_y - f64::from(cell[1]);
        let original_y = relative_y as f32;
        let yr = if let Some(scale) = fudge {
            let limit = if y >= 0.0 && y < relative_y {
                y
            } else {
                relative_y
            };
            (relative_y - f64::from(mth::floor(limit / scale + 1.0000000116860974E-7)) * scale)
                as f32
        } else {
            original_y
        };
        let (gradients, scalar) = self.corners(cell, [xr, yr, zr]);
        let values = if volume {
            std::array::from_fn(|i| {
                let g = gradients[i];
                (g[0] as f32 * (xr - (i & 1) as f32) + g[2] as f32 * (zr - ((i >> 2) & 1) as f32))
                    + g[1] as f32 * (yr - ((i >> 1) & 1) as f32)
            })
        } else {
            scalar
        };
        lerp_corners(
            mth::smoothstep_f32(xr),
            mth::smoothstep_f32(original_y),
            mth::smoothstep_f32(zr),
            values,
        )
    }
    fn add_volume_fudged(
        &self,
        buffer: &mut [f32],
        volume: NoiseVolume,
        xz_scale: f64,
        y_scale: f64,
        amplitude: f32,
        fudge: Option<f64>,
    ) {
        assert_eq!(buffer.len(), volume.len());
        for (index, value) in buffer.iter_mut().enumerate() {
            let [x, y, z] = volume.position(index);
            *value += amplitude
                * self.sample_fudged(
                    f64::from(x) * xz_scale,
                    f64::from(y) * y_scale,
                    f64::from(z) * xz_scale,
                    fudge,
                    true,
                );
        }
    }
    /// Port of PerlinNoise#noiseWithDerivative. Adds to the caller's derivative.
    pub fn noise_with_derivative(&self, x: f64, y: f64, z: f64, derivative: &mut [f32; 3]) -> f32 {
        let (cell, [xr, yr, zr]) = self.coordinates(x, y, z);
        let (g, d) = self.corners(cell, [xr, yr, zr]);
        let xa = mth::smoothstep_f32(xr);
        let ya = mth::smoothstep_f32(yr);
        let za = mth::smoothstep_f32(zr);
        let d1: [f32; 3] =
            std::array::from_fn(|axis| lerp_corners(xa, ya, za, g.map(|g| g[axis] as f32)));
        let d2x = mth::lerp2_f32(ya, za, d[1] - d[0], d[3] - d[2], d[5] - d[4], d[7] - d[6]);
        let d2y = mth::lerp2_f32(za, xa, d[2] - d[0], d[6] - d[4], d[3] - d[1], d[7] - d[5]);
        let d2z = mth::lerp2_f32(xa, ya, d[4] - d[0], d[5] - d[1], d[6] - d[2], d[7] - d[3]);
        derivative[0] += d1[0] + mth::smoothstep_derivative_f32(xr) * d2x;
        derivative[1] += d1[1] + mth::smoothstep_derivative_f32(yr) * d2y;
        derivative[2] += d1[2] + mth::smoothstep_derivative_f32(zr) * d2z;
        lerp_corners(xa, ya, za, d)
    }
}
impl Noise for PerlinNoise {
    fn add_to_volume(
        &self,
        buffer: &mut [f32],
        volume: NoiseVolume,
        xz_scale: f64,
        y_scale: f64,
        amplitude: f32,
    ) {
        self.add_volume_fudged(buffer, volume, xz_scale, y_scale, amplitude, None);
    }
    #[inline]
    fn get_2d(&self, x: f64, y: f64) -> f32 {
        self.get(GradientNoise::wrap(x), 0.0, GradientNoise::wrap(y))
    }
    #[inline]
    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let (cell, [xr, yr, zr]) = self.coordinates(x, y, z);
        let (_, values) = self.corners(cell, [xr, yr, zr]);
        lerp_corners(
            mth::smoothstep_f32(xr),
            mth::smoothstep_f32(yr),
            mth::smoothstep_f32(zr),
            values,
        )
    }
    fn range(&self) -> [f32; 2] {
        [-2.0, 2.0]
    }
}
/// Port of SmearedPerlinNoise, used exclusively by old_blended_noise.
#[derive(Clone, Debug)]
pub struct SmearedPerlinNoise {
    perlin: PerlinNoise,
    fudge_y_scale: f64,
}
impl SmearedPerlinNoise {
    pub fn new(random: &mut impl RandomSource, fudge_y_scale: f64) -> Self {
        Self {
            perlin: PerlinNoise::new(random),
            fudge_y_scale,
        }
    }
}
impl Noise for SmearedPerlinNoise {
    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        self.perlin
            .sample_fudged(x, y, z, Some(self.fudge_y_scale), false)
    }
    fn get_2d(&self, x: f64, y: f64) -> f32 {
        self.get(GradientNoise::wrap(x), 0.0, GradientNoise::wrap(y))
    }
    fn range(&self) -> [f32; 2] {
        let max = (self.fudge_y_scale.abs() + 2.0) as f32;
        [-max, max]
    }
    fn add_to_volume(
        &self,
        buffer: &mut [f32],
        volume: NoiseVolume,
        xz_scale: f64,
        y_scale: f64,
        amplitude: f32,
    ) {
        self.perlin.add_volume_fudged(
            buffer,
            volume,
            xz_scale,
            y_scale,
            amplitude,
            Some(self.fudge_y_scale),
        );
    }
}

/// Port of SimplexNoise. Unlike old versions, get() includes the random offsets.
#[derive(Clone, Debug)]
pub struct SimplexNoise {
    pub gradient: GradientNoise,
}
impl SimplexNoise {
    pub const STANDARD_DEVIATION: f64 = 0.42544;
    pub fn new(random: &mut impl RandomSource) -> Self {
        Self::with_discarded_offset(random, false)
    }
    pub fn with_discarded_offset(random: &mut impl RandomSource, discard: bool) -> Self {
        Self {
            gradient: GradientNoise::new(random, if discard { 0.0 } else { 256.0 }),
        }
    }
    #[inline]
    fn corner(index: usize, x: f64, y: f64, z: f64, base: f64) -> f64 {
        let mut t = base - x * x - y * y - z * z;
        if t < 0.0 {
            0.0
        } else {
            t *= t;
            t * t * dot(GRADIENT[index], x, y, z)
        }
    }
}
impl Noise for SimplexNoise {
    fn range(&self) -> [f32; 2] {
        [-2.0, 2.0]
    }
    fn get_2d(&self, x: f64, y: f64) -> f32 {
        let x = x + self.gradient.offset_x;
        let y = y + self.gradient.offset_y;
        let sqrt3 = 3.0_f64.sqrt();
        let f2 = 0.5 * (sqrt3 - 1.0);
        let g2 = (3.0 - sqrt3) / 6.0;
        let s = (x + y) * f2;
        let i = mth::floor(x + s);
        let j = mth::floor(y + s);
        let t = f64::from(i.wrapping_add(j)) * g2;
        let x0 = x - (f64::from(i) - t);
        let y0 = y - (f64::from(j) - t);
        let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
        let x1 = x0 - f64::from(i1) + g2;
        let y1 = y0 - f64::from(j1) + g2;
        let x2 = x0 - 1.0 + 2.0 * g2;
        let y2 = y0 - 1.0 + 2.0 * g2;
        let ii = i & 255;
        let jj = j & 255;
        let p = &self.gradient;
        let gi0 = (p.permute(ii + p.permute(jj)) % 12) as usize;
        let gi1 = (p.permute(ii + i1 + p.permute(jj + j1)) % 12) as usize;
        let gi2 = (p.permute(ii + 1 + p.permute(jj + 1)) % 12) as usize;
        (70.0
            * (Self::corner(gi0, x0, y0, 0.0, 0.5)
                + Self::corner(gi1, x1, y1, 0.0, 0.5)
                + Self::corner(gi2, x2, y2, 0.0, 0.5))) as f32
    }
    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let x = x + self.gradient.offset_x;
        let y = y + self.gradient.offset_y;
        let z = z + self.gradient.offset_z;
        let s = (x + y + z) * (1.0 / 3.0);
        let i = mth::floor(x + s);
        let j = mth::floor(y + s);
        let k = mth::floor(z + s);
        let t = f64::from(i.wrapping_add(j).wrapping_add(k)) * (1.0 / 6.0);
        let x0 = x - (f64::from(i) - t);
        let y0 = y - (f64::from(j) - t);
        let z0 = z - (f64::from(k) - t);
        let ([i1, j1, k1], [i2, j2, k2]) = if x0 >= y0 {
            if y0 >= z0 {
                ([1, 0, 0], [1, 1, 0])
            } else if x0 >= z0 {
                ([1, 0, 0], [1, 0, 1])
            } else {
                ([0, 0, 1], [1, 0, 1])
            }
        } else if y0 < z0 {
            ([0, 0, 1], [0, 1, 1])
        } else if x0 < z0 {
            ([0, 1, 0], [0, 1, 1])
        } else {
            ([0, 1, 0], [1, 1, 0])
        };
        let x1 = x0 - f64::from(i1) + 1.0 / 6.0;
        let y1 = y0 - f64::from(j1) + 1.0 / 6.0;
        let z1 = z0 - f64::from(k1) + 1.0 / 6.0;
        let x2 = x0 - f64::from(i2) + 1.0 / 3.0;
        let y2 = y0 - f64::from(j2) + 1.0 / 3.0;
        let z2 = z0 - f64::from(k2) + 1.0 / 3.0;
        let x3 = x0 - 1.0 + 0.5;
        let y3 = y0 - 1.0 + 0.5;
        let z3 = z0 - 1.0 + 0.5;
        let ii = i & 255;
        let jj = j & 255;
        let kk = k & 255;
        let p = &self.gradient;
        let gi0 = (p.permute(ii + p.permute(jj + p.permute(kk))) % 12) as usize;
        let gi1 = (p.permute(ii + i1 + p.permute(jj + j1 + p.permute(kk + k1))) % 12) as usize;
        let gi2 = (p.permute(ii + i2 + p.permute(jj + j2 + p.permute(kk + k2))) % 12) as usize;
        let gi3 = (p.permute(ii + 1 + p.permute(jj + 1 + p.permute(kk + 1))) % 12) as usize;
        (32.0
            * (Self::corner(gi0, x0, y0, z0, 0.6)
                + Self::corner(gi1, x1, y1, z1, 0.6)
                + Self::corner(gi2, x2, y2, z2, 0.6)
                + Self::corner(gi3, x3, y3, z3, 0.6))) as f32
    }
}

/// Scalar primitives supported by NoiseStack. No heap indirection per sample.
#[derive(Clone, Debug)]
pub enum NoiseSampler {
    Perlin(PerlinNoise),
    Simplex(SimplexNoise),
    SmearedPerlin(SmearedPerlinNoise),
}
impl From<PerlinNoise> for NoiseSampler {
    fn from(n: PerlinNoise) -> Self {
        Self::Perlin(n)
    }
}
impl From<SmearedPerlinNoise> for NoiseSampler {
    fn from(n: SmearedPerlinNoise) -> Self {
        Self::SmearedPerlin(n)
    }
}
impl From<SimplexNoise> for NoiseSampler {
    fn from(n: SimplexNoise) -> Self {
        Self::Simplex(n)
    }
}
impl Noise for NoiseSampler {
    fn add_to_volume(
        &self,
        buffer: &mut [f32],
        volume: NoiseVolume,
        xz_scale: f64,
        y_scale: f64,
        amplitude: f32,
    ) {
        match self {
            Self::Perlin(n) => n.add_to_volume(buffer, volume, xz_scale, y_scale, amplitude),
            Self::Simplex(n) => n.add_to_volume(buffer, volume, xz_scale, y_scale, amplitude),
            Self::SmearedPerlin(n) => n.add_to_volume(buffer, volume, xz_scale, y_scale, amplitude),
        }
    }
    #[inline]
    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        match self {
            Self::Perlin(n) => n.get(x, y, z),
            Self::Simplex(n) => n.get(x, y, z),
            Self::SmearedPerlin(n) => n.get(x, y, z),
        }
    }
    #[inline]
    fn get_2d(&self, x: f64, y: f64) -> f32 {
        match self {
            Self::Perlin(n) => n.get_2d(x, y),
            Self::Simplex(n) => n.get_2d(x, y),
            Self::SmearedPerlin(n) => n.get_2d(x, y),
        }
    }
    fn range(&self) -> [f32; 2] {
        match self {
            Self::Perlin(n) => n.range(),
            Self::Simplex(n) => n.range(),
            Self::SmearedPerlin(n) => n.range(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct NoiseLayer {
    pub noise: NoiseSampler,
    pub frequency: f64,
    pub amplitude: f32,
}
/// Port of NoiseStack. Layers retain insertion order and float accumulation.
#[derive(Clone, Debug, Default)]
pub struct NoiseStack {
    layers: Vec<NoiseLayer>,
}
impl NoiseStack {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn layers(&self) -> &[NoiseLayer] {
        &self.layers
    }
    pub fn add(
        &mut self,
        noise: impl Into<NoiseSampler>,
        frequency: f64,
        amplitude: f32,
    ) -> &mut Self {
        self.layers.push(NoiseLayer {
            noise: noise.into(),
            frequency,
            amplitude,
        });
        self
    }
    pub fn add_stack(&mut self, stack: Self, frequency: f64, amplitude: f32) -> &mut Self {
        self.layers
            .extend(stack.layers.into_iter().map(|layer| NoiseLayer {
                noise: layer.noise,
                frequency: layer.frequency * frequency,
                amplitude: layer.amplitude * amplitude,
            }));
        self
    }
}
impl Noise for NoiseStack {
    fn add_to_volume(
        &self,
        buffer: &mut [f32],
        volume: NoiseVolume,
        xz_scale: f64,
        y_scale: f64,
        amplitude: f32,
    ) {
        for layer in &self.layers {
            layer.noise.add_to_volume(
                buffer,
                volume,
                xz_scale * layer.frequency,
                y_scale * layer.frequency,
                amplitude * layer.amplitude,
            );
        }
    }
    #[inline]
    fn get(&self, x: f64, y: f64, z: f64) -> f32 {
        let mut value = 0.0;
        for layer in &self.layers {
            let f = layer.frequency;
            value += layer.amplitude * layer.noise.get(x * f, y * f, z * f);
        }
        value
    }
    #[inline]
    fn get_2d(&self, x: f64, y: f64) -> f32 {
        let mut value = 0.0;
        for layer in &self.layers {
            let f = layer.frequency;
            value += layer.amplitude * layer.noise.get_2d(x * f, y * f);
        }
        value
    }
    fn range(&self) -> [f32; 2] {
        let mut range = [0.0, 0.0];
        for layer in &self.layers {
            let a = layer.amplitude;
            let mul = |b: f32| if a != 0.0 && b != 0.0 { a * b } else { 0.0 };
            let [min, max] = layer.noise.range();
            let low = mul(min);
            let high = mul(max);
            range[0] += java::min_f32(low, high);
            range[1] += java::max_f32(low, high);
        }
        range
    }
}

/// Port of NormalNoise.Normalization.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Normalization {
    Disabled,
    #[default]
    Enabled,
    Legacy,
}
/// Plain equivalent of NormalNoise.Parameters. Empty modifiers mean all ones.
#[derive(Clone, Debug)]
pub struct NoiseParameters {
    pub base_amplitude: f64,
    pub base_octave: i32,
    pub octave_count: usize,
    pub normalize: Normalization,
    pub amplitude_modifiers: Vec<f64>,
}
impl Default for NoiseParameters {
    fn default() -> Self {
        Self {
            base_amplitude: 1.0,
            base_octave: 0,
            octave_count: 1,
            normalize: Normalization::Enabled,
            amplitude_modifiers: Vec::new(),
        }
    }
}
#[derive(Clone, Debug)]
struct OctaveInfo {
    index: i32,
    frequency: f64,
    amplitude: f64,
}
/// Port of NormalNoise's seeded-stack configuration. Calling create consumes
/// two positional factories even when every amplitude modifier is zero.
#[derive(Clone, Debug)]
pub struct NormalNoise {
    pub parameters: NoiseParameters,
    octaves: Vec<OctaveInfo>,
    normalization_factor: f64,
    range: [f32; 2],
}
impl NormalNoise {
    pub fn new(parameters: NoiseParameters) -> Self {
        assert!(
            (1..=32).contains(&parameters.octave_count),
            "octave count outside [1,32]"
        );
        assert!(
            parameters.amplitude_modifiers.is_empty()
                || parameters.amplitude_modifiers.len() == parameters.octave_count,
            "amplitude modifier count mismatch"
        );
        let octaves = Self::build_octaves(&parameters);
        let mut target = compensated_sum(octaves.iter().map(|o| o.amplitude.abs()));
        let mut factor = Self::normalization(target, &octaves);
        if parameters.normalize == Normalization::Legacy && factor != 0.0 {
            let parity = Self::parity_normalization(&parameters);
            target *= parity / factor;
            factor = parity;
        }
        let max = (target * (1.0 / 3.0) * 6.0) as f32;
        Self {
            parameters,
            octaves,
            normalization_factor: factor,
            range: [-max, max],
        }
    }
    fn build_octaves(p: &NoiseParameters) -> Vec<OctaveInfo> {
        let mut frequency = 2.0_f64.powi(p.base_octave);
        let mut amplitude = p.base_amplitude;
        if p.normalize != Normalization::Disabled {
            amplitude *= 0.5_f64.powi(-((p.octave_count - 1) as i32))
                / (0.5_f64.powi(-(p.octave_count as i32)) - 1.0);
        }
        let mut octaves = Vec::new();
        for i in 0..p.octave_count {
            let modifier = if p.amplitude_modifiers.is_empty() {
                1.0
            } else {
                p.amplitude_modifiers[i]
            };
            if modifier != 0.0 {
                octaves.push(OctaveInfo {
                    index: p.base_octave.wrapping_add(i as i32),
                    frequency,
                    amplitude: amplitude * modifier,
                });
            }
            frequency *= 2.0;
            amplitude *= 0.5;
        }
        octaves
    }
    fn normalization(target: f64, octaves: &[OctaveInfo]) -> f64 {
        let mut variance = 0.0;
        for o in octaves {
            let deviation = PerlinNoise::STANDARD_DEVIATION * o.amplitude.abs();
            variance += deviation * deviation;
        }
        let deviation = variance.sqrt();
        if deviation == 0.0 {
            0.0
        } else {
            target * (1.0 / 3.0) / (deviation * 2.0_f64.sqrt())
        }
    }
    fn parity_normalization(p: &NoiseParameters) -> f64 {
        let mut min = i32::MAX;
        let mut max = i32::MIN;
        for i in 0..p.octave_count {
            if p.amplitude_modifiers.is_empty() || p.amplitude_modifiers[i] != 0.0 {
                min = min.min(i as i32);
                max = max.max(i as i32);
            }
        }
        let span = max.wrapping_sub(min);
        p.base_amplitude * 0.5 * (1.0 / 3.0) / (0.1 * (1.0 + 1.0 / f64::from(span.wrapping_add(1))))
    }
    pub fn create_parity(first_octave: i32, amplitudes: Vec<f64>) -> Self {
        assert!(!amplitudes.is_empty(), "need at least one amplitude");
        let mut p = NoiseParameters {
            base_octave: first_octave,
            octave_count: amplitudes.len(),
            amplitude_modifiers: amplitudes,
            ..NoiseParameters::default()
        };
        let octaves = Self::build_octaves(&p);
        let target = compensated_sum(octaves.iter().map(|o| o.amplitude.abs()));
        let normalization = Self::normalization(target, &octaves);
        p.base_amplitude = if normalization == 0.0 {
            1.0
        } else {
            Self::parity_normalization(&p) / normalization
        };
        Self::new(p)
    }
    pub fn range(&self) -> [f32; 2] {
        self.range
    }
    pub fn create(&self, random: &mut impl RandomSource) -> NoiseStack {
        let first = random.fork_positional();
        let second = random.fork_positional();
        let mut stack = NoiseStack::new();
        for octave in &self.octaves {
            let seed = format!("octave_{}", octave.index);
            let first_noise = PerlinNoise::new(&mut first.from_hash_of(&seed));
            let second_noise = PerlinNoise::new(&mut second.from_hash_of(&seed));
            let factor = (self.normalization_factor * octave.amplitude) as f32;
            stack.add(first_noise, octave.frequency, factor);
            stack.add(second_noise, octave.frequency * 1.0181268882175227, factor);
        }
        stack
    }
    pub fn create_for_legacy_nether_biome(&self, random: &mut impl RandomSource) -> NoiseStack {
        let p = &self.parameters;
        let amplitudes = if p.amplitude_modifiers.is_empty() {
            vec![1.0; p.octave_count]
        } else {
            p.amplitude_modifiers.clone()
        };
        let first = LegacyFbmInitializer::create_for_legacy_nether_biome(
            random,
            p.base_octave,
            &amplitudes,
        );
        let second = LegacyFbmInitializer::create_for_legacy_nether_biome(
            random,
            p.base_octave,
            &amplitudes,
        );
        let factor = (self.normalization_factor * p.base_amplitude) as f32;
        let mut stack = NoiseStack::new();
        stack.add_stack(first, 1.0, factor);
        stack.add_stack(second, 1.0181268882175227, factor);
        stack
    }
}
// DoubleStream.sum uses compensated accumulation; ordinary Iterator::sum can
// change NormalNoise's normalization with nonuniform octave amplitudes.
fn compensated_sum(values: impl Iterator<Item = f64>) -> f64 {
    let mut sum = 0.0;
    let mut compensation = 0.0;
    let mut simple = 0.0;
    for value in values {
        simple += value;
        let corrected = value - compensation;
        let next = sum + corrected;
        compensation = (next - sum) - corrected;
        sum = next;
    }
    let result = sum - compensation;
    if result.is_nan() && simple.is_infinite() {
        simple
    } else {
        result
    }
}
/// Port of LegacyFbmInitializer. Positive octaves are rejected like vanilla.
pub struct LegacyFbmInitializer;
impl LegacyFbmInitializer {
    pub fn create_for_legacy_nether_biome(
        random: &mut impl RandomSource,
        first_octave: i32,
        amplitudes: &[f64],
    ) -> NoiseStack {
        let count = amplitudes.len();
        let zero_index = first_octave.wrapping_neg();
        let mut levels: Vec<Option<PerlinNoise>> = vec![None; count];
        let zero = PerlinNoise::new(random);
        if zero_index >= 0
            && (zero_index as usize) < count
            && amplitudes[zero_index as usize] != 0.0
        {
            levels[zero_index as usize] = Some(zero);
        }
        for i in (0..zero_index).rev() {
            if (i as usize) < count && amplitudes[i as usize] != 0.0 {
                levels[i as usize] = Some(PerlinNoise::new(random));
            } else {
                random.consume_count(262);
            }
        }
        assert_eq!(
            levels.iter().filter(|o| o.is_some()).count(),
            amplitudes.iter().filter(|&&a| a != 0.0).count(),
            "failed to create noise levels"
        );
        assert!(
            zero_index >= count as i32 - 1,
            "positive octaves are temporarily disabled"
        );
        let mut frequency = 2.0_f64.powi(-zero_index);
        let mut factor = 2.0_f64.powi(count as i32 - 1) / (2.0_f64.powi(count as i32) - 1.0);
        let mut stack = NoiseStack::new();
        for (level, &amplitude) in levels.into_iter().zip(amplitudes) {
            if let Some(noise) = level {
                stack.add(noise, frequency, (factor * amplitude) as f32);
            }
            frequency *= 2.0;
            factor /= 2.0;
        }
        stack
    }
}
