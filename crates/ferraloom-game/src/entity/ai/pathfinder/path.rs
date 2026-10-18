use super::{NodeId, PathData, PathNodesId};
use ferraloom_util::{BlockPos, Vec3, collections::JavaHash};
use std::sync::Arc;
/// Port of Path.DebugData. These are node identities in the same search owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DebugData {
    pub open_set: Vec<NodeId>,
    pub closed_set: Vec<NodeId>,
    pub targets: Vec<NodeId>,
}
/// Port of Path. Cursor copies share the mutable list identified by nodes.
#[derive(Debug)]
pub struct Path {
    nodes: PathNodesId,
    next_node_index: i32,
    target: BlockPos,
    dist_to_target: f32,
    reached: bool,
    debug_data: Option<Arc<DebugData>>,
}
impl Path {
    pub fn new(nodes: Vec<NodeId>, target: BlockPos, reached: bool, data: &mut PathData) -> Self {
        let dist_to_target = nodes
            .last()
            .map_or(f32::MAX, |id| data[*id].distance_manhattan(target));
        Self {
            nodes: data.add_nodes(nodes),
            next_node_index: 0,
            target,
            dist_to_target,
            reached,
            debug_data: None,
        }
    }
    pub fn nodes_id(&self) -> PathNodesId {
        self.nodes
    }
    pub fn nodes<'a>(&self, data: &'a PathData) -> &'a [NodeId] {
        data.path_nodes(self.nodes)
    }
    pub fn advance(&mut self) {
        self.next_node_index = self.next_node_index.wrapping_add(1);
    }
    pub fn not_started(&self) -> bool {
        self.next_node_index <= 0
    }
    pub fn is_done(&self, data: &PathData) -> bool {
        self.next_node_index >= self.node_count(data) as i32
    }
    pub fn end_node(&self, data: &PathData) -> Option<NodeId> {
        self.nodes(data).last().copied()
    }
    pub fn node(&self, index: usize, data: &PathData) -> NodeId {
        self.nodes(data)[index]
    }
    pub fn truncate_nodes(&self, index: usize, data: &mut PathData) {
        data.path_nodes_mut(self.nodes).truncate(index);
    }
    pub fn replace_node(&self, index: usize, node: NodeId, data: &mut PathData) {
        data.path_nodes_mut(self.nodes)[index] = node;
    }
    pub fn node_count(&self, data: &PathData) -> usize {
        self.nodes(data).len()
    }
    pub fn next_node_index(&self) -> i32 {
        self.next_node_index
    }
    pub fn set_next_node_index(&mut self, index: i32) {
        self.next_node_index = index;
    }
    pub fn node_pos(&self, index: usize, data: &PathData) -> BlockPos {
        data[self.node(index, data)].pos
    }
    pub fn entity_pos_at_node(&self, width: f32, index: usize, data: &PathData) -> Vec3 {
        let pos = self.node_pos(index, data);
        let offset = f64::from((width + 1.0) as i32) * 0.5;
        Vec3::new(
            f64::from(pos.x) + offset,
            f64::from(pos.y),
            f64::from(pos.z) + offset,
        )
    }
    pub fn next_entity_pos(&self, width: f32, data: &PathData) -> Vec3 {
        self.entity_pos_at_node(width, self.next_node_index as usize, data)
    }
    pub fn next_node(&self, data: &PathData) -> NodeId {
        self.node(self.next_node_index as usize, data)
    }
    pub fn previous_node(&self, data: &PathData) -> Option<NodeId> {
        (self.next_node_index > 0).then(|| self.node((self.next_node_index - 1) as usize, data))
    }
    pub fn same_as(&self, other: Option<&Self>, data: &PathData) -> bool {
        other.is_some_and(|path| {
            self.nodes(data).len() == path.nodes(data).len()
                && self
                    .nodes(data)
                    .iter()
                    .zip(path.nodes(data))
                    .all(|(a, b)| data[*a] == data[*b])
        })
    }
    pub fn equals(&self, other: &Self, data: &PathData) -> bool {
        let same_debug = match (&self.debug_data, &other.debug_data) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        self.next_node_index == other.next_node_index
            && same_debug
            && self.reached == other.reached
            && self.target == other.target
            && self.same_as(Some(other), data)
    }
    pub fn hash_code(&self, data: &PathData) -> i32 {
        let list_hash = self.nodes(data).iter().fold(1i32, |hash, id| {
            hash.wrapping_mul(31)
                .wrapping_add(data[*id].java_hash_code())
        });
        self.next_node_index
            .wrapping_add(list_hash.wrapping_mul(31))
    }
    pub fn target(&self) -> BlockPos {
        self.target
    }
    pub fn dist_to_target(&self) -> f32 {
        self.dist_to_target
    }
    pub fn can_reach(&self) -> bool {
        self.reached
    }
    pub fn set_debug(&mut self, debug: DebugData) {
        self.debug_data = Some(Arc::new(debug));
    }
    pub fn debug_data(&self) -> Option<&DebugData> {
        self.debug_data.as_deref()
    }
    /// Native copy aliases its mutable list, retaining the current cursor.
    pub fn copy(&self) -> Self {
        Self {
            nodes: self.nodes,
            next_node_index: self.next_node_index,
            target: self.target,
            dist_to_target: self.dist_to_target,
            reached: self.reached,
            debug_data: self.debug_data.clone(),
        }
    }
}
