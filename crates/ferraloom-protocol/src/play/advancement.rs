//! Port of advancement network forms, which omit rewards and trigger definitions.
use super::*;
wire_enum!(AdvancementType {Task=0,Challenge=1,Goal=2},zero);
#[derive(Clone, Debug, PartialEq)]
pub struct AdvancementDisplay {
    pub title: Component,
    pub description: Component,
    pub icon: crate::component::ItemStackTemplate,
    pub advancement_type: AdvancementType,
    pub background: Option<Identifier>,
    pub show_toast: bool,
    pub hidden: bool,
}
impl Wire for AdvancementDisplay {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let title = Component::decode_trusted(r)?;
        let description = Component::decode_trusted(r)?;
        let icon = r.read()?;
        let advancement_type = r.read()?;
        let flags = r.i32()?;
        Ok(Self {
            title,
            description,
            icon,
            advancement_type,
            background: if flags & 1 != 0 {
                Some(r.read()?)
            } else {
                None
            },
            show_toast: flags & 2 != 0,
            hidden: flags & 4 != 0,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.title.encode(w)?;
        self.description.encode(w)?;
        self.icon.encode(w)?;
        self.advancement_type.encode(w)?;
        w.i32(
            i32::from(self.background.is_some())
                | (i32::from(self.show_toast) << 1)
                | (i32::from(self.hidden) << 2),
        );
        if let Some(v) = &self.background {
            v.encode(w)?;
        }
        Ok(())
    }
}
packet!(/// Port of Advancement.STREAM_CODEC.
Advancement {parent:Option<Identifier>,display:Option<AdvancementDisplay>,requirements:Vec<Vec<String>>,sends_telemetry_event:bool});
packet!(/// Port of AdvancementHolder.
AdvancementHolder {id:Identifier,value:Advancement});
packet!(/// Port of ClientboundUpdateAdvancementsPacket.PositionedAdvancement.
PositionedAdvancement {advancement:AdvancementHolder,x:f32,y:f32});
packet!(/// Port of CriterionProgress, with an optional epoch-millisecond timestamp.
CriterionProgress {name:String,obtained:Option<i64>});
packet!(/// Port of ClientboundUpdateAdvancementsPacket's progress map entries.
AdvancementProgress {id:Identifier,criteria:Vec<CriterionProgress>});
