use crate::{DATA_VERSION, Error, Result, check_tag_version, files};
use ferraloom_nbt::{CompoundTag, Tag};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Registry-aware game systems implement their own codec; storage owns the versioned envelope.
pub trait SavedDataCodec: Sized {
    fn decode(tag: &Tag) -> Result<Self>;
    fn encode(&self) -> Result<Tag>;
}
impl SavedDataCodec for Tag {
    fn decode(tag: &Tag) -> Result<Self> {
        Ok(tag.clone())
    }
    fn encode(&self) -> Result<Tag> {
        Ok(self.clone())
    }
}
impl SavedDataCodec for CompoundTag {
    fn decode(tag: &Tag) -> Result<Self> {
        tag.as_compound()
            .cloned()
            .ok_or_else(|| Error::Invalid("saved data payload is not a compound".into()))
    }
    fn encode(&self) -> Result<Tag> {
        Ok(Tag::Compound(self.clone()))
    }
}

/// Port of SavedData's dirty flag, with a lossless envelope for codec-independent fields.
#[derive(Clone, Debug)]
pub struct SavedData<T = Tag> {
    value: T,
    extra: CompoundTag,
    dirty: bool,
}
impl<T> SavedData<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            extra: CompoundTag::new(),
            dirty: true,
        }
    }
    pub fn value(&self) -> &T {
        &self.value
    }
    pub fn value_mut(&mut self) -> &mut T {
        self.dirty = true;
        &mut self.value
    }
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }
    pub fn extra(&self) -> &CompoundTag {
        &self.extra
    }
    pub fn extra_mut(&mut self) -> &mut CompoundTag {
        self.dirty = true;
        &mut self.extra
    }
}
impl<T: SavedDataCodec> SavedData<T> {
    pub fn from_nbt(mut tag: CompoundTag) -> Result<Self> {
        check_tag_version(&tag)?;
        let value = T::decode(
            &tag.remove("data")
                .ok_or_else(|| Error::Invalid("saved data has no data tag".into()))?,
        )?;
        Ok(Self {
            value,
            extra: tag,
            dirty: false,
        })
    }
    pub fn to_nbt(&self) -> Result<CompoundTag> {
        let mut tag = self.extra.clone();
        tag.insert("data", self.value.encode()?);
        tag.insert("DataVersion", Tag::Int(DATA_VERSION));
        Ok(tag)
    }
}

/// Port of 26.3 SavedDataStorage (formerly DimensionDataStorage).
/// Identifiers resolve to data/<namespace>/<path>.dat, including the minecraft namespace.
/// Synchronous methods must run off tick; callers can move the storage onto their IO executor.
pub struct SavedDataStorage {
    folder: PathBuf,
    cache: HashMap<String, Option<SavedData>>,
}
impl SavedDataStorage {
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
            cache: HashMap::new(),
        }
    }
    pub fn data_file(&self, id: &str) -> Result<PathBuf> {
        let mut path = files::identifier_path(&self.folder, id)?;
        let name = path
            .file_name()
            .ok_or_else(|| Error::Invalid("missing saved data filename".into()))?
            .to_string_lossy()
            .into_owned();
        path.set_file_name(format!("{name}.dat"));
        Ok(path)
    }
    pub fn load<T: SavedDataCodec>(&self, id: &str) -> Result<Option<SavedData<T>>> {
        let file = self.data_file(id)?;
        if !file.exists() {
            return Ok(None);
        }
        Ok(Some(SavedData::from_nbt(files::read_nbt(&file)?)?))
    }
    pub fn save<T: SavedDataCodec>(&self, id: &str, data: &mut SavedData<T>) -> Result<()> {
        if data.is_dirty() {
            write(&self.data_file(id)?, data)?;
            data.set_dirty(false);
        }
        Ok(())
    }
    pub fn get(&mut self, id: &str) -> Result<Option<&SavedData>> {
        self.ensure_cached(id)?;
        Ok(self.cache.get(id).and_then(Option::as_ref))
    }
    pub fn get_mut(&mut self, id: &str) -> Result<Option<&mut SavedData>> {
        self.ensure_cached(id)?;
        Ok(self.cache.get_mut(id).and_then(Option::as_mut))
    }
    fn ensure_cached(&mut self, id: &str) -> Result<()> {
        if !self.cache.contains_key(id) {
            let data = self.load(id)?;
            self.cache.insert(id.to_owned(), data);
        }
        Ok(())
    }
    pub fn set(&mut self, id: &str, mut data: SavedData) -> Result<()> {
        self.data_file(id)?;
        data.set_dirty(true);
        self.cache.insert(id.to_owned(), Some(data));
        Ok(())
    }
    pub fn compute_if_absent(
        &mut self,
        id: &str,
        create: impl FnOnce() -> Tag,
    ) -> Result<&mut SavedData> {
        self.ensure_cached(id)?;
        let entry = self.cache.entry(id.to_owned()).or_default();
        Ok(entry.get_or_insert_with(|| SavedData::new(create())))
    }
    pub fn save_all(&mut self) -> Result<()> {
        let ids: Vec<_> = self.cache.keys().cloned().collect();
        let mut error = None;
        for id in ids {
            let path = self.data_file(&id)?;
            if let Some(Some(data)) = self.cache.get_mut(&id)
                && data.is_dirty()
            {
                match write(&path, data) {
                    Ok(()) => data.set_dirty(false),
                    Err(e) => {
                        log::error!("Could not save {id}: {e}");
                        error = Some(e);
                    }
                }
            }
        }
        error.map_or(Ok(()), Err)
    }
    pub fn close(&mut self) -> Result<()> {
        self.save_all()
    }
}
fn write<T: SavedDataCodec>(path: &Path, data: &SavedData<T>) -> Result<()> {
    files::write_nbt(path, &data.to_nbt()?, None)
}
