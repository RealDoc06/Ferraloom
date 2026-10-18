use super::{NodeId, PathData};
/// Port of BinaryHeap. Equal children choose the right child during downHeap.
#[derive(Debug)]
pub struct BinaryHeap {
    heap: Vec<NodeId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapError {
    AlreadyInHeap,
}
impl Default for BinaryHeap {
    fn default() -> Self {
        Self {
            heap: Vec::with_capacity(128),
        }
    }
}
impl BinaryHeap {
    pub fn new() -> Self {
        Self::default()
    }
    /// Native clear changes only size; stale Node.heapIdx values remain intact.
    pub fn clear(&mut self) {
        self.heap.clear();
    }
    pub fn size(&self) -> usize {
        self.heap.len()
    }
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
    pub fn get_heap(&self) -> &[NodeId] {
        &self.heap
    }
    pub fn peek(&self) -> NodeId {
        self.heap[0]
    }
    pub fn insert(&mut self, node: NodeId, data: &mut PathData) -> Result<NodeId, HeapError> {
        if data[node].heap_idx >= 0 {
            return Err(HeapError::AlreadyInHeap);
        }
        data[node].heap_idx = self.heap.len() as i32;
        self.heap.push(node);
        self.up_heap(self.heap.len() - 1, data);
        Ok(node)
    }
    pub fn pop(&mut self, data: &mut PathData) -> NodeId {
        let node = self.heap[0];
        let last = self.heap.pop().expect("empty BinaryHeap");
        if !self.heap.is_empty() {
            self.heap[0] = last;
            self.down_heap(0, data);
        }
        data[node].heap_idx = -1;
        node
    }
    pub fn remove(&mut self, node: NodeId, data: &mut PathData) {
        let index = data[node].heap_idx as usize;
        let last = self.heap.pop().expect("empty BinaryHeap");
        if index < self.heap.len() {
            self.heap[index] = last;
            if data[last].f < data[node].f {
                self.up_heap(index, data);
            } else {
                self.down_heap(index, data);
            }
        }
        data[node].heap_idx = -1;
    }
    pub fn change_cost(&mut self, node: NodeId, cost: f32, data: &mut PathData) {
        let old = data[node].f;
        data[node].f = cost;
        let index = data[node].heap_idx as usize;
        if cost < old {
            self.up_heap(index, data);
        } else {
            self.down_heap(index, data);
        }
    }
    fn up_heap(&mut self, mut index: usize, data: &mut PathData) {
        let node = self.heap[index];
        let cost = data[node].f;
        while index > 0 {
            let parent = (index - 1) >> 1;
            let other = self.heap[parent];
            if cost.partial_cmp(&data[other].f) != Some(std::cmp::Ordering::Less) {
                break;
            }
            self.heap[index] = other;
            data[other].heap_idx = index as i32;
            index = parent;
        }
        self.heap[index] = node;
        data[node].heap_idx = index as i32;
    }
    fn down_heap(&mut self, mut index: usize, data: &mut PathData) {
        let node = self.heap[index];
        let cost = data[node].f;
        loop {
            let left = 1 + (index << 1);
            if left >= self.heap.len() {
                break;
            }
            let right = left + 1;
            let left_cost = data[self.heap[left]].f;
            let right_cost = if right < self.heap.len() {
                data[self.heap[right]].f
            } else {
                f32::INFINITY
            };
            let child = if left_cost < right_cost { left } else { right };
            let child_cost = if child == left { left_cost } else { right_cost };
            if child_cost.partial_cmp(&cost) != Some(std::cmp::Ordering::Less) {
                break;
            }
            let other = self.heap[child];
            self.heap[index] = other;
            data[other].heap_idx = index as i32;
            index = child;
        }
        self.heap[index] = node;
        data[node].heap_idx = index as i32;
    }
}
