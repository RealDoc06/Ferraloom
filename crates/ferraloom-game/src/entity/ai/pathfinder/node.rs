use super::PathType;
use ferraloom_util::{BlockPos, Vec3, collections::JavaHash};
use std::ops::{Index, IndexMut};

/// Identity of one Node in the owning PathData. It is never a coordinate hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(usize);
/// Identity of the mutable node list shared by Path#copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathNodesId(usize);
/// Value equality/hash used by Java maps of Node/Target (distinct from identity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeKey(pub BlockPos);
impl JavaHash for NodeKey {
    fn java_hash_code(&self) -> i32 {
        Node::create_hash(self.0.x, self.0.y, self.0.z)
    }
}
/// Port of Node. Float fields retain Java float arithmetic and bit patterns.
#[derive(Debug)]
pub struct Node {
    pub pos: BlockPos,
    pub heap_idx: i32,
    pub g: f32,
    pub h: f32,
    pub f: f32,
    pub came_from: Option<NodeId>,
    pub closed: bool,
    pub walked_distance: f32,
    pub cost_malus: f32,
    pub path_type: PathType,
}
impl Node {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self {
            pos: BlockPos::new(x, y, z),
            heap_idx: -1,
            g: 0.0,
            h: 0.0,
            f: 0.0,
            came_from: None,
            closed: false,
            walked_distance: 0.0,
            cost_malus: 0.0,
            path_type: PathType::Blocked,
        }
    }
    pub fn create_hash(x: i32, y: i32, z: i32) -> i32 {
        (y & 255)
            | ((x & 32767) << 8)
            | ((z & 32767).wrapping_shl(24))
            | if x < 0 { i32::MIN } else { 0 }
            | if z < 0 { 32768 } else { 0 }
    }
    pub fn key(&self) -> NodeKey {
        NodeKey(self.pos)
    }
    pub fn in_open_set(&self) -> bool {
        self.heap_idx >= 0
    }
    pub fn as_vec3(&self) -> Vec3 {
        Vec3::new(self.pos.x as f64, self.pos.y as f64, self.pos.z as f64)
    }
    pub fn distance_to_sqr(&self, to: BlockPos) -> f32 {
        let x = to.x.wrapping_sub(self.pos.x) as f32;
        let y = to.y.wrapping_sub(self.pos.y) as f32;
        let z = to.z.wrapping_sub(self.pos.z) as f32;
        x * x + y * y + z * z
    }
    pub fn distance_to(&self, to: BlockPos) -> f32 {
        self.distance_to_sqr(to).sqrt()
    }
    pub fn distance_to_xz(&self, to: BlockPos) -> f32 {
        let x = to.x.wrapping_sub(self.pos.x) as f32;
        let z = to.z.wrapping_sub(self.pos.z) as f32;
        (x * x + z * z).sqrt()
    }
    pub fn distance_manhattan(&self, to: BlockPos) -> f32 {
        // Math.abs(int) deliberately leaves MIN_VALUE negative before float conversion.
        to.x.wrapping_sub(self.pos.x).wrapping_abs() as f32
            + to.y.wrapping_sub(self.pos.y).wrapping_abs() as f32
            + to.z.wrapping_sub(self.pos.z).wrapping_abs() as f32
    }
    /// Port of Node#cloneAndMove: new identity, copied search links and metadata.
    pub fn clone_and_move(&self, x: i32, y: i32, z: i32) -> Self {
        Self {
            pos: BlockPos::new(x, y, z),
            heap_idx: self.heap_idx,
            g: self.g,
            h: self.h,
            f: self.f,
            came_from: self.came_from,
            closed: self.closed,
            walked_distance: self.walked_distance,
            cost_malus: self.cost_malus,
            path_type: self.path_type,
        }
    }
}
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.pos == other.pos
    }
}
impl Eq for Node {}
impl JavaHash for Node {
    fn java_hash_code(&self) -> i32 {
        self.key().java_hash_code()
    }
}
/// Explicit owner of search nodes and shared mutable path lists. Keep this owner
/// with its search result; copied Path cursors access the same arena.
#[derive(Debug, Default)]
pub struct PathData {
    nodes: Vec<Node>,
    lists: Vec<Vec<NodeId>>,
}
impl PathData {
    pub fn add_node(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().enumerate().map(|(i, n)| (NodeId(i), n))
    }
    pub fn add_nodes(&mut self, nodes: Vec<NodeId>) -> PathNodesId {
        let id = PathNodesId(self.lists.len());
        self.lists.push(nodes);
        id
    }
    pub fn path_nodes(&self, id: PathNodesId) -> &[NodeId] {
        &self.lists[id.0]
    }
    pub(super) fn path_nodes_mut(&mut self, id: PathNodesId) -> &mut Vec<NodeId> {
        &mut self.lists[id.0]
    }
}
impl Index<NodeId> for PathData {
    type Output = Node;
    fn index(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }
}
impl IndexMut<NodeId> for PathData {
    fn index_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0]
    }
}
