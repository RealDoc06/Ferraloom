use super::{
    ContextFloatProvider, ContextIntProvider, EntityTarget, EvalContext, Holder, Identifier,
    LootData, Result, codec,
};
use crate::loot::codec::{DecodeInput, PersistentInput};
use ferraloom_data::Item;
use ferraloom_protocol::component::*;
use ferraloom_util::random::RandomSource;

#[derive(Clone, Debug, PartialEq)]
pub struct LootAttributeModifier {
    pub id: Identifier,
    pub attribute: Attribute,
    pub operation: AttributeOperation,
    pub amount: Holder<ContextFloatProvider>,
    pub slots: Vec<EquipmentSlotGroup>,
}
/// Ports of SetAttributesFunction, SetRandomDyesFunction and FillPlayerHead.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemFunction {
    SetAttributes {
        modifiers: Vec<LootAttributeModifier>,
        replace: bool,
    },
    SetRandomDyes(Holder<ContextIntProvider>),
    FillPlayerHead(EntityTarget),
}
impl ItemFunction {
    pub(crate) fn decode<I: DecodeInput>(kind: &str, v: &I) -> Result<Option<Self>> {
        Ok(Some(match kind {
            "minecraft:set_attributes" => Self::SetAttributes {
                modifiers: codec::list(codec::field(v, "modifiers")?, |v| {
                    let slot = codec::field(v, "slot")?;
                    let slots = if slot.is_array() {
                        Vec::<EquipmentSlotGroup>::from_input(slot)?
                    } else {
                        vec![EquipmentSlotGroup::from_input(slot)?]
                    };
                    if slots.is_empty() {
                        return Err(codec::error("empty modifier slots"));
                    }
                    Ok(LootAttributeModifier {
                        id: codec::id(codec::field(v, "id")?)?,
                        attribute: Attribute::from_input(codec::field(v, "attribute")?)?,
                        operation: AttributeOperation::from_input(codec::field(v, "operation")?)?,
                        amount: Holder::decode(
                            codec::field(v, "amount")?,
                            ContextFloatProvider::decode,
                        )?,
                        slots,
                    })
                })?,
                replace: codec::optional(v, "replace", codec::boolean)?.unwrap_or(true),
            },
            "minecraft:set_random_dyes" => Self::SetRandomDyes(Holder::decode(
                codec::field(v, "number_of_dyes")?,
                ContextIntProvider::decode,
            )?),
            "minecraft:fill_player_head" => {
                Self::FillPlayerHead(EntityTarget::decode(codec::field(v, "entity")?)?)
            }
            _ => return Ok(None),
        }))
    }
    pub(crate) fn apply(
        &self,
        mut stack: ItemStack,
        data: &LootData,
        c: &mut EvalContext<'_>,
    ) -> Result<ItemStack> {
        match self {
            Self::SetAttributes { modifiers, replace } => {
                let mut value = if *replace {
                    Vec::new()
                } else {
                    match stack
                        .components()?
                        .get(ComponentTypeId::ATTRIBUTE_MODIFIERS)
                    {
                        Some(DataComponent::AttributeModifiers(v)) => v.clone(),
                        _ => Vec::new(),
                    }
                };
                for modifier in modifiers {
                    let slot = modifier.slots
                        [c.random().next_int_bound(modifier.slots.len() as i32) as usize];
                    let amount = f64::from(modifier.amount.get(data, c)?);
                    value.retain(|v: &AttributeEntry| {
                        !(v.attribute == modifier.attribute && v.modifier.id == modifier.id)
                    });
                    value.push(AttributeEntry {
                        attribute: modifier.attribute.clone(),
                        modifier: AttributeModifier {
                            id: modifier.id.clone(),
                            amount: amount.into(),
                            operation: modifier.operation,
                        },
                        slot,
                        display: AttributeDisplay::Default,
                    });
                }
                if let Some(s) = &mut stack.0 {
                    s.components.set(DataComponent::AttributeModifiers(value));
                }
            }
            Self::FillPlayerHead(target) => {
                if !stack.is_empty()
                    && stack
                        .0
                        .as_ref()
                        .is_some_and(|s| s.item == Item::PLAYER_HEAD)
                    && let Some(entity) = c.params.entity(*target)
                    && let Some(profile) = c.facts().player_profile(entity)?
                {
                    stack
                        .0
                        .as_mut()
                        .expect("nonempty")
                        .components
                        .set(DataComponent::Profile(profile));
                }
            }
            Self::SetRandomDyes(rolls) => {
                let rolls = rolls.get(data, c)?;
                if rolls <= 0 {
                    return Ok(stack);
                }
                // DyeColor.VALUES texture diffuse colors in vanilla enum order.
                const COLORS: [i32; 16] = [
                    16383998, 16351261, 13061821, 3847130, 16701501, 8439583, 15961002, 4673362,
                    10329495, 1481884, 8991416, 3949738, 8606770, 6192150, 11546150, 1908001,
                ];
                let mut red = 0_i32;
                let mut green = 0_i32;
                let mut blue = 0_i32;
                let mut intensity = 0_i32;
                let mut count = 0_i32;
                let mut add = |rgb: i32| {
                    let r = (rgb >> 16) & 255;
                    let g = (rgb >> 8) & 255;
                    let b = rgb & 255;
                    red = red.wrapping_add(r);
                    green = green.wrapping_add(g);
                    blue = blue.wrapping_add(b);
                    intensity = intensity.wrapping_add(r.max(g).max(b));
                    count = count.wrapping_add(1);
                };
                if let Some(DataComponent::DyedColor(rgb)) =
                    stack.components()?.get(ComponentTypeId::DYED_COLOR)
                {
                    add(*rgb);
                }
                for _ in 0..rolls {
                    add(COLORS[c.random().next_int_bound(16) as usize]);
                }
                red /= count;
                green /= count;
                blue /= count;
                let average = intensity as f32 / count as f32;
                let maximum = red.max(green).max(blue) as f32;
                red = (red as f32 * average / maximum) as i32;
                green = (green as f32 * average / maximum) as i32;
                blue = (blue as f32 * average / maximum) as i32;
                if !stack.is_empty() {
                    let s = stack.0.as_mut().expect("nonempty");
                    s.count = 1;
                    s.components
                        .set(DataComponent::DyedColor((red << 16) | (green << 8) | blue));
                }
            }
        }
        Ok(stack)
    }
}
