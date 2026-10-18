use super::{NodeId, PathData};
/// Port of BinaryHeap. The backing slots survive clear, and equal children
/// choose the right child during downHeap. This is not a total float ordering.
#[derive(Debug)]
pub struct BinaryHeap {
    heap: Vec<Option<NodeId>>,
    size: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeapError {
    AlreadyInHeap,
}
impl Default for BinaryHeap {
    fn default() -> Self {
        Self {
            heap: vec![None; 128],
            size: 0,
        }
    }
}
impl BinaryHeap {
    pub fn new() -> Self {
        Self::default()
    }
    /// Native clear changes only size, preserving slots and Node.heapIdx.
    pub fn clear(&mut self) {
        self.size = 0;
    }
    pub fn size(&self) -> usize {
        self.size
    }
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
    /// Port of getHeap: a copy of the active array prefix, sharing node identity.
    pub fn get_heap(&self) -> Vec<NodeId> {
        self.heap[..self.size]
            .iter()
            .map(|id| id.expect("null heap slot"))
            .collect()
    }
    pub fn peek(&self) -> Option<NodeId> {
        self.heap[0]
    }
    pub fn insert(&mut self, node: NodeId, data: &mut PathData) -> Result<NodeId, HeapError> {
        if data[node].heap_idx >= 0 {
            return Err(HeapError::AlreadyInHeap);
        }
        if self.size == self.heap.len() {
            self.heap.resize(self.size * 2, None);
        }
        self.heap[self.size] = Some(node);
        data[node].heap_idx = self.size as i32;
        let index = self.size;
        self.size += 1;
        self.up_heap(index, data);
        Ok(node)
    }
    pub fn pop(&mut self, data: &mut PathData) -> NodeId {
        let node = self.at(0);
        self.size = self.size.checked_sub(1).expect("empty BinaryHeap");
        self.heap[0] = self.heap[self.size];
        self.heap[self.size] = None;
        if self.size > 0 {
            self.down_heap(0, data);
        }
        data[node].heap_idx = -1;
        node
    }
    pub fn remove(&mut self, node: NodeId, data: &mut PathData) {
        let index = data[node].heap_idx as usize;
        self.size = self.size.checked_sub(1).expect("empty BinaryHeap");
        self.heap[index] = self.heap[self.size];
        self.heap[self.size] = None;
        if self.size > index {
            if data[self.at(index)].f < data[node].f {
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
    fn at(&self, index: usize) -> NodeId {
        self.heap[index].expect("null heap slot")
    }
    fn up_heap(&mut self, mut index: usize, data: &mut PathData) {
        let node = self.at(index);
        let cost = data[node].f;
        while index > 0 {
            let parent = (index - 1) >> 1;
            let other = self.at(parent);
            if cost.partial_cmp(&data[other].f) != Some(std::cmp::Ordering::Less) {
                break;
            }
            self.heap[index] = Some(other);
            data[other].heap_idx = index as i32;
            index = parent;
        }
        self.heap[index] = Some(node);
        data[node].heap_idx = index as i32;
    }
    fn down_heap(&mut self, mut index: usize, data: &mut PathData) {
        let node = self.at(index);
        let cost = data[node].f;
        loop {
            let left = 1 + (index << 1);
            if left >= self.size {
                break;
            }
            let right = left + 1;
            let left_cost = data[self.at(left)].f;
            let right_cost = if right < self.size {
                data[self.at(right)].f
            } else {
                f32::INFINITY
            };
            let child = if left_cost < right_cost { left } else { right };
            let child_cost = if child == left { left_cost } else { right_cost };
            if child_cost.partial_cmp(&cost) != Some(std::cmp::Ordering::Less) {
                break;
            }
            let other = self.at(child);
            self.heap[index] = Some(other);
            data[other].heap_idx = index as i32;
            index = child;
        }
        self.heap[index] = Some(node);
        data[node].heap_idx = index as i32;
    }
}
