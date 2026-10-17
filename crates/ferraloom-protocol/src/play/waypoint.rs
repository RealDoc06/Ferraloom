//! Port of Waypoint.Icon and TrackedWaypoint's discriminated contents.
use super::*;
packet!(/// Port of Waypoint.Icon.
WaypointIcon {style:Identifier,color:Option<RgbColor>});
#[derive(Clone, Debug, PartialEq)]
pub struct RgbColor(pub u32);
impl Wire for RgbColor {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(
            (u32::from(r.u8()?) << 16) | (u32::from(r.u8()?) << 8) | u32::from(r.u8()?),
        ))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8((self.0 >> 16) as u8);
        w.u8((self.0 >> 8) as u8);
        w.u8(self.0 as u8);
        Ok(())
    }
}
wire_union!(WaypointContents:var_int {Empty=0 {},Vec3i=1 {position:Vec3i},Chunk=2 {x:VarInt,z:VarInt},Azimuth=3 {angle:f32}});
packet!(/// Port of TrackedWaypoint.
TrackedWaypointData {identifier:crate::codec::Either<Uuid,String>,icon:WaypointIcon,contents:WaypointContents});
wire_enum!(WaypointOperation {Track=0,Untrack=1,Update=2}, wrapping);
