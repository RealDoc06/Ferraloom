//! Port of map decorations and optional rectangular map color patches.
use super::*;
packet!(/// Port of MapDecoration.
MapDecoration {decoration_type:MapDecorationTypeId,x:i8,y:i8,rotation:u8,name:Option<Component>});
#[derive(Clone, Debug, PartialEq)]
pub struct MapPatch {
    pub width: u8,
    pub height: u8,
    pub start_x: u8,
    pub start_y: u8,
    pub colors: ByteArray,
}
/// A zero width marks absence. There is no presence boolean.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionalMapPatch(pub Option<MapPatch>);
impl Wire for OptionalMapPatch {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let width = r.u8()?;
        Ok(Self(if width == 0 {
            None
        } else {
            Some(MapPatch {
                width,
                height: r.u8()?,
                start_x: r.u8()?,
                start_y: r.u8()?,
                colors: r.read()?,
            })
        }))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        if let Some(v) = &self.0 {
            if v.width == 0 {
                return Err(Error::invalid(
                    "map patch",
                    "present patch requires nonzero width",
                ));
            }
            w.u8(v.width);
            w.u8(v.height);
            w.u8(v.start_x);
            w.u8(v.start_y);
            v.colors.encode(w)
        } else {
            w.u8(0);
            Ok(())
        }
    }
}
