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
