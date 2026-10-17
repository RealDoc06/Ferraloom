//! Initial flat terrain, stored and serialized by Ferraloom itself.
//!
//! This establishes chunk storage and wire output. Generation, lighting updates,
//! persistence, entities and vanilla simulation are separate unfinished work.

use std::{collections::BTreeMap, io};

use crate::protocol::put_var_int;

pub const MIN_Y: i32 = -64;
pub const HEIGHT: usize = 384;

// Default state IDs from official 26.3 blocks.json, SHA256
// 7a0de8aaf7b04d00c40b8ffdc4cfe75184cff2e821d336a020573201867105a4.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BlockState(pub u32);
pub const AIR: BlockState = BlockState(0);
pub const STONE: BlockState = BlockState(1);
pub const GRASS: BlockState = BlockState(9);
pub const DIRT: BlockState = BlockState(10);
pub const BEDROCK: BlockState = BlockState(88);

pub struct Chunk {
    blocks: Vec<BlockState>,
}

impl Chunk {
    pub fn flat() -> Self {
        let mut blocks = Vec::with_capacity(16 * 16 * HEIGHT);
        for y in MIN_Y..MIN_Y + HEIGHT as i32 {
            let state = match y {
                -64 => BEDROCK,
                -63..=60 => STONE,
                61..=62 => DIRT,
                63 => GRASS,
                _ => AIR,
            };
            blocks.extend(std::iter::repeat_n(state, 256));
        }
        Self { blocks }
    }

    fn index(x: u8, y: i32, z: u8) -> Option<usize> {
        if x >= 16 || z >= 16 || !(MIN_Y..MIN_Y + HEIGHT as i32).contains(&y) {
            return None;
        }
        Some(((y - MIN_Y) as usize * 16 + usize::from(z)) * 16 + usize::from(x))
    }

    pub fn block(&self, x: u8, y: i32, z: u8) -> Option<BlockState> {
        Self::index(x, y, z).map(|index| self.blocks[index])
    }

    pub fn set_block(&mut self, x: u8, y: i32, z: u8, block: BlockState) -> io::Result<()> {
        let index = Self::index(x, y, z)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "block outside chunk"))?;
        self.blocks[index] = block;
        Ok(())
    }

    pub fn packet(&self, x: i32, z: i32, biome: i32) -> Vec<u8> {
        let mut sections = Vec::new();
        for section in self.blocks.as_chunks::<4096>().0 {
            let count = section.iter().filter(|&&state| state != AIR).count() as u16;
            sections.extend_from_slice(&count.to_be_bytes());
            sections.extend_from_slice(&0u16.to_be_bytes()); // No fluids in this generator.
            write_palette(&mut sections, section);
            sections.push(0); // Single biome palette, zero packed longs.
            put_var_int(&mut sections, biome);
        }
        let mut packet = Vec::new();
        packet.extend_from_slice(&x.to_be_bytes());
        packet.extend_from_slice(&z.to_be_bytes());
        // Heightmaps are a map of enum raw IDs to long arrays in protocol777.
        // Both maps use the first air position relative to the dimension floor.
        // The initial terrain has only solid non-fluid blocks.
        let mut surface = vec![0; 256];
        for z in 0..16u8 {
            for x in 0..16u8 {
                surface[usize::from(z) * 16 + usize::from(x)] = (MIN_Y..MIN_Y + HEIGHT as i32)
                    .rev()
                    .find(|&y| self.block(x, y, z) != Some(AIR))
                    .map_or(0, |y| (y + 1 - MIN_Y) as u32);
            }
        }
        put_var_int(&mut packet, 2);
        for kind in [1, 4] {
            put_var_int(&mut packet, kind);
            let heights = packed(&surface, 9);
            put_var_int(&mut packet, heights.len() as i32);
            for value in heights {
                packet.extend_from_slice(&value.to_be_bytes());
            }
        }
        put_var_int(&mut packet, sections.len() as i32);
        packet.extend_from_slice(&sections);
        put_var_int(&mut packet, 0); // Block entities.
        // Light covers24 terrain sections plus a lower and upper padding
        // section. Flat opaque terrain ends at63; full sky begins at64 (bit9).
        bitset(&mut packet, ((1u32 << 26) - 1) ^ ((1u32 << 9) - 1));
        bitset(&mut packet, 0);
        bitset(&mut packet, (1u32 << 9) - 1);
        bitset(&mut packet, (1u32 << 26) - 1);
        put_var_int(&mut packet, 17);
        for _ in 0..17 {
            put_var_int(&mut packet, 2048);
            packet.extend(std::iter::repeat_n(0xff, 2048));
        }
        put_var_int(&mut packet, 0); // No nonempty block-light arrays.
        packet
    }
}

fn bitset(output: &mut Vec<u8>, value: u32) {
    // This packet selects ByteBufCodecs.BIT_SET, a little-endian byte array.
    // FriendlyByteBuf.writeBitSet's long-array encoding is a different codec.
    let bytes = value.to_le_bytes();
    let count = bytes
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |index| index + 1);
    put_var_int(output, count as i32);
    output.extend_from_slice(&bytes[..count]);
}

fn write_palette(output: &mut Vec<u8>, blocks: &[BlockState]) {
    let mut palette = BTreeMap::new();
    for &block in blocks {
        let next = palette.len() as u32;
        palette.entry(block).or_insert(next);
    }
    if palette.len() == 1 {
        output.push(0);
        put_var_int(output, blocks[0].0 as i32);
        return;
    }
    let bits = (usize::BITS - (palette.len() - 1).leading_zeros()).max(4) as u8;
    let direct = bits > 8;
    let bits = if direct { 16 } else { bits };
    output.push(bits);
    if !direct {
        put_var_int(output, palette.len() as i32);
        let mut entries = vec![AIR; palette.len()];
        for (&state, &index) in &palette {
            entries[index as usize] = state;
        }
        for entry in entries {
            put_var_int(output, entry.0 as i32);
        }
    }
    let values: Vec<u32> = blocks
        .iter()
        .map(|state| if direct { state.0 } else { palette[state] })
        .collect();
    // Protocol777 fixes this count from palette bits/container size. No length
    // prefix belongs here, including for a zero-bit single-value palette.
    for value in packed(&values, bits) {
        output.extend_from_slice(&value.to_be_bytes());
    }
}

fn packed(values: &[u32], bits: u8) -> Vec<u64> {
    let per_long = 64 / usize::from(bits);
    values
        .chunks(per_long)
        .map(|entries| {
            entries.iter().enumerate().fold(0, |word, (index, value)| {
                word | (u64::from(*value) << (index * usize::from(bits)))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_coordinates_and_mutations_stay_in_the_chunk() {
        let mut chunk = Chunk::flat();
        for (y, state) in [
            (-64, BEDROCK),
            (-63, STONE),
            (60, STONE),
            (61, DIRT),
            (63, GRASS),
            (64, AIR),
            (319, AIR),
        ] {
            assert_eq!(chunk.block(0, y, 0), Some(state));
        }
        chunk.set_block(15, 63, 15, STONE).unwrap();
        assert_eq!(chunk.block(15, 63, 15), Some(STONE));
        assert_eq!(chunk.block(14, 63, 15), Some(GRASS));
        assert!(chunk.set_block(16, 63, 0, AIR).is_err());
        assert!(chunk.set_block(0, 320, 0, AIR).is_err());
    }

    #[test]
    fn palettes_have_fixed_packed_lengths_and_single_values_have_no_tail() {
        let mut bytes = Vec::new();
        write_palette(&mut bytes, &[STONE; 4096]);
        assert_eq!(bytes, [0, 1]);
        let mut mixed = [AIR; 4096];
        mixed[0] = STONE;
        write_palette(&mut bytes, &mixed);
        assert_eq!(bytes.len(), 2 + 1 + 1 + 2 + 256 * 8);
        assert_eq!(&bytes[2..6], &[4, 2, 1, 0]);
        assert_eq!(&bytes[6..14], &0x1111111111111110u64.to_be_bytes());
    }
}
