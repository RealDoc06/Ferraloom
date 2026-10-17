use crate::{Error, Result};
use std::io::{Read, Write};

/// Port of RegionFileVersion. Custom (127) is recognized but unsupported by vanilla.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum RegionFileVersion {
    Gzip = 1,
    #[default]
    Deflate = 2,
    None = 3,
    Lz4 = 4,
    Custom = 127,
}
impl RegionFileVersion {
    pub fn from_id(id: u8) -> Option<Self> {
        Some(match id {
            1 => Self::Gzip,
            2 => Self::Deflate,
            3 => Self::None,
            4 => Self::Lz4,
            127 => Self::Custom,
            _ => return None,
        })
    }
    /// Port of RegionFileVersion#configure: gzip is readable, but not a server.properties option.
    pub fn from_option(name: &str) -> Option<Self> {
        match name {
            "deflate" => Some(Self::Deflate),
            "none" => Some(Self::None),
            "lz4" => Some(Self::Lz4),
            _ => None,
        }
    }
    pub fn compress(self, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        match self {
            Self::Gzip => {
                let mut w = flate2::write::GzEncoder::new(output, flate2::Compression::default());
                w.write_all(bytes)?;
                output = w.finish()?;
            }
            Self::Deflate => {
                let mut w = flate2::write::ZlibEncoder::new(output, flate2::Compression::default());
                w.write_all(bytes)?;
                output = w.finish()?;
            }
            Self::None => output.extend_from_slice(bytes),
            Self::Lz4 => return Ok(lz4_encode(bytes)),
            Self::Custom => {
                return Err(Error::Invalid(
                    "vanilla does not implement custom region compression".into(),
                ));
            }
        }
        Ok(output)
    }
    pub fn decompress(self, bytes: &[u8]) -> Result<Vec<u8>> {
        let mut output = Vec::new();
        match self {
            Self::Gzip => {
                flate2::read::GzDecoder::new(bytes).read_to_end(&mut output)?;
            }
            Self::Deflate => {
                flate2::read::ZlibDecoder::new(bytes).read_to_end(&mut output)?;
            }
            Self::None => output.extend_from_slice(bytes),
            Self::Lz4 => return lz4_decode(bytes),
            Self::Custom => {
                return Err(Error::Invalid(
                    "unrecognized custom region compression".into(),
                ));
            }
        }
        Ok(output)
    }
}

// lz4-java uses LZ4Block streams, not the standard LZ4 frame format.
const MAGIC: &[u8; 8] = b"LZ4Block";
const SEED: u32 = 0x9747_b28c;
// StreamingXXHash32#asChecksum masks to 28 bits; Java LZ4Block streams retain that quirk.
fn checksum(bytes: &[u8]) -> u32 {
    twox_hash::XxHash32::oneshot(SEED, bytes) & 0x0fff_ffff
}
fn lz4_encode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for block in bytes.chunks(65536) {
        let compressed = lz4_flex::block::compress(block);
        let (method, payload) = if compressed.len() < block.len() {
            (0x20, compressed.as_slice())
        } else {
            (0x10, block)
        };
        out.extend_from_slice(MAGIC);
        out.push(method | 6);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(block.len() as u32).to_le_bytes());
        out.extend_from_slice(&checksum(block).to_le_bytes());
        out.extend_from_slice(payload);
    }
    out.extend_from_slice(MAGIC);
    out.push(0x16);
    out.extend_from_slice(&[0; 12]);
    out
}
fn lz4_decode(mut bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        if bytes.len() < 21 || &bytes[..8] != MAGIC {
            return Err(Error::Invalid("corrupt LZ4Block header".into()));
        }
        let token = bytes[8];
        let method = token & 0xf0;
        let number = |i| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        let compressed = number(9) as usize;
        let original = number(13) as usize;
        let hash = number(17);
        bytes = &bytes[21..];
        if !matches!(method, 0x10 | 0x20)
            || original > (1usize << ((token & 15) + 10))
            || (compressed == 0) != (original == 0)
            || (method == 0x10 && compressed != original)
            || compressed > original
        {
            return Err(Error::Invalid("invalid LZ4Block lengths or method".into()));
        }
        if original == 0 {
            if hash != 0 {
                return Err(Error::Invalid("invalid LZ4Block terminator".into()));
            }
            return Ok(out);
        }
        if compressed > bytes.len() {
            return Err(Error::Invalid("truncated LZ4Block".into()));
        }
        let payload = &bytes[..compressed];
        bytes = &bytes[compressed..];
        let block = if method == 0x10 {
            payload.to_vec()
        } else {
            lz4_flex::block::decompress(payload, original)
                .map_err(|e| Error::Invalid(e.to_string()))?
        };
        if block.len() != original || checksum(&block) != hash {
            return Err(Error::Invalid("LZ4Block checksum mismatch".into()));
        }
        out.extend_from_slice(&block);
    }
}

#[cfg(test)]
pub(crate) fn vector_payload() -> Vec<u8> {
    let mut random = 12345u32;
    (0..140000)
        .map(|i| {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            if i < 65536 {
                (i % 71) as u8
            } else {
                random as u8
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_compressions_and_lz4_checksum() {
        let bytes: Vec<u8> = (0..180000).map(|i| (i % 71) as u8).collect();
        for version in [
            RegionFileVersion::Gzip,
            RegionFileVersion::Deflate,
            RegionFileVersion::None,
            RegionFileVersion::Lz4,
        ] {
            let packed = version.compress(&bytes).unwrap();
            assert_eq!(version.decompress(&packed).unwrap(), bytes);
            assert_eq!(
                version.decompress(&version.compress(&[]).unwrap()).unwrap(),
                Vec::<u8>::new()
            );
        }
        let mut packed = lz4_encode(&bytes);
        packed[17] ^= 1;
        assert!(lz4_decode(&packed).is_err());
        assert!(RegionFileVersion::Custom.compress(&bytes).is_err());
    }
}
