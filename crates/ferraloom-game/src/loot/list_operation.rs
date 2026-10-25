use super::{Result, codec};
use crate::loot::codec::DecodeInput;

/// Port of ListOperation. Out-of-bounds insert/replacement and size overflows
/// preserve the original list; replace_all returns the replacement directly.
#[derive(Clone, Debug, PartialEq)]
pub enum ListOperation {
    ReplaceAll,
    Append,
    Insert { offset: usize },
    ReplaceSection { offset: usize, size: Option<usize> },
}
impl ListOperation {
    pub fn decode<I: DecodeInput>(v: &I, max: usize) -> Result<Self> {
        let offset = || -> Result<usize> {
            let n = codec::optional(v, "offset", codec::int)?.unwrap_or(0);
            usize::try_from(n).map_err(|_| codec::error("negative offset"))
        };
        Ok(match codec::string(codec::field(v, "mode")?)? {
            "replace_all" => Self::ReplaceAll,
            "append" => Self::Append,
            "insert" => Self::Insert { offset: offset()? },
            "replace_section" => {
                let size = codec::optional(v, "size", codec::int)?
                    .map(|n| usize::try_from(n).map_err(|_| codec::error("negative size")))
                    .transpose()?;
                if size.is_some_and(|n| n > max) {
                    return Err(codec::error("section size exceeds limit"));
                }
                Self::ReplaceSection {
                    offset: offset()?,
                    size,
                }
            }
            _ => return Err(codec::error("invalid list operation")),
        })
    }
    pub fn apply<T: Clone>(&self, original: &[T], replacement: &[T], max: usize) -> Result<Vec<T>> {
        let (offset, resume) = match self {
            Self::ReplaceAll => return Ok(replacement.to_vec()),
            Self::Append => (original.len(), original.len()),
            Self::Insert { offset } => (*offset, *offset),
            Self::ReplaceSection { offset, size } => {
                let resume = offset + size.unwrap_or(replacement.len());
                if resume > i32::MAX as usize {
                    return Err(codec::error("list resume index overflow"));
                }
                (*offset, resume.min(original.len()))
            }
        };
        if offset > original.len() {
            return Ok(original.to_vec());
        }
        let length = offset + replacement.len() + original.len() - resume;
        if length > max {
            return Ok(original.to_vec());
        }
        Ok(original[..offset]
            .iter()
            .chain(replacement)
            .chain(&original[resume..])
            .cloned()
            .collect())
    }
}
/// Port of ListOperation.StandAlone's mandatory values+mode record.
#[derive(Clone, Debug, PartialEq)]
pub struct ListEdit<T> {
    pub values: Vec<T>,
    pub operation: ListOperation,
}
impl<T> ListEdit<T> {
    pub fn decode<I: DecodeInput>(
        v: &I,
        max: usize,
        decode: impl Fn(&I) -> Result<T>,
    ) -> Result<Self> {
        let values = codec::list(codec::field(v, "values")?, decode)?;
        if values.len() > max {
            return Err(codec::error("list exceeds limit"));
        }
        Ok(Self {
            values,
            operation: ListOperation::decode(v, max)?,
        })
    }
}
