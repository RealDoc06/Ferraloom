//! Container, equipment, merchant, and recipe wire forms.
use super::*;
use crate::component::{DataComponent, HashedStack, ItemStack};
wire_enum!(ContainerInput {Pickup=0,QuickMove=1,Swap=2,Clone=3,Throw=4,QuickCraft=5,PickupAll=6}, zero);
packet!(/// Port of ServerboundContainerClickPacket's changed slot entries.
ChangedSlot {slot:i16,stack:HashedStack});
#[derive(Clone, Debug, PartialEq)]
pub struct ChangedSlots(pub Vec<ChangedSlot>);
impl Wire for ChangedSlots {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        r.list(128, Wire::decode).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.0, 128, |w, v| v.encode(w))
    }
}
packet!(/// Port of ItemCost#STREAM_CODEC.
ItemCost {item:ferraloom_data::Item,count:VarInt,components:Vec<DataComponent>});
/// Port of ItemStack.STREAM_CODEC's non-empty requirement.
#[derive(Clone, Debug, PartialEq)]
pub struct NonEmptyItemStack(pub ItemStack);
impl Wire for NonEmptyItemStack {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        use crate::codec::StreamCodec;
        crate::component::ITEM_STACK.decode(r).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        use crate::codec::StreamCodec;
        crate::component::ITEM_STACK.encode(w, &self.0)
    }
}
/// Port of ItemStack.OPTIONAL_UNTRUSTED_STREAM_CODEC's delimited component values.
#[derive(Clone, Debug, PartialEq)]
pub struct UntrustedItemStack(pub ItemStack);
impl Wire for UntrustedItemStack {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        use crate::codec::StreamCodec;
        crate::component::OPTIONAL_UNTRUSTED_ITEM_STACK
            .decode(r)
            .map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        use crate::codec::StreamCodec;
        crate::component::OPTIONAL_UNTRUSTED_ITEM_STACK.encode(w, &self.0)
    }
}
/// Port of MerchantOffer#writeToStream/createFromStream. Stock status derives from usage.
#[derive(Clone, Debug, PartialEq)]
pub struct MerchantOffer {
    pub cost_a: ItemCost,
    pub result: NonEmptyItemStack,
    pub cost_b: Option<ItemCost>,
    pub uses: i32,
    pub max_uses: i32,
    pub xp: i32,
    pub special_price_diff: i32,
    pub price_multiplier: f32,
    pub demand: i32,
}
impl Wire for MerchantOffer {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let cost_a = r.read()?;
        let result = r.read()?;
        let cost_b = r.read()?;
        let exhausted = r.bool()?;
        let mut uses = r.i32()?;
        let max_uses = r.i32()?;
        let xp = r.i32()?;
        let special_price_diff = r.i32()?;
        let price_multiplier = r.f32()?;
        let demand = r.i32()?;
        if exhausted {
            uses = max_uses;
        }
        Ok(Self {
            cost_a,
            result,
            cost_b,
            uses,
            max_uses,
            xp,
            special_price_diff,
            price_multiplier,
            demand,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.cost_a.encode(w)?;
        self.result.encode(w)?;
        self.cost_b.encode(w)?;
        w.bool(self.uses >= self.max_uses);
        w.i32(self.uses);
        w.i32(self.max_uses);
        w.i32(self.xp);
        w.i32(self.special_price_diff);
        w.f32(self.price_multiplier);
        w.i32(self.demand);
        Ok(())
    }
}
packet!(/// Port of RecipeBookSettings.TypeSettings.
RecipeBookTypeSettings {open:bool,filtering:bool});
/// Port of RecipeBookSettings. Four book types have no list prefix.
#[derive(Clone, Debug, PartialEq)]
pub struct RecipeBookSettings(pub [RecipeBookTypeSettings; 4]);
impl Wire for RecipeBookSettings {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self([r.read()?, r.read()?, r.read()?, r.read()?]))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        for settings in &self.0 {
            settings.encode(w)?;
        }
        Ok(())
    }
}
