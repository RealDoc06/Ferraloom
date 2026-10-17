//! Ports of 26.3's densityfunction generators and operations.
mod decode;
mod equality;
mod sampler;

use crate::{Error, Identifier};
pub use ferraloom_util::noise::NoiseVolume as DensityVolume;
use ferraloom_util::noise::{NoiseParameters, NormalNoise};
pub use sampler::{DensityFunctionCompiler, Interval, NodeId, SamplerContext};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub enum Holder<T> {
    Reference(Identifier),
    Inline(T),
}
pub type Function = Arc<DensityFunction>;

/// Port of NormalNoise.Parameters' codec, with validation before construction.
#[derive(Clone, Debug)]
pub struct NoiseSpec {
    pub base_amplitude: f64,
    pub base_octave: i32,
    pub octave_count: usize,
    pub normalization: Normalization,
    pub amplitude_modifiers: Vec<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Normalization {
    Enabled,
    Disabled,
    Legacy,
}
impl NoiseSpec {
    pub fn noise(&self) -> NormalNoise {
        NormalNoise::new(NoiseParameters {
            base_amplitude: self.base_amplitude,
            base_octave: self.base_octave,
            octave_count: self.octave_count,
            amplitude_modifiers: self.amplitude_modifiers.clone(),
            normalize: match self.normalization {
                Normalization::Enabled => ferraloom_util::noise::Normalization::Enabled,
                Normalization::Disabled => ferraloom_util::noise::Normalization::Disabled,
                Normalization::Legacy => ferraloom_util::noise::Normalization::Legacy,
            },
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    X,
    Y,
    Z,
}
impl Axis {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TilingMode {
    ClampToEdge,
    Repeat,
    MirroredRepeat,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DistanceMetric {
    Euclidean,
    EuclideanSquared,
    Manhattan,
    Chebyshev,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unary {
    Abs,
    Square,
    Cube,
    Sqrt,
    HalfNegative,
    QuarterNegative,
    Reciprocal,
    Negate,
    Squeeze,
    Log,
    Sign,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Binary {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Round {
    Floor,
    Round,
    Ceil,
    Truncate,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Simple {
    BlendAlpha,
    BlendOffset,
    Beardifier,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shift {
    All,
    A,
    B,
}

/// Port of CubicSpline. Nested values retain float locations and derivatives.
#[derive(Clone, Debug)]
pub enum CubicSpline {
    Constant(f32),
    Multipoint {
        coordinate: Function,
        points: Vec<SplinePoint>,
    },
}
#[derive(Clone, Debug)]
pub struct SplinePoint {
    pub location: f32,
    pub value: CubicSpline,
    pub derivative: f32,
}

/// Every density function registered by DensityFunctions#bootstrap in 26.3.
#[derive(Clone, Debug)]
pub enum DensityFunction {
    Reference(Identifier),
    Constant(f32),
    Simple(Simple),
    Noise {
        noise: Holder<NoiseSpec>,
        xz_scale: f64,
        y_scale: f64,
        shift: [Function; 3],
    },
    Shift {
        kind: Shift,
        noise: Holder<NoiseSpec>,
    },
    EndOuterIslands,
    DistanceToPoint {
        point: [i32; 3],
        metric: DistanceMetric,
    },
    Gradient {
        axis: Axis,
        tiling: TilingMode,
        from: i32,
        to: i32,
        from_value: f32,
        to_value: f32,
    },
    Unary {
        kind: Unary,
        input: Function,
    },
    Binary {
        kind: Binary,
        left: Function,
        right: Function,
    },
    Round {
        kind: Round,
        input: Function,
        multiple: Function,
    },
    Pow {
        base: Function,
        exponent: Function,
    },
    Spline(CubicSpline),
    Lerp {
        alpha: Function,
        first: Function,
        second: Function,
    },
    Clamp {
        input: Function,
        min: f32,
        max: f32,
    },
    RangeChoice {
        input: Function,
        min: f32,
        max: f32,
        inside: Function,
        outside: Function,
    },
    IntervalSelect {
        input: Function,
        thresholds: Vec<f32>,
        functions: Vec<Function>,
    },
    Cache(Function),
    BlendDensity(Function),
    Interpolated {
        input: Function,
        cell_xz: i32,
        cell_y: i32,
    },
    Slice {
        input: Function,
        axis: Axis,
        coordinate: i32,
    },
    FindTopSurface {
        density: Function,
        upper: Function,
        lower: i32,
        cell_height: i32,
    },
    OldBlendedNoise {
        xz_scale: f64,
        y_scale: f64,
        xz_factor: f64,
        y_factor: f64,
        smear_scale: f64,
    },
}
impl DensityFunction {
    pub fn decode(value: &Value) -> Result<Function, Error> {
        decode::function(value)
    }
}

/// Registry definitions stay seed-independent and can be shared by dimensions.
#[derive(Clone, Debug, Default)]
pub struct DensityRegistry {
    pub functions: BTreeMap<Identifier, Function>,
    pub noises: BTreeMap<Identifier, NoiseSpec>,
}
impl DensityRegistry {
    pub fn load(resources: &ferraloom_data::datapack::ResourceManager) -> Result<Self, Error> {
        let functions = resources
            .registry("worldgen/density_function")
            .into_iter()
            .map(|(id, r)| Ok((id, DensityFunction::decode(&r.json()?)?)))
            .collect::<Result<_, Error>>()?;
        let noises = resources
            .registry("worldgen/noise")
            .into_iter()
            .map(|(id, r)| Ok((id, NoiseSpec::decode(&r.json()?)?)))
            .collect::<Result<_, Error>>()?;
        Ok(Self { functions, noises })
    }
}
