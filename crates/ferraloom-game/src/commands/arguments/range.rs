use crate::commands::exceptions::error_at;
use ferraloom_brigadier::{CommandSyntaxException, ExceptionKind, StringReader};
/// Port of MinMaxBounds.Bounds; integer, double and wrapping degree ranges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NumberRange {
    pub min: Option<f64>,
    pub max: Option<f64>,
}
impl NumberRange {
    pub fn contains(self, value: f64) -> bool {
        self.min.is_none_or(|min| value >= min) && self.max.is_none_or(|max| value <= max)
    }
    pub fn parse(
        reader: &mut StringReader,
        integer: bool,
        degrees: bool,
    ) -> Result<Self, CommandSyntaxException> {
        let start = reader.cursor();
        let parse = |reader: &mut StringReader| -> Result<Option<f64>, CommandSyntaxException> {
            let number_start = reader.cursor();
            while reader.can_read()
                && (matches!(reader.peek(), 48..=57 | 45)
                    || reader.peek() == 46
                        && !(reader.can_read_length(2) && reader.peek_offset(1) == 46))
            {
                reader.skip();
            }
            if number_start == reader.cursor() {
                return Ok(None);
            }
            let text = reader.slice(number_start, reader.cursor());
            let value = if integer {
                text.parse::<i32>().map(f64::from).ok()
            } else if degrees {
                text.parse::<f32>().map(f64::from).ok()
            } else {
                text.parse::<f64>().ok()
            };
            value.map(Some).ok_or_else(|| {
                reader.error(
                    ExceptionKind::InvalidNumber(if integer {
                        "int"
                    } else if degrees {
                        "float"
                    } else {
                        "double"
                    }),
                    &[text],
                )
            })
        };
        let result = (|| {
            if !reader.can_read() {
                return Err(error_at(reader, "argument.range.empty", &[]));
            }
            let min = parse(reader)?;
            let max = if reader.can_read_length(2)
                && reader.peek() == 46
                && reader.peek_offset(1) == 46
            {
                reader.skip();
                reader.skip();
                parse(reader)?
            } else {
                min
            };
            if min.is_none() && max.is_none() {
                return Err(error_at(reader, "argument.range.empty", &[]));
            }
            if !degrees && min.zip(max).is_some_and(|(min, max)| min > max) {
                return Err(error_at(reader, "argument.range.swapped", &[]));
            }
            Ok(Self { min, max })
        })();
        result.map_err(|mut error| {
            reader.set_cursor(start);
            error.cursor = Some(start);
            error
        })
    }
}
