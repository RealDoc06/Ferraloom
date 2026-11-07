//! Palette indices packed into padded 64-bit words, not a continuous bitstream.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    InvalidBits(u8),
    InvalidLength { expected: usize, actual: usize },
    OutOfBounds { index: usize, len: usize },
    ValueTooLarge(u32),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for StorageError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedStorage {
    bits: u8,
    len: usize,
    per_word: usize,
    mask: u64,
    words: Vec<u64>,
}

impl PackedStorage {
    pub fn new(bits: u8, len: usize) -> Result<Self, StorageError> {
        let per_word = Self::per_word(bits)?;
        Self::from_words(bits, len, vec![0; len.div_ceil(per_word)])
    }

    /// Retains padding and unused tail bits from input, as the Java long[] does.
    pub fn from_words(bits: u8, len: usize, words: Vec<u64>) -> Result<Self, StorageError> {
        let per_word = Self::per_word(bits)?;
        let expected = len.div_ceil(per_word);
        if words.len() != expected {
            return Err(StorageError::InvalidLength {
                expected,
                actual: words.len(),
            });
        }
        Ok(Self {
            bits,
            len,
            per_word,
            mask: (1_u64 << bits) - 1,
            words,
        })
    }

    fn per_word(bits: u8) -> Result<usize, StorageError> {
        if !(1..=32).contains(&bits) {
            return Err(StorageError::InvalidBits(bits));
        }
        Ok(64 / usize::from(bits))
    }

    pub fn bits(&self) -> u8 {
        self.bits
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    fn location(&self, index: usize) -> Result<(usize, usize), StorageError> {
        if index >= self.len {
            return Err(StorageError::OutOfBounds {
                index,
                len: self.len,
            });
        }
        Ok((
            index / self.per_word,
            (index % self.per_word) * usize::from(self.bits),
        ))
    }

    pub fn get(&self, index: usize) -> Result<u32, StorageError> {
        let (word, shift) = self.location(index)?;
        Ok(((self.words[word] >> shift) & self.mask) as u32)
    }

    /// Reads `index` without a bounds check. Callers use this once they hold a
    /// slice length or a section index they have already validated.
    /// `index` must be less than `len`.
    #[inline]
    pub fn read(&self, index: usize) -> u32 {
        debug_assert!(index < self.len);
        // Each possible divisor is constant in its arm, avoiding hardware div.
        let word = match self.per_word {
            2 => index / 2,
            3 => index / 3,
            4 => index / 4,
            5 => index / 5,
            6 => index / 6,
            7 => index / 7,
            8 => index / 8,
            9 => index / 9,
            10 => index / 10,
            12 => index / 12,
            16 => index / 16,
            21 => index / 21,
            32 => index / 32,
            64 => index / 64,
            _ => unreachable!("validated bits"),
        };
        let shift = (index - word * self.per_word) * usize::from(self.bits);
        ((self.words[word] >> shift) & self.mask) as u32
    }

    /// Writes `value` at `index` without revalidating either. `index` must be
    /// less than `len` and `value` must fit `bits`, which holds for a section
    /// palette index from the same container. Returns the previous value.
    pub fn write(&mut self, index: usize, value: u32) -> u32 {
        debug_assert!(index < self.len && u64::from(value) <= self.mask);
        let word = index / self.per_word;
        let shift = (index % self.per_word) * usize::from(self.bits);
        let old = ((self.words[word] >> shift) & self.mask) as u32;
        self.words[word] = (self.words[word] & !(self.mask << shift)) | (u64::from(value) << shift);
        old
    }

    /// Validation fails before mutation. Writes accept nonnegative Java ints;
    /// imported 32-bit words may still decode to any u32 bit pattern.
    pub fn set(&mut self, index: usize, value: u32) -> Result<u32, StorageError> {
        let (word, shift) = self.location(index)?;
        if u64::from(value) > self.mask || value > i32::MAX as u32 {
            return Err(StorageError::ValueTooLarge(value));
        }
        let old = ((self.words[word] >> shift) & self.mask) as u32;
        self.words[word] = (self.words[word] & !(self.mask << shift)) | (u64::from(value) << shift);
        Ok(old)
    }

    /// Vanilla's doubling fill also writes padding and unused tail slots.
    pub fn fill(&mut self, value: u32) -> Result<(), StorageError> {
        if u64::from(value) > self.mask || value > i32::MAX as u32 {
            return Err(StorageError::ValueTooLarge(value));
        }
        let mut word = u64::from(value);
        let mut shift = u32::from(self.bits);
        while shift < 64 {
            word |= word << shift;
            shift *= 2;
        }
        self.words.fill(word);
        Ok(())
    }

    pub fn unpack(&self) -> Vec<u32> {
        let mut values = Vec::with_capacity(self.len);
        for &word in &self.words {
            for slot in 0..self.per_word.min(self.len - values.len()) {
                values.push(((word >> (slot * usize::from(self.bits))) & self.mask) as u32);
            }
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "release read microbenchmark"]
    fn benchmark_storage_reads() {
        use std::{hint::black_box, time::Instant};
        for bits in [4, 5, 6, 7, 8, 16] {
            let s = PackedStorage::from_words(
                bits,
                4096,
                vec![0x123456789abcdef0; 4096usize.div_ceil(64 / usize::from(bits))],
            )
            .unwrap();
            let mut times = [Vec::new(), Vec::new()];
            for round in 0..11 {
                for new in [round % 2 == 0, round % 2 != 0] {
                    let start = Instant::now();
                    for j in 0..500_000usize {
                        let s = black_box(&s);
                        let i = black_box(j.wrapping_mul(31337) & 4095);
                        black_box(if new {
                            s.read(i)
                        } else {
                            ((s.words[i / s.per_word] >> ((i % s.per_word) * usize::from(s.bits)))
                                & s.mask) as u32
                        });
                    }
                    if round > 0 {
                        times[usize::from(new)]
                            .push(start.elapsed().as_secs_f64() * 1e9 / 500_000.0);
                    }
                }
            }
            for t in &mut times {
                t.sort_by(f64::total_cmp);
            }
            println!(
                "storage bits={bits}: old {:.3} new {:.3} ns/op",
                times[0][5], times[1][5]
            );
        }
    }
    #[test]
    fn reads_match_division_at_every_width() {
        for bits in 1..=32 {
            for len in [1usize, 63, 64, 65, 4096, 8193] {
                let per = 64 / usize::from(bits);
                let words = (0..len.div_ceil(per))
                    .map(|i| (i as u64).wrapping_mul(0x9e3779b97f4a7c15))
                    .collect();
                let s = PackedStorage::from_words(bits, len, words).unwrap();
                for i in 0..len {
                    let old =
                        ((s.words[i / per] >> ((i % per) * usize::from(bits))) & s.mask) as u32;
                    assert_eq!(s.read(i), old);
                    assert_eq!(s.get(i), Ok(old));
                }
            }
        }
    }
    #[test]
    fn padded_words_never_split_entries() {
        let mut storage = PackedStorage::new(5, 13).unwrap();
        assert_eq!(storage.words().len(), 2);
        storage.set(11, 31).unwrap();
        storage.set(12, 7).unwrap();
        assert_eq!(storage.words(), &[31 << 55, 7]);
    }

    #[test]
    fn all_widths_survive_overwrites_and_unpack() {
        for bits in 1..=32 {
            let mut storage = PackedStorage::new(bits, 4096).unwrap();
            let mut expected = vec![0; 4096];
            for pass in 0..3 {
                for (i, item) in expected.iter_mut().enumerate() {
                    let value = ((i as u64 * 7 + pass) & storage.mask) as u32;
                    assert_eq!(storage.set(i, value).unwrap(), *item);
                    *item = value;
                }
                assert_eq!(storage.unpack(), expected);
            }
        }
    }

    #[test]
    fn imported_padding_is_preserved_and_invalid_writes_are_atomic() {
        let mut storage = PackedStorage::from_words(5, 1, vec![u64::MAX]).unwrap();
        assert_eq!(storage.set(0, 0).unwrap(), 31);
        assert_eq!(storage.words(), &[u64::MAX << 5]);
        let unchanged = storage.clone();
        assert!(storage.set(1, 0).is_err());
        assert!(storage.set(0, 32).is_err());
        assert_eq!(storage, unchanged);
        assert!(PackedStorage::new(0, 1).is_err());
        assert!(PackedStorage::new(33, 1).is_err());
        assert!(PackedStorage::from_words(5, 13, vec![0]).is_err());
        assert!(PackedStorage::new(1, 0).unwrap().unpack().is_empty());
    }
}
