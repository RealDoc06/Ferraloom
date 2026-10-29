//! Contact broad phase scoped to one world's contact pass.
use std::collections::HashMap;

/// Exact section eligibility at a world revision. Create a fresh cache for each
/// contact pass; revisions need only identify writes within that pass. Every
/// write or chunk replacement must advance the revision before the next query.
#[derive(Default)]
pub struct ContactSections {
    sections: HashMap<(i32, i32, i32), (u64, bool)>,
    last: Option<((i32, i32, i32), u64, bool)>,
}
impl ContactSections {
    pub fn get(
        &mut self,
        section: (i32, i32, i32),
        revision: u64,
        scan: impl FnOnce() -> bool,
    ) -> bool {
        if let Some((key, read, answer)) = self.last {
            if key == section && read == revision {
                return answer;
            }
        }
        let answer = match self.sections.entry(section) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if entry.get().0 != revision {
                    entry.insert((revision, scan()));
                }
                entry.get().1
            }
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert((revision, scan())).1,
        };
        self.last = Some((section, revision, answer));
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_sections_observe_additions_removals_and_other_sections() {
        let mut cache = ContactSections::default();
        let section = (-1, -4, 2);
        assert!(!cache.get(section, 0, || false));
        assert!(!cache.get(section, 0, || panic!("unchanged section rescanned")));
        assert!(cache.get(section, 1, || true));
        assert!(!cache.get((0, -4, 2), 1, || false));
        assert!(!cache.get(section, 2, || false));
        assert!(cache.get(section, 3, || true));
    }
}

/// Current chunk for a random-tick read. A strong reference lives only for the
/// read, so callbacks can mutate without cloning storage retained by this cache.
pub struct RandomTickChunk<T> {
    revision: Option<u64>,
    chunk: std::sync::Weak<T>,
}
impl<T> Default for RandomTickChunk<T> {
    fn default() -> Self {
        Self {
            revision: None,
            chunk: std::sync::Weak::new(),
        }
    }
}
impl<T> RandomTickChunk<T> {
    pub fn get<'a>(
        &mut self,
        revision: u64,
        load: impl FnOnce() -> Option<&'a std::sync::Arc<T>>,
    ) -> Option<std::sync::Arc<T>>
    where
        T: 'a,
    {
        if self.revision == Some(revision) {
            if let Some(chunk) = self.chunk.upgrade() {
                return Some(chunk);
            }
        }
        // Arc::make_mut can detach weak pointers on a no-op write, without
        // advancing the world's block revision. An expired pointer reloads too.
        let chunk = load()?;
        self.revision = Some(revision);
        self.chunk = std::sync::Arc::downgrade(chunk);
        Some(chunk.clone())
    }
}

#[cfg(test)]
mod random_chunk_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn reader_observes_writes_replacements_and_no_op_detachment() {
        let mut chunk = Arc::new(vec![0]);
        let mut cache = RandomTickChunk::default();
        assert_eq!(cache.get(0, || Some(&chunk)).unwrap()[0], 0);
        assert_eq!(
            cache.get(0, || panic!("unchanged chunk reloaded")).unwrap()[0],
            0
        );
        // No retained strong reference: make_mut must move, rather than clone.
        Arc::make_mut(&mut chunk)[0] = 0;
        assert_eq!(cache.get(0, || Some(&chunk)).unwrap()[0], 0);
        Arc::make_mut(&mut chunk)[0] = 1;
        assert_eq!(cache.get(1, || Some(&chunk)).unwrap()[0], 1);
        chunk = Arc::new(vec![2]);
        assert_eq!(cache.get(2, || Some(&chunk)).unwrap()[0], 2);
        assert!(cache.get(3, || None).is_none());
        chunk = Arc::new(vec![3]);
        assert_eq!(cache.get(4, || Some(&chunk)).unwrap()[0], 3);
    }
}
