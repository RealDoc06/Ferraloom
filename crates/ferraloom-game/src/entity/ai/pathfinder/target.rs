use super::{Node, NodeId, PathData};
/// Port of Target. An equal heuristic retains the earlier best node.
#[derive(Debug)]
pub struct Target {
    pub node: NodeId,
    best_heuristic: f32,
    best_node: Option<NodeId>,
    reached: bool,
}
impl Target {
    pub fn new(node: NodeId, data: &mut PathData) -> Self {
        let pos = data[node].pos;
        let node = data.add_node(Node::new(pos.x, pos.y, pos.z));
        Self::at(node)
    }
    pub fn at(node: NodeId) -> Self {
        Self {
            node,
            best_heuristic: f32::MAX,
            best_node: None,
            reached: false,
        }
    }
    pub fn update_best(&mut self, heuristic: f32, node: NodeId) {
        if heuristic < self.best_heuristic {
            self.best_heuristic = heuristic;
            self.best_node = Some(node);
        }
    }
    pub fn best_node(&self) -> Option<NodeId> {
        self.best_node
    }
    pub fn best_heuristic(&self) -> f32 {
        self.best_heuristic
    }
    pub fn set_reached(&mut self) {
        self.reached = true;
    }
    pub fn is_reached(&self) -> bool {
        self.reached
    }
}
