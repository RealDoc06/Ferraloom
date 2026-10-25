use anyhow::{Context, Result};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_storage::{
    BlockStatePaletteEntry, EntityChunk, LevelData, LevelStorageAccess, RegionFileStorage,
    RegionFileVersion, SerializableChunkData,
};
use ferraloom_util::{BlockPos, ChunkPos};
use std::path::Path;

/// Read-only, post-stop inspection through the real storage codecs.
pub struct SavedWorld {
    access: LevelStorageAccess,
}
impl SavedWorld {
    pub(crate) fn open(root: &Path) -> Result<Self> {
        Ok(Self {
            access: LevelStorageAccess::open(root.join("world"))?,
        })
    }
    pub fn level(&self) -> Result<LevelData> {
        Ok(self.access.read_level_data()?)
    }
    /// Read the namespaced saved-data payload through storage's DataVersion validation.
    pub fn saved_data(&self, id: &str) -> Result<Option<Tag>> {
        Ok(
            ferraloom_storage::SavedDataStorage::new(self.access.path().join("data"))
                .load::<Tag>(id)?
                .map(|data| data.value().clone()),
        )
    }
    /// Read the actual on-disk PlayerAdvancements JSON after the server has saved and stopped.
    pub fn player_advancements(&self, name: &str) -> Result<serde_json::Value> {
        let uuid = ferraloom_protocol::Uuid::offline_player(name);
        let path = self
            .access
            .path()
            .join("players/advancements")
            .join(format!("{uuid}.json"));
        Ok(serde_json::from_slice(
            &std::fs::read(&path).with_context(|| format!("saved player advancements {name}"))?,
        )?)
    }
    pub fn player(&self, name: &str) -> Result<CompoundTag> {
        self.access
            .player_data()
            .load(&ferraloom_protocol::Uuid::offline_player(name).to_string())?
            .with_context(|| format!("saved player {name} missing"))
    }
    pub fn player_position(&self, name: &str) -> Result<[f64; 3]> {
        let player = self.player(name)?;
        let pos = &player
            .get_list("Pos")
            .context("saved player Pos missing")?
            .0;
        let values = pos
            .iter()
            .map(|tag| match tag {
                Tag::Double(v) => Ok(*v),
                _ => anyhow::bail!("non-double player position"),
            })
            .collect::<Result<Vec<_>>>()?;
        values
            .try_into()
            .map_err(|_| anyhow::anyhow!("Pos must contain exactly three doubles"))
    }
    pub fn chunk(&self, dimension: &str, pos: ChunkPos) -> Result<Option<SerializableChunkData>> {
        let mut files = RegionFileStorage::new(
            self.access.dimension(dimension)?.region,
            RegionFileVersion::Deflate,
            false,
        );
        files
            .read(pos)?
            .map(SerializableChunkData::parse)
            .transpose()
            .map_err(Into::into)
    }
    pub fn entities(&self, dimension: &str, pos: ChunkPos) -> Result<Option<EntityChunk>> {
        let mut files = RegionFileStorage::new(
            self.access.dimension(dimension)?.entities,
            RegionFileVersion::Deflate,
            false,
        );
        files
            .read(pos)?
            .map(EntityChunk::parse)
            .transpose()
            .map_err(Into::into)
    }
    pub fn block_entities(&self, dimension: &str, pos: ChunkPos) -> Result<Vec<CompoundTag>> {
        Ok(self
            .chunk(dimension, pos)?
            .context("saved chunk missing")?
            .block_entities
            .unwrap_or_default())
    }
    pub fn block(&self, dimension: &str, pos: BlockPos) -> Result<BlockStatePaletteEntry> {
        let chunk = self
            .chunk(dimension, ChunkPos::new(pos.x >> 4, pos.z >> 4))?
            .context("saved chunk missing")?;
        let section = chunk
            .sections
            .context("chunk sections missing")?
            .into_iter()
            .find(|s| s.y.map(i32::from) == Some(pos.y >> 4))
            .context("saved section missing")?;
        let palette = section
            .block_states
            .context("section block palette missing")?;
        let index = ((pos.y & 15) * 256 + (pos.z & 15) * 16 + (pos.x & 15)) as usize;
        let entry = if palette.palette.len() == 1 {
            0
        } else {
            let bits = (usize::BITS - (palette.palette.len() - 1).leading_zeros()).max(4) as usize;
            let per = 64 / bits;
            let data = palette.data.context("packed block data missing")?;
            ((*data
                .0
                .get(index / per)
                .context("truncated block palette data")? as u64
                >> ((index % per) * bits))
                & ((1 << bits) - 1)) as usize
        };
        palette
            .palette
            .get(entry)
            .cloned()
            .context("block palette index out of bounds")
    }
}
