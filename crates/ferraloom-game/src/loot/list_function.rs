use super::{
    ContextFloatProvider, ContextIntProvider, EntityTarget, EvalContext, Holder, ListEdit,
    ListOperation, LootData, Result, codec,
};
use crate::loot::codec::{DecodeInput, PersistentInput};
use ferraloom_protocol::{chat::Component, codec::VarInt, component::*};

/// Ports of list- and page-editing loot functions using shared ListOperation.
#[derive(Clone, Debug, PartialEq)]
pub enum ListFunction {
    SetLore {
        lore: Vec<Component>,
        mode: ListOperation,
        entity: Option<EntityTarget>,
    },
    SetBookCover {
        title: Option<Filterable<BoundedString<32>>>,
        author: Option<String>,
        generation: Option<i32>,
    },
    SetWrittenBookPages {
        pages: Vec<Filterable<Component>>,
        mode: ListOperation,
    },
    SetWritableBookPages {
        pages: Vec<Filterable<BoundedString<1024>>>,
        mode: ListOperation,
    },
    SetFireworks {
        explosions: Option<ListEdit<FireworkExplosion>>,
        flight_duration: Option<i32>,
    },
    SetFireworkExplosion {
        shape: Option<FireworkShape>,
        colors: Option<Vec<i32>>,
        fade_colors: Option<Vec<i32>>,
        trail: Option<bool>,
        twinkle: Option<bool>,
    },
    SetCustomModelData {
        floats: Option<ListEdit<Holder<ContextFloatProvider>>>,
        flags: Option<ListEdit<bool>>,
        strings: Option<ListEdit<String>>,
        colors: Option<ListEdit<Holder<ContextIntProvider>>>,
    },
}
impl ListFunction {
    pub(crate) fn decode<I: DecodeInput>(kind: &str, v: &I) -> Result<Option<Self>> {
        Ok(Some(match kind {
            "minecraft:set_lore" => {
                let lore = codec::list(codec::field(v, "lore")?, |v| {
                    Ok(<Component as PersistentInput>::from_input(v)?)
                })?;
                if lore.len() > 256 {
                    return Err(codec::error("too much lore"));
                }
                Self::SetLore {
                    lore,
                    mode: ListOperation::decode(v, 256)?,
                    entity: codec::optional(v, "entity", EntityTarget::decode)?,
                }
            }
            "minecraft:set_book_cover" => {
                let generation = codec::optional(v, "generation", codec::int)?;
                if generation.is_some_and(|g| !(0..=3).contains(&g)) {
                    return Err(codec::error("book generation outside [0,3]"));
                }
                Self::SetBookCover {
                    title: codec::optional(v, "title", |v| {
                        Ok(Filterable::<BoundedString<32>>::from_input(v)?)
                    })?,
                    author: codec::optional(v, "author", |v| Ok(codec::string(v)?.into()))?,
                    generation,
                }
            }
            "minecraft:set_written_book_pages" => Self::SetWrittenBookPages {
                pages: written_pages(codec::field(v, "pages")?)?,
                mode: ListOperation::decode(v, i32::MAX as usize)?,
            },
            "minecraft:set_writable_book_pages" => Self::SetWritableBookPages {
                pages: BoundedList::<Filterable<BoundedString<1024>>, 100>::from_input(
                    codec::field(v, "pages")?,
                )?
                .0,
                mode: ListOperation::decode(v, 100)?,
            },
            "minecraft:set_fireworks" => Self::SetFireworks {
                explosions: codec::optional(v, "explosions", |v| {
                    ListEdit::decode(v, 256, |v| Ok(FireworkExplosion::from_input(v)?))
                })?,
                flight_duration: codec::optional(v, "flight_duration", |v| {
                    Ok(i32::from(codec::int(v)? as u8))
                })?,
            },
            "minecraft:set_firework_explosion" => Self::SetFireworkExplosion {
                shape: codec::optional(v, "shape", |v| Ok(FireworkShape::from_input(v)?))?,
                colors: codec::optional(v, "colors", |v| codec::list(v, codec::int))?,
                fade_colors: codec::optional(v, "fade_colors", |v| codec::list(v, codec::int))?,
                trail: codec::optional(v, "trail", codec::boolean)?,
                twinkle: codec::optional(v, "twinkle", codec::boolean)?,
            },
            "minecraft:set_custom_model_data" => Self::SetCustomModelData {
                floats: codec::optional(v, "floats", |v| {
                    ListEdit::decode(v, i32::MAX as usize, |v| {
                        Holder::decode(v, ContextFloatProvider::decode)
                    })
                })?,
                flags: codec::optional(v, "flags", |v| {
                    ListEdit::decode(v, i32::MAX as usize, codec::boolean)
                })?,
                strings: codec::optional(v, "strings", |v| {
                    ListEdit::decode(v, i32::MAX as usize, |v| Ok(codec::string(v)?.into()))
                })?,
                colors: codec::optional(v, "colors", |v| {
                    ListEdit::decode(v, i32::MAX as usize, |v| {
                        if let Some(rgb) = v.as_array() {
                            if rgb.len() != 3 {
                                return Err(codec::error("expected RGB vector"));
                            }
                            let channel = |v: &I| -> Result<i32> {
                                Ok((codec::float(v)? * 255.0).floor() as i32)
                            };
                            let r = channel(&rgb[0])?;
                            let g = channel(&rgb[1])?;
                            let b = channel(&rgb[2])?;
                            Ok(Holder::direct(ContextIntProvider::Constant(
                                (255 << 24) | (r << 16) | (g << 8) | b,
                            )))
                        } else {
                            Holder::decode(v, ContextIntProvider::decode)
                        }
                    })
                })?,
            },
            _ => return Ok(None),
        }))
    }
    pub(crate) fn apply(
        &self,
        mut stack: ItemStack,
        data: &LootData,
        c: &mut EvalContext<'_>,
    ) -> Result<ItemStack> {
        let map = stack.components()?;
        let component = match self {
            Self::SetLore { lore, mode, entity } => {
                let mut lines = Vec::new();
                for line in lore {
                    lines.push(
                        if let Some(entity) = entity.and_then(|target| c.params.entity(target)) {
                            c.facts().resolve_component(line, entity)?
                        } else {
                            line.clone()
                        },
                    );
                }
                let original = match map.get(ComponentTypeId::LORE) {
                    Some(DataComponent::Lore(value)) => value.0.as_slice(),
                    _ => &[],
                };
                DataComponent::Lore(BoundedList(mode.apply(original, &lines, 256)?))
            }
            Self::SetBookCover {
                title,
                author,
                generation,
            } => {
                let mut original = written_book(&map);
                if let Some(title) = title {
                    original.title = title.clone();
                }
                if let Some(author) = author {
                    original.author = author.clone();
                }
                if let Some(generation) = generation {
                    original.generation = VarInt(*generation);
                }
                DataComponent::WrittenBookContent(original)
            }
            Self::SetWrittenBookPages { pages, mode } => {
                let mut original = written_book(&map);
                original.pages = mode.apply(&original.pages, pages, i32::MAX as usize)?;
                original.resolved = false;
                DataComponent::WrittenBookContent(original)
            }
            Self::SetWritableBookPages { pages, mode } => {
                let original = match map.get(ComponentTypeId::WRITABLE_BOOK_CONTENT) {
                    Some(DataComponent::WritableBookContent(value)) => value.0.as_slice(),
                    _ => &[],
                };
                DataComponent::WritableBookContent(BoundedList(mode.apply(original, pages, 100)?))
            }
            Self::SetFireworks {
                explosions,
                flight_duration,
            } => {
                let mut original = match map.get(ComponentTypeId::FIREWORKS) {
                    Some(DataComponent::Fireworks(value)) => value.clone(),
                    _ => Fireworks {
                        flight_duration: VarInt(0),
                        explosions: BoundedList(Vec::new()),
                    },
                };
                if let Some(explosions) = explosions {
                    original.explosions.0 = explosions.operation.apply(
                        &original.explosions.0,
                        &explosions.values,
                        i32::MAX as usize,
                    )?;
                }
                if let Some(duration) = flight_duration {
                    original.flight_duration = VarInt(*duration);
                }
                DataComponent::Fireworks(original)
            }
            Self::SetFireworkExplosion {
                shape,
                colors,
                fade_colors,
                trail,
                twinkle,
            } => {
                let mut original = match map.get(ComponentTypeId::FIREWORK_EXPLOSION) {
                    Some(DataComponent::FireworkExplosion(value)) => value.clone(),
                    _ => FireworkExplosion {
                        shape: FireworkShape::SmallBall,
                        colors: Vec::new(),
                        fade_colors: Vec::new(),
                        has_trail: false,
                        has_twinkle: false,
                    },
                };
                if let Some(shape) = shape {
                    original.shape = *shape;
                }
                if let Some(colors) = colors {
                    original.colors = colors.clone();
                }
                if let Some(colors) = fade_colors {
                    original.fade_colors = colors.clone();
                }
                if let Some(trail) = trail {
                    original.has_trail = *trail;
                }
                if let Some(twinkle) = twinkle {
                    original.has_twinkle = *twinkle;
                }
                DataComponent::FireworkExplosion(original)
            }
            Self::SetCustomModelData {
                floats,
                flags,
                strings,
                colors,
            } => {
                let mut original = match map.get(ComponentTypeId::CUSTOM_MODEL_DATA) {
                    Some(DataComponent::CustomModelData(value)) => value.clone(),
                    _ => CustomModelData {
                        floats: Vec::new(),
                        flags: Vec::new(),
                        strings: Vec::new(),
                        colors: Vec::new(),
                    },
                };
                if let Some(edit) = floats {
                    let values = edit
                        .values
                        .iter()
                        .map(|v| v.get(data, c).map(Into::into))
                        .collect::<Result<Vec<_>>>()?;
                    original.floats =
                        edit.operation
                            .apply(&original.floats, &values, i32::MAX as usize)?;
                }
                if let Some(edit) = flags {
                    original.flags =
                        edit.operation
                            .apply(&original.flags, &edit.values, i32::MAX as usize)?;
                }
                if let Some(edit) = strings {
                    original.strings =
                        edit.operation
                            .apply(&original.strings, &edit.values, i32::MAX as usize)?;
                }
                if let Some(edit) = colors {
                    let values = edit
                        .values
                        .iter()
                        .map(|v| v.get(data, c))
                        .collect::<Result<Vec<_>>>()?;
                    original.colors =
                        edit.operation
                            .apply(&original.colors, &values, i32::MAX as usize)?;
                }
                DataComponent::CustomModelData(original)
            }
        };
        if let Some(s) = &mut stack.0 {
            s.components.set(component);
        }
        Ok(stack)
    }
}
/// Port of WrittenBookContent#CONTENT_CODEC's Gson JSON character limit.
fn written_pages<I: DecodeInput>(v: &I) -> Result<Vec<Filterable<Component>>> {
    let pages = Vec::<Filterable<Component>>::from_input(v)?;
    for page in &pages {
        for value in std::iter::once(&page.raw).chain(page.filtered.iter()) {
            if let Ok(json) = value.to_json() {
                // Gson's JsonWriter escapes these two Unicode line separators.
                let length = json.encode_utf16().count()
                    + json
                        .chars()
                        .filter(|c| matches!(*c, '\u{2028}' | '\u{2029}'))
                        .count()
                        * 5;
                if length > 32767 {
                    return Err(codec::error("written book page component too large"));
                }
            }
        }
    }
    Ok(pages)
}
fn written_book(map: &DataComponentMap) -> WrittenBookContent {
    match map.get(ComponentTypeId::WRITTEN_BOOK_CONTENT) {
        Some(DataComponent::WrittenBookContent(value)) => value.clone(),
        _ => WrittenBookContent {
            title: Filterable {
                raw: BoundedString(String::new()),
                filtered: None,
            },
            author: String::new(),
            generation: VarInt(0),
            pages: Vec::new(),
            resolved: true,
        },
    }
}
