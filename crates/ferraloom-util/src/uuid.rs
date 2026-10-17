//! Port of `net.minecraft.core.UUIDUtil` and the UUID helpers used by Mth.
use crate::random::RandomSource;
use md5::{Digest, Md5};

/// Java UUID's two signed 64-bit words, in network order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Uuid {
    pub most: i64,
    pub least: i64,
}

impl Uuid {
    pub const NIL: Self = Self { most: 0, least: 0 };
    #[inline]
    pub const fn new(most: i64, least: i64) -> Self {
        Self { most, least }
    }
    #[inline]
    pub fn from_int_array(words: [i32; 4]) -> Self {
        Self::new(
            (i64::from(words[0]) << 32) | i64::from(words[1] as u32),
            (i64::from(words[2]) << 32) | i64::from(words[3] as u32),
        )
    }
    #[inline]
    pub fn to_int_array(self) -> [i32; 4] {
        [
            (self.most >> 32) as i32,
            self.most as i32,
            (self.least >> 32) as i32,
            self.least as i32,
        ]
    }
    #[inline]
    pub fn to_byte_array(self) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&self.most.to_be_bytes());
        bytes[8..].copy_from_slice(&self.least.to_be_bytes());
        bytes
    }
    #[inline]
    pub fn from_byte_array(bytes: [u8; 16]) -> Self {
        Self::new(
            i64::from_be_bytes(bytes[..8].try_into().unwrap()),
            i64::from_be_bytes(bytes[8..].try_into().unwrap()),
        )
    }
    /// Port of UUID#nameUUIDFromBytes (MD5, version 3).
    pub fn from_name(bytes: &[u8]) -> Self {
        let mut digest: [u8; 16] = Md5::digest(bytes).into();
        digest[6] = (digest[6] & 0x0f) | 0x30;
        digest[8] = (digest[8] & 0x3f) | 0x80;
        Self::from_byte_array(digest)
    }
    pub fn offline_player(name: &str) -> Self {
        Self::from_name(format!("OfflinePlayer:{name}").as_bytes())
    }
    /// Port of Mth#createInsecureUUID.
    pub fn insecure(random: &mut impl RandomSource) -> Self {
        Self::new(
            (random.next_long() & -61441) | 16384,
            (random.next_long() & 0x3fff_ffff_ffff_ffff) | i64::MIN,
        )
    }
}
impl std::fmt::Display for Uuid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = u128::from_be_bytes(self.to_byte_array());
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            n >> 96,
            (n >> 80) & 65535,
            (n >> 64) & 65535,
            (n >> 48) & 65535,
            n & 0xffff_ffff_ffff
        )
    }
}
