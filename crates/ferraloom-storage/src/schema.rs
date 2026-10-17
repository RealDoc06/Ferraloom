//! Storage models follow SerializableChunkData, SavedTick, EntityStorage and SectionStorage.
//! Optional fields retain presence (including empty lists); every compound keeps unknown tags.
//! Runtime interpretation and registry resolution belong to the game/chunk crates.
use crate::{ChunkPos, Error, Result, check_tag_version};
use ferraloom_nbt::{CompoundTag, ListTag, NbtString, Tag};
use std::collections::BTreeMap;

/// A typed field codec. Implementations reject malformed known fields instead of dropping them.
pub trait NbtField: Sized {
    fn from_tag(tag: Tag) -> Result<Self>;
    fn to_tag(&self) -> Tag;
}
fn wrong_type(expected: &str) -> Error {
    Error::Invalid(format!("expected {expected} in chunk NBT"))
}
macro_rules! scalar {
    ($ty:ty,$variant:ident,$expected:literal) => {
        impl NbtField for $ty {
            fn from_tag(tag: Tag) -> Result<Self> {
                if let Tag::$variant(value) = tag {
                    Ok(value)
                } else {
                    Err(wrong_type($expected))
                }
            }
            fn to_tag(&self) -> Tag {
                Tag::$variant(self.clone())
            }
        }
    };
}
scalar!(i8, Byte, "byte");
scalar!(i16, Short, "short");
scalar!(i32, Int, "int");
scalar!(i64, Long, "long");
scalar!(f32, Float, "float");
scalar!(NbtString, String, "string");
scalar!(CompoundTag, Compound, "compound");
impl NbtField for bool {
    fn from_tag(tag: Tag) -> Result<Self> {
        if let Tag::Byte(v) = tag {
            Ok(v != 0)
        } else {
            Err(wrong_type("boolean byte"))
        }
    }
    fn to_tag(&self) -> Tag {
        Tag::Byte(i8::from(*self))
    }
}
impl<T: NbtField> NbtField for Vec<T> {
    fn from_tag(tag: Tag) -> Result<Self> {
        if let Tag::List(list) = tag {
            list.0.into_iter().map(T::from_tag).collect()
        } else {
            Err(wrong_type("list"))
        }
    }
    fn to_tag(&self) -> Tag {
        Tag::List(ListTag(self.iter().map(T::to_tag).collect()))
    }
}
impl<T: NbtField> NbtField for BTreeMap<NbtString, T> {
    fn from_tag(tag: Tag) -> Result<Self> {
        if let Tag::Compound(compound) = tag {
            compound
                .iter()
                .map(|(key, value)| Ok((key.clone(), T::from_tag(value.clone())?)))
                .collect()
        } else {
            Err(wrong_type("compound map"))
        }
    }
    fn to_tag(&self) -> Tag {
        Tag::Compound(
            self.iter()
                .map(|(key, value)| (key.clone(), value.to_tag()))
                .collect(),
        )
    }
}
macro_rules! array {
    ($name:ident,$ty:ty,$variant:ident) => {
        #[derive(Clone, Debug, Default, PartialEq, Eq)]
        pub struct $name(pub Vec<$ty>);
        impl NbtField for $name {
            fn from_tag(tag: Tag) -> Result<Self> {
                if let Tag::$variant(v) = tag {
                    Ok(Self(v))
                } else {
                    Err(wrong_type(stringify!($variant)))
                }
            }
            fn to_tag(&self) -> Tag {
                Tag::$variant(self.0.clone())
            }
        }
    };
}
array!(ByteArray, i8, ByteArray);
array!(IntArray, i32, IntArray);
array!(LongArray, i64, LongArray);

// Optional typed fields plus an opaque remainder avoid normalizing or losing absent tags.
macro_rules! model {
    ($(#[$doc:meta])* $name:ident { $($field:ident : $ty:ty => $key:literal),* $(,)? })=>{
        $(#[$doc])* #[derive(Clone,Debug,Default,PartialEq)]
        pub struct $name {$(pub $field:Option<$ty>,)* pub extra:CompoundTag}
        impl $name {
            pub fn from_nbt(mut tag:CompoundTag)->Result<Self>{
                $(let $field=tag.remove($key).map(<$ty as NbtField>::from_tag).transpose().map_err(|e|Error::Invalid(format!("{}: {e}",$key)))?;)*
                Ok(Self{$($field,)*extra:tag})
            }
            pub fn to_nbt(&self)->CompoundTag{
                let mut tag=self.extra.clone();$(tag.remove($key);if let Some(value)=&self.$field{tag.insert($key,value.to_tag());})*tag
            }
        }
        impl NbtField for $name {
            fn from_tag(tag:Tag)->Result<Self>{Self::from_nbt(CompoundTag::from_tag(tag)?)}
            fn to_tag(&self)->Tag{Tag::Compound(self.to_nbt())}
        }
    };
}

/// Port of BlockState#CODEC: default states use a string; other states use Name/Properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockStatePaletteEntry {
    pub name: Option<NbtString>,
    pub properties: Option<BTreeMap<NbtString, NbtString>>,
    pub extra: CompoundTag,
    /// Retains whether the original palette used BlockState.CODEC's default-state shorthand.
    pub shorthand: bool,
}
impl BlockStatePaletteEntry {
    pub fn from_nbt(mut tag: CompoundTag) -> Result<Self> {
        let name = tag.remove("Name").map(NbtString::from_tag).transpose()?;
        let properties = tag
            .remove("Properties")
            .map(BTreeMap::<NbtString, NbtString>::from_tag)
            .transpose()?;
        Ok(Self {
            name,
            properties,
            extra: tag,
            shorthand: false,
        })
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = self.extra.clone();
        tag.remove("Name");
        tag.remove("Properties");
        if let Some(name) = &self.name {
            tag.insert("Name", name.to_tag());
        }
        if let Some(properties) = &self.properties {
            tag.insert("Properties", properties.to_tag());
        }
        tag
    }
}
impl NbtField for BlockStatePaletteEntry {
    fn from_tag(tag: Tag) -> Result<Self> {
        match tag {
            Tag::String(name) => Ok(Self {
                name: Some(name),
                shorthand: true,
                ..Self::default()
            }),
            Tag::Compound(tag) => Self::from_nbt(tag),
            _ => Err(wrong_type("block state string or compound")),
        }
    }
    fn to_tag(&self) -> Tag {
        if self.shorthand
            && self.properties.is_none()
            && self.extra.is_empty()
            && let Some(name) = &self.name
        {
            return name.to_tag();
        }
        Tag::Compound(self.to_nbt())
    }
}

/// Port of PalettedContainer's disk codec. Values remain names until registries are available.
#[derive(Clone, Debug, PartialEq)]
pub struct PackedPalette<T> {
    pub palette: Vec<T>,
    pub data: Option<LongArray>,
    pub extra: CompoundTag,
}
impl<T: NbtField> NbtField for PackedPalette<T> {
    fn from_tag(tag: Tag) -> Result<Self> {
        let mut tag = CompoundTag::from_tag(tag)?;
        let palette =
            Vec::<T>::from_tag(tag.remove("palette").ok_or_else(|| wrong_type("palette"))?)?;
        if palette.is_empty() {
            return Err(Error::Invalid(
                "paletted container has an empty palette".into(),
            ));
        }
        let data = tag.remove("data").map(LongArray::from_tag).transpose()?;
        Ok(Self {
            palette,
            data,
            extra: tag,
        })
    }
    fn to_tag(&self) -> Tag {
        let mut tag = self.extra.clone();
        tag.insert("palette", self.palette.to_tag());
        tag.remove("data");
        if let Some(data) = &self.data {
            tag.insert("data", data.to_tag());
        }
        Tag::Compound(tag)
    }
}

model! {/// Port of SerializableChunkData.SectionData (light-only sections have no palettes).
ChunkSection {
    y:i8=>"Y",block_states:PackedPalette<BlockStatePaletteEntry> =>"block_states",biomes:PackedPalette<NbtString> =>"biomes",
    block_light:ByteArray=>"BlockLight",sky_light:ByteArray=>"SkyLight"
}}
model! {/// Port of SavedTick#codec. Priority is vanilla's -3..3 integer representation.
SavedTick {id:NbtString=>"i",x:i32=>"x",y:i32=>"y",z:i32=>"z",delay:i32=>"t",priority:i32=>"p"}}
model! {/// Port of UpgradeData#write.
UpgradeData {indices:BTreeMap<NbtString,IntArray> =>"Indices",sides:i8=>"Sides",neighbor_block_ticks:Vec<SavedTick> =>"neighbor_block_ticks",neighbor_fluid_ticks:Vec<SavedTick> =>"neighbor_fluid_ticks"}}
model! {/// Port of BlendingData.Packed#CODEC.
BlendingData {min_section:i32=>"min_section",max_section:i32=>"max_section",heights:Vec<f32> =>"heights"}}
model! {/// Port of BelowZeroRetrogen#CODEC.
BelowZeroRetrogen {target_status:NbtString=>"target_status",missing_bedrock:LongArray=>"missing_bedrock"}}
model! {/// Port of SerializableChunkData#packStructureData. Structure starts stay opaque.
StructureData {starts:BTreeMap<NbtString,CompoundTag> =>"starts",references:BTreeMap<NbtString,LongArray> =>"References"}}

model! {/// Exact disk fields written by SerializableChunkData#write in 26.3.
/// Carving masks are no longer written by this version; any inherited mask tags stay in extra.
SerializableChunkData {
    data_version:i32=>"DataVersion",x_pos:i32=>"xPos",y_pos:i32=>"yPos",z_pos:i32=>"zPos",
    last_update:i64=>"LastUpdate",inhabited_time:i64=>"InhabitedTime",status:NbtString=>"Status",
    sections:Vec<ChunkSection> =>"sections",is_light_on:bool=>"isLightOn",
    heightmaps:BTreeMap<NbtString,LongArray> =>"Heightmaps",block_entities:Vec<CompoundTag> =>"block_entities",
    entities:Vec<CompoundTag> =>"entities",block_ticks:Vec<SavedTick> =>"block_ticks",fluid_ticks:Vec<SavedTick> =>"fluid_ticks",
    structures:StructureData=>"structures",post_processing:Vec<Vec<i16>> =>"PostProcessing",
    upgrade_data:UpgradeData=>"UpgradeData",blending_data:BlendingData=>"blending_data",below_zero_retrogen:BelowZeroRetrogen=>"below_zero_retrogen"
}}
impl SerializableChunkData {
    /// Version-gated entry point for loading a chunk from disk. `from_nbt` is a schema-only codec.
    pub fn parse(tag: CompoundTag) -> Result<Self> {
        check_tag_version(&tag)?;
        if tag.get_string("Status").is_none() {
            return Err(Error::Invalid("chunk has no Status".into()));
        }
        Self::from_nbt(tag)
    }
    pub fn position(&self) -> ChunkPos {
        ChunkPos::new(self.x_pos.unwrap_or(0), self.z_pos.unwrap_or(0))
    }
    pub fn write(&self) -> Result<CompoundTag> {
        let tag = self.to_nbt();
        check_tag_version(&tag)?;
        Ok(tag)
    }
}
model! {/// Port of EntityStorage#storeEntities. Position is ChunkPos.CODEC's two-int array.
EntityChunk {data_version:i32=>"DataVersion",position:IntArray=>"Position",entities:Vec<CompoundTag> =>"Entities"}}
impl EntityChunk {
    pub fn parse(tag: CompoundTag) -> Result<Self> {
        check_tag_version(&tag)?;
        let chunk = Self::from_nbt(tag)?;
        if chunk.position.as_ref().is_none_or(|p| p.0.len() != 2) {
            return Err(Error::Invalid(
                "entity chunk Position must contain two ints".into(),
            ));
        }
        Ok(chunk)
    }
    pub fn write(&self) -> Result<CompoundTag> {
        let tag = self.to_nbt();
        check_tag_version(&tag)?;
        Ok(tag)
    }
}
model! {/// Port of PoiRecord.Packed#CODEC.
PoiRecord {position:IntArray=>"pos",poi_type:NbtString=>"type",free_tickets:i32=>"free_tickets"}}
model! {/// Port of PoiSection.Packed#CODEC.
PoiSection {valid:bool=>"Valid",records:Vec<PoiRecord> =>"Records"}}
model! {/// Port of SectionStorage#writeChunk for POI sections, keyed by signed section Y.
PoiChunk {data_version:i32=>"DataVersion",sections:BTreeMap<NbtString,PoiSection> =>"Sections"}}
impl PoiChunk {
    pub fn parse(tag: CompoundTag) -> Result<Self> {
        check_tag_version(&tag)?;
        Self::from_nbt(tag)
    }
    pub fn write(&self) -> Result<CompoundTag> {
        let tag = self.to_nbt();
        check_tag_version(&tag)?;
        Ok(tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_unknown_fields_and_absence_round_trip() {
        let tag=ferraloom_nbt::parse_compound(r#"{DataVersion:5023,xPos:-1,zPos:3,Status:"minecraft:full",sections:[{Y:-4b,block_states:{palette:[{Name:"minecraft:air",future:3}],future_palette:{nested:[1,2,3]}},future_section:4}],future_root:{opaque:[L;1L,2L]}}"#).unwrap();
        let parsed = SerializableChunkData::parse(tag.clone()).unwrap();
        assert_eq!(parsed.position(), ChunkPos::new(-1, 3));
        assert_eq!(parsed.write().unwrap(), tag);
    }
}
