//! Which sections hold a block with a contact hook (`entityInside` of plates,
//! tripwires, detector rails and arrow buttons), kept across ticks.
//!
//! `Game::tick_blocks` asks this for the section of every entity box each
//! tick. The answer depends only on the section's blocks, and a resident
//! section is immutable behind its `Arc`: an edit goes through
//! `Arc::make_mut`, which never writes in place while a `Weak` to the section
//! is held. So an answer stays exact while the section is the same allocation
//! it was asked of, and the `Weak` keeps that allocation from being reused.
use crate::{chunk::Section, fast_hash::FxHashMap};
use std::sync::{Arc, Weak};

type Key = (i32, i32, i32);

/// The answers of this pass and the last; an answer not asked for during a
/// whole pass is dropped, so sections that left do not pile up.
#[derive(Default)]
pub struct ContactSections {
    current: FxHashMap<Key, (Weak<Section>, bool)>,
    previous: FxHashMap<Key, (Weak<Section>, bool)>,
    /// The hook table the answers were computed with.
    owner: usize,
}

impl ContactSections {
    /// Start a pass whose hooks come from `owner` (any address identifying
    /// the hook table): answers computed under another owner are dropped.
    pub fn begin(&mut self, owner: usize) {
        if owner != self.owner {
            self.current.clear();
            self.owner = owner;
        }
        self.previous = std::mem::take(&mut self.current);
    }
    /// Whether `section` (at `key`) has a block `hook` accepts, by
    /// [`crate::chunk::Section`]'s exact `has` (unused palette entries do not
    /// count). `hook` runs only when the section is new or changed.
    pub fn get(&mut self, key: Key, section: &Arc<Section>, hook: impl FnMut(u32) -> bool) -> bool {
        let same = |entry: &(Weak<Section>, bool)| entry.0.as_ptr() == Arc::as_ptr(section);
        if let Some(entry) = self.current.get(&key)
            && same(entry)
        {
            return entry.1;
        }
        let answer = match self.previous.remove(&key) {
            Some(entry) if same(&entry) => entry.1,
            _ => section.blocks.has(hook),
        };
        self.current.insert(key, (Arc::downgrade(section), answer));
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_follow_edits_and_expire_after_a_pass() {
        let mut cache = ContactSections::default();
        let mut section = Arc::new(Section::default());
        cache.begin(1);
        assert!(!cache.get((0, 0, 0), &section, |id| id == 7));
        assert!(!cache.get((0, 0, 0), &section, |_| panic!(
            "unchanged section rescanned"
        )));
        Arc::make_mut(&mut section).set(5, 7);
        assert!(cache.get((0, 0, 0), &section, |id| id == 7));
        // The next pass still knows it; one more without asking forgets it.
        cache.begin(1);
        assert!(cache.get((0, 0, 0), &section, |_| panic!("kept answer rescanned")));
        cache.begin(1);
        cache.begin(1);
        assert!(cache.get((0, 0, 0), &section, |id| id == 7));
        // Another hook table asks again.
        cache.begin(2);
        assert!(!cache.get((0, 0, 0), &section, |_| false));
    }
}
