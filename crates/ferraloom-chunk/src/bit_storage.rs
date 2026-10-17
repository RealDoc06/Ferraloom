use crate::ContainerError;

/// Port of SimpleBitStorage. Entries never span longs; unused high bits are padding.
#[derive(Clone, Debug)]
pub struct SimpleBitStorage {
    bits: u8,
    size: usize,
    values_per_long: usize,
    mask: u64,
    data: Vec<u64>,
}

impl SimpleBitStorage {
    pub fn new(bits: u8, size: usize) -> Result<Self, ContainerError> {
        if !(1..=32).contains(&bits) {
            return Err(ContainerError::InvalidBits(bits));
        }
        Self::from_raw(bits, size, vec![0; size.div_ceil(64 / usize::from(bits))])
    }

    pub fn from_raw(bits: u8, size: usize, data: Vec<u64>) -> Result<Self, ContainerError> {
        if !(1..=32).contains(&bits) {
            return Err(ContainerError::InvalidBits(bits));
        }
        let values_per_long = 64 / usize::from(bits);
        let expected = size.div_ceil(values_per_long);
        if data.len() != expected {
            return Err(ContainerError::InvalidLength {
                expected,
                actual: data.len(),
            });
        }
        Ok(Self {
            bits,
            size,
            values_per_long,
            mask: (1_u64 << bits) - 1,
            data,
        })
    }

    /// The vanilla int-array constructor masks values, unlike checked set().
    pub fn from_values(bits: u8, values: &[u32]) -> Result<Self, ContainerError> {
        let mut storage = Self::new(bits, values.len())?;
        for (index, &value) in values.iter().enumerate() {
            storage.set(index, (u64::from(value) & storage.mask) as u32);
        }
        Ok(storage)
    }

    pub fn bits(&self) -> u8 {
        self.bits
    }
    pub fn len(&self) -> usize {
        self.size
    }
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
    pub fn raw(&self) -> &[u64] {
        &self.data
    }

    pub fn get(&self, index: usize) -> u32 {
        assert!(index < self.size, "bit storage index out of bounds");
        let cell = index / self.values_per_long;
        let offset = (index % self.values_per_long) * usize::from(self.bits);
        ((self.data[cell] >> offset) & self.mask) as u32
    }

    pub fn set(&mut self, index: usize, value: u32) {
        self.get_and_set(index, value);
    }

    pub fn get_and_set(&mut self, index: usize, value: u32) -> u32 {
        assert!(index < self.size, "bit storage index out of bounds");
        assert!(
            u64::from(value) <= self.mask,
            "value exceeds bit storage width"
        );
        let cell = index / self.values_per_long;
        let offset = (index % self.values_per_long) * usize::from(self.bits);
        let old = ((self.data[cell] >> offset) & self.mask) as u32;
        self.data[cell] = (self.data[cell] & !(self.mask << offset)) | (u64::from(value) << offset);
        old
    }

    /// Preserve vanilla's padding bits too: fill repeats into the top partial entry.
    pub fn fill(&mut self, value: u32) {
        assert!(
            u64::from(value) <= self.mask,
            "value exceeds bit storage width"
        );
        let mut cell = u64::from(value);
        let mut offset = u32::from(self.bits);
        loop {
            cell |= cell << offset;
            offset <<= 1;
            if offset >= 64 {
                break;
            }
        }
        self.data.fill(cell);
    }
}

/// Port of ZeroBitStorage. Only zero is representable and no longs are allocated.
#[derive(Clone, Debug)]
pub struct ZeroBitStorage {
    size: usize,
}

impl ZeroBitStorage {
    pub fn new(size: usize) -> Self {
        Self { size }
    }
    pub fn len(&self) -> usize {
        self.size
    }
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
    pub fn get(&self, index: usize) -> u32 {
        assert!(index < self.size);
        0
    }
    pub fn get_and_set(&mut self, index: usize, value: u32) -> u32 {
        assert_eq!(value, 0);
        self.get(index)
    }
    pub fn fill(&mut self, value: u32) {
        assert_eq!(value, 0);
    }
}

#[derive(Clone, Debug)]
pub enum BitStorage {
    Zero(ZeroBitStorage),
    Simple(SimpleBitStorage),
}

impl BitStorage {
    pub fn new(bits: u8, size: usize) -> Result<Self, ContainerError> {
        if bits == 0 {
            Ok(Self::Zero(ZeroBitStorage::new(size)))
        } else {
            SimpleBitStorage::new(bits, size).map(Self::Simple)
        }
    }
    pub fn from_raw(bits: u8, size: usize, raw: Vec<u64>) -> Result<Self, ContainerError> {
        if bits == 0 {
            if !raw.is_empty() {
                return Err(ContainerError::InvalidLength {
                    expected: 0,
                    actual: raw.len(),
                });
            }
            Ok(Self::Zero(ZeroBitStorage::new(size)))
        } else {
            SimpleBitStorage::from_raw(bits, size, raw).map(Self::Simple)
        }
    }
    pub fn bits(&self) -> u8 {
        match self {
            Self::Zero(_) => 0,
            Self::Simple(s) => s.bits(),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Zero(s) => s.len(),
            Self::Simple(s) => s.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn raw(&self) -> &[u64] {
        match self {
            Self::Zero(_) => &[],
            Self::Simple(s) => s.raw(),
        }
    }
    pub fn get(&self, index: usize) -> u32 {
        match self {
            Self::Zero(s) => s.get(index),
            Self::Simple(s) => s.get(index),
        }
    }
    pub fn get_and_set(&mut self, index: usize, value: u32) -> u32 {
        match self {
            Self::Zero(s) => s.get_and_set(index, value),
            Self::Simple(s) => s.get_and_set(index, value),
        }
    }
    pub fn fill(&mut self, value: u32) {
        match self {
            Self::Zero(s) => s.fill(value),
            Self::Simple(s) => s.fill(value),
        }
    }
}
