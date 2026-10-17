use crate::{Error, RegionFileVersion, Result, files};
use ferraloom_nbt::{CompoundTag, Compression, NbtAccounter, NbtIo};
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const SECTOR: usize = 4096;
const HEADER: usize = 8192;

/// Port of ChunkPos's storage coordinates (arithmetic right shift handles negative chunks).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}
impl ChunkPos {
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
    pub const fn region(self) -> (i32, i32) {
        (self.x >> 5, self.z >> 5)
    }
    pub const fn offset_index(self) -> usize {
        ((self.x & 31) + (self.z & 31) * 32) as usize
    }
}

/// Port of RegionBitmap's first-fit allocation. Old sectors stay reserved until commit.
#[derive(Default, Debug)]
pub struct RegionBitmap {
    used: Vec<bool>,
}
impl RegionBitmap {
    pub fn force(&mut self, start: usize, size: usize) {
        self.used.resize(self.used.len().max(start + size), false);
        self.used[start..start + size].fill(true);
    }
    pub fn free(&mut self, start: usize, size: usize) {
        if start < self.used.len() {
            let end = (start + size).min(self.used.len());
            self.used[start..end].fill(false);
        }
    }
    pub fn allocate(&mut self, size: usize) -> usize {
        let mut start = 0;
        let mut run = 0;
        for (i, used) in self.used.iter().enumerate() {
            if *used {
                run = 0;
                start = i + 1;
            } else {
                run += 1;
                if run == size {
                    self.force(start, size);
                    return start;
                }
            }
        }
        self.force(start, size);
        start
    }
}

/// Port of RegionFile: Anvil headers, copy-on-write sectors, and external oversized streams.
pub struct RegionFile {
    path: PathBuf,
    external_dir: PathBuf,
    file: File,
    header: [u8; HEADER],
    used: RegionBitmap,
    version: RegionFileVersion,
    sync: bool,
    closed: bool,
}
impl RegionFile {
    pub fn open(
        path: impl AsRef<Path>,
        external_dir: impl AsRef<Path>,
        version: RegionFileVersion,
        sync: bool,
    ) -> Result<Self> {
        let path = path.as_ref().to_owned();
        let external_dir = external_dir.as_ref().to_owned();
        if !external_dir.is_dir() {
            return Err(Error::Invalid(format!(
                "expected external chunk directory: {}",
                external_dir.display()
            )));
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        let mut header = [0; HEADER];
        let mut read = 0;
        while read < HEADER {
            match file.read(&mut header[read..])? {
                0 => break,
                n => read += n,
            }
        }
        if read != 0 && read != HEADER {
            log::warn!(
                "Region file {} has truncated header: {read}",
                path.display()
            );
        }
        let size = file.metadata()?.len();
        let mut used = RegionBitmap::default();
        used.force(0, 2);
        for index in 0..1024 {
            let offset = header_int(&header, index);
            if offset == 0 {
                continue;
            }
            let sector = (offset >> 8) as usize;
            let count = (offset & 255) as usize;
            // Vanilla checks the start, not the end; partial final sectors are readable.
            if sector < 2 || count == 0 || sector as u64 * SECTOR as u64 > size {
                log::warn!(
                    "Region file {} has invalid sector at index {index}: {sector}+{count}",
                    path.display()
                );
                set_header_int(&mut header, index, 0);
            } else {
                used.force(sector, count);
            }
        }
        Ok(Self {
            path,
            external_dir,
            file,
            header,
            used,
            version,
            sync,
            closed: false,
        })
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn offset(&self, pos: ChunkPos) -> u32 {
        header_int(&self.header, pos.offset_index())
    }
    pub fn timestamp(&self, pos: ChunkPos) -> u32 {
        header_int(&self.header, 1024 + pos.offset_index())
    }
    pub fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.offset(pos) != 0
    }
    fn external_path(&self, pos: ChunkPos) -> PathBuf {
        self.external_dir.join(format!("c.{}.{}.mcc", pos.x, pos.z))
    }
    pub fn read(&mut self, pos: ChunkPos) -> Result<Option<CompoundTag>> {
        let Some(bytes) = self.read_bytes(pos)? else {
            return Ok(None);
        };
        Ok(Some(NbtIo::read_compound(
            bytes.as_slice(),
            Compression::None,
            &mut NbtAccounter::unlimited(),
        )?))
    }
    pub fn read_bytes(&mut self, pos: ChunkPos) -> Result<Option<Vec<u8>>> {
        self.ensure_open()?;
        let offset = self.offset(pos);
        if offset == 0 {
            return Ok(None);
        }
        let sector = offset >> 8;
        let count = offset & 255;
        self.file
            .seek(SeekFrom::Start(u64::from(sector) * SECTOR as u64))?;
        let mut bytes = Vec::with_capacity(count as usize * SECTOR);
        Read::by_ref(&mut self.file)
            .take(u64::from(count) * SECTOR as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() < 5 {
            log::error!("Chunk {pos:?} header is truncated");
            return Ok(None);
        }
        let length = i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let tag = bytes[4];
        if length == 0 {
            log::warn!("Chunk {pos:?} is allocated, but stream is missing");
            return Ok(None);
        }
        let payload = if tag & 128 != 0 {
            if length != 1 {
                log::warn!("Chunk {pos:?} has both internal and external streams");
            }
            let path = self.external_path(pos);
            if !path.is_file() {
                log::error!("External chunk path {} is not a file", path.display());
                return Ok(None);
            }
            fs::read(path)?
        } else {
            if length < 1 || length as usize - 1 > bytes.len() - 5 {
                log::error!("Chunk {pos:?} has negative or truncated stream length {length}");
                return Ok(None);
            }
            bytes[5..length as usize + 4].to_vec()
        };
        let Some(version) = RegionFileVersion::from_id(tag & 127) else {
            log::error!("Chunk {pos:?} has invalid stream version {tag}");
            return Ok(None);
        };
        if version == RegionFileVersion::Custom {
            // Java readUTF is also responsible for detecting malformed custom identifiers.
            if payload.len() < 2
                || u16::from_be_bytes([payload[0], payload[1]]) as usize > payload.len() - 2
            {
                return Err(Error::Invalid("truncated custom compression id".into()));
            }
            log::error!("Unrecognized custom region compression in chunk {pos:?}");
            return Ok(None);
        }
        Ok(Some(version.decompress(&payload)?))
    }
    /// Port of RegionFile#doesChunkExist. This only validates the stream header.
    pub fn does_chunk_exist(&mut self, pos: ChunkPos) -> bool {
        let offset = self.offset(pos);
        if offset == 0 || self.closed {
            return false;
        }
        let result = (|| -> std::io::Result<bool> {
            self.file
                .seek(SeekFrom::Start(u64::from(offset >> 8) * SECTOR as u64))?;
            let mut h = [0; 5];
            self.file.read_exact(&mut h)?;
            let length = i32::from_be_bytes([h[0], h[1], h[2], h[3]]);
            if RegionFileVersion::from_id(h[4] & 127).is_none() {
                return Ok(false);
            }
            if h[4] & 128 != 0 {
                Ok(self.external_path(pos).is_file())
            } else {
                Ok(length > 0 && i64::from(length) - 1 <= i64::from(offset & 255) * SECTOR as i64)
            }
        })();
        result.unwrap_or(false)
    }
    pub fn write(&mut self, pos: ChunkPos, value: &CompoundTag) -> Result<()> {
        let mut bytes = Vec::new();
        NbtIo::write_compound(&mut bytes, Compression::None, value)?;
        self.write_bytes(pos, &bytes)
    }
    pub fn write_bytes(&mut self, pos: ChunkPos, bytes: &[u8]) -> Result<()> {
        self.ensure_open()?;
        let payload = self.version.compress(bytes)?;
        let length = u32::try_from(payload.len() + 1)
            .map_err(|_| Error::Invalid("chunk stream exceeds 4 GiB".into()))?;
        let external = (payload.len() + 5).div_ceil(SECTOR) >= 256;
        let count = if external {
            1
        } else {
            (payload.len() + 5).div_ceil(SECTOR)
        };
        let start = self.used.allocate(count);
        if start > 0x00ff_ffff {
            self.used.free(start, count);
            return Err(Error::Invalid("region offset exceeds 24 bits".into()));
        }
        let old = self.offset(pos);
        let mut external_tmp = None;
        let result = (|| {
            if external {
                let (tmp, mut file) = files::temp_file(&self.external_dir, "chunk")?;
                external_tmp = Some(tmp);
                file.write_all(&payload)?;
                if self.sync {
                    file.sync_all()?;
                }
            }
            self.file
                .seek(SeekFrom::Start(start as u64 * SECTOR as u64))?;
            self.file
                .write_all(&if external { 1u32 } else { length }.to_be_bytes())?;
            self.file
                .write_all(&[self.version as u8 | if external { 128 } else { 0 }])?;
            if !external {
                self.file.write_all(&payload)?;
            }
            if self.sync {
                self.file.sync_data()?;
            }
            set_header_int(
                &mut self.header,
                pos.offset_index(),
                ((start as u32) << 8) | count as u32,
            );
            set_header_int(&mut self.header, 1024 + pos.offset_index(), timestamp());
            self.write_header()?;
            if let Some(tmp) = external_tmp.as_ref() {
                fs::rename(tmp, self.external_path(pos))?;
                if self.sync {
                    files::sync_directory(&self.external_dir)?;
                }
            } else {
                files::remove_if_exists(&self.external_path(pos))?;
            }
            Ok(())
        })();
        if result.is_ok() {
            if old != 0 {
                self.used.free((old >> 8) as usize, (old & 255) as usize);
            }
        } else if let Some(tmp) = external_tmp {
            let _ = files::remove_if_exists(&tmp);
        }
        result
    }
    pub fn clear(&mut self, pos: ChunkPos) -> Result<()> {
        self.ensure_open()?;
        let old = self.offset(pos);
        if old == 0 {
            return Ok(());
        }
        set_header_int(&mut self.header, pos.offset_index(), 0);
        set_header_int(&mut self.header, 1024 + pos.offset_index(), timestamp());
        self.write_header()?;
        files::remove_if_exists(&self.external_path(pos))?;
        self.used.free((old >> 8) as usize, (old & 255) as usize);
        Ok(())
    }
    fn write_header(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&self.header)?;
        if self.sync {
            self.file.sync_data()?;
        }
        Ok(())
    }
    pub fn flush(&self) -> Result<()> {
        self.ensure_open()?;
        self.file.sync_all()?;
        Ok(())
    }
    pub fn close(&mut self) -> Result<()> {
        if self.closed {
            return Ok(());
        }
        let size = self.file.metadata()?.len();
        let padded = size.div_ceil(SECTOR as u64) * SECTOR as u64;
        if size != padded {
            self.file.set_len(padded)?;
        }
        self.file.sync_all()?;
        self.closed = true;
        Ok(())
    }
    fn ensure_open(&self) -> Result<()> {
        if self.closed {
            Err(Error::Closed)
        } else {
            Ok(())
        }
    }
}
impl Drop for RegionFile {
    fn drop(&mut self) {
        if let Err(e) = self.close() {
            log::error!("Failed to close {}: {e}", self.path.display());
        }
    }
}
fn header_int(header: &[u8; HEADER], index: usize) -> u32 {
    let i = index * 4;
    u32::from_be_bytes([header[i], header[i + 1], header[i + 2], header[i + 3]])
}
fn set_header_int(header: &mut [u8; HEADER], index: usize, value: u32) {
    header[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
}
fn timestamp() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32
}

/// Port of RegionFileStorage, including the 256-entry LRU and negative lookups.
pub struct RegionFileStorage {
    folder: PathBuf,
    sync: bool,
    version: RegionFileVersion,
    cache: HashMap<(i32, i32), Option<RegionFile>>,
    lru: VecDeque<(i32, i32)>,
}
impl RegionFileStorage {
    pub fn new(folder: impl Into<PathBuf>, version: RegionFileVersion, sync: bool) -> Self {
        Self {
            folder: folder.into(),
            sync,
            version,
            cache: HashMap::new(),
            lru: VecDeque::new(),
        }
    }
    fn region(&mut self, pos: ChunkPos, create: bool) -> Result<Option<&mut RegionFile>> {
        let key = pos.region();
        self.lru.retain(|k| *k != key);
        self.lru.push_front(key);
        if !self.cache.contains_key(&key)
            || (create && self.cache.get(&key).is_some_and(Option::is_none))
        {
            let path = self.folder.join(format!("r.{}.{}.mca", key.0, key.1));
            let entry = if !create && !path.is_file() {
                None
            } else {
                fs::create_dir_all(&self.folder)?;
                Some(RegionFile::open(
                    path,
                    &self.folder,
                    self.version,
                    self.sync,
                )?)
            };
            self.cache.insert(key, entry);
        }
        if self.cache.len() > 256 {
            if let Some(key) = self.lru.pop_back() {
                if let Some(Some(mut region)) = self.cache.remove(&key) {
                    region.close()?;
                }
            }
        }
        Ok(self.cache.get_mut(&key).and_then(Option::as_mut))
    }
    pub fn read(&mut self, pos: ChunkPos) -> Result<Option<CompoundTag>> {
        match self.region(pos, false)? {
            Some(r) => r.read(pos),
            None => Ok(None),
        }
    }
    pub fn write(&mut self, pos: ChunkPos, value: Option<&CompoundTag>) -> Result<()> {
        if let Some(r) = self.region(pos, true)? {
            match value {
                Some(v) => r.write(pos, v),
                None => r.clear(pos),
            }
        } else {
            Ok(())
        }
    }
    pub fn flush(&mut self) -> Result<()> {
        for region in self.cache.values_mut().flatten() {
            region.flush()?;
        }
        Ok(())
    }
    pub fn close(&mut self) -> Result<()> {
        let mut error = None;
        for region in self.cache.values_mut().flatten() {
            if let Err(e) = region.close() {
                error = Some(e);
            }
        }
        self.cache.clear();
        self.lru.clear();
        error.map_or(Ok(()), Err)
    }
}
