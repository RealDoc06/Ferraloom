//! Port of BossEvent and ClientboundBossEventPacket's operation dispatch.
use super::*;
wire_enum!(BossBarColor {Pink=0,Blue=1,Red=2,Green=3,Yellow=4,Purple=5,White=6});
wire_enum!(BossBarOverlay {Progress=0,Notched6=1,Notched10=2,Notched12=3,Notched20=4});
#[derive(Clone, Debug, PartialEq)]
pub struct BossProperties {
    pub darken_screen: bool,
    pub play_music: bool,
    pub create_world_fog: bool,
}
impl Wire for BossProperties {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let b = r.u8()?;
        Ok(Self {
            darken_screen: b & 1 != 0,
            play_music: b & 2 != 0,
            create_world_fog: b & 4 != 0,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(u8::from(self.darken_screen)
            | (u8::from(self.play_music) << 1)
            | (u8::from(self.create_world_fog) << 2));
        Ok(())
    }
}
wire_union!(BossOperation:var_int {
    Add=0 {name:Component => Component::decode_trusted,progress:f32,color:BossBarColor,overlay:BossBarOverlay,properties:BossProperties},
    Remove=1 {},
    UpdateProgress=2 {progress:f32},
    UpdateName=3 {name:Component => Component::decode_trusted},
    UpdateStyle=4 {color:BossBarColor,overlay:BossBarOverlay},
    UpdateProperties=5 {properties:BossProperties}
});
