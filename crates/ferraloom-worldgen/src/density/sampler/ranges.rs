//! Port of Interval and density-function bounds used by compiler shortcuts.
use super::*;
use ferraloom_util::java::{max_f32 as max, min_f32 as min};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Interval {
    pub min: f32,
    pub max: f32,
}
impl Interval {
    const NAI: Self = Self {
        min: f32::NAN,
        max: f32::NAN,
    };
    const INFINITE: Self = Self {
        min: f32::NEG_INFINITY,
        max: f32::INFINITY,
    };
    fn of(min: f32, max: f32) -> Self {
        if min.is_nan() || max.is_nan() {
            Self::NAI
        } else if min == 0.0 && max == 1.0 {
            // Interval.of reuses ZERO_TO_ONE, normalizing a negative zero bound.
            Self { min: 0.0, max }
        } else {
            Self { min, max }
        }
    }
    fn exact(v: f32) -> Self {
        Self::of(v, v)
    }
    fn contains(self, v: f32) -> bool {
        v >= self.min && v <= self.max
    }
    fn nai(self) -> bool {
        self.min.is_nan()
    }
    fn encapsulate(values: impl IntoIterator<Item = Self>) -> Self {
        let mut low = f32::INFINITY;
        let mut high = f32::NEG_INFINITY;
        for v in values {
            if !v.nai() {
                low = min(v.min, low);
                high = max(v.max, high);
            }
        }
        if high < low {
            Self::NAI
        } else {
            Self::of(low, high)
        }
    }
    fn pair(a: f32, b: f32) -> Self {
        Self::encapsulate([Self::exact(a), Self::exact(b)])
    }
    fn add(self, b: Self) -> Self {
        Self::of(self.min + b.min, self.max + b.max)
    }
    fn sub(self, b: Self) -> Self {
        Self::of(self.min - b.max, self.max - b.min)
    }
    fn mul(self, b: Self) -> Self {
        if self.nai() || b.nai() {
            return Self::NAI;
        }
        let a = bound_mul(self.min, b.min);
        let c = bound_mul(self.min, b.max);
        let d = bound_mul(self.max, b.min);
        let e = bound_mul(self.max, b.max);
        Self::of(min(min(a, c), min(d, e)), max(max(a, c), max(d, e)))
    }
    fn reciprocal(self) -> Self {
        if self.nai() || (self.min == 0.0 && self.max == 0.0) {
            Self::NAI
        } else if !self.contains(0.0) {
            Self::of(1.0 / self.max, 1.0 / self.min)
        } else if self.max == 0.0 {
            Self::of(f32::NEG_INFINITY, 1.0 / self.min)
        } else if self.min == 0.0 {
            Self::of(1.0 / self.max, f32::INFINITY)
        } else {
            Self::INFINITE
        }
    }
    fn minimum(self, b: Self) -> Self {
        if self.nai() || b.nai() {
            Self::NAI
        } else {
            Self::of(min(self.min, b.min), min(self.max, b.max))
        }
    }
    fn maximum(self, b: Self) -> Self {
        if self.nai() || b.nai() {
            Self::NAI
        } else {
            Self::of(max(self.min, b.min), max(self.max, b.max))
        }
    }
    fn clamp(self, low: f32, high: f32) -> Self {
        if self.nai() {
            Self::NAI
        } else if self.min >= high {
            Self::exact(high)
        } else if self.max <= low {
            Self::exact(low)
        } else {
            Self::of(max(self.min, low), min(self.max, high))
        }
    }
    fn map(self, op: impl Fn(f32) -> f32) -> Self {
        if self.nai() {
            Self::NAI
        } else {
            Self::pair(op(self.min), op(self.max))
        }
    }
    fn abs(self, square: bool) -> Self {
        if self.nai() {
            return Self::NAI;
        }
        let op = |v: f32| if square { v * v } else { v.abs() };
        let high = max(op(self.min), op(self.max));
        Self::of(
            if self.contains(0.0) {
                0.0
            } else {
                min(op(self.min), op(self.max))
            },
            high,
        )
    }
    fn pow(self, exponent: Self) -> Self {
        if self.nai() || exponent.nai() {
            return Self::NAI;
        }
        if self.min == self.max {
            return pow_base(self.min, exponent);
        }
        let mut result =
            Self::encapsulate([pow_base(self.min, exponent), pow_base(self.max, exponent)]);
        if self.contains(0.0) {
            if self.max > 0.0 {
                result = Self::encapsulate([result, pow_base(0.0, exponent)]);
            }
            if self.min < 0.0 {
                result = Self::encapsulate([result, pow_base(-0.0, exponent)]);
            }
        }
        result
    }
    fn lerp(self, first: Self, second: Self) -> Self {
        if self.nai() || first.nai() || second.nai() {
            return Self::NAI;
        }
        Self::encapsulate([
            lerp_bounds(self, first.min, second.min),
            lerp_bounds(self, first.max, second.min),
            lerp_bounds(self, first.min, second.max),
            lerp_bounds(self, first.max, second.max),
        ])
    }
}
fn bound_mul(a: f32, b: f32) -> f32 {
    if a != 0.0 && b != 0.0 { a * b } else { 0.0 }
}
fn pow_value(a: f32, b: f32) -> f32 {
    f64::from(a).powf(f64::from(b)) as f32
}
fn pow_base(base: f32, e: Interval) -> Interval {
    if base.is_nan() || e.nai() {
        return Interval::NAI;
    }
    if e.min == e.max {
        return Interval::exact(pow_value(base, e.min));
    }
    if base == 0.0 {
        let zero = if e.contains(0.0) {
            if e.max == 0.0 {
                Interval::of(1.0, f32::INFINITY)
            } else if e.min == 0.0 {
                Interval::of(0.0, 1.0)
            } else {
                Interval::of(0.0, f32::INFINITY)
            }
        } else {
            Interval::exact(if e.max < 0.0 { f32::INFINITY } else { 0.0 })
        };
        return zero.mul(Interval::exact(1.0_f32.copysign(base)));
    }
    if base == 1.0 {
        return Interval::exact(1.0);
    }
    if base > 0.0 {
        if e.min.is_finite() && e.max.is_finite() {
            return Interval::pair(pow_value(base, e.min), pow_value(base, e.max));
        }
        return if e.min.is_infinite() && e.max.is_infinite() {
            Interval::of(0.0, f32::INFINITY)
        } else if e.min.is_infinite() {
            if base < 1.0 {
                Interval::of(pow_value(base, e.max), f32::INFINITY)
            } else {
                Interval::of(0.0, pow_value(base, e.max))
            }
        } else if base < 1.0 {
            Interval::of(0.0, pow_value(base, e.min))
        } else {
            Interval::of(pow_value(base, e.min), f32::INFINITY)
        };
    }
    let low = e.min.ceil();
    let high = e.max.floor();
    if high < low {
        return Interval::NAI;
    }
    let a = pow_value(base, low);
    let b = pow_value(base, high);
    let mut result = Interval::pair(a, b);
    if low.is_infinite() {
        result = Interval::encapsulate([result, Interval::exact(-a)]);
    } else if low + 1.0 < high {
        result = Interval::encapsulate([result, Interval::exact(pow_value(base, low + 1.0))]);
    }
    if high.is_infinite() {
        result = Interval::encapsulate([result, Interval::exact(-b)]);
    } else if high - 1.0 > low {
        result = Interval::encapsulate([result, Interval::exact(pow_value(base, high - 1.0))]);
    }
    result
}
fn lerp_bounds(alpha: Interval, first: f32, second: f32) -> Interval {
    if alpha.nai() || first.is_nan() || second.is_nan() {
        return Interval::NAI;
    }
    let op = |a: f32| {
        if first.is_finite() && second.is_finite() {
            return first + bound_mul(a, second - first);
        }
        if first == second {
            return first;
        }
        let f = bound_mul(1.0 - a, first);
        let s = bound_mul(a, second);
        if !f.is_infinite() || !s.is_infinite() {
            f + s
        } else if a <= 0.0 {
            if second > first {
                f32::NEG_INFINITY
            } else {
                f32::INFINITY
            }
        } else if a >= 1.0 {
            if second > first {
                f32::INFINITY
            } else {
                f32::NEG_INFINITY
            }
        } else {
            f32::NAN
        }
    };
    let low = op(alpha.min);
    let high = op(alpha.max);
    if (!first.is_finite() || !second.is_finite()) && (low.is_nan() || high.is_nan()) {
        Interval::NAI
    } else {
        Interval::pair(low, high)
    }
}
impl DensityFunctionCompiler {
    pub(super) fn compute_range(&self, node: &Compiled) -> Interval {
        use DensityFunction as D;
        let r = |i: usize| self.ranges[node.children[i].0];
        match node.definition.as_ref() {
            D::Reference(_) => unreachable!(),
            D::Constant(v) => Interval::exact(*v),
            D::Simple(Simple::BlendAlpha) => Interval::of(0.0, 1.0),
            D::Simple(_) => Interval::INFINITE,
            D::Noise { noise, .. } | D::Shift { noise, .. } => {
                let spec = match noise {
                    Holder::Reference(id) => &self.registry.noises[id],
                    Holder::Inline(spec) => spec,
                };
                let [min, max] = spec.noise().range();
                let value = Interval::of(min, max);
                if matches!(node.definition.as_ref(), D::Shift { .. }) {
                    value.mul(Interval::exact(4.0))
                } else {
                    value
                }
            }
            D::EndOuterIslands => Interval::of(-0.84375, 0.5625),
            D::DistanceToPoint { .. } => Interval::of(0.0, f32::INFINITY),
            D::Gradient {
                from_value,
                to_value,
                ..
            } => Interval::pair(*from_value, *to_value),
            D::Unary { kind, .. } => {
                let input = r(0);
                match kind {
                    Unary::Abs => input.abs(false),
                    Unary::Square => input.abs(true),
                    Unary::Cube => input.map(|x| x * x * x),
                    Unary::Sqrt => input.pow(Interval::exact(0.5)),
                    Unary::HalfNegative | Unary::QuarterNegative | Unary::Squeeze => {
                        input.map(|x| unary(*kind, x))
                    }
                    Unary::Sign => {
                        if input.nai() {
                            input
                        } else if input.min == input.max {
                            Interval::exact(unary(Unary::Sign, input.min))
                        } else if input.contains(0.0) {
                            if input.min == 0.0 {
                                Interval::of(0.0, 1.0)
                            } else if input.max == 0.0 {
                                Interval::of(-1.0, 0.0)
                            } else {
                                Interval::of(-1.0, 1.0)
                            }
                        } else {
                            Interval::exact(if input.min > 0.0 { 1.0 } else { -1.0 })
                        }
                    }
                    Unary::Reciprocal => input.reciprocal(),
                    Unary::Negate => Interval::exact(0.0).sub(input),
                    Unary::Log => {
                        if input.max < 0.0 {
                            Interval::NAI
                        } else {
                            input
                                .maximum(Interval::exact(0.0))
                                .map(|x| f64::from(x).ln() as f32)
                        }
                    }
                }
            }
            D::Binary { kind, .. } => match kind {
                Binary::Add => r(0).add(r(1)),
                Binary::Sub => r(0).sub(r(1)),
                Binary::Mul => r(0).mul(r(1)),
                Binary::Div => r(0).mul(r(1).reciprocal()),
                Binary::Min => r(0).minimum(r(1)),
                Binary::Max => r(0).maximum(r(1)),
            },
            D::Round { kind, .. } => r(0)
                .mul(r(1).reciprocal())
                .map(|v| round(*kind, v, 1.0))
                .mul(r(1)),
            D::Pow { .. } => r(0).pow(r(1)),
            D::Spline(_) => self.spline_range(node.spline.as_ref().unwrap()),
            D::Lerp { .. } => r(0).lerp(r(1), r(2)),
            D::Clamp { min, max, .. } => r(0).clamp(*min, *max),
            D::RangeChoice { .. } => Interval::encapsulate([r(1), r(2)]),
            D::IntervalSelect { .. } => {
                Interval::encapsulate(node.children[1..].iter().map(|id| self.ranges[id.0]))
            }
            D::Cache(_) | D::BlendDensity(_) | D::Interpolated { .. } | D::Slice { .. } => r(0),
            D::FindTopSurface { lower, .. } => {
                Interval::of(*lower as f32, max(*lower as f32, r(1).max))
            }
            D::OldBlendedNoise {
                y_scale,
                smear_scale,
                ..
            } => {
                let mut range = Interval::exact(0.0);
                let mut frequency = 1.0;
                let mut amplitude = 0.9999847412109375 / (2.0_f64.powi(16) - 1.0);
                let smear = 684.412 * y_scale * smear_scale;
                for _ in 0..16 {
                    let value = ((smear * frequency).abs() + 2.0) as f32;
                    range = range
                        .add(Interval::of(-value, value).mul(Interval::exact(amplitude as f32)));
                    frequency /= 2.0;
                    amplitude *= 2.0;
                }
                range
            }
        }
    }
    fn spline_range(&self, spline: &CompiledSpline) -> Interval {
        let CompiledSpline::Multipoint { coordinate, points } = spline else {
            let CompiledSpline::Constant(v) = spline else {
                unreachable!()
            };
            return Interval::exact(*v);
        };
        let input = self.ranges[coordinate.0];
        if input.nai() {
            return input;
        }
        let values = points
            .iter()
            .map(|(_, value, _)| self.spline_range(value))
            .collect::<Vec<_>>();
        let mut low = f32::INFINITY;
        let mut high = f32::NEG_INFINITY;
        for (index, bound) in [(0, input.min), (points.len() - 1, input.max)] {
            if (index == 0 && bound < points[index].0)
                || (index == points.len() - 1 && bound > points[index].0)
            {
                let extend = |v: f32| {
                    if points[index].2 == 0.0 {
                        v
                    } else {
                        v + points[index].2 * (bound - points[index].0)
                    }
                };
                low = min(
                    low,
                    min(extend(values[index].min), extend(values[index].max)),
                );
                high = max(
                    high,
                    max(extend(values[index].min), extend(values[index].max)),
                );
            }
        }
        for value in &values {
            low = min(low, value.min);
            high = max(high, value.max);
        }
        for i in 0..points.len() - 1 {
            let (x1, _, d1) = &points[i];
            let (x2, _, d2) = &points[i + 1];
            if *d1 != 0.0 || *d2 != 0.0 {
                let p1 = d1 * (x2 - x1);
                let p2 = d2 * (x2 - x1);
                let a = values[i];
                let b = values[i + 1];
                let min_a = p1 - b.max + a.min;
                let max_a = p1 - b.min + a.max;
                let min_b = -p2 + b.min - a.max;
                let max_b = -p2 + b.max - a.min;
                low = min(low, min(a.min, b.min) + 0.25 * min(min_a, min_b));
                high = max(high, max(a.max, b.max) + 0.25 * max(max_a, max_b));
            }
        }
        Interval::of(low, high)
    }
}
