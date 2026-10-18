use crate::{CompoundTag, ListTag, NbtString, Tag};
use flate2::{
    read::{GzDecoder, ZlibDecoder},
    write::{GzEncoder, ZlibEncoder},
};
use std::{
    fmt,
    io::{self, Read, Write},
};

#[derive(Debug)]
pub enum NbtError {
    Io(io::Error),
    Format(String),
    Account(String),
    Utf(String),
}
impl fmt::Display for NbtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Format(s) | Self::Account(s) | Self::Utf(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for NbtError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(e) = self {
            Some(e)
        } else {
            None
        }
    }
}
impl From<io::Error> for NbtError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
type Result<T> = std::result::Result<T, NbtError>;

/// Port of `NbtAccounter`, accounting decoded allocations rather than compressed bytes.
#[derive(Clone, Debug)]
pub struct NbtAccounter {
    quota: i64,
    usage: i64,
    max_depth: usize,
    depth: usize,
}
impl Default for NbtAccounter {
    fn default() -> Self {
        Self::new(2_097_152, 512)
    }
}
impl NbtAccounter {
    pub const DEFAULT_NBT_QUOTA: i64 = 2_097_152;
    pub const UNCOMPRESSED_NBT_QUOTA: i64 = 104_857_600;
    pub const MAX_DEPTH: usize = 512;
    pub fn account_entries(&mut self, bytes_per_entry: i64, count: i64) -> Result<()> {
        self.account_bytes(bytes_per_entry.wrapping_mul(count))
    }

    pub fn new(quota: i64, max_depth: usize) -> Self {
        Self {
            quota,
            usage: 0,
            max_depth,
            depth: 0,
        }
    }
    pub fn unlimited() -> Self {
        Self::new(i64::MAX, 512)
    }
    pub fn uncompressed() -> Self {
        Self::new(104_857_600, 512)
    }
    pub fn usage(&self) -> i64 {
        self.usage
    }
    pub fn depth(&self) -> usize {
        self.depth
    }
    pub fn account_bytes(&mut self, size: i64) -> Result<()> {
        if size < 0 {
            return Err(NbtError::Account(format!(
                "Tried to account NBT tag with negative size: {size}"
            )));
        }
        if self.usage.wrapping_add(size) > self.quota {
            return Err(NbtError::Account(format!(
                "Tried to read NBT tag that was too big; tried to allocate: {} + {size} bytes where max allowed: {}",
                self.usage, self.quota
            )));
        }
        self.usage = self.usage.wrapping_add(size);
        Ok(())
    }
    pub fn push_depth(&mut self) -> Result<()> {
        if self.depth >= self.max_depth {
            return Err(NbtError::Account(format!(
                "Tried to read NBT tag with too high complexity, depth > {}",
                self.max_depth
            )));
        }
        self.depth += 1;
        Ok(())
    }
    pub fn pop_depth(&mut self) -> Result<()> {
        if self.depth == 0 {
            return Err(NbtError::Account(
                "NBT-Accounter tried to pop stack-depth at top-level".into(),
            ));
        }
        self.depth -= 1;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Compression {
    None,
    Gzip,
    Zlib,
}
#[derive(Clone, Debug)]
pub enum RootFormat {
    Named(NbtString),
    Network,
}

#[derive(Clone, Copy)]
enum FloatOutput {
    Canonical,
    Raw,
}

/// Port of `NbtIo`. File writes use vanilla's oversized-string fallback; network writes are strict.
pub struct NbtIo;
impl NbtIo {
    /// Names are retained when valid; vanilla skips their bytes, so malformed names are ignored.
    pub fn read(
        reader: impl Read,
        compression: Compression,
        named: bool,
        accounter: &mut NbtAccounter,
    ) -> Result<(NbtString, Tag)> {
        match compression {
            Compression::None => read_root(reader, named, accounter),
            Compression::Gzip => read_root(GzDecoder::new(reader), named, accounter),
            Compression::Zlib => read_root(ZlibDecoder::new(reader), named, accounter),
        }
    }
    pub fn write(
        writer: impl Write,
        compression: Compression,
        root: &RootFormat,
        tag: &Tag,
    ) -> Result<()> {
        match compression {
            Compression::None => write_root(
                writer,
                root,
                tag,
                matches!(root, RootFormat::Named(_)),
                FloatOutput::Canonical,
            ),
            Compression::Gzip => {
                let mut w = GzEncoder::new(writer, flate2::Compression::default());
                write_root(
                    &mut w,
                    root,
                    tag,
                    matches!(root, RootFormat::Named(_)),
                    FloatOutput::Canonical,
                )?;
                w.finish()?;
                Ok(())
            }
            Compression::Zlib => {
                let mut w = ZlibEncoder::new(writer, flate2::Compression::default());
                write_root(
                    &mut w,
                    root,
                    tag,
                    matches!(root, RootFormat::Named(_)),
                    FloatOutput::Canonical,
                )?;
                w.finish()?;
                Ok(())
            }
        }
    }
    pub fn read_compound(
        reader: impl Read,
        compression: Compression,
        accounter: &mut NbtAccounter,
    ) -> Result<CompoundTag> {
        match Self::read(reader, compression, true, accounter)?.1 {
            Tag::Compound(v) => Ok(v),
            _ => Err(NbtError::Format(
                "Root tag must be a named compound tag".into(),
            )),
        }
    }
    pub fn write_compound(
        writer: impl Write,
        compression: Compression,
        tag: &CompoundTag,
    ) -> Result<()> {
        Self::write(
            writer,
            compression,
            &RootFormat::Named(NbtString::default()),
            &Tag::Compound(tag.clone()),
        )
    }
    pub fn read_any_tag(reader: impl Read, accounter: &mut NbtAccounter) -> Result<Tag> {
        Ok(Self::read(reader, Compression::None, false, accounter)?.1)
    }
    pub fn write_any_tag(writer: impl Write, tag: &Tag) -> Result<()> {
        Self::write(writer, Compression::None, &RootFormat::Network, tag)
    }
    /// Port of FriendlyByteBuf#writeNbt through ByteBufOutputStream, preserving raw NaN bits.
    pub fn write_network(writer: impl Write, tag: &Tag) -> Result<()> {
        write_root(writer, &RootFormat::Network, tag, false, FloatOutput::Raw)
    }
    /// Port of NbtIo#writeUnnamedTag, with strict DataOutput.writeUTF failure behavior.
    pub fn write_unnamed_tag(writer: impl Write, tag: &Tag) -> Result<()> {
        write_root(
            writer,
            &RootFormat::Named(NbtString::default()),
            tag,
            false,
            FloatOutput::Canonical,
        )
    }
    /// Port of NbtIo#writeUnnamedTagWithFallback, used by file writes.
    pub fn write_unnamed_tag_with_fallback(writer: impl Write, tag: &Tag) -> Result<()> {
        write_root(
            writer,
            &RootFormat::Named(NbtString::default()),
            tag,
            true,
            FloatOutput::Canonical,
        )
    }
    pub fn read_utf(reader: &mut impl Read) -> Result<NbtString> {
        read_utf(reader)
    }
    pub fn write_utf(writer: &mut impl Write, value: &NbtString) -> Result<()> {
        write_utf(writer, value, false)
    }
}
fn read_root(mut r: impl Read, named: bool, a: &mut NbtAccounter) -> Result<(NbtString, Tag)> {
    let id = read_num::<1>(&mut r)?[0];
    let name = if named && id != 0 {
        match read_utf(&mut r) {
            Ok(name) => name,
            Err(NbtError::Utf(_)) => NbtString::default(),
            Err(e) => return Err(e),
        }
    } else {
        NbtString::default()
    };
    let tag = if id == 0 {
        Tag::End
    } else {
        read_payload(&mut r, id, a)?
    };
    Ok((name, tag))
}
fn write_root(
    mut w: impl Write,
    root: &RootFormat,
    tag: &Tag,
    fallback: bool,
    floats: FloatOutput,
) -> Result<()> {
    w.write_all(&[tag.id()])?;
    if tag.id() == 0 {
        return Ok(());
    }
    if let RootFormat::Named(name) = root {
        write_utf(&mut w, name, fallback)?;
    }
    write_payload(&mut w, tag, fallback, floats)
}
fn read_num<const N: usize>(r: &mut impl Read) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    r.read_exact(&mut bytes)?;
    Ok(bytes)
}
fn read_i32(r: &mut impl Read) -> Result<i32> {
    Ok(i32::from_be_bytes(read_num(r)?))
}
fn read_utf(r: &mut impl Read) -> Result<NbtString> {
    let len = usize::from(u16::from_be_bytes(read_num(r)?));
    let mut bytes = vec![0; len];
    r.read_exact(&mut bytes)?;
    let mut out = Vec::new();
    let mut i = 0;
    while i < len {
        let c = bytes[i];
        match c >> 4 {
            0..=7 => {
                out.push(u16::from(c));
                i += 1;
            }
            12 | 13 => {
                i += 2;
                if i > len {
                    return Err(NbtError::Utf(
                        "malformed input: partial character at end".into(),
                    ));
                }
                let d = bytes[i - 1];
                if d & 0xc0 != 0x80 {
                    return Err(NbtError::Utf(format!("malformed input around byte {i}")));
                }
                out.push((u16::from(c & 31) << 6) | u16::from(d & 63));
            }
            14 => {
                i += 3;
                if i > len {
                    return Err(NbtError::Utf(
                        "malformed input: partial character at end".into(),
                    ));
                }
                let d = bytes[i - 2];
                let e = bytes[i - 1];
                if d & 0xc0 != 0x80 || e & 0xc0 != 0x80 {
                    return Err(NbtError::Utf(format!(
                        "malformed input around byte {}",
                        i - 1
                    )));
                }
                out.push((u16::from(c & 15) << 12) | (u16::from(d & 63) << 6) | u16::from(e & 63));
            }
            _ => return Err(NbtError::Utf(format!("malformed input around byte {i}"))),
        }
    }
    Ok(NbtString::from_utf16(out))
}
fn write_utf(w: &mut impl Write, value: &NbtString, fallback: bool) -> Result<()> {
    let len: usize = value
        .as_utf16()
        .iter()
        .map(|c| {
            if (1..=127).contains(c) {
                1
            } else if *c <= 2047 {
                2
            } else {
                3
            }
        })
        .sum();
    if len > 65535 {
        if fallback {
            w.write_all(&[0, 0])?;
            return Ok(());
        }
        return Err(NbtError::Utf(format!(
            "encoded string ({}...{}) too long: {len} bytes",
            NbtString::from_utf16(value.as_utf16()[..8].to_vec()),
            NbtString::from_utf16(value.as_utf16()[value.len() - 8..].to_vec())
        )));
    }
    w.write_all(&(len as u16).to_be_bytes())?;
    for &c in value.as_utf16() {
        if (1..=127).contains(&c) {
            w.write_all(&[c as u8])?;
        } else if c <= 2047 {
            w.write_all(&[(0xc0 | (c >> 6)) as u8, (0x80 | (c & 63)) as u8])?;
        } else {
            w.write_all(&[
                (0xe0 | (c >> 12)) as u8,
                (0x80 | ((c >> 6) & 63)) as u8,
                (0x80 | (c & 63)) as u8,
            ])?;
        }
    }
    Ok(())
}
fn read_payload(r: &mut impl Read, id: u8, a: &mut NbtAccounter) -> Result<Tag> {
    if id == 9 || id == 10 {
        a.push_depth()?;
        let result = read_container(r, id, a);
        a.pop_depth()?;
        return result;
    }
    a.account_bytes(match id {
        0 => 8,
        1 => 9,
        2 => 10,
        3 | 5 => 12,
        4 | 6 => 16,
        7 | 11 | 12 => 24,
        8 => 36,
        _ => return Err(NbtError::Format(format!("Invalid tag id: {}", id as i8))),
    })?;
    Ok(match id {
        0 => Tag::End,
        1 => Tag::Byte(read_num::<1>(r)?[0] as i8),
        2 => Tag::Short(i16::from_be_bytes(read_num(r)?)),
        3 => Tag::Int(read_i32(r)?),
        4 => Tag::Long(i64::from_be_bytes(read_num(r)?)),
        5 => {
            let n = f32::from_be_bytes(read_num(r)?);
            Tag::Float(if n == 0.0 { 0.0 } else { n })
        }
        6 => {
            let n = f64::from_be_bytes(read_num(r)?);
            Tag::Double(if n == 0.0 { 0.0 } else { n })
        }
        8 => {
            let s = read_utf(r)?;
            a.account_bytes(2 * s.len() as i64)?;
            Tag::String(s)
        }
        7 | 11 | 12 => {
            let count = read_i32(r)?;
            let width = match id {
                7 => 1,
                11 => 4,
                _ => 8,
            };
            a.account_bytes(i64::from(count) * width)?;
            match id {
                7 => {
                    let mut v = Vec::new();
                    for _ in 0..count {
                        v.push(read_num::<1>(r)?[0] as i8)
                    }
                    Tag::ByteArray(v)
                }
                11 => {
                    let mut v = Vec::new();
                    for _ in 0..count {
                        v.push(read_i32(r)?)
                    }
                    Tag::IntArray(v)
                }
                _ => {
                    let mut v = Vec::new();
                    for _ in 0..count {
                        v.push(i64::from_be_bytes(read_num(r)?))
                    }
                    Tag::LongArray(v)
                }
            }
        }
        _ => unreachable!(),
    })
}
fn read_container(r: &mut impl Read, id: u8, a: &mut NbtAccounter) -> Result<Tag> {
    if id == 9 {
        a.account_bytes(36)?;
        let element = read_num::<1>(r)?[0];
        let count = read_i32(r)?;
        if count < 0 {
            return Err(NbtError::Format(format!(
                "ListTag length cannot be negative: {count}"
            )));
        }
        if element == 0 && count > 0 {
            return Err(NbtError::Format("Missing type on ListTag".into()));
        }
        a.account_bytes(4 * i64::from(count))?;
        let mut list = ListTag::default();
        for _ in 0..count {
            let tag = read_payload(r, element, a)?;
            list.push(if let Tag::Compound(mut c) = tag {
                if c.len() == 1 && c.contains_key("") {
                    c.remove("").unwrap()
                } else {
                    Tag::Compound(c)
                }
            } else {
                tag
            });
        }
        Ok(Tag::List(list))
    } else {
        a.account_bytes(48)?;
        let mut c = CompoundTag::new();
        loop {
            let id = read_num::<1>(r)?[0];
            if id == 0 {
                break;
            }
            let key = read_utf(r)?;
            a.account_bytes(28)?;
            a.account_bytes(2 * key.len() as i64)?;
            let value = read_payload(r, id, a)?;
            if c.insert(key, value).is_none() {
                a.account_bytes(36)?;
            }
        }
        Ok(Tag::Compound(c))
    }
}
fn write_len(w: &mut impl Write, len: usize) -> Result<()> {
    let len = i32::try_from(len)
        .map_err(|_| NbtError::Format("NBT collection exceeds i32::MAX elements".into()))?;
    w.write_all(&len.to_be_bytes())?;
    Ok(())
}
fn write_payload(w: &mut impl Write, tag: &Tag, fallback: bool, floats: FloatOutput) -> Result<()> {
    match tag {
        Tag::End => {}
        Tag::Byte(n) => w.write_all(&n.to_be_bytes())?,
        Tag::Short(n) => w.write_all(&n.to_be_bytes())?,
        Tag::Int(n) => w.write_all(&n.to_be_bytes())?,
        Tag::Long(n) => w.write_all(&n.to_be_bytes())?,
        Tag::Float(n) => {
            w.write_all(&if matches!(floats, FloatOutput::Canonical) && n.is_nan() {
                f32::NAN.to_be_bytes()
            } else {
                n.to_be_bytes()
            })?
        }
        Tag::Double(n) => {
            w.write_all(&if matches!(floats, FloatOutput::Canonical) && n.is_nan() {
                f64::NAN.to_be_bytes()
            } else {
                n.to_be_bytes()
            })?
        }
        Tag::String(s) => write_utf(w, s, fallback)?,
        Tag::ByteArray(v) => {
            write_len(w, v.len())?;
            for n in v {
                w.write_all(&n.to_be_bytes())?;
            }
        }
        Tag::IntArray(v) => {
            write_len(w, v.len())?;
            for n in v {
                w.write_all(&n.to_be_bytes())?;
            }
        }
        Tag::LongArray(v) => {
            write_len(w, v.len())?;
            for n in v {
                w.write_all(&n.to_be_bytes())?;
            }
        }
        Tag::Compound(c) => {
            for key in c.java_keys() {
                let value = c.get_key(key).unwrap();
                w.write_all(&[value.id()])?;
                if value.id() != 0 {
                    write_utf(w, key, fallback)?;
                    write_payload(w, value, fallback, floats)?;
                }
            }
            w.write_all(&[0])?;
        }
        Tag::List(v) => {
            let mut id = 0;
            for value in v.iter() {
                if id == 0 {
                    id = value.id()
                } else if id != value.id() {
                    id = 10;
                    break;
                }
            }
            w.write_all(&[id])?;
            write_len(w, v.len())?;
            for value in v.iter() {
                let wrap = id == 10
                    && !matches!(value,Tag::Compound(c) if !(c.len()==1&&c.contains_key("")));
                if wrap {
                    w.write_all(&[value.id(), 0, 0])?;
                    write_payload(w, value, fallback, floats)?;
                    w.write_all(&[0])?;
                } else {
                    write_payload(w, value, fallback, floats)?;
                }
            }
        }
    }
    Ok(())
}
