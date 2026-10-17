//! Bounded, uncompressed Minecraft packet framing and primitive codecs.

use std::io::{self, ErrorKind};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

// A frame's length prefix is limited to three bytes by the Minecraft transport.
pub const MAX_FRAME_BYTES: usize = (1 << 21) - 1;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message)
}

#[derive(Debug)]
pub struct Packet {
    pub id: i32,
    pub payload: Vec<u8>,
}

/// Read exactly one frame. A clean EOF is distinct from a truncated prefix/body.
pub async fn read_packet(input: &mut (impl AsyncRead + Unpin)) -> io::Result<Option<Packet>> {
    let mut length = 0usize;
    for index in 0..3 {
        let mut byte = [0u8];
        if input.read(&mut byte).await? == 0 {
            return if index == 0 {
                Ok(None)
            } else {
                Err(io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "truncated frame length",
                ))
            };
        }
        length |= usize::from(byte[0] & 0x7f) << (index * 7);
        if byte[0] & 0x80 == 0 {
            if length == 0 || length > MAX_FRAME_BYTES {
                return Err(invalid("invalid frame length"));
            }
            let mut body = vec![0; length];
            input.read_exact(&mut body).await?;
            let mut reader = Reader::new(&body);
            let id = reader.var_int()?;
            if id < 0 {
                return Err(invalid("negative packet id"));
            }
            return Ok(Some(Packet {
                id,
                payload: reader.remaining().to_vec(),
            }));
        }
    }
    Err(invalid("frame length exceeds three bytes"))
}

pub async fn write_packet(
    output: &mut (impl AsyncWrite + Unpin),
    id: i32,
    payload: &[u8],
) -> io::Result<()> {
    if id < 0 {
        return Err(invalid("negative packet id"));
    }
    let mut body = Vec::with_capacity(payload.len().saturating_add(5));
    put_var_int(&mut body, id);
    body.extend_from_slice(payload);
    if body.len() > MAX_FRAME_BYTES {
        return Err(invalid("outgoing packet exceeds frame limit"));
    }
    let mut prefix = Vec::with_capacity(3);
    put_var_int(&mut prefix, body.len() as i32);
    output.write_all(&prefix).await?;
    output.write_all(&body).await?;
    output.flush().await
}

pub fn put_var_int(output: &mut Vec<u8>, value: i32) {
    let mut remaining = value as u32;
    loop {
        let byte = (remaining & 0x7f) as u8;
        remaining >>= 7;
        output.push(byte | if remaining == 0 { 0 } else { 0x80 });
        if remaining == 0 {
            break;
        }
    }
}

pub fn put_string(output: &mut Vec<u8>, value: &str) -> io::Result<()> {
    let length = i32::try_from(value.len()).map_err(|_| invalid("string too long"))?;
    put_var_int(output, length);
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

pub struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }
    pub fn remaining(&self) -> &'a [u8] {
        self.remaining
    }

    pub fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        if count > self.remaining.len() {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "truncated packet field",
            ));
        }
        let (value, rest) = self.remaining.split_at(count);
        self.remaining = rest;
        Ok(value)
    }

    pub fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn var_int(&mut self) -> io::Result<i32> {
        let mut value = 0u32;
        for index in 0..5 {
            let byte = self.byte()?;
            if index == 4 && byte > 0x0f {
                return Err(invalid("VarInt exceeds 32 bits"));
            }
            value |= u32::from(byte & 0x7f) << (index * 7);
            if byte & 0x80 == 0 {
                return Ok(value as i32);
            }
        }
        Err(invalid("VarInt exceeds five bytes"))
    }

    pub fn string(&mut self, max_utf16_units: usize) -> io::Result<&'a str> {
        let bytes =
            usize::try_from(self.var_int()?).map_err(|_| invalid("negative string length"))?;
        if bytes > max_utf16_units.saturating_mul(3) {
            return Err(invalid("encoded string exceeds limit"));
        }
        let value = std::str::from_utf8(self.take(bytes)?).map_err(|_| invalid("invalid UTF-8"))?;
        if value.encode_utf16().count() > max_utf16_units {
            return Err(invalid("decoded string exceeds limit"));
        }
        Ok(value)
    }

    pub fn unsigned_short(&mut self) -> io::Result<u16> {
        let bytes: [u8; 2] = self.take(2)?.try_into().expect("two bytes were read");
        Ok(u16::from_be_bytes(bytes))
    }

    pub fn finish(self) -> io::Result<()> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(invalid("trailing packet bytes"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_varints_and_exact_field_boundaries() {
        for value in [0, 1, 127, 128, 255, i32::MAX, -1, i32::MIN] {
            let mut bytes = Vec::new();
            put_var_int(&mut bytes, value);
            bytes.push(42);
            let mut reader = Reader::new(&bytes);
            assert_eq!(reader.var_int().unwrap(), value);
            assert_eq!(reader.byte().unwrap(), 42);
            reader.finish().unwrap();
        }
        assert!(Reader::new(&[255, 255, 255, 255, 16]).var_int().is_err());
        assert!(Reader::new(&[128]).var_int().is_err());
    }

    #[test]
    fn strings_enforce_utf8_and_java_utf16_bounds() {
        let mut bytes = Vec::new();
        put_string(&mut bytes, "a😀").unwrap();
        assert_eq!(Reader::new(&bytes).string(3).unwrap(), "a😀");
        assert!(Reader::new(&bytes).string(2).is_err());
        assert!(Reader::new(&[1, 255]).string(16).is_err());
        assert!(Reader::new(&[255, 255, 255, 255, 15]).string(16).is_err());
    }

    #[tokio::test]
    async fn frames_survive_fragmentation_and_coalescing() {
        let (mut writer, mut reader) = tokio::io::duplex(8);
        let send = tokio::spawn(async move {
            write_packet(&mut writer, 128, &[7; 300]).await.unwrap();
            write_packet(&mut writer, 0, &[]).await.unwrap();
        });
        let packet = read_packet(&mut reader).await.unwrap().unwrap();
        assert_eq!(packet.id, 128);
        assert_eq!(packet.payload, vec![7; 300]);
        assert_eq!(read_packet(&mut reader).await.unwrap().unwrap().id, 0);
        assert!(read_packet(&mut reader).await.unwrap().is_none());
        send.await.unwrap();
    }

    #[tokio::test]
    async fn invalid_or_truncated_frames_are_refused() {
        for bytes in [
            vec![0],
            vec![128],
            vec![128, 128, 128],
            vec![3, 0, 1],
            vec![1, 128],
            vec![5, 255, 255, 255, 255, 15],
        ] {
            assert!(
                read_packet(&mut bytes.as_slice()).await.is_err(),
                "{bytes:?}"
            );
        }
    }
}
