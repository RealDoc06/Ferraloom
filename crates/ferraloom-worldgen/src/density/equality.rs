//! Java record equality distinguishes signed zero and canonicalizes NaNs. This
//! matters when DensityFunctionCompiler deduplicates cache inputs and samplers.
use super::*;
fn float(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}
fn double(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}
impl PartialEq for NoiseSpec {
    fn eq(&self, other: &Self) -> bool {
        double(self.base_amplitude, other.base_amplitude)
            && self.base_octave == other.base_octave
            && self.octave_count == other.octave_count
            && self.normalization == other.normalization
            && self.amplitude_modifiers.len() == other.amplitude_modifiers.len()
            && self
                .amplitude_modifiers
                .iter()
                .zip(&other.amplitude_modifiers)
                .all(|(&a, &b)| double(a, b))
    }
}
impl PartialEq for SplinePoint {
    fn eq(&self, other: &Self) -> bool {
        float(self.location, other.location)
            && self.value == other.value
            && float(self.derivative, other.derivative)
    }
}
impl PartialEq for CubicSpline {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Constant(a), Self::Constant(b)) => float(*a, *b),
            (
                Self::Multipoint {
                    coordinate: a,
                    points: ap,
                },
                Self::Multipoint {
                    coordinate: b,
                    points: bp,
                },
            ) => a == b && ap == bp,
            _ => false,
        }
    }
}
impl PartialEq for DensityFunction {
    fn eq(&self, other: &Self) -> bool {
        use DensityFunction as D;
        match (self, other) {
            (D::Reference(a), D::Reference(b)) => a == b,
            (D::Constant(a), D::Constant(b)) => float(*a, *b),
            (D::Simple(a), D::Simple(b)) => a == b,
            (
                D::Noise {
                    noise: a,
                    xz_scale: ax,
                    y_scale: ay,
                    shift: ash,
                },
                D::Noise {
                    noise: b,
                    xz_scale: bx,
                    y_scale: by,
                    shift: bsh,
                },
            ) => a == b && double(*ax, *bx) && double(*ay, *by) && ash == bsh,
            (D::Shift { kind: a, noise: an }, D::Shift { kind: b, noise: bn }) => {
                a == b && an == bn
            }
            (D::EndOuterIslands, D::EndOuterIslands) => true,
            (
                D::DistanceToPoint {
                    point: a,
                    metric: am,
                },
                D::DistanceToPoint {
                    point: b,
                    metric: bm,
                },
            ) => a == b && am == bm,
            (
                D::Gradient {
                    axis: a,
                    tiling: at,
                    from: af,
                    to: ato,
                    from_value: av,
                    to_value: atv,
                },
                D::Gradient {
                    axis: b,
                    tiling: bt,
                    from: bf,
                    to: bto,
                    from_value: bv,
                    to_value: btv,
                },
            ) => {
                a == b && at == bt && af == bf && ato == bto && float(*av, *bv) && float(*atv, *btv)
            }
            (D::Unary { kind: a, input: ai }, D::Unary { kind: b, input: bi }) => {
                a == b && ai == bi
            }
            (
                D::Binary {
                    kind: a,
                    left: al,
                    right: ar,
                },
                D::Binary {
                    kind: b,
                    left: bl,
                    right: br,
                },
            ) => a == b && al == bl && ar == br,
            (
                D::Round {
                    kind: a,
                    input: ai,
                    multiple: am,
                },
                D::Round {
                    kind: b,
                    input: bi,
                    multiple: bm,
                },
            ) => a == b && ai == bi && am == bm,
            (
                D::Pow {
                    base: a,
                    exponent: ae,
                },
                D::Pow {
                    base: b,
                    exponent: be,
                },
            ) => a == b && ae == be,
            (D::Spline(a), D::Spline(b)) => a == b,
            (
                D::Lerp {
                    alpha: a,
                    first: af,
                    second: as_,
                },
                D::Lerp {
                    alpha: b,
                    first: bf,
                    second: bs,
                },
            ) => a == b && af == bf && as_ == bs,
            (
                D::Clamp {
                    input: a,
                    min: amin,
                    max: amax,
                },
                D::Clamp {
                    input: b,
                    min: bmin,
                    max: bmax,
                },
            ) => a == b && float(*amin, *bmin) && float(*amax, *bmax),
            (
                D::RangeChoice {
                    input: a,
                    min: amin,
                    max: amax,
                    inside: ai,
                    outside: ao,
                },
                D::RangeChoice {
                    input: b,
                    min: bmin,
                    max: bmax,
                    inside: bi,
                    outside: bo,
                },
            ) => a == b && float(*amin, *bmin) && float(*amax, *bmax) && ai == bi && ao == bo,
            (
                D::IntervalSelect {
                    input: a,
                    thresholds: at,
                    functions: af,
                },
                D::IntervalSelect {
                    input: b,
                    thresholds: bt,
                    functions: bf,
                },
            ) => {
                a == b
                    && at.len() == bt.len()
                    && at.iter().zip(bt).all(|(&a, &b)| float(a, b))
                    && af == bf
            }
            (D::Cache(a), D::Cache(b)) | (D::BlendDensity(a), D::BlendDensity(b)) => a == b,
            (
                D::Interpolated {
                    input: a,
                    cell_xz: ax,
                    cell_y: ay,
                },
                D::Interpolated {
                    input: b,
                    cell_xz: bx,
                    cell_y: by,
                },
            ) => a == b && ax == bx && ay == by,
            (
                D::Slice {
                    input: a,
                    axis: aa,
                    coordinate: ac,
                },
                D::Slice {
                    input: b,
                    axis: ba,
                    coordinate: bc,
                },
            ) => a == b && aa == ba && ac == bc,
            (
                D::FindTopSurface {
                    density: a,
                    upper: au,
                    lower: al,
                    cell_height: ah,
                },
                D::FindTopSurface {
                    density: b,
                    upper: bu,
                    lower: bl,
                    cell_height: bh,
                },
            ) => a == b && au == bu && al == bl && ah == bh,
            (
                D::OldBlendedNoise {
                    xz_scale: ax,
                    y_scale: ay,
                    xz_factor: af,
                    y_factor: ag,
                    smear_scale: as_,
                },
                D::OldBlendedNoise {
                    xz_scale: bx,
                    y_scale: by,
                    xz_factor: bf,
                    y_factor: bg,
                    smear_scale: bs,
                },
            ) => {
                double(*ax, *bx)
                    && double(*ay, *by)
                    && double(*af, *bf)
                    && double(*ag, *bg)
                    && double(*as_, *bs)
            }
            _ => false,
        }
    }
}
