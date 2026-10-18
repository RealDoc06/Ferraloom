//! Common StructurePiece state and world-position helpers.
use super::{BoundingBox, nbt::string};
use crate::{Error, Identifier};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::Direction;

/// Port of StructurePiece's persisted base fields.
#[derive(Clone, Debug)]
pub struct StructurePieceData {
    pub bounds: BoundingBox,
    pub gen_depth: i32,
    pub orientation: Option<Direction>,
}
impl StructurePieceData {
    pub fn new(bounds: BoundingBox, gen_depth: i32, orientation: Option<Direction>) -> Self {
        Self {
            bounds,
            gen_depth,
            orientation,
        }
    }
    pub fn to_nbt(&self, id: &Identifier) -> CompoundTag {
        [
            ("id", string(id)),
            (
                "BB",
                Tag::IntArray(self.bounds.min.into_iter().chain(self.bounds.max).collect()),
            ),
            ("GD", Tag::Int(self.gen_depth)),
            (
                "O",
                Tag::Int(self.orientation.map_or(-1, Direction::data_2d)),
            ),
        ]
        .into_iter()
        .collect()
    }
    pub fn from_nbt(tag: &CompoundTag) -> Result<Self, Error> {
        // BoundingBox.CODEC consumes an int stream, including homogeneous lists.
        let bounds = match tag.get("BB") {
            Some(Tag::IntArray(a)) => a.clone(),
            Some(Tag::List(a)) => a
                .iter()
                .map(|v| v.as_int().ok_or_else(|| Error("invalid piece BB".into())))
                .collect::<Result<Vec<_>, _>>()?,
            _ => return Err(Error("missing piece BB".into())),
        };
        if bounds.len() != 6 {
            return Err(Error("piece BB requires six integers".into()));
        }
        let orientation = tag.get_int("O").unwrap_or(0);
        Ok(Self {
            bounds: BoundingBox::new(
                [bounds[0], bounds[1], bounds[2]],
                [bounds[3], bounds[4], bounds[5]],
            ),
            gen_depth: tag.get_int("GD").unwrap_or(0),
            orientation: (orientation != -1).then(|| Direction::from_2d_data_value(orientation)),
        })
    }
}
