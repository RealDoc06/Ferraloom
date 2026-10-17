use super::*;
use crate::codec::*;

pub(super) fn axis(v: &Value) -> Result<Axis, Error> {
    match string(v)? {
        "x" => Ok(Axis::X),
        "y" => Ok(Axis::Y),
        "z" => Ok(Axis::Z),
        _ => Err(Error("unknown axis".into())),
    }
}
impl NoiseSpec {
    pub fn decode(v: &Value) -> Result<Self, Error> {
        let base_amplitude = optional(v, "base_amplitude", 1.0, |v| {
            bounded(v, 9.999999747378752E-6, 1_000_000.0)
        })?;
        let base_octave = int_range(field(v, "base_octave")?, -32, 32)?;
        let octave_count = optional(v, "octave_count", 1, |v| int_range(v, 1, 32))? as usize;
        let normalization = optional(v, "normalize", Normalization::Enabled, |v| match v {
            Value::Bool(true) => Ok(Normalization::Enabled),
            Value::Bool(false) => Ok(Normalization::Disabled),
            Value::String(s) if s == "legacy" => Ok(Normalization::Legacy),
            _ => Err(Error("normalize must be boolean or legacy".into())),
        })?;
        let amplitude_modifiers = optional(v, "amplitude_modifiers", Vec::new(), |v| {
            list(v)?
                .iter()
                .map(|v| bounded(v, 0.0, 1_000_000.0))
                .collect()
        })?;
        if amplitude_modifiers.len() > 32
            || (!amplitude_modifiers.is_empty() && amplitude_modifiers.len() != octave_count)
        {
            return Err(Error(
                "amplitude_modifiers size differs from octave_count".into(),
            ));
        }
        Ok(Self {
            base_amplitude,
            base_octave,
            octave_count,
            normalization,
            amplitude_modifiers,
        })
    }
}
fn noise(v: &Value) -> Result<Holder<NoiseSpec>, Error> {
    if v.is_string() {
        Ok(Holder::Reference(id(v)?))
    } else {
        Ok(Holder::Inline(NoiseSpec::decode(v)?))
    }
}
fn spline(v: &Value) -> Result<CubicSpline, Error> {
    if v.is_number() {
        return Ok(CubicSpline::Constant(float(v)?));
    }
    let coordinate = function(field(v, "coordinate")?)?;
    let points = list(field(v, "points")?)?
        .iter()
        .map(|v| {
            Ok(SplinePoint {
                location: float(field(v, "location")?)?,
                value: spline(field(v, "value")?)?,
                derivative: float(field(v, "derivative")?)?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    if points.is_empty() {
        return Err(Error("multipoint spline needs at least one point".into()));
    }
    // The direct codec checks sizes, but deliberately does not sort locations.
    Ok(CubicSpline::Multipoint { coordinate, points })
}
pub(super) fn function(v: &Value) -> Result<Function, Error> {
    if v.is_number() {
        return Ok(Arc::new(DensityFunction::Constant(noise_value(v)?)));
    }
    if v.is_string() {
        return Ok(Arc::new(DensityFunction::Reference(id(v)?)));
    }
    let ty = kind(v)?;
    if ty.namespace() != "minecraft" {
        return Err(Error(format!("unknown density type {ty}")));
    }
    let f = |name| function(field(v, name)?);
    let n = |name| noise_value(field(v, name)?);
    use DensityFunction as D;
    let result = match ty.path() {
        "constant" => D::Constant(n("value")?),
        "blend_alpha" => D::Simple(Simple::BlendAlpha),
        "blend_offset" => D::Simple(Simple::BlendOffset),
        "beardifier" => D::Simple(Simple::Beardifier),
        "noise" => D::Noise {
            noise: noise(field(v, "noise")?)?,
            xz_scale: double(field(v, "xz_scale")?)?,
            y_scale: double(field(v, "y_scale")?)?,
            shift: [
                optional(v, "shift_x", Arc::new(D::Constant(0.0)), function)?,
                optional(v, "shift_y", Arc::new(D::Constant(0.0)), function)?,
                optional(v, "shift_z", Arc::new(D::Constant(0.0)), function)?,
            ],
        },
        "shift" | "shift_a" | "shift_b" => D::Shift {
            kind: match ty.path() {
                "shift_a" => Shift::A,
                "shift_b" => Shift::B,
                _ => Shift::All,
            },
            noise: noise(field(v, "noise")?)?,
        },
        "end_outer_islands" => D::EndOuterIslands,
        "distance_to_point" => {
            let p = list(field(v, "point")?)?;
            if p.len() != 3 {
                return Err(Error("point needs exactly 3 integers".into()));
            }
            let metric = match string(field(v, "metric")?)? {
                "euclidean" => DistanceMetric::Euclidean,
                "euclidean_squared" => DistanceMetric::EuclideanSquared,
                "manhattan" => DistanceMetric::Manhattan,
                "chebyshev" => DistanceMetric::Chebyshev,
                _ => return Err(Error("unknown distance metric".into())),
            };
            D::DistanceToPoint {
                point: [integer(&p[0])?, integer(&p[1])?, integer(&p[2])?],
                metric,
            }
        }
        "gradient" => {
            let from = integer(field(v, "from_coordinate")?)?;
            let to = integer(field(v, "to_coordinate")?)?;
            if from == to {
                return Err(Error("from_coordinate cannot equal to_coordinate".into()));
            }
            let tiling = optional(v, "tiling", TilingMode::ClampToEdge, |v| match string(v)? {
                "clamp_to_edge" => Ok(TilingMode::ClampToEdge),
                "repeat" => Ok(TilingMode::Repeat),
                "mirrored_repeat" => Ok(TilingMode::MirroredRepeat),
                _ => Err(Error("unknown tiling mode".into())),
            })?;
            D::Gradient {
                axis: axis(field(v, "axis")?)?,
                tiling,
                from,
                to,
                from_value: n("from_value")?,
                to_value: n("to_value")?,
            }
        }
        "abs" | "square" | "cube" | "sqrt" | "half_negative" | "quarter_negative"
        | "reciprocal" | "negate" | "squeeze" | "log" | "sign" => D::Unary {
            kind: match ty.path() {
                "abs" => Unary::Abs,
                "square" => Unary::Square,
                "cube" => Unary::Cube,
                "sqrt" => Unary::Sqrt,
                "half_negative" => Unary::HalfNegative,
                "quarter_negative" => Unary::QuarterNegative,
                "reciprocal" => Unary::Reciprocal,
                "negate" => Unary::Negate,
                "squeeze" => Unary::Squeeze,
                "log" => Unary::Log,
                _ => Unary::Sign,
            },
            input: f("input")?,
        },
        "add" | "sub" | "mul" | "div" | "min" | "max" => D::Binary {
            kind: match ty.path() {
                "add" => Binary::Add,
                "sub" => Binary::Sub,
                "mul" => Binary::Mul,
                "div" => Binary::Div,
                "min" => Binary::Min,
                _ => Binary::Max,
            },
            left: f("left")?,
            right: f("right")?,
        },
        "floor" | "round" | "ceil" | "truncate" => D::Round {
            kind: match ty.path() {
                "floor" => Round::Floor,
                "round" => Round::Round,
                "ceil" => Round::Ceil,
                _ => Round::Truncate,
            },
            input: f("input")?,
            multiple: optional(v, "multiple", Arc::new(D::Constant(1.0)), function)?,
        },
        "pow" => D::Pow {
            base: f("base")?,
            exponent: f("exponent")?,
        },
        "spline" => D::Spline(spline(field(v, "spline")?)?),
        "lerp" => D::Lerp {
            alpha: f("alpha")?,
            first: f("first")?,
            second: f("second")?,
        },
        "clamp" => {
            let min = n("min")?;
            let max = n("max")?;
            if max < min {
                return Err(Error("clamp min exceeds max".into()));
            }
            D::Clamp {
                input: f("input")?,
                min,
                max,
            }
        }
        "range_choice" => D::RangeChoice {
            input: f("input")?,
            min: n("min_inclusive")?,
            max: n("max_exclusive")?,
            inside: f("when_in_range")?,
            outside: f("when_out_of_range")?,
        },
        "interval_select" => {
            let thresholds = list(field(v, "thresholds")?)?
                .iter()
                .map(noise_value)
                .collect::<Result<Vec<_>, _>>()?;
            let functions = list(field(v, "functions")?)?
                .iter()
                .map(function)
                .collect::<Result<Vec<_>, _>>()?;
            if functions.len() < 2
                || thresholds.len() != functions.len() - 1
                || thresholds.windows(2).any(|p| p[0].total_cmp(&p[1]).is_gt())
            {
                return Err(Error(
                    "interval_select needs ordered thresholds and one extra function".into(),
                ));
            }
            D::IntervalSelect {
                input: f("input")?,
                thresholds,
                functions,
            }
        }
        "cache" => D::Cache(f("input")?),
        "blend_density" => D::BlendDensity(f("input")?),
        "interpolated" => D::Interpolated {
            input: f("input")?,
            cell_xz: int_range(field(v, "cell_size_xz")?, 1, i32::MAX)?,
            cell_y: int_range(field(v, "cell_size_y")?, 1, i32::MAX)?,
        },
        "slice" => D::Slice {
            input: f("input")?,
            axis: axis(field(v, "axis")?)?,
            coordinate: integer(field(v, "coordinate")?)?,
        },
        "find_top_surface" => D::FindTopSurface {
            density: f("density")?,
            upper: f("upper_bound")?,
            lower: int_range(field(v, "lower_bound")?, -4064, 4062)?,
            cell_height: int_range(field(v, "cell_height")?, 1, i32::MAX)?,
        },
        "old_blended_noise" => D::OldBlendedNoise {
            xz_scale: bounded(field(v, "xz_scale")?, 0.001, 1000.0)?,
            y_scale: bounded(field(v, "y_scale")?, 0.001, 1000.0)?,
            xz_factor: bounded(field(v, "xz_factor")?, 0.001, 1000.0)?,
            y_factor: bounded(field(v, "y_factor")?, 0.001, 1000.0)?,
            smear_scale: bounded(field(v, "smear_scale_multiplier")?, 1.0, 8.0)?,
        },
        _ => return Err(Error(format!("unknown density type {ty}"))),
    };
    Ok(Arc::new(result))
}
