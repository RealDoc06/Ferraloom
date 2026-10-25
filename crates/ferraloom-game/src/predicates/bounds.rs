use crate::loot::codec::DecodeInput;
use crate::loot::{Result, codec};

/// Port of MinMaxBounds.Doubles. Squared bounds preserve negative endpoints.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DoubleBounds {
    pub min: Option<f64>,
    pub max: Option<f64>,
}
impl DoubleBounds {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        let number = codec::double;
        if v.is_number() {
            let n = number(v)?;
            return Ok(Self {
                min: Some(n),
                max: Some(n),
            });
        }
        let result = Self {
            min: codec::optional(v, "min", number)?,
            max: codec::optional(v, "max", number)?,
        };
        if matches!((result.min,result.max),(Some(a),Some(b)) if a.total_cmp(&b).is_gt()) {
            return Err(codec::error("swapped bounds"));
        }
        Ok(result)
    }
    pub fn matches(self, value: f64) -> bool {
        !self.min.is_some_and(|min| min > value) && !self.max.is_some_and(|max| max < value)
    }
    pub fn matches_sqr(self, value: f64) -> bool {
        !self.min.is_some_and(|min| min * min > value)
            && !self.max.is_some_and(|max| max * max < value)
    }
}
/// Port of MinMaxBounds.Ints.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IntBounds {
    pub min: Option<i32>,
    pub max: Option<i32>,
}
impl IntBounds {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        if v.is_number() {
            let n = codec::int(v)?;
            return Ok(Self {
                min: Some(n),
                max: Some(n),
            });
        }
        let result = Self {
            min: codec::optional(v, "min", codec::int)?,
            max: codec::optional(v, "max", codec::int)?,
        };
        if matches!((result.min,result.max),(Some(a),Some(b)) if a > b) {
            return Err(codec::error("swapped bounds"));
        }
        Ok(result)
    }
    pub fn matches_sqr(self, value: i64) -> bool {
        self.min
            .is_none_or(|v| value >= i64::from(v) * i64::from(v))
            && self
                .max
                .is_none_or(|v| value <= i64::from(v) * i64::from(v))
    }
    pub fn matches(self, value: i32) -> bool {
        self.min.is_none_or(|v| value >= v) && self.max.is_none_or(|v| value <= v)
    }
}
/// Port of DistancePredicate#matches, including its f32 intermediate arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DistancePredicate {
    pub x: DoubleBounds,
    pub y: DoubleBounds,
    pub z: DoubleBounds,
    pub horizontal: DoubleBounds,
    pub absolute: DoubleBounds,
}
impl DistancePredicate {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        let bound = |name| -> Result<DoubleBounds> {
            Ok(codec::optional(v, name, DoubleBounds::decode)?.unwrap_or_default())
        };
        Ok(Self {
            x: bound("x")?,
            y: bound("y")?,
            z: bound("z")?,
            horizontal: bound("horizontal")?,
            absolute: bound("absolute")?,
        })
    }
    pub fn matches(self, from: ferraloom_util::Vec3, to: ferraloom_util::Vec3) -> bool {
        let x = (from.x - to.x) as f32;
        let y = (from.y - to.y) as f32;
        let z = (from.z - to.z) as f32;
        self.x.matches(f64::from(x.abs()))
            && self.y.matches(f64::from(y.abs()))
            && self.z.matches(f64::from(z.abs()))
            && self.horizontal.matches_sqr(f64::from(x * x + z * z))
            && self.absolute.matches_sqr(f64::from(x * x + y * y + z * z))
    }
}

/// Port of MinMaxBounds.FloatDegrees' codec. Reversed endpoints are permitted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FloatDegrees {
    pub min: Option<f32>,
    pub max: Option<f32>,
}
impl FloatDegrees {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        if v.is_number() {
            let value = codec::float(v)?;
            Ok(Self {
                min: Some(value),
                max: Some(value),
            })
        } else {
            Ok(Self {
                min: codec::optional(v, "min", codec::float)?,
                max: codec::optional(v, "max", codec::float)?,
            })
        }
    }
}
