//! JDK 25 PriorityQueue heap layout and removable iterator semantics.
use super::ReferenceKey;
use std::{cmp::Ordering, collections::VecDeque};

/// Port of java.util.PriorityQueue. Comparator equality does not imply object
/// equality: equal-priority entries retain JDK's exact heap tie behavior.
/// Each owned element has stable allocation identity for Iterator#removeEq.
#[derive(Debug)]
pub struct JavaPriorityQueue<T, C = fn(&T, &T) -> Ordering> {
    // Boxes keep element identity stable while heap slots move, without Clone or unsafe.
    #[allow(clippy::vec_box)] // Stable allocation identity is required by forgetMeNot.
    heap: Vec<Box<T>>,
    capacity: usize,
    compare: C,
}
impl<T, C: Fn(&T, &T) -> Ordering> JavaPriorityQueue<T, C> {
    /// Port of PriorityQueue(Comparator): default backing array length eleven.
    pub fn new(compare: C) -> Self {
        Self::with_capacity(11, compare)
    }
    pub fn with_capacity(capacity: usize, compare: C) -> Self {
        assert!(
            capacity >= 1 && capacity <= i32::MAX as usize,
            "invalid priority queue capacity"
        );
        Self {
            heap: Vec::with_capacity(capacity),
            capacity,
            compare,
        }
    }
    /// Port of PriorityQueue(Collection): copy array order, then bottom-up heapify.
    /// An empty collection still allocates one slot in JDK 25.
    pub fn from_collection(iter: impl IntoIterator<Item = T>, compare: C) -> Self {
        let heap = iter.into_iter().map(Box::new).collect::<Vec<_>>();
        let capacity = heap.len().max(1);
        let mut queue = Self {
            heap,
            capacity,
            compare,
        };
        queue.heapify();
        queue
    }
    pub fn len(&self) -> usize {
        self.heap.len()
    }
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
    /// JDK's logical backing array length, independent of allocator rounding.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn comparator(&self) -> &C {
        &self.compare
    }
    pub fn peek(&self) -> Option<&T> {
        self.heap.first().map(Box::as_ref)
    }
    pub fn offer(&mut self, value: T) -> bool {
        if self.len() == self.capacity {
            self.grow(self.len() + 1);
        }
        let pos = self.len();
        self.heap.push(Box::new(value));
        self.sift_up(pos);
        true
    }
    pub fn poll(&mut self) -> Option<T> {
        self.remove_at(0)
    }
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.heap.iter().any(|v| **v == *value)
    }
    /// Port of remove(Object): removes the first equal element in array order.
    pub fn remove(&mut self, value: &T) -> bool
    where
        T: PartialEq,
    {
        if let Some(pos) = self.heap.iter().position(|v| **v == *value) {
            self.remove_at(pos);
            true
        } else {
            false
        }
    }
    /// Remove a heap-array index. The removed element is returned; internally
    /// the iterator also receives JDK removeAt's "moved before cursor" result.
    pub fn remove_at(&mut self, pos: usize) -> Option<T> {
        Some(*self.remove_at_entry(pos)?.0)
    }
    fn remove_at_entry(&mut self, pos: usize) -> Option<(Box<T>, bool)> {
        if pos >= self.len() {
            return None;
        }
        let removed = self.heap.swap_remove(pos);
        let moved = pos < self.len() && self.sift_down(pos) == pos && self.sift_up(pos) < pos;
        Some((removed, moved))
    }
    fn grow(&mut self, min: usize) {
        let old = self.capacity;
        let preferred = if old < 64 { old + 2 } else { old >> 1 };
        let growth = preferred.max(min - old);
        // ArraysSupport.newLength's soft maximum and hugeLength fallback.
        let soft_max = i32::MAX as usize - 8;
        let next = old.checked_add(growth).expect("priority queue too large");
        self.capacity = if next <= soft_max {
            next
        } else {
            assert!(min <= i32::MAX as usize, "priority queue too large");
            min.max(soft_max)
        };
        self.heap.reserve_exact(self.capacity - self.len());
    }
    fn sift_up(&mut self, mut pos: usize) -> usize {
        while pos > 0 {
            let parent = (pos - 1) >> 1;
            if (self.compare)(&self.heap[pos], &self.heap[parent]) != Ordering::Less {
                break;
            }
            self.heap.swap(pos, parent);
            pos = parent;
        }
        pos
    }
    fn sift_down(&mut self, mut pos: usize) -> usize {
        let size = self.len();
        let half = size >> 1;
        while pos < half {
            let mut child = pos * 2 + 1;
            let right = child + 1;
            // Strict comparison is essential: equal children select the left one.
            if right < size
                && (self.compare)(&self.heap[child], &self.heap[right]) == Ordering::Greater
            {
                child = right;
            }
            if (self.compare)(&self.heap[pos], &self.heap[child]) != Ordering::Greater {
                break;
            }
            self.heap.swap(pos, child);
            pos = child;
        }
        pos
    }
    /// Port of PriorityQueue#heapify, visiting internal nodes in reverse array order.
    pub fn heapify(&mut self) {
        for pos in (0..self.len() / 2).rev() {
            self.sift_down(pos);
        }
    }
    pub fn clear(&mut self) {
        self.heap.clear();
    }
    /// Heap-array order, not priority order, exactly like PriorityQueue#iterator.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &T> + DoubleEndedIterator {
        self.heap.iter().map(Box::as_ref)
    }
    pub fn cursor(&mut self) -> PriorityCursor<'_, T, C> {
        PriorityCursor {
            queue: self,
            cursor: 0,
            last: None,
            forget_me_not: VecDeque::new(),
            reference_address: None,
        }
    }
    /// For Rc/Arc/shared references, Java removeEq must match object identity even
    /// when the same object reference is offered more than once.
    pub fn reference_cursor(&mut self) -> PriorityCursor<'_, T, C>
    where
        T: ReferenceKey,
    {
        PriorityCursor {
            queue: self,
            cursor: 0,
            last: None,
            forget_me_not: VecDeque::new(),
            reference_address: Some(ReferenceKey::reference_address),
        }
    }
}
impl<T: Ord> JavaPriorityQueue<T> {
    pub fn natural_order() -> Self {
        Self::new(T::cmp)
    }
}
impl<T: Ord> Default for JavaPriorityQueue<T> {
    fn default() -> Self {
        Self::natural_order()
    }
}
impl<'a, T, C> IntoIterator for &'a JavaPriorityQueue<T, C> {
    type Item = &'a T;
    type IntoIter = std::iter::Map<std::slice::Iter<'a, Box<T>>, fn(&'a Box<T>) -> &'a T>;
    fn into_iter(self) -> Self::IntoIter {
        self.heap.iter().map(Box::as_ref)
    }
}
impl<T, C> IntoIterator for JavaPriorityQueue<T, C> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.heap
            .into_iter()
            .map(|v| *v)
            .collect::<Vec<_>>()
            .into_iter()
    }
}
enum Last {
    Slot(usize),
    Forgotten(usize),
}
/// Port of PriorityQueue.Itr, including its FIFO forgetMeNot list. Rust's mutable
/// borrow rules prevent concurrent structural modification during the iteration.
pub struct PriorityCursor<'a, T, C> {
    queue: &'a mut JavaPriorityQueue<T, C>,
    cursor: usize,
    last: Option<Last>,
    forget_me_not: VecDeque<usize>,
    reference_address: Option<fn(&T) -> usize>,
}
impl<T, C: Fn(&T, &T) -> Ordering> PriorityCursor<'_, T, C> {
    fn address(&self, value: &T) -> usize {
        self.reference_address
            .map_or_else(|| std::ptr::from_ref(value) as usize, |f| f(value))
    }
    pub fn next_entry(&mut self) -> Option<&T> {
        let pos = if self.cursor < self.queue.len() {
            let pos = self.cursor;
            self.cursor += 1;
            self.last = Some(Last::Slot(pos));
            pos
        } else {
            let address = self.forget_me_not.pop_front()?;
            self.last = Some(Last::Forgotten(address));
            self.queue
                .heap
                .iter()
                .position(|v| self.address(v) == address)
                .expect("forgotten object remains in the heap")
        };
        Some(&self.queue.heap[pos])
    }
    pub fn remove(&mut self) -> Option<T> {
        match self.last.take()? {
            Last::Slot(pos) => {
                let original_last = self.queue.heap.last().map(|v| self.address(v));
                let (removed, moved) = self.queue.remove_at_entry(pos)?;
                if moved {
                    self.forget_me_not.push_back(original_last.unwrap());
                } else {
                    self.cursor -= 1;
                }
                Some(*removed)
            }
            Last::Forgotten(address) => {
                let pos = self
                    .queue
                    .heap
                    .iter()
                    .position(|v| self.address(v) == address)?;
                self.queue.remove_at(pos)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fmt::Debug, rc::Rc};
    #[derive(Debug, PartialEq, Eq)]
    struct Item {
        id: u8,
        priority: i32,
    }
    fn item(id: u8) -> Item {
        Item {
            id,
            priority: (i32::from(id) * 37) % 7 - 3,
        }
    }
    struct Step {
        action: u8,
        key: u8,
        capacity: usize,
        result: Option<u8>,
        heap: Vec<u8>,
        visited: Vec<u8>,
    }
    struct Case {
        initial: usize,
        descending: bool,
        from_collection: bool,
        shared: bool,
        steps: Vec<Step>,
    }
    fn cases() -> Vec<Case> {
        let data = include_bytes!("priority_queue.bin");
        assert_eq!(&data[..4], b"FLQ1");
        let mut pos = 4;
        let mut cases = Vec::<Case>::new();
        let short = |p| u16::from_le_bytes([data[p], data[p + 1]]) as usize;
        while pos < data.len() {
            if data[pos] == 255 {
                let flags = data[pos + 3];
                cases.push(Case {
                    initial: short(pos + 1),
                    descending: flags & 1 != 0,
                    from_collection: flags & 2 != 0,
                    shared: flags & 4 != 0,
                    steps: Vec::new(),
                });
                pos += 4;
            } else {
                let action = data[pos];
                let key = data[pos + 1];
                let capacity = short(pos + 2);
                let result = i16::from_le_bytes([data[pos + 4], data[pos + 5]]);
                let prefix = short(pos + 6);
                let removed = short(pos + 8);
                let inserted = short(pos + 10);
                let visited_len = short(pos + 12);
                pos += 14;
                let case = cases.last_mut().unwrap();
                let mut heap = case.steps.last().map_or_else(Vec::new, |s| s.heap.clone());
                heap.splice(
                    prefix..prefix + removed,
                    data[pos..pos + inserted].iter().copied(),
                );
                pos += inserted;
                let visited = data[pos..pos + visited_len].to_vec();
                pos += visited_len;
                case.steps.push(Step {
                    action,
                    key,
                    capacity,
                    result: if result < 0 { None } else { Some(result as u8) },
                    heap,
                    visited,
                });
            }
        }
        assert_eq!(cases.len(), 24);
        for case in &cases {
            assert_eq!(case.steps.len(), 1601);
        }
        cases
    }
    fn run<T: PartialEq + Debug, C: Fn(&T, &T) -> Ordering>(
        case: &Case,
        make: impl Fn(u8) -> T,
        id: fn(&T) -> u8,
        compare: C,
        cursor: for<'a> fn(&'a mut JavaPriorityQueue<T, C>) -> PriorityCursor<'a, T, C>,
    ) {
        let mut queue = if case.from_collection {
            JavaPriorityQueue::from_collection(
                case.steps[0].visited.iter().copied().map(&make),
                compare,
            )
        } else {
            JavaPriorityQueue::with_capacity(case.initial, compare)
        };
        for (step_index, step) in case.steps.iter().enumerate() {
            let mut visited = Vec::new();
            let result = match step.action {
                0 => {
                    visited = step.visited.clone();
                    None
                }
                1 => {
                    assert!(queue.offer(make(step.key)));
                    None
                }
                2 => queue.poll().as_ref().map(id),
                3 => queue.remove(&make(step.key)).then_some(step.key),
                4 => {
                    queue.clear();
                    None
                }
                5 => {
                    let mut it = cursor(&mut queue);
                    let mut count = 0;
                    while let Some(value) = it.next_entry() {
                        visited.push(id(value));
                        if count % 3 == usize::from(step.key) % 3 {
                            it.remove();
                        }
                        count += 1;
                    }
                    None
                }
                6 => {
                    let len = queue.len();
                    if len == 0 {
                        None
                    } else {
                        queue
                            .remove_at(usize::from(step.key) % len)
                            .as_ref()
                            .map(id)
                    }
                }
                7 => queue.peek().map(id),
                other => panic!("unexpected op {other}"),
            };
            let context = format!(
                "initial={} descending={} heapify={} shared={} step={step_index}",
                case.initial, case.descending, case.from_collection, case.shared
            );
            assert_eq!(result, step.result, "return {context}");
            assert_eq!(visited, step.visited, "Iterator.remove visits {context}");
            assert_eq!(queue.capacity(), step.capacity, "growth {context}");
            assert_eq!(queue.len(), step.heap.len(), "size {context}");
            assert_eq!(
                queue.iter().map(id).collect::<Vec<_>>(),
                step.heap,
                "array order {context}"
            );
            assert_eq!(
                queue.peek().map(id),
                step.heap.first().copied(),
                "peek {context}"
            );
        }
        let expected = queue.iter().map(id).collect::<Vec<_>>();
        assert_eq!((&queue).into_iter().map(id).collect::<Vec<_>>(), expected);
        assert_eq!(
            queue.into_iter().map(|v| id(&v)).collect::<Vec<_>>(),
            expected
        );
    }
    #[test]
    fn owned_heap_order_growth_heapify_and_iterator_removal_match_jdk() {
        for case in cases().iter().filter(|c| !c.shared) {
            let compare = |a: &Item, b: &Item| {
                if case.descending {
                    b.priority.cmp(&a.priority)
                } else {
                    a.priority.cmp(&b.priority)
                }
            };
            run(case, item, |v| v.id, compare, JavaPriorityQueue::cursor);
        }
    }
    #[test]
    fn repeated_reference_iterator_remove_eq_matches_jdk() {
        let pool = (0..192).map(|id| Rc::new(item(id))).collect::<Vec<_>>();
        for case in cases().iter().filter(|c| c.shared) {
            let compare = |a: &Rc<Item>, b: &Rc<Item>| {
                if case.descending {
                    b.priority.cmp(&a.priority)
                } else {
                    a.priority.cmp(&b.priority)
                }
            };
            run(
                case,
                |id| pool[id as usize].clone(),
                |v| v.id,
                compare,
                JavaPriorityQueue::reference_cursor,
            );
        }
    }
}
