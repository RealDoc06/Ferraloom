//! Port of ClientboundLevelChunkPacketData and PalettedContainer's 26.3 wire form.
use super::*;

wire_enum!(HeightmapType { WorldSurfaceWg=0, WorldSurface=1, OceanFloorWg=2, OceanFloor=3, MotionBlocking=4, MotionBlockingNoLeaves=5 });
packet!(/// Port of ClientboundLevelChunkPacketData's heightmap entries.
Heightmap { kind:HeightmapType, data:Vec<i64> });
packet!(/// Port of ClientboundLevelChunkPacketData.BlockEntityInfo.
BlockEntityInfo { packed_xz:u8, y:i16, block_entity_type:BlockEntityTypeId, tag:OptionalNbt });

/// Port of PalettedContainer.Data#write. The packed longs have no length prefix in 26.3.
#[derive(Clone, Debug, PartialEq)]
pub struct PalettedContainerData {
    pub bits: u8,
    pub palette: Vec<VarInt>,
    pub data: Vec<i64>,
}
impl PalettedContainerData {
    fn decode(r: &mut Reader<'_>, entries: usize, local_max: u8, min_bits: u8) -> Result<Self> {
        let bits = r.u8()?;
        if bits > 64 {
            return Err(Error::invalid("palette bits", bits.to_string()));
        }
        let palette = if bits == 0 {
            vec![r.read()?]
        } else if bits <= local_max {
            r.read()?
        } else {
            Vec::new()
        };
        let effective_bits = if bits == 0 {
            0
        } else if bits <= local_max {
            bits.max(min_bits)
        } else {
            bits
        };
        let words = if effective_bits == 0 {
            0
        } else {
            entries.div_ceil(64 / usize::from(effective_bits))
        };
        let mut data = Vec::with_capacity(words);
        for _ in 0..words {
            data.push(r.i64()?);
        }
        Ok(Self {
            bits: effective_bits,
            palette,
            data,
        })
    }
    fn encode(&self, w: &mut Writer<'_>, entries: usize, local_max: u8) -> Result<()> {
        if self.bits > 64 {
            return Err(Error::invalid("palette bits", self.bits.to_string()));
        }
        let expected = if self.bits == 0 {
            0
        } else {
            entries.div_ceil(64 / usize::from(self.bits))
        };
        if self.data.len() != expected {
            return Err(Error::invalid(
                "palette storage",
                "wrong number of packed longs",
            ));
        }
        w.u8(self.bits);
        if self.bits == 0 {
            if self.palette.len() != 1 {
                return Err(Error::invalid("single palette", "expected one entry"));
            }
            self.palette[0].encode(w)?;
        } else if self.bits <= local_max {
            self.palette.encode(w)?;
        }
        for word in &self.data {
            w.i64(*word);
        }
        Ok(())
    }
    pub fn from_container<T: Copy + Eq + std::hash::Hash>(
        container: &ferraloom_chunk::PalettedContainer<T>,
        mut id: impl FnMut(T) -> i32,
    ) -> Self {
        Self {
            bits: container.bits_per_entry(),
            palette: container
                .palette_entries()
                .iter()
                .map(|v| VarInt(id(*v)))
                .collect(),
            data: container.raw().iter().map(|v| *v as i64).collect(),
        }
    }
}

/// Port of LevelChunkSection#write, including the separate fluid counter.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelChunkSectionData {
    pub non_empty_block_count: u16,
    pub fluid_count: u16,
    pub states: PalettedContainerData,
    pub biomes: PalettedContainerData,
}
impl From<&ferraloom_chunk::LevelChunkSection> for LevelChunkSectionData {
    fn from(section: &ferraloom_chunk::LevelChunkSection) -> Self {
        Self {
            non_empty_block_count: section.counts().non_empty_blocks,
            fluid_count: section.counts().fluids,
            states: PalettedContainerData::from_container(section.states(), |v| i32::from(v.id())),
            biomes: PalettedContainerData::from_container(section.biomes(), |v| v.0 as i32),
        }
    }
}
impl Wire for LevelChunkSectionData {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            non_empty_block_count: r.u16()?,
            fluid_count: r.u16()?,
            states: PalettedContainerData::decode(r, 4096, 8, 4)?,
            biomes: PalettedContainerData::decode(r, 64, 3, 1)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u16(self.non_empty_block_count);
        w.u16(self.fluid_count);
        self.states.encode(w, 4096, 8)?;
        self.biomes.encode(w, 64, 3)
    }
}

/// Port of ClientboundLevelChunkPacketData.
#[derive(Clone, Debug, PartialEq)]
pub struct LevelChunkPacketData {
    pub heightmaps: Vec<Heightmap>,
    pub sections: Vec<LevelChunkSectionData>,
    pub block_entities: Vec<BlockEntityInfo>,
}
impl Wire for LevelChunkPacketData {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let heightmaps = r.read()?;
        let data = r.byte_array(2097152)?;
        let mut input = Reader::new(data);
        let mut sections = Vec::new();
        while !input.remaining().is_empty() {
            sections.push(input.read()?);
        }
        Ok(Self {
            heightmaps,
            sections,
            block_entities: r.read()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.heightmaps.encode(w)?;
        let mut body = Writer::new();
        for section in &self.sections {
            section.encode(&mut body)?;
        }
        w.byte_array(body.as_slice(), 2097152)?;
        self.block_entities.encode(w)
    }
}

packet!(/// Port of ClientboundLightUpdatePacketData.
LightUpdatePacketData {sky_y_mask:BitSet,block_y_mask:BitSet,empty_sky_y_mask:BitSet,empty_block_y_mask:BitSet,sky_updates:Vec<ByteArray<2048>>,block_updates:Vec<ByteArray<2048>>});
packet!(/// Port of ClientboundChunksBiomesPacket.ChunkBiomeData.
ChunkBiomeData {pos:i64,buffer:ByteArray<2097152>});
