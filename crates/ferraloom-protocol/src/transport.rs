//! Port of Varint21FrameDecoder, CompressionEncoder/Decoder, CipherEncoder/Decoder
//! and Crypt. These types do not own sockets or impose an async runtime.
use crate::{Error, Reader, Result, Writer};
use aes::cipher::KeyIvInit;
use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress};
use rand::rngs::OsRng;
use rsa::{
    Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey,
    pkcs8::{DecodePrivateKey, EncodePublicKey},
};
use sha1::{Digest, Sha1};
use std::borrow::Cow;

pub const MAX_FRAME_LENGTH: usize = (1 << 21) - 1;
pub const MAX_UNCOMPRESSED_LENGTH: usize = 8_388_608;

/// Port of Varint21FrameDecoder, including incremental input and zero-frame rejection.
#[derive(Default)]
pub struct FrameDecoder {
    pending: Vec<u8>,
    start: usize,
}
impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }
    /// Append an arbitrary transport chunk. Call next_frame until it returns None.
    pub fn push(&mut self, bytes: &[u8]) {
        if self.start != 0 {
            self.pending.drain(..self.start);
            self.start = 0;
        }
        self.pending.extend_from_slice(bytes);
    }
    pub fn buffered_len(&self) -> usize {
        self.pending.len() - self.start
    }
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>> {
        let bytes = &self.pending[self.start..];
        let Some((header, length)) = frame_header(bytes)? else {
            return Ok(None);
        };
        if bytes.len() - header < length {
            return Ok(None);
        }
        let value = bytes[header..header + length].to_vec();
        self.start += header + length;
        if self.start == self.pending.len() {
            self.pending.clear();
            self.start = 0;
        }
        Ok(Some(value))
    }
    /// Borrow a single complete frame. On incomplete input no bytes are consumed.
    pub fn decode_slice(bytes: &[u8]) -> Result<Option<(&[u8], usize)>> {
        let Some((header, length)) = frame_header(bytes)? else {
            return Ok(None);
        };
        if bytes.len() - header < length {
            return Ok(None);
        }
        Ok(Some((&bytes[header..header + length], header + length)))
    }
}
fn frame_header(bytes: &[u8]) -> Result<Option<(usize, usize)>> {
    let mut length = 0;
    for index in 0..3 {
        let Some(byte) = bytes.get(index) else {
            return Ok(None);
        };
        length |= usize::from(byte & 127) << (index * 7);
        if byte & 128 == 0 {
            if length == 0 {
                return Err(Error::invalid("frame", "Frame length cannot be zero"));
            }
            return Ok(Some((index + 1, length)));
        }
    }
    Err(Error::invalid("frame", "length wider than 21-bit"))
}
/// Port of Varint21LengthFieldPrepender.
pub struct FrameEncoder;
impl FrameEncoder {
    pub fn encode(packet: &[u8]) -> Result<Vec<u8>> {
        if packet.len() > MAX_FRAME_LENGTH {
            return Err(Error::Length {
                length: packet.len() as i64,
                max: MAX_FRAME_LENGTH,
            });
        }
        let mut output = Writer::new();
        output.var_int(packet.len() as i32);
        output.extend(packet);
        Ok(output.into_inner())
    }
}

/// Port of CompressionEncoder. A negative threshold means the compression
/// handler is absent, as in Connection#setupCompression.
pub struct CompressionEncoder {
    threshold: i32,
    compressor: Compress,
}
impl CompressionEncoder {
    pub fn new(threshold: i32) -> Self {
        Self {
            threshold,
            compressor: Compress::new(Compression::default(), true),
        }
    }
    pub fn threshold(&self) -> i32 {
        self.threshold
    }
    pub fn set_threshold(&mut self, threshold: i32) {
        self.threshold = threshold;
    }
    pub fn encode(&mut self, packet: &[u8]) -> Result<Vec<u8>> {
        if self.threshold < 0 {
            return Ok(packet.to_vec());
        }
        if packet.len() > MAX_UNCOMPRESSED_LENGTH {
            return Err(Error::Length {
                length: packet.len() as i64,
                max: MAX_UNCOMPRESSED_LENGTH,
            });
        }
        let mut output = Writer::new();
        if packet.len() < self.threshold as usize {
            output.var_int(0);
            output.extend(packet);
            return Ok(output.into_inner());
        }
        output.var_int(packet.len() as i32);
        self.compressor.reset();
        let mut compressed = Vec::with_capacity(packet.len() + packet.len() / 1000 + 64);
        self.compressor
            .compress_vec(packet, &mut compressed, FlushCompress::Finish)
            .map_err(|e| Error::invalid("zlib", e.to_string()))?;
        output.extend(&compressed);
        Ok(output.into_inner())
    }
}
/// Port of CompressionDecoder. `validate_decompressed` corresponds to the
/// server's strict threshold and 8 MiB checks. Raw packets stay borrowed.
pub struct CompressionDecoder {
    threshold: i32,
    validate_decompressed: bool,
    decompressor: Decompress,
}
impl CompressionDecoder {
    pub fn new(threshold: i32, validate_decompressed: bool) -> Self {
        Self {
            threshold,
            validate_decompressed,
            decompressor: Decompress::new(true),
        }
    }
    pub fn set_threshold(&mut self, threshold: i32, validate_decompressed: bool) {
        self.threshold = threshold;
        self.validate_decompressed = validate_decompressed;
    }
    pub fn decode<'a>(&mut self, packet: &'a [u8]) -> Result<Cow<'a, [u8]>> {
        if self.threshold < 0 {
            return Ok(Cow::Borrowed(packet));
        }
        let mut input = Reader::new(packet);
        let length = input.var_int()?;
        if length == 0 {
            return Ok(Cow::Borrowed(input.rest()));
        }
        if length < 0 {
            return Err(Error::invalid(
                "compression",
                "Negative uncompressed length",
            ));
        }
        if self.validate_decompressed {
            if length < self.threshold {
                return Err(Error::invalid(
                    "compression",
                    format!(
                        "Badly compressed packet - size of {length} is below server threshold of {}",
                        self.threshold
                    ),
                ));
            }
            if length as usize > MAX_UNCOMPRESSED_LENGTH {
                return Err(Error::invalid(
                    "compression",
                    format!(
                        "Badly compressed packet - size of {length} is larger than protocol maximum of 8388608"
                    ),
                ));
            }
        }
        self.decompressor.reset(true);
        let mut output = Vec::new();
        output
            .try_reserve_exact(length as usize)
            .map_err(|e| Error::invalid("compression", e.to_string()))?;
        output.resize(length as usize, 0);
        let result = self
            .decompressor
            .decompress(input.rest(), &mut output, FlushDecompress::None);
        let actual = self.decompressor.total_out() as usize;
        self.decompressor.reset(true);
        result.map_err(|e| Error::invalid("zlib", e.to_string()))?;
        // Like Inflater#inflate, extra compressed input and extra uncompressed
        // data beyond the declared buffer are not inspected.
        if actual != length as usize {
            return Err(Error::invalid(
                "compression",
                format!(
                    "Badly compressed packet - actual length of uncompressed payload {actual} is does not match declared size {length}"
                ),
            ));
        }
        Ok(Cow::Owned(output))
    }
}

/// Port of CipherEncoder using AES/CFB8/NoPadding. State persists across packets;
/// vanilla uses the shared secret as both the AES key and initial IV.
pub struct CipherEncoder(cfb8::Encryptor<aes::Aes128>);
impl CipherEncoder {
    pub fn new(shared_secret: &[u8; 16]) -> Self {
        Self(cfb8::Encryptor::new(
            shared_secret.into(),
            shared_secret.into(),
        ))
    }
    pub fn encrypt(&mut self, bytes: &mut [u8]) {
        self.0.encrypt(bytes);
    }
}
/// Port of CipherDecoder, with independent inbound feedback state.
pub struct CipherDecoder(cfb8::Decryptor<aes::Aes128>);
impl CipherDecoder {
    pub fn new(shared_secret: &[u8; 16]) -> Self {
        Self(cfb8::Decryptor::new(
            shared_secret.into(),
            shared_secret.into(),
        ))
    }
    pub fn decrypt(&mut self, bytes: &mut [u8]) {
        self.0.decrypt(bytes);
    }
}

/// Port of Crypt#generateKeyPair and decryptUsingKey. The DER public key is the
/// X.509 SubjectPublicKeyInfo representation sent in ClientboundHelloPacket.
pub struct LoginKeyPair {
    private: RsaPrivateKey,
    public_der: Vec<u8>,
}
impl LoginKeyPair {
    pub fn generate() -> Result<Self> {
        let private = RsaPrivateKey::new(&mut OsRng, 1024)
            .map_err(|e| Error::invalid("RSA", e.to_string()))?;
        Self::from_private(private)
    }
    pub fn from_pkcs8_der(bytes: &[u8]) -> Result<Self> {
        Self::from_private(
            RsaPrivateKey::from_pkcs8_der(bytes)
                .map_err(|e| Error::invalid("RSA", e.to_string()))?,
        )
    }
    fn from_private(private: RsaPrivateKey) -> Result<Self> {
        let public_der = RsaPublicKey::from(&private)
            .to_public_key_der()
            .map_err(|e| Error::invalid("RSA", e.to_string()))?
            .as_bytes()
            .to_vec();
        Ok(Self {
            private,
            public_der,
        })
    }
    pub fn public_key_der(&self) -> &[u8] {
        &self.public_der
    }
    pub fn decrypt(&self, encrypted: &[u8]) -> Result<Vec<u8>> {
        self.private
            .decrypt_blinded(&mut OsRng, Pkcs1v15Encrypt, encrypted)
            .map_err(|e| Error::invalid("RSA", e.to_string()))
    }
    pub fn decrypt_secret(&self, encrypted: &[u8]) -> Result<[u8; 16]> {
        self.decrypt(encrypted)?
            .try_into()
            .map_err(|_| Error::invalid("AES", "Shared secret must be 16 bytes"))
    }
}
/// Port of Crypt#digestData, formatted like Java's signed BigInteger.toString(16)
/// for the session server's `serverId`. Login's server ID is Latin-1.
pub fn server_id_hash(server_id: &str, shared_secret: &[u8; 16], public_key_der: &[u8]) -> String {
    let mut hasher = Sha1::new();
    for ch in server_id.encode_utf16() {
        hasher.update([u8::try_from(ch).unwrap_or(b'?')]);
    }
    hasher.update(shared_secret);
    hasher.update(public_key_der);
    let mut digest: [u8; 20] = hasher.finalize().into();
    let negative = digest[0] & 128 != 0;
    if negative {
        for b in &mut digest {
            *b = !*b;
        }
        for b in digest.iter_mut().rev() {
            let (next, carry) = b.overflowing_add(1);
            *b = next;
            if !carry {
                break;
            }
        }
    }
    let text: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    let magnitude = text.trim_start_matches('0');
    format!(
        "{}{}",
        if negative { "-" } else { "" },
        if magnitude.is_empty() { "0" } else { magnitude }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hex(text: &str) -> Vec<u8> {
        text.trim()
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn vanilla_compression_threshold_and_zlib_bytes() {
        let mut encoder = CompressionEncoder::new(256);
        let mut decoder = CompressionDecoder::new(256, true);
        for (size, fixture) in [
            (255, include_str!("../tests/fixtures/compression_255.hex")),
            (256, include_str!("../tests/fixtures/compression_256.hex")),
            (257, include_str!("../tests/fixtures/compression_257.hex")),
        ] {
            let bytes = hex(fixture);
            let plain: Vec<_> = (0..size).map(|i| (i % 13) as u8).collect();
            assert_eq!(decoder.decode(&bytes).unwrap().as_ref(), plain);
            assert_eq!(encoder.encode(&plain).unwrap(), bytes);
        }
        let mut below = hex(include_str!("../tests/fixtures/compression_256.hex"));
        below[0] = 255;
        below[1] = 1;
        assert!(decoder.decode(&below).is_err());
        let mut too_large = Writer::new();
        too_large.var_int(MAX_UNCOMPRESSED_LENGTH as i32 + 1);
        assert!(decoder.decode(too_large.as_slice()).is_err());
    }
    #[test]
    fn vanilla_cipher_feedback_across_arbitrary_boundaries() {
        let key: [u8; 16] = std::array::from_fn(|i| i as u8);
        let mut actual: Vec<_> = (0..257).map(|i| (i % 13) as u8).collect();
        let expected = hex(include_str!("../tests/fixtures/cipher.hex"));
        let mut encoder = CipherEncoder::new(&key);
        for chunk in actual.chunks_mut(7) {
            encoder.encrypt(chunk);
        }
        assert_eq!(actual, expected);
        let mut decoder = CipherDecoder::new(&key);
        for chunk in actual.chunks_mut(11) {
            decoder.decrypt(chunk);
        }
        assert_eq!(actual, (0..257).map(|i| (i % 13) as u8).collect::<Vec<_>>());
    }
    #[test]
    fn vanilla_rsa_x509_pkcs8_pkcs1_and_session_hash() {
        let private = hex(include_str!("../tests/fixtures/rsa_private.hex"));
        let public = hex(include_str!("../tests/fixtures/rsa_public.hex"));
        let encrypted = hex(include_str!("../tests/fixtures/rsa_secret.hex"));
        let pair = LoginKeyPair::from_pkcs8_der(&private).unwrap();
        assert_eq!(pair.public_key_der(), public);
        let secret = std::array::from_fn(|i| i as u8);
        assert_eq!(pair.decrypt_secret(&encrypted).unwrap(), secret);
        assert_eq!(
            server_id_hash("", &secret, &public),
            include_str!("../tests/fixtures/server_hash.txt").trim()
        );
        assert!(pair.decrypt(&[0; 128]).is_err());
    }
    #[test]
    fn incremental_frames_and_21_bit_boundary() {
        let body = vec![42; 128];
        let encoded = FrameEncoder::encode(&body).unwrap();
        let mut decoder = FrameDecoder::new();
        decoder.push(&encoded[..1]);
        assert!(decoder.next_frame().unwrap().is_none());
        decoder.push(&encoded[1..50]);
        assert!(decoder.next_frame().unwrap().is_none());
        decoder.push(&encoded[50..]);
        assert_eq!(decoder.next_frame().unwrap().unwrap(), body);
        assert!(decoder.next_frame().unwrap().is_none());
        assert!(FrameDecoder::decode_slice(&[0]).is_err());
        assert!(FrameDecoder::decode_slice(&[128, 128, 128]).is_err());
        let max = FrameEncoder::encode(&vec![1; MAX_FRAME_LENGTH]).unwrap();
        assert_eq!(&max[..3], &[255, 255, 127]);
        assert!(FrameEncoder::encode(&vec![1; MAX_FRAME_LENGTH + 1]).is_err());
    }
}
