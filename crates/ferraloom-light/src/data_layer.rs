/// Port of `net.minecraft.world.level.chunk.DataLayer`.
///
/// A homogeneous layer remains unallocated until its first write or `data()`.
/// Allocated zeroes are deliberately not considered empty, as in vanilla.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataLayer {
    bytes: Option<Box<[u8; Self::SIZE]>>,
    default_value: u8,
}

impl Default for DataLayer {
    fn default() -> Self {
        Self::new(0)
    }
}

impl DataLayer {
    pub const SIZE: usize = 2048;

    pub fn new(default_value: u8) -> Self {
        assert!(default_value <= 15);
        Self {
            bytes: None,
            default_value,
        }
    }

    pub fn from_bytes(bytes: Box<[u8; Self::SIZE]>) -> Self {
        Self {
            bytes: Some(bytes),
            default_value: 0,
        }
    }

    pub fn get(&self, x: u8, y: u8, z: u8) -> u8 {
        self.get_index(index(x, y, z))
    }

    pub fn set(&mut self, x: u8, y: u8, z: u8, value: u8) {
        let index = index(x, y, z);
        let shift = (index & 1) * 4;
        let byte = &mut self.data()[index >> 1];
        *byte = (*byte & !(15 << shift)) | ((value & 15) << shift);
    }

    pub fn get_index(&self, index: usize) -> u8 {
        assert!(index < 4096);
        self.bytes.as_ref().map_or(self.default_value, |b| {
            (b[index >> 1] >> ((index & 1) * 4)) & 15
        })
    }

    pub fn fill(&mut self, value: u8) {
        assert!(value <= 15);
        self.default_value = value;
        self.bytes = None;
    }

    /// Materialize the backing bytes, preserving vanilla's lazy-state change.
    pub fn data(&mut self) -> &mut [u8; Self::SIZE] {
        self.bytes
            .get_or_insert_with(|| Box::new([self.default_value * 17; Self::SIZE]))
    }

    /// Export without changing whether this layer is definitely homogeneous.
    pub fn to_bytes(&self) -> Box<[u8; Self::SIZE]> {
        self.bytes
            .clone()
            .unwrap_or_else(|| Box::new([self.default_value * 17; Self::SIZE]))
    }

    pub fn is_definitely_homogeneous(&self) -> bool {
        self.bytes.is_none()
    }
    pub fn is_definitely_filled_with(&self, value: u8) -> bool {
        self.bytes.is_none() && self.default_value == value
    }
    pub fn is_empty(&self) -> bool {
        self.is_definitely_filled_with(0)
    }

    pub(crate) fn repeat_first_layer(&self) -> Self {
        if self.is_definitely_homogeneous() {
            return self.clone();
        }
        let input = self.bytes.as_ref().unwrap();
        let mut output = Box::new([0; Self::SIZE]);
        for layer in output.as_chunks_mut::<128>().0 {
            layer.copy_from_slice(&input[..128]);
        }
        Self::from_bytes(output)
    }
}

fn index(x: u8, y: u8, z: u8) -> usize {
    assert!(x < 16 && y < 16 && z < 16);
    (usize::from(y) << 8) | (usize::from(z) << 4) | usize::from(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazy_fill_and_nibble_order() {
        let mut layer = DataLayer::new(7);
        assert!(layer.is_definitely_filled_with(7));
        layer.set(1, 0, 0, 15);
        assert!(!layer.is_definitely_homogeneous());
        assert_eq!(layer.data()[0], 0xf7);
        layer.set(0, 15, 15, 3);
        assert_eq!(layer.data()[2040], 0x73);
        layer.fill(0);
        assert!(layer.is_empty());
        layer.data();
        assert!(!layer.is_empty());
        assert_eq!(layer.to_bytes().as_ref(), &[0; 2048]);
    }
}
