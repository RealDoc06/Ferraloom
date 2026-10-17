//! Port of RecipeDisplay and SlotDisplay's registry-dispatched network forms.
use super::*;
use crate::component::{ComponentTypeId, ItemSet, ItemStackTemplate, TrimPatternHolder};
wire_union!(SlotDisplay:var_int {
    Empty=0 {},
    AnyFuel=1 {},
    WithAnyPotion=2 {display:Box<SlotDisplay>},
    OnlyWithComponent=3 {source:Box<SlotDisplay>,component:ComponentTypeId},
    Item=4 {item:ferraloom_data::Item},
    ItemStack=5 {stack:ItemStackTemplate},
    Tag=6 {tag:ItemSet},
    Dyed=7 {dye:Box<SlotDisplay>,target:Box<SlotDisplay>},
    SmithingTrim=8 {base:Box<SlotDisplay>,material:Box<SlotDisplay>,pattern:Box<TrimPatternHolder>},
    WithRemainder=9 {input:Box<SlotDisplay>,remainder:Box<SlotDisplay>},
    Composite=10 {contents:Vec<SlotDisplay>}
});
wire_union!(RecipeDisplay:var_int {
    CraftingShapeless=0 {ingredients:Vec<SlotDisplay>,result:Box<SlotDisplay>,crafting_station:Box<SlotDisplay>},
    CraftingShaped=1 {width:VarInt,height:VarInt,ingredients:Vec<SlotDisplay>,result:Box<SlotDisplay>,crafting_station:Box<SlotDisplay>},
    Furnace=2 {ingredient:Box<SlotDisplay>,fuel:Box<SlotDisplay>,result:Box<SlotDisplay>,crafting_station:Box<SlotDisplay>,duration:VarInt,experience:f32},
    Stonecutter=3 {input:Box<SlotDisplay>,result:Box<SlotDisplay>,crafting_station:Box<SlotDisplay>},
    Smithing=4 {template:Box<SlotDisplay>,base:Box<SlotDisplay>,addition:Box<SlotDisplay>,result:Box<SlotDisplay>,crafting_station:Box<SlotDisplay>}
}, validate_recipe_display);
packet!(/// Port of RecipeDisplayEntry.
RecipeDisplayEntry {id:RecipeDisplayId,display:RecipeDisplay,group:OptionalVarInt,category:VarInt,crafting_requirements:Option<Vec<ItemSet>>});
packet!(/// Port of ClientboundRecipeBookAddPacket.Entry.
RecipeBookEntry {contents:RecipeDisplayEntry,flags:u8});
packet!(/// Port of ClientboundUpdateRecipesPacket's recipe property set entry.
RecipePropertySet {key:Identifier,items:Vec<ferraloom_data::Item>});
packet!(/// Port of SelectableRecipe.SingleInputEntry.noRecipeCodec.
SelectableRecipe {input:ItemSet,option_display:Box<SlotDisplay>});

fn validate_recipe_display(value: &RecipeDisplay) -> Result<()> {
    if let RecipeDisplay::CraftingShaped {
        width,
        height,
        ingredients,
        ..
    } = value
    {
        if usize::try_from(width.0.wrapping_mul(height.0)).ok() != Some(ingredients.len()) {
            return Err(Error::invalid(
                "shaped recipe display",
                "ingredient count differs from width * height",
            ));
        }
    }
    Ok(())
}
