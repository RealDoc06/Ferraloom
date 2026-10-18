use crate::commands::exceptions::error;
use ferraloom_brigadier::CommandSyntaxException;
/// Port of OperationArgument's scoreboard arithmetic (wrapping Java ints).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Assign,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Min,
    Max,
    Swap,
}
impl Operation {
    pub fn parse(value: &str) -> Result<Self, CommandSyntaxException> {
        match value {
            "=" => Ok(Self::Assign),
            "+=" => Ok(Self::Add),
            "-=" => Ok(Self::Subtract),
            "*=" => Ok(Self::Multiply),
            "/=" => Ok(Self::Divide),
            "%=" => Ok(Self::Modulo),
            "<" => Ok(Self::Min),
            ">" => Ok(Self::Max),
            "><" => Ok(Self::Swap),
            _ => Err(error("arguments.operation.invalid", &[])),
        }
    }
    pub fn apply(self, a: &mut i32, b: &mut i32) -> Result<(), CommandSyntaxException> {
        *a = match self {
            Self::Assign => *b,
            Self::Add => a.wrapping_add(*b),
            Self::Subtract => a.wrapping_sub(*b),
            Self::Multiply => a.wrapping_mul(*b),
            Self::Divide | Self::Modulo => {
                if *b == 0 {
                    return Err(error("arguments.operation.div0", &[]));
                }
                if self == Self::Divide {
                    let quotient = a.wrapping_div(*b);
                    let remainder = a.wrapping_rem(*b);
                    if remainder != 0 && (*a < 0) != (*b < 0) {
                        quotient.wrapping_sub(1)
                    } else {
                        quotient
                    }
                } else {
                    a.wrapping_rem(*b).wrapping_add(*b).wrapping_rem(*b)
                }
            }
            Self::Min => (*a).min(*b),
            Self::Max => (*a).max(*b),
            Self::Swap => {
                let old = *a;
                let next = *b;
                *b = old;
                next
            }
        };
        Ok(())
    }
}
