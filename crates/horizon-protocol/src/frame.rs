//! Packet framing (Varint21FrameDecoder / Varint21LengthFieldPrepender) and
//! zlib compression above a threshold (CompressionEncoder / CompressionDecoder).
use crate::{
    Error, Result,
    buf::{Reader, varint_size, write_varint},
};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use std::io::{Read, Write};

/// Frames are at most three VarInt bytes long (21 bits).
pub const MAX_FRAME: usize = (1 << 21) - 1;
/// CompressionDecoder.MAXIMUM_UNCOMPRESSED_LENGTH.
pub const MAX_UNCOMPRESSED: usize = 8_388_608;

/// Build the frame for one packet body (`id` + payload already concatenated):
/// length prefix, and with compression enabled the uncompressed-length
/// field (0 when below the threshold).
pub fn encode(body: &[u8], threshold: Option<usize>) -> Result<Vec<u8>> {
    Ok(prefixed(&contents(body, threshold)?))
}

/// What [`encode`] puts after the length prefix: the same for every
/// connection with the same threshold.
pub fn contents(body: &[u8], threshold: Option<usize>) -> Result<Vec<u8>> {
    let inner = match threshold {
        None => body.to_vec(),
        Some(t) if body.len() < t => {
            let mut v = Vec::with_capacity(body.len() + 1);
            v.push(0);
            v.extend_from_slice(body);
            v
        }
        Some(_) => {
            let mut v = Vec::with_capacity(body.len() / 2 + 8);
            write_varint(&mut v, body.len() as i32);
            compress_into(v, body)?
        }
    };
    if inner.len() > MAX_FRAME {
        return Err(Error::new(format!(
            "packet too large: {} bytes",
            inner.len()
        )));
    }
    Ok(inner)
}

/// Idle zlib encoders, reused across packets: setting up a fresh one (its
/// window and hash tables) cost about as much as compressing a small packet.
/// A reset encoder starts from the same state as a new one, so the bytes are
/// the same. Bounded by how many threads compress at once.
static ENCODERS: std::sync::Mutex<Vec<ZlibEncoder<Vec<u8>>>> = std::sync::Mutex::new(Vec::new());

/// Append the zlib stream of `body` to `out`.
fn compress_into(out: Vec<u8>, body: &[u8]) -> Result<Vec<u8>> {
    let idle = || {
        ENCODERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    };
    let pooled = idle().pop();
    let mut z = match pooled {
        Some(mut z) => {
            z.reset(out)?;
            z
        }
        None => ZlibEncoder::new(out, Compression::default()),
    };
    z.write_all(body)?;
    z.try_finish()?;
    let compressed = z.reset(Vec::new())?;
    idle().push(z);
    Ok(compressed)
}

/// A frame: the length prefix, then the [`contents`].
pub fn prefixed(contents: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(contents.len() + 3);
    prefix_to(&mut out, contents);
    out
}

/// Append a frame to `out`.
pub fn prefix_to(out: &mut Vec<u8>, contents: &[u8]) {
    write_varint(out, contents.len() as i32);
    out.extend_from_slice(contents);
}

/// Split one complete frame off the front of `buffer`, if available.
/// Returns the frame contents (without the length prefix).
pub fn split(buffer: &mut Vec<u8>) -> Result<Option<Vec<u8>>> {
    let mut len = 0usize;
    for i in 0..3 {
        let Some(&b) = buffer.get(i) else {
            return Ok(None);
        };
        len |= usize::from(b & 0x7F) << (7 * i);
        if b & 0x80 == 0 {
            let start = i + 1;
            if buffer.len() < start + len {
                return Ok(None);
            }
            let frame = buffer[start..start + len].to_vec();
            buffer.drain(..start + len);
            return Ok(Some(frame));
        }
    }
    Err(Error::new("length wider than 21 bits"))
}

/// Undo compression on a frame, giving the packet body (id + payload).
pub fn decompress(frame: Vec<u8>, threshold: Option<usize>) -> Result<Vec<u8>> {
    let Some(threshold) = threshold else {
        return Ok(frame);
    };
    let mut r = Reader::new(&frame);
    let size = r.varint()?;
    if size == 0 {
        return Ok(frame[1..].to_vec());
    }
    let size = usize::try_from(size).map_err(|_| Error::new("negative uncompressed length"))?;
    if size < threshold {
        return Err(Error::new(format!(
            "Badly compressed packet - size of {size} is below server threshold of {threshold}"
        )));
    }
    if size > MAX_UNCOMPRESSED {
        return Err(Error::new(format!(
            "Badly compressed packet - size of {size} is larger than protocol maximum"
        )));
    }
    let start = varint_size(size as i32);
    let mut out = Vec::with_capacity(size);
    ZlibDecoder::new(&frame[start..])
        .take(size as u64 + 1)
        .read_to_end(&mut out)?;
    if out.len() != size {
        return Err(Error::new(format!(
            "Badly compressed packet - actual length {} does not match declared size {size}",
            out.len()
        )));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncompressed_frames_round_trip() {
        let body = vec![0x00, 1, 2, 3];
        let framed = encode(&body, None).unwrap();
        assert_eq!(framed, [4, 0, 1, 2, 3]);
        let mut buffer = framed.clone();
        buffer.extend_from_slice(&framed[..2]);
        assert_eq!(split(&mut buffer).unwrap(), Some(body));
        assert_eq!(split(&mut buffer).unwrap(), None);
        assert_eq!(buffer, [4, 0]);
    }

    #[test]
    fn compression_threshold_like_vanilla() {
        // Below the threshold: a zero data length, then the raw body.
        let small = vec![7u8; 10];
        let framed = encode(&small, Some(256)).unwrap();
        assert_eq!(&framed[..2], &[11, 0]);
        let mut buffer = framed;
        let frame = split(&mut buffer).unwrap().unwrap();
        assert_eq!(decompress(frame, Some(256)).unwrap(), small);
        // At or above: the uncompressed size and a zlib stream.
        let big: Vec<u8> = (0..5000u32).map(|i| (i % 7) as u8).collect();
        let framed = encode(&big, Some(256)).unwrap();
        assert!(framed.len() < big.len());
        let mut buffer = framed;
        let frame = split(&mut buffer).unwrap().unwrap();
        assert_eq!(Reader::new(&frame).varint().unwrap(), 5000);
        assert_eq!(decompress(frame, Some(256)).unwrap(), big);
    }

    #[test]
    fn reused_encoders_compress_like_new_ones() {
        let bodies: Vec<Vec<u8>> = (0..4u32)
            .map(|n| {
                (0..3000 + n * 997)
                    .map(|i| (i * (n + 3) % 251) as u8)
                    .collect()
            })
            .collect();
        for body in bodies.iter().chain(bodies.iter()) {
            let mut fresh = Vec::new();
            write_varint(&mut fresh, body.len() as i32);
            let mut z = ZlibEncoder::new(fresh, Compression::default());
            z.write_all(body).unwrap();
            assert_eq!(contents(body, Some(256)).unwrap(), z.finish().unwrap());
        }
    }

    #[test]
    fn rejects_bad_compressed_lengths() {
        // Declared size below the threshold is a protocol error.
        let mut frame = vec![10];
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(&[1; 10]).unwrap();
        frame.extend(z.finish().unwrap());
        assert!(decompress(frame, Some(256)).is_err());
        // Declared size that does not match the stream.
        let mut frame = Vec::new();
        write_varint(&mut frame, 300);
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(&[1; 299]).unwrap();
        frame.extend(z.finish().unwrap());
        assert!(decompress(frame, Some(256)).is_err());
    }
}
