use super::registry::RegistryValue;
use super::{
    EntityTarget, EvalContext, Holder, HolderSet, Identifier, LootData, LootError,
    LootItemCondition, Result, ScoreHolder, codec,
};
use crate::loot::codec::DecodeInput;
use ferraloom_nbt::{NbtPath, Tag};
use ferraloom_util::{mth, random::RandomSource};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Abs,
    Negate,
    Floor,
    Ceil,
    Round,
    Truncate,
    Sin,
    Cos,
    Sqrt,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Sub,
    Div,
    Mod,
    FloorDiv,
    FloorMod,
    Pow,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateOp {
    Add,
    Mul,
    Min,
    Max,
    Avg,
    Length,
}
/// Port of the integer and float ContextProvider registries. The scalar type
/// selects Java's arithmetic rules; recursive provider calls use unsafe values
/// until the outer getInt/getFloat catches an arithmetic error.
#[derive(Clone, Debug, PartialEq)]
pub enum NumberProvider<T> {
    Constant(T),
    EnchantmentLevel(super::LevelBasedValue),
    EnvironmentAttribute {
        attribute: Identifier,
        positional: bool,
    },
    Unary(UnaryOp, Holder<Self>),
    Binary(BinaryOp, Holder<Self>, Holder<Self>),
    Aggregate(AggregateOp, HolderSet<Self>),
    Uniform(Holder<Self>, Holder<Self>),
    Conditional(Holder<LootItemCondition>, Holder<Self>, Holder<Self>),
    Dispatcher(Vec<(Holder<LootItemCondition>, Holder<Self>)>, Holder<Self>),
    Weighted(Vec<(Holder<Self>, i32)>),
    FromInt(Holder<ContextIntProvider>),
    FromFloat(Holder<ContextFloatProvider>),
    Binomial(Holder<ContextIntProvider>, Holder<ContextFloatProvider>),
    Storage(Identifier, String, Holder<Self>),
    Score(ScoreboardNameProvider, String, Holder<Self>),
}
pub type ContextIntProvider = NumberProvider<i32>;
pub type ContextFloatProvider = NumberProvider<f32>;
/// Port of ScoreboardNameProviders#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub enum ScoreboardNameProvider {
    Context(EntityTarget),
    Fixed(String),
}
impl ScoreboardNameProvider {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        if v.is_string() {
            return Ok(Self::Context(EntityTarget::decode(v)?));
        }
        Ok(match codec::kind(v)?.as_str() {
            "minecraft:context" => Self::Context(EntityTarget::decode(codec::field(v, "target")?)?),
            "minecraft:fixed" => Self::Fixed(codec::string(codec::field(v, "name")?)?.into()),
            _ => return Err(codec::error("unknown score provider")),
        })
    }
    pub(crate) fn holder<'a>(&'a self, params: &super::LootParams) -> Option<ScoreHolder<'a>> {
        match self {
            Self::Fixed(name) => Some(ScoreHolder::Name(name)),
            Self::Context(target) => params.entity(*target).map(ScoreHolder::Entity),
        }
    }
}
pub trait Scalar: Copy + Clone + std::fmt::Debug + PartialEq + Sized {
    const FLOAT: bool;
    fn decode<I: DecodeInput>(v: &I) -> Result<Self>;
    fn zero() -> Self;
    fn unary(op: UnaryOp, input: Self) -> Result<Self>;
    fn binary(op: BinaryOp, left: Self, right: Self) -> Result<Self>;
    fn aggregate(op: AggregateOp, inputs: &[Self]) -> Result<Self>;
    fn uniform(random: &mut dyn RandomSource, min: Self, max: Self) -> Result<Self>;
    fn from_int(value: i32) -> Result<Self>;
    fn from_float(value: f32) -> Result<Self>;
    fn from_tag(value: &Tag) -> Option<Self>;
    fn sanitize(value: Self) -> Self;
    fn is_zero(value: Self) -> bool;
}
impl<T: Scalar> NumberProvider<T> {
    pub fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        if v.is_number() {
            return Ok(Self::Constant(T::decode(v)?));
        }
        let h = |name| Holder::decode(codec::field(v, name)?, Self::decode);
        let default = |name| -> Result<Holder<Self>> {
            Ok(
                codec::optional(v, name, |v| Holder::decode(v, Self::decode))?
                    .unwrap_or_else(|| Holder::direct(Self::Constant(T::zero()))),
            )
        };
        let kind = codec::kind(v)?;
        Ok(match kind.as_str() {
            "minecraft:constant" => Self::Constant(T::decode(codec::field(v, "value")?)?),
            "minecraft:uniform" => Self::Uniform(h("min")?, h("max")?),
            "minecraft:abs" => Self::Unary(UnaryOp::Abs, h("input")?),
            "minecraft:negate" => Self::Unary(UnaryOp::Negate, h("input")?),
            "minecraft:floor" if T::FLOAT => Self::Unary(UnaryOp::Floor, h("input")?),
            "minecraft:ceil" if T::FLOAT => Self::Unary(UnaryOp::Ceil, h("input")?),
            "minecraft:round" if T::FLOAT => Self::Unary(UnaryOp::Round, h("input")?),
            "minecraft:truncate" if T::FLOAT => Self::Unary(UnaryOp::Truncate, h("input")?),
            "minecraft:sin" if T::FLOAT => Self::Unary(UnaryOp::Sin, h("input")?),
            "minecraft:cos" if T::FLOAT => Self::Unary(UnaryOp::Cos, h("input")?),
            "minecraft:sqrt" if T::FLOAT => Self::Unary(UnaryOp::Sqrt, h("input")?),
            "minecraft:sub" => Self::Binary(BinaryOp::Sub, h("left")?, h("right")?),
            "minecraft:div" => Self::Binary(BinaryOp::Div, h("left")?, h("right")?),
            "minecraft:mod" => Self::Binary(BinaryOp::Mod, h("left")?, h("right")?),
            "minecraft:floor_div" if !T::FLOAT => {
                Self::Binary(BinaryOp::FloorDiv, h("left")?, h("right")?)
            }
            "minecraft:floor_mod" if !T::FLOAT => {
                Self::Binary(BinaryOp::FloorMod, h("left")?, h("right")?)
            }
            "minecraft:pow" => Self::Binary(BinaryOp::Pow, h("base")?, h("exponent")?),
            "minecraft:add" | "minecraft:mul" | "minecraft:min" | "minecraft:max"
            | "minecraft:avg" | "minecraft:length" => {
                let op = match kind.path() {
                    "add" => AggregateOp::Add,
                    "mul" => AggregateOp::Mul,
                    "min" => AggregateOp::Min,
                    "max" => AggregateOp::Max,
                    "avg" => AggregateOp::Avg,
                    "length" if T::FLOAT => AggregateOp::Length,
                    _ => return Err(codec::error("unknown integer aggregate")),
                };
                Self::Aggregate(
                    op,
                    HolderSet::decode(codec::field(v, "inputs")?, Self::decode)?,
                )
            }
            "minecraft:conditional" => Self::Conditional(
                Holder::decode(codec::field(v, "condition")?, LootItemCondition::decode)?,
                h("on_true")?,
                default("on_false")?,
            ),
            "minecraft:number_dispatcher" => Self::Dispatcher(
                codec::list(codec::field(v, "cases")?, |case| {
                    Ok((
                        Holder::decode(
                            codec::field(case, "condition")?,
                            LootItemCondition::decode,
                        )?,
                        Holder::decode(codec::field(case, "value")?, Self::decode)?,
                    ))
                })?,
                default("default")?,
            ),
            "minecraft:weighted_list" => {
                let values = codec::list(codec::field(v, "distribution")?, |entry| {
                    let weight = codec::int(codec::field(entry, "weight")?)?;
                    if weight < 0 {
                        return Err(codec::error("negative weight"));
                    }
                    Ok((
                        Holder::decode(codec::field(entry, "data")?, Self::decode)?,
                        weight,
                    ))
                })?;
                let total: i64 = values.iter().map(|(_, w)| i64::from(*w)).sum();
                if values.is_empty() || total == 0 || total > i64::from(i32::MAX) {
                    return Err(codec::error("invalid weighted distribution"));
                }
                Self::Weighted(values)
            }
            "minecraft:from_int" if T::FLOAT => Self::FromInt(Holder::decode(
                codec::field(v, "input")?,
                ContextIntProvider::decode,
            )?),
            "minecraft:from_float" if !T::FLOAT => Self::FromFloat(Holder::decode(
                codec::field(v, "input")?,
                ContextFloatProvider::decode,
            )?),
            "minecraft:binomial" if !T::FLOAT => Self::Binomial(
                Holder::decode(codec::field(v, "n")?, ContextIntProvider::decode)?,
                Holder::decode(codec::field(v, "p")?, ContextFloatProvider::decode)?,
            ),
            "minecraft:score" if !T::FLOAT => Self::Score(
                ScoreboardNameProvider::decode(codec::field(v, "target")?)?,
                codec::string(codec::field(v, "score")?)?.into(),
                default("fallback")?,
            ),
            "minecraft:storage" => {
                let path = codec::string(codec::field(v, "path")?)?;
                NbtPath::parse(path).map_err(|e| codec::error(e.to_string()))?;
                Self::Storage(
                    codec::id(codec::field(v, "storage")?)?,
                    path.into(),
                    default("fallback")?,
                )
            }
            "minecraft:enchantment_level" if T::FLOAT => {
                Self::EnchantmentLevel(super::LevelBasedValue::decode(codec::field(v, "amount")?)?)
            }
            "minecraft:environment_attribute" => {
                let attribute = codec::id(codec::field(v, "attribute")?)?;
                let positional = super::environment::numeric_attribute(&attribute, T::FLOAT)?;
                Self::EnvironmentAttribute {
                    attribute,
                    positional,
                }
            }
            _ => return Err(codec::error(format!("unknown number provider {kind}"))),
        })
    }
}
impl<T: Scalar> NumberProvider<T>
where
    Self: RegistryValue,
{
    #[cfg(test)]
    pub(crate) fn get(&self, data: &LootData, context: &mut EvalContext<'_>) -> Result<T> {
        match self.eval(data, context) {
            Ok(value) => Ok(T::sanitize(value)),
            Err(LootError::Arithmetic) => Ok(T::zero()),
            Err(e) => Err(e),
        }
    }
    pub(crate) fn eval(&self, data: &LootData, c: &mut EvalContext<'_>) -> Result<T> {
        let eval = |h: &Holder<Self>, c: &mut EvalContext<'_>| h.eval(data, c);
        match self {
            Self::Constant(v) => Ok(*v),
            Self::EnvironmentAttribute { attribute, .. } => {
                T::from_float(c.facts().environment_float(attribute, c.params)?)
            }
            Self::EnchantmentLevel(value) => {
                T::from_float(value.calculate(c.params.enchantment_level.unwrap_or(0))?)
            }
            Self::Unary(op, input) => T::unary(*op, eval(input, c)?),
            Self::Binary(BinaryOp::Mod, left, right) if T::FLOAT => {
                let r = eval(right, c)?;
                if T::is_zero(r) {
                    return T::from_float(f32::NAN);
                }
                T::binary(BinaryOp::Mod, eval(left, c)?, r)
            }
            Self::Binary(op, left, right) => {
                let l = eval(left, c)?;
                let r = eval(right, c)?;
                T::binary(*op, l, r)
            }
            Self::Aggregate(op, inputs) => {
                let values = inputs
                    .resolve(data)?
                    .iter()
                    .map(|h| eval(h, c))
                    .collect::<Result<Vec<_>>>()?;
                T::aggregate(*op, &values)
            }
            Self::Uniform(min, max) => {
                let min = eval(min, c)?;
                let max = eval(max, c)?;
                T::uniform(&mut c.random(), min, max)
            }
            Self::Conditional(test, on_true, on_false) => eval(
                if test.test(data, c)? {
                    on_true
                } else {
                    on_false
                },
                c,
            ),
            Self::Dispatcher(cases, default) => {
                for (test, value) in cases {
                    if test.test(data, c)? {
                        return eval(value, c);
                    }
                }
                eval(default, c)
            }
            Self::Weighted(values) => {
                let total = values.iter().map(|(_, w)| *w).sum();
                let mut index = c.random().next_int_bound(total);
                for (value, weight) in values {
                    index -= weight;
                    if index < 0 {
                        return eval(value, c);
                    }
                }
                unreachable!("validated weighted distribution")
            }
            Self::FromInt(input) => T::from_int(input.eval(data, c)?),
            Self::FromFloat(input) => T::from_float(input.eval(data, c)?),
            Self::Binomial(n, p) => {
                let n = n.eval(data, c)?;
                let p = p.eval(data, c)?;
                if !p.is_finite() {
                    return Err(LootError::Arithmetic);
                }
                let mut value = 0;
                for _ in 0..n {
                    if c.random().next_float() < p {
                        value += 1;
                    }
                }
                T::from_int(value)
            }
            Self::Score(target, objective, fallback) => {
                let value = if let Some(holder) = target.holder(c.params) {
                    c.facts().score(holder, objective)?
                } else {
                    None
                };
                if let Some(value) = value {
                    T::from_int(value)
                } else {
                    eval(fallback, c)
                }
            }
            Self::Storage(id, path, fallback) => {
                let storage = c.facts().storage(id)?;
                let selected = NbtPath::parse(path)
                    .expect("validated path")
                    .get(&storage)
                    .unwrap_or_default();
                if let [value] = selected.as_slice()
                    && let Some(value) = T::from_tag(value)
                {
                    return Ok(value);
                }
                eval(fallback, c)
            }
        }
    }
}
impl<T: Scalar> Holder<NumberProvider<T>>
where
    NumberProvider<T>: RegistryValue,
{
    pub(crate) fn eval(&self, data: &LootData, c: &mut EvalContext<'_>) -> Result<T> {
        if let Self::Reference(id) = self {
            let key = (NumberProvider::<T>::REGISTRY, id.clone());
            if !c.visited.insert(key.clone()) {
                return Err(codec::error("recursive number provider"));
            }
            let result = self.resolve(data)?.eval(data, c);
            c.visited.remove(&key);
            result
        } else {
            self.resolve(data)?.eval(data, c)
        }
    }
    pub(crate) fn get(&self, data: &LootData, c: &mut EvalContext<'_>) -> Result<T> {
        match self.eval(data, c) {
            Ok(v) => Ok(T::sanitize(v)),
            Err(LootError::Arithmetic) => Ok(T::zero()),
            Err(e) => Err(e),
        }
    }
}
impl Scalar for i32 {
    const FLOAT: bool = false;
    fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        codec::int(v)
    }
    fn zero() -> Self {
        0
    }
    fn unary(op: UnaryOp, v: Self) -> Result<Self> {
        match op {
            UnaryOp::Abs => v.checked_abs(),
            UnaryOp::Negate => v.checked_neg(),
            _ => None,
        }
        .ok_or(LootError::Arithmetic)
    }
    fn binary(op: BinaryOp, a: Self, b: Self) -> Result<Self> {
        let value = match op {
            BinaryOp::Sub => a.checked_sub(b),
            BinaryOp::Div => (b != 0).then(|| a.wrapping_div(b)),
            BinaryOp::Mod => (b != 0).then(|| a.wrapping_rem(b)),
            BinaryOp::FloorDiv => {
                if b == 0 || (a == i32::MIN && b == -1) {
                    None
                } else {
                    let q = a / b;
                    let r = a % b;
                    Some(q - i32::from(r != 0 && (a ^ b) < 0))
                }
            }
            BinaryOp::FloorMod => {
                if b == 0 {
                    None
                } else {
                    let r = a.wrapping_rem(b);
                    Some(if r != 0 && (a ^ b) < 0 {
                        r.wrapping_add(b)
                    } else {
                        r
                    })
                }
            }
            BinaryOp::Pow => {
                if b < 0 || (a == 0 && b == 0) {
                    None
                } else {
                    a.checked_pow(b as u32)
                }
            }
        };
        value.ok_or(LootError::Arithmetic)
    }
    fn aggregate(op: AggregateOp, values: &[Self]) -> Result<Self> {
        let value = match op {
            AggregateOp::Add => values
                .iter()
                .fold(0_i64, |a, v| a.wrapping_add(i64::from(*v))),
            AggregateOp::Mul => values
                .iter()
                .fold(1_i64, |a, v| a.wrapping_mul(i64::from(*v))),
            AggregateOp::Min => return Ok(values.iter().copied().min().unwrap_or(i32::MAX)),
            AggregateOp::Max => return Ok(values.iter().copied().max().unwrap_or(i32::MIN)),
            AggregateOp::Avg => {
                if values.is_empty() {
                    return Err(LootError::Arithmetic);
                }
                values.iter().map(|v| i64::from(*v)).sum::<i64>() / values.len() as i64
            }
            _ => return Err(LootError::Arithmetic),
        };
        i32::try_from(value).map_err(|_| LootError::Arithmetic)
    }
    fn uniform(r: &mut dyn RandomSource, min: Self, max: Self) -> Result<Self> {
        if min >= max {
            Ok(min)
        } else {
            let bound = max.wrapping_sub(min).wrapping_add(1);
            if bound <= 0 {
                return Err(codec::error("nonpositive random bound"));
            }
            Ok(r.next_int_bound(bound).wrapping_add(min))
        }
    }
    fn from_int(v: i32) -> Result<Self> {
        Ok(v)
    }
    fn from_float(v: f32) -> Result<Self> {
        if !v.is_finite() {
            return Err(LootError::Arithmetic);
        }
        i32::try_from(v as i64).map_err(|_| LootError::Arithmetic)
    }
    fn from_tag(v: &Tag) -> Option<Self> {
        match v {
            Tag::Float(n) => Some(*n as i32),
            Tag::Double(n) => Some(*n as i32),
            _ => v.as_int(),
        }
    }
    fn sanitize(v: Self) -> Self {
        v
    }
    fn is_zero(v: Self) -> bool {
        v == 0
    }
}
impl Scalar for f32 {
    const FLOAT: bool = true;
    fn decode<I: DecodeInput>(v: &I) -> Result<Self> {
        codec::float(v)
    }
    fn zero() -> Self {
        0.0
    }
    fn unary(op: UnaryOp, v: Self) -> Result<Self> {
        Ok(match op {
            UnaryOp::Abs => v.abs(),
            UnaryOp::Negate => -v,
            UnaryOp::Floor => v.floor(),
            UnaryOp::Ceil => mth::ceil(f64::from(v)) as f32,
            UnaryOp::Round => (f64::from(v) + 0.5).floor() as i32 as f32,
            UnaryOp::Truncate => v.trunc(),
            UnaryOp::Sin => mth::sin(f64::from(v)),
            UnaryOp::Cos => mth::cos(f64::from(v)),
            UnaryOp::Sqrt => v.sqrt(),
        })
    }
    fn binary(op: BinaryOp, a: Self, b: Self) -> Result<Self> {
        Ok(match op {
            BinaryOp::Sub => a - b,
            BinaryOp::Div => a / b,
            BinaryOp::Mod => a % b,
            BinaryOp::Pow => {
                if a == 0.0 && b == 0.0 {
                    f32::NAN
                } else {
                    f64::from(a).powf(f64::from(b)) as f32
                }
            }
            _ => return Err(LootError::Arithmetic),
        })
    }
    fn aggregate(op: AggregateOp, values: &[Self]) -> Result<Self> {
        let java_min = |a: f32, b: f32| {
            if a.is_nan() || b.is_nan() {
                f32::NAN
            } else {
                a.min(b)
            }
        };
        let java_max = |a: f32, b: f32| {
            if a.is_nan() || b.is_nan() {
                f32::NAN
            } else {
                a.max(b)
            }
        };
        Ok(match op {
            AggregateOp::Add => values.iter().fold(0.0, |a, v| a + v),
            AggregateOp::Mul => values.iter().fold(1.0, |a, v| a * v),
            AggregateOp::Avg => values.iter().fold(0.0, |a, v| a + v) / values.len() as f32,
            AggregateOp::Min => values.iter().copied().fold(f32::MAX, java_min),
            AggregateOp::Max => values.iter().copied().fold(-f32::MAX, java_max),
            AggregateOp::Length => values.iter().fold(0.0, |a, v| a + v * v).sqrt(),
        })
    }
    fn uniform(r: &mut dyn RandomSource, min: Self, max: Self) -> Result<Self> {
        Ok(if min >= max {
            min
        } else {
            r.next_float() * (max - min) + min
        })
    }
    fn from_int(v: i32) -> Result<Self> {
        Ok(v as f32)
    }
    fn from_float(v: f32) -> Result<Self> {
        Ok(v)
    }
    fn from_tag(v: &Tag) -> Option<Self> {
        v.as_float()
    }
    fn sanitize(v: Self) -> Self {
        if v.is_finite() { v } else { 0.0 }
    }
    fn is_zero(v: Self) -> bool {
        v == 0.0
    }
}
