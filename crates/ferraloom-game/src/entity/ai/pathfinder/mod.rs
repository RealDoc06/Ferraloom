//! Ports of world.level.pathfinder. Search state is owned by an explicit arena;
//! handles preserve Java node/list identity without interior mutability.
mod binary_heap;
mod debug_codec;
mod node;
mod path;
mod path_computation_type;
mod path_finder;
mod path_type;
mod path_type_cache;
pub mod pathfinding_context;
mod target;
pub mod walk_node_evaluator;
pub use binary_heap::{BinaryHeap, HeapError};
pub use node::{Node, NodeId, NodeKey, PathData, PathNodesId};
pub use path::{DebugData, Path};
pub use path_computation_type::PathComputationType;
pub use path_finder::{DistanceMetric, NodeEvaluator, PathFinder, SearchError};
pub use path_type::PathType;
pub use path_type_cache::PathTypeCache;
pub use target::Target;
#[cfg(test)]
mod classification_tests;
#[cfg(test)]
mod pathfindability_tests;
#[cfg(test)]
mod tests;

mod mob_access;
pub use mob_access::{Capabilities, MobAccess};
pub mod amphibious_node_evaluator;
pub mod fly_node_evaluator;
pub mod swim_node_evaluator;
pub use amphibious_node_evaluator::AmphibiousNodeEvaluator;
pub use fly_node_evaluator::FlyNodeEvaluator;
pub use swim_node_evaluator::SwimNodeEvaluator;
pub use walk_node_evaluator::WalkNodeEvaluator;

#[cfg(test)]
mod evaluator_tests;
