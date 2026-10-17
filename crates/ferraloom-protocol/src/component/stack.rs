use super::{DataComponentPatch, invalid};
use crate::{Reader, Result, Wire, Writer, codec::Codec};

/// Port of ItemStack#OPTIONAL_STREAM_CODEC. `None` represents the empty stack.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemStack(pub Option<ItemStackContents>);
#[derive(Clone, Debug, PartialEq)]
pub struct ItemStackContents {
    pub item: ferraloom_data::Item,
    pub count: i32,
    pub components: DataComponentPatch,
}
impl ItemStack {
    pub fn new(item: ferraloom_data::Item, count: i32, components: DataComponentPatch) -> Self {
        if count <= 0 || item == ferraloom_data::Item::AIR {
            Self::default()
        } else {
            Self(Some(ItemStackContents {
                item,
                count,
                components,
            }))
        }
    }
    pub fn is_empty(&self) -> bool {
        self.0
            .as_ref()
            .is_none_or(|v| v.count <= 0 || v.item == ferraloom_data::Item::AIR)
    }
    fn decode_with(r: &mut Reader<'_>, untrusted: bool) -> Result<Self> {
        let count = r.var_int()?;
        if count <= 0 {
            return Ok(Self::default());
        }
        let item = r.read()?;
        let components = if untrusted {
            DataComponentPatch::decode_delimited(r)?
        } else {
            r.read()?
        };
        Ok(Self::new(item, count, components))
    }
    fn encode_with(&self, w: &mut Writer<'_>, untrusted: bool) -> Result<()> {
        if self.is_empty() {
            w.var_int(0);
            return Ok(());
        }
        let stack = self.0.as_ref().expect("nonempty");
        w.var_int(stack.count);
        w.write(&stack.item)?;
        if untrusted {
            stack.components.encode_delimited(w)
        } else {
            w.write(&stack.components)
        }
    }
}
impl Wire for ItemStack {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Self::decode_with(r, false)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.encode_with(w, false)
    }
}
/// ItemStack#STREAM_CODEC rejects empty stacks on both paths.
pub const ITEM_STACK: Codec<ItemStack> = Codec {
    read: |r| {
        let value = r.read::<ItemStack>()?;
        if value.is_empty() {
            Err(invalid("empty item stack"))
        } else {
            Ok(value)
        }
    },
    write: |w, v| {
        if v.is_empty() {
            Err(invalid("empty item stack"))
        } else {
            v.encode(w)
        }
    },
};
/// ItemStack#OPTIONAL_STREAM_CODEC.
pub const OPTIONAL_ITEM_STACK: Codec<ItemStack> = Codec {
    read: ItemStack::decode,
    write: |w, v| v.encode(w),
};
/// ItemStack#OPTIONAL_UNTRUSTED_STREAM_CODEC delimits each added component value.
pub const OPTIONAL_UNTRUSTED_ITEM_STACK: Codec<ItemStack> = Codec {
    read: |r| ItemStack::decode_with(r, true),
    write: |w, v| v.encode_with(w, true),
};

/// Port of ItemStackTemplate#STREAM_CODEC: item, count, patch (in that order).
/// Unlike ItemStack, its constructor permits negative counts and rejects only zero/air.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemStackTemplate {
    pub item: ferraloom_data::Item,
    pub count: i32,
    pub components: DataComponentPatch,
}
impl ItemStackTemplate {
    pub fn new(
        item: ferraloom_data::Item,
        count: i32,
        components: DataComponentPatch,
    ) -> Result<Self> {
        if count == 0 || item == ferraloom_data::Item::AIR {
            return Err(invalid("empty item stack template"));
        }
        Ok(Self {
            item,
            count,
            components,
        })
    }
}
impl Wire for ItemStackTemplate {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let item = r.read()?;
        let count = r.var_int()?;
        let components = r.read()?;
        Self::new(item, count, components)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        if self.count == 0 || self.item == ferraloom_data::Item::AIR {
            return Err(invalid("empty item stack template"));
        }
        w.write(&self.item)?;
        w.var_int(self.count);
        w.write(&self.components)
    }
}

/// Port of HashedPatchMap#STREAM_CODEC; hashes are fixed big-endian ints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HashedPatchMap {
    pub added: Vec<(super::ComponentTypeId, i32)>,
    pub removed: Vec<super::ComponentTypeId>,
}
impl Wire for HashedPatchMap {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            added: r.list(256, |r| Ok((r.read()?, r.i32()?)))?,
            removed: r.list(256, |r| r.read())?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.added, 256, |w, (key, hash)| {
            w.write(key)?;
            w.i32(*hash);
            Ok(())
        })?;
        w.list(&self.removed, 256, |w, key| w.write(key))
    }
}
/// Port of HashedStack#STREAM_CODEC. Presence uses a boolean, followed by item, count, hashes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HashedStack(pub Option<HashedStackContents>);
#[derive(Clone, Debug, PartialEq)]
pub struct HashedStackContents {
    pub item: ferraloom_data::Item,
    pub count: i32,
    pub components: HashedPatchMap,
}
impl Wire for HashedStack {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(r.optional(|r| {
            Ok(HashedStackContents {
                item: r.read()?,
                count: r.var_int()?,
                components: r.read()?,
            })
        })?))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.optional(&self.0, |w, v| {
            w.write(&v.item)?;
            w.var_int(v.count);
            w.write(&v.components)
        })
    }
}
