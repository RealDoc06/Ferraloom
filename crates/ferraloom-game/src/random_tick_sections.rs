//! Memoized live random-tick section eligibility, without retaining chunk data.
use crate::{
    chunk::{MIN_Y, SECTIONS, Section},
    fast_hash::FxHashMap,
};
use std::sync::{Arc, Weak};

/// The live sections of the same owner whose Arc identity invalidates the cache.
/// Section edits must invalidate the owner's Arc identity, as resident
/// `Arc::make_mut` edits already do; the cache holds only weak references.
pub trait SectionOwner {
    fn sections(&self) -> &[Arc<Section>];
}

/// A value snapshot, in ascending section order, with no per-chunk allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sections(u32);
impl Sections {
    /// The minimum Y of the lowest section in the set from block `min_y` up.
    pub fn from(self, min_y: i32) -> Option<i32> {
        let index = ((min_y - MIN_Y) >> 4).max(0) as u32;
        Sections(self.0 & u32::MAX.checked_shl(index).unwrap_or(0)).next()
    }
}
impl Iterator for Sections {
    type Item = i32;
    fn next(&mut self) -> Option<i32> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros();
        self.0 &= self.0 - 1;
        Some(MIN_Y + index as i32 * 16)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.count_ones() as usize;
        (len, Some(len))
    }
}
impl ExactSizeIterator for Sections {}

struct Cached<C> {
    chunk: Weak<C>,
    /// The section each bit of `sections` was read from.
    read: [Weak<Section>; SECTIONS],
    sections: Sections,
}
/// Vanilla keeps a count of randomly ticking blocks and fluids per section,
/// updated by every `setBlockState`, and ticks the sections where it is above
/// zero. Here a section's answer is read from its blocks after any edit:
/// resident chunks and sections are immutable behind `Arc` and edited through
/// `Arc::make_mut`, so a weak reference tells an edited or replaced one from
/// the one read, without retaining block data or forcing a deep copy on the
/// next edit, and prevents address reuse. An unchanged chunk costs one
/// pointer comparison; a changed one, a comparison per section and a read of
/// the sections that changed. Clear this cache whenever the gameplay facts
/// change (including resource reload).
pub struct RandomTickSections<C> {
    chunks: FxHashMap<(i32, i32), Cached<C>>,
}
impl<C> Default for RandomTickSections<C> {
    fn default() -> Self {
        Self {
            chunks: FxHashMap::default(),
        }
    }
}
impl<C: SectionOwner> RandomTickSections<C> {
    pub fn clear(&mut self) {
        self.chunks.clear();
    }
    pub fn remove(&mut self, key: &(i32, i32)) {
        self.chunks.remove(key);
    }
    /// The sections of `chunk` holding a randomly ticking block or fluid.
    pub fn get(
        &mut self,
        key: (i32, i32),
        chunk: &Arc<C>,
        mut random: impl FnMut(u32) -> bool,
    ) -> Sections {
        const {
            assert!(SECTIONS <= u32::BITS as usize);
        }
        let cached = self.chunks.entry(key).or_insert_with(|| Cached {
            chunk: Weak::new(),
            read: std::array::from_fn(|_| Weak::new()),
            sections: Sections::default(),
        });
        if cached.chunk.as_ptr() != Arc::as_ptr(chunk) {
            for (i, (section, read)) in chunk.sections().iter().zip(&mut cached.read).enumerate() {
                if read.as_ptr() == Arc::as_ptr(section) {
                    continue;
                }
                // `isRandomlyTicking`: the blocks themselves, not the palette,
                // which keeps the states no block uses any more.
                if section.blocks.has(&mut random) {
                    cached.sections.0 |= 1 << i;
                } else {
                    cached.sections.0 &= !(1 << i);
                }
                *read = Arc::downgrade(section);
            }
            cached.chunk = Arc::downgrade(chunk);
        }
        cached.sections
    }
}
