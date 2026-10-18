use super::{BinaryHeap, DebugData, HeapError, NodeId, Path, PathData, Target};
use ferraloom_util::{
    BlockPos,
    collections::{HashMap, HashSet},
};

/// Port of NodeEvaluator's search-facing virtual methods. Concrete evaluators
/// own their prepared world/mob context; this core has no fabricated Mob facts.
pub trait NodeEvaluator {
    type Error;
    fn prepare(&mut self, data: &mut PathData) -> Result<(), Self::Error>;
    fn done(&mut self, data: &mut PathData) -> Result<(), Self::Error>;
    fn get_start(&mut self, data: &mut PathData) -> Result<Option<NodeId>, Self::Error>;
    fn get_target(&mut self, pos: BlockPos, data: &mut PathData) -> Result<Target, Self::Error>;
    fn get_neighbors(
        &mut self,
        node: NodeId,
        neighbors: &mut Vec<NodeId>,
        data: &mut PathData,
    ) -> Result<(), Self::Error>;
}
/// PathFinder#distance is overridden by Warden's navigation, while getBestH
/// continues to use the ordinary three-dimensional Euclidean distance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DistanceMetric {
    #[default]
    Euclidean,
    Horizontal,
}
#[derive(Debug, PartialEq, Eq)]
pub enum SearchError<E> {
    Evaluator(E),
    Heap(HeapError),
    DuplicateTarget,
}
/// Port of PathFinder. The caller owns PathData alongside every returned Path.
#[derive(Debug)]
pub struct PathFinder {
    max_visited_nodes: i32,
    open_set: BinaryHeap,
    capture_debug: bool,
    pub distance_metric: DistanceMetric,
}
impl PathFinder {
    pub fn new(max_visited_nodes: i32) -> Self {
        Self {
            max_visited_nodes,
            open_set: BinaryHeap::new(),
            capture_debug: false,
            distance_metric: DistanceMetric::Euclidean,
        }
    }
    pub fn set_max_visited_nodes(&mut self, max: i32) {
        self.max_visited_nodes = max;
    }
    pub fn set_capture_debug(&mut self, capture: bool) {
        self.capture_debug = capture;
    }
    /// Targets use Java set/map traversal. Supplied nodes and all returned path
    /// lists belong to data, which must outlive this result and its cursor copies.
    pub fn find_path<E: NodeEvaluator>(
        &mut self,
        evaluator: &mut E,
        data: &mut PathData,
        positions: &HashSet<BlockPos>,
        max_path_length: f32,
        reach_range: i32,
        multiplier: f32,
    ) -> Result<Option<Path>, SearchError<E::Error>> {
        self.open_set.clear();
        evaluator.prepare(data).map_err(SearchError::Evaluator)?;
        let Some(from) = evaluator.get_start(data).map_err(SearchError::Evaluator)? else {
            return Ok(None);
        };
        let mut targets = Vec::with_capacity(positions.len());
        let mut map = HashMap::new();
        for pos in positions.iter() {
            let target = evaluator
                .get_target(*pos, data)
                .map_err(SearchError::Evaluator)?;
            let key = data[target.node].key();
            if map.insert(key, targets.len()).is_some() {
                return Err(SearchError::DuplicateTarget);
            }
            targets.push((target, *pos));
        }
        let order: Vec<usize> = map.values().copied().collect();
        data[from].g = 0.0;
        data[from].h = best_h(from, &order, &mut targets, data);
        data[from].f = data[from].h;
        self.open_set.clear();
        self.open_set
            .insert(from, data)
            .map_err(SearchError::Heap)?;
        let mut closed = HashMap::new();
        // Guava Maps.capacity, used by Sets.newHashSetWithExpectedSize.
        let capacity = if targets.len() < 3 {
            targets.len() + 1
        } else {
            ((targets.len() as f32) / 0.75 + 1.0) as usize
        };
        let mut reached = HashSet::with_capacity(capacity);
        let limit = (self.max_visited_nodes as f32 * multiplier) as i32;
        let mut count = 0i32;
        let mut neighbors = Vec::with_capacity(32);
        while !self.open_set.is_empty() {
            count = count.wrapping_add(1);
            if count >= limit {
                break;
            }
            let current = self.open_set.pop(data);
            data[current].closed = true;
            for &index in &order {
                let target = &mut targets[index].0;
                if data[current].distance_manhattan(data[target.node].pos) <= reach_range as f32 {
                    target.set_reached();
                    reached.insert(data[target.node].key());
                }
            }
            if !reached.is_empty() {
                break;
            }
            if self.capture_debug && !closed.contains_key(&data[current].key()) {
                closed.insert(data[current].key(), current);
            }
            if data[current].distance_to(data[from].pos) >= max_path_length {
                continue;
            }
            neighbors.clear();
            evaluator
                .get_neighbors(current, &mut neighbors, data)
                .map_err(SearchError::Evaluator)?;
            assert!(
                neighbors.len() <= 32,
                "NodeEvaluator returned more than 32 neighbors"
            );
            for &neighbor in &neighbors {
                let distance = match self.distance_metric {
                    DistanceMetric::Euclidean => data[current].distance_to(data[neighbor].pos),
                    DistanceMetric::Horizontal => data[current].distance_to_xz(data[neighbor].pos),
                };
                data[neighbor].walked_distance = data[current].walked_distance + distance;
                let tentative = data[current].g + distance + data[neighbor].cost_malus;
                if data[neighbor].walked_distance < max_path_length
                    && (!data[neighbor].in_open_set() || tentative < data[neighbor].g)
                {
                    data[neighbor].came_from = Some(current);
                    data[neighbor].g = tentative;
                    data[neighbor].h = best_h(neighbor, &order, &mut targets, data) * 1.5;
                    let cost = data[neighbor].g + data[neighbor].h;
                    if data[neighbor].in_open_set() {
                        self.open_set.change_cost(neighbor, cost, data);
                    } else {
                        data[neighbor].f = cost;
                        self.open_set
                            .insert(neighbor, data)
                            .map_err(SearchError::Heap)?;
                    }
                }
            }
        }
        let did_reach = !reached.is_empty();
        let candidates: Vec<usize> = if did_reach {
            reached.iter().map(|key| *map.peek(key).unwrap()).collect()
        } else {
            order.clone()
        };
        let mut best: Option<Path> = None;
        for index in candidates {
            let (target, pos) = &targets[index];
            let mut id = target
                .best_node()
                .expect("target without a finite heuristic");
            let mut nodes = vec![id];
            while let Some(previous) = data[id].came_from {
                nodes.push(previous);
                id = previous;
            }
            nodes.reverse();
            let path = Path::new(nodes, *pos, did_reach, data);
            if best.as_ref().is_none_or(|old| {
                if did_reach {
                    path.node_count(data) < old.node_count(data)
                } else {
                    path.dist_to_target()
                        .total_cmp(&old.dist_to_target())
                        .then_with(|| path.node_count(data).cmp(&old.node_count(data)))
                        .is_lt()
                }
            }) {
                best = Some(path);
            }
        }
        if self.capture_debug
            && let Some(path) = &mut best
        {
            let closed_nodes = closed.values().copied().collect();
            let mut target_states: Vec<_> = targets
                .into_iter()
                .map(|(target, _)| Some(target))
                .collect();
            path.set_debug(DebugData {
                open_set: self.open_set.get_heap().to_vec(),
                closed_set: closed_nodes,
                targets: order
                    .iter()
                    .map(|&index| target_states[index].take().unwrap())
                    .collect(),
            });
        }
        evaluator.done(data).map_err(SearchError::Evaluator)?;
        Ok(best)
    }
}
fn best_h(
    from: NodeId,
    order: &[usize],
    targets: &mut [(Target, BlockPos)],
    data: &PathData,
) -> f32 {
    let mut best = f32::MAX;
    for &index in order {
        let target = &mut targets[index].0;
        let h = data[from].distance_to(data[target.node].pos);
        target.update_best(h, from);
        best = best.min(h);
    }
    best
}
