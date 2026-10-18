use super::{UnsupportedAttribute, supports_scalar};
use crate::{Error, codec::*};
use ferraloom_data::registries::{attribute_type::Id as Type, environment_attribute::Id};
use ferraloom_util::{java, mth};
use serde_json::Value;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriState {
    True,
    False,
    Default,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScalarValue {
    Boolean(bool),
    Float(f32),
    TriState(TriState),
}
pub trait AttributeValue: Sized {
    fn project(value: ScalarValue) -> Option<Self>;
}
impl AttributeValue for bool {
    fn project(value: ScalarValue) -> Option<Self> {
        if let ScalarValue::Boolean(value) = value {
            Some(value)
        } else {
            None
        }
    }
}
impl AttributeValue for f32 {
    fn project(value: ScalarValue) -> Option<Self> {
        if let ScalarValue::Float(value) = value {
            Some(value)
        } else {
            None
        }
    }
}
impl AttributeValue for TriState {
    fn project(value: ScalarValue) -> Option<Self> {
        if let ScalarValue::TriState(value) = value {
            Some(value)
        } else {
            None
        }
    }
}
impl ScalarValue {
    pub fn native_default(id: Id) -> Result<Self, UnsupportedAttribute> {
        if !supports_scalar(id) {
            return Err(UnsupportedAttribute(id));
        }
        let json: Value =
            serde_json::from_str(id.data().default_json).expect("extracted native default JSON");
        Self::decode(id, &json, true).map_err(|_| UnsupportedAttribute(id))
    }
    fn decode(id: Id, value: &Value, validate: bool) -> Result<Self, Error> {
        Ok(match id.data().attribute_type {
            Type::BOOLEAN => Self::Boolean(boolean(value)?),
            Type::FLOAT | Type::ANGLE_DEGREES => {
                let scalar = float(value)?;
                let in_range = id.data().float_min.is_none_or(|min| scalar >= min)
                    && id.data().float_max.is_none_or(|max| scalar <= max);
                if validate && !in_range {
                    return Err(Error(format!(
                        "attribute {} value outside range",
                        id.name()
                    )));
                }
                Self::Float(scalar)
            }
            Type::TRI_STATE => Self::TriState(match value {
                Value::Bool(true) => TriState::True,
                Value::Bool(false) => TriState::False,
                Value::String(value) => match value.as_str() {
                    "true" => TriState::True,
                    "false" => TriState::False,
                    "default" => TriState::Default,
                    _ => return Err(Error("invalid tri-state".into())),
                },
                _ => return Err(Error("invalid tri-state".into())),
            }),
            _ => {
                return Err(Error(format!(
                    "unported attribute value type {}",
                    id.data().attribute_type.name()
                )));
            }
        })
    }
    pub fn sanitize(self, id: Id) -> Self {
        if let Self::Float(mut value) = self {
            if let Some(min) = id.data().float_min
                && value < min
            {
                value = min;
            }
            if let Some(max) = id.data().float_max
                && value > max
            {
                value = max;
            }
            Self::Float(value)
        } else {
            self
        }
    }
}
/// BooleanModifier's exact seven operations, shared with dimension FAST_LAVA.
#[derive(Clone, Copy, Debug)]
pub enum BooleanModifier {
    Override,
    And,
    Nand,
    Or,
    Nor,
    Xor,
    Xnor,
}
impl BooleanModifier {
    pub fn decode(operation: &str) -> Result<Self, Error> {
        match operation {
            "override" => Ok(Self::Override),
            "and" => Ok(Self::And),
            "nand" => Ok(Self::Nand),
            "or" => Ok(Self::Or),
            "nor" => Ok(Self::Nor),
            "xor" => Ok(Self::Xor),
            "xnor" => Ok(Self::Xnor),
            _ => Err(Error(format!("invalid BOOLEAN modifier {operation}"))),
        }
    }
    pub fn apply(self, subject: bool, argument: bool) -> bool {
        match self {
            Self::Override => argument,
            Self::And => subject && argument,
            Self::Nand => !(subject && argument),
            Self::Or => subject || argument,
            Self::Nor => !(subject || argument),
            Self::Xor => subject ^ argument,
            Self::Xnor => !(subject ^ argument),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum FloatModifier {
    Override,
    Add,
    Subtract,
    Multiply,
    Minimum,
    Maximum,
    AlphaBlend,
}
#[derive(Clone, Debug)]
pub enum AttributeEntry {
    Boolean {
        operation: BooleanModifier,
        argument: bool,
    },
    Float {
        operation: FloatModifier,
        argument: f32,
        alpha: f32,
    },
    TriState(TriState),
    /// Retain unported encoded families for the later typed decoder. Required
    /// sampling fails explicitly; this entry is never silently ignored.
    Unported(Value),
}
impl AttributeEntry {
    pub fn decode(id: Id, value: &Value) -> Result<Self, Error> {
        if !supports_scalar(id) {
            return Ok(Self::Unported(value.clone()));
        }
        let (operation, argument) = if value.is_object() {
            (
                string(field(value, "modifier")?)?,
                field(value, "argument")?,
            )
        } else {
            ("override", value)
        };
        if !id.data().modifiers.contains(&operation) {
            return Err(Error(format!(
                "modifier {operation} not allowed for {}",
                id.name()
            )));
        }
        Ok(match id.data().attribute_type {
            Type::BOOLEAN => Self::Boolean {
                operation: BooleanModifier::decode(operation)?,
                argument: boolean(argument)?,
            },
            Type::FLOAT | Type::ANGLE_DEGREES => {
                let (argument, alpha) = if operation == "alpha_blend" && argument.is_object() {
                    (
                        float(field(argument, "value")?)?,
                        optional(argument, "alpha", 1.0, |value| float_range(value, 0.0, 1.0))?,
                    )
                } else {
                    let ScalarValue::Float(value) =
                        ScalarValue::decode(id, argument, operation == "override")?
                    else {
                        unreachable!()
                    };
                    (value, 1.0)
                };
                let operation = match operation {
                    "override" => FloatModifier::Override,
                    "add" => FloatModifier::Add,
                    "subtract" => FloatModifier::Subtract,
                    "multiply" => FloatModifier::Multiply,
                    "minimum" => FloatModifier::Minimum,
                    "maximum" => FloatModifier::Maximum,
                    "alpha_blend" => FloatModifier::AlphaBlend,
                    _ => unreachable!("validated native modifier library"),
                };
                Self::Float {
                    operation,
                    argument,
                    alpha,
                }
            }
            Type::TRI_STATE => {
                let ScalarValue::TriState(value) = ScalarValue::decode(id, argument, true)? else {
                    unreachable!()
                };
                Self::TriState(value)
            }
            _ => unreachable!("checked scalar type"),
        })
    }
    pub fn apply(&self, id: Id, base: ScalarValue) -> Result<ScalarValue, UnsupportedAttribute> {
        Ok(match (self, base) {
            (
                Self::Boolean {
                    operation,
                    argument,
                },
                ScalarValue::Boolean(subject),
            ) => ScalarValue::Boolean(operation.apply(subject, *argument)),
            (
                Self::Float {
                    operation,
                    argument,
                    alpha,
                },
                ScalarValue::Float(subject),
            ) => ScalarValue::Float(match operation {
                FloatModifier::Override => *argument,
                FloatModifier::Add => subject + argument,
                FloatModifier::Subtract => subject - argument,
                FloatModifier::Multiply => subject * argument,
                FloatModifier::Minimum => java::min_f32(subject, *argument),
                FloatModifier::Maximum => java::max_f32(subject, *argument),
                FloatModifier::AlphaBlend => mth::lerp_f32(*alpha, subject, *argument),
            }),
            (Self::TriState(value), ScalarValue::TriState(_)) => ScalarValue::TriState(*value),
            _ => return Err(UnsupportedAttribute(id)),
        })
    }
}
