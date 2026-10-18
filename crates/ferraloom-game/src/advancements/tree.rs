use super::{AdvancementHolder, Identifier};
use std::collections::BTreeMap;

/// Port of AdvancementNode. Parent and child links retain object identity even
/// after map replacement/removal. Identity-set traversal uses insertion order,
/// following ferraloom-util's ReferenceOpenHashSet policy.
#[derive(Clone, Debug)]
pub struct AdvancementNode {
    pub holder: AdvancementHolder,
    pub parent: Option<Identifier>,
    pub children: Vec<Identifier>,
    pub x: f32,
    pub y: f32,
    parent_node: Option<usize>,
    child_nodes: Vec<usize>,
}
/// Port of AdvancementTree's linked node map and parent-resolution passes.
/// The arena retains old nodes while their parent/child links still reach them.
#[derive(Clone, Debug, Default)]
pub struct AdvancementTree {
    nodes: BTreeMap<Identifier, usize>,
    order: Vec<Identifier>,
    arena: Vec<AdvancementNode>,
}
impl AdvancementTree {
    pub fn get(&self, id: &Identifier) -> Option<&AdvancementNode> {
        self.nodes.get(id).map(|&index| &self.arena[index])
    }
    pub fn nodes(&self) -> impl Iterator<Item = &AdvancementNode> {
        self.order.iter().filter_map(|id| self.get(id))
    }
    pub fn roots(&self) -> impl Iterator<Item = &AdvancementNode> {
        self.nodes().filter(|node| node.parent_node.is_none())
    }
    fn root_index(&self, mut index: usize) -> usize {
        while let Some(parent) = self.arena[index].parent_node {
            index = parent;
        }
        index
    }
    pub fn root(&self, id: &Identifier) -> Option<&AdvancementNode> {
        Some(&self.arena[self.root_index(*self.nodes.get(id)?)])
    }
    /// Returns unresolved entries for the caller's reload diagnostic.
    pub fn add_all(&mut self, holders: &[AdvancementHolder]) -> Vec<Identifier> {
        let mut pending: Vec<_> = holders.iter().collect();
        loop {
            let before = pending.len();
            pending.retain(|holder| {
                let parent = match &holder.value.parent {
                    Some(id) => match self.nodes.get(id) {
                        Some(&index) => Some(index),
                        None => return true,
                    },
                    None => None,
                };
                let index = self.arena.len();
                self.arena.push(AdvancementNode {
                    holder: (*holder).clone(),
                    parent: holder.value.parent.clone(),
                    children: Vec::new(),
                    x: 0.0,
                    y: 0.0,
                    parent_node: parent,
                    child_nodes: Vec::new(),
                });
                if let Some(parent) = parent {
                    self.arena[parent].children.push(holder.id.clone());
                    self.arena[parent].child_nodes.push(index);
                }
                if !self.nodes.contains_key(&holder.id) {
                    self.order.push(holder.id.clone());
                }
                self.nodes.insert(holder.id.clone(), index);
                false
            });
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }
        pending
            .into_iter()
            .map(|holder| holder.id.clone())
            .collect()
    }
    /// Removed nodes remain linked from their retained parents, as in Java.
    pub fn remove(&mut self, ids: &[Identifier]) {
        for id in ids {
            if let Some(&index) = self.nodes.get(id) {
                self.remove_node(index);
            }
        }
    }
    fn remove_node(&mut self, index: usize) {
        for child in self.arena[index].child_nodes.clone() {
            self.remove_node(child);
        }
        let id = &self.arena[index].holder.id;
        self.nodes.remove(id);
        self.order.retain(|key| key != id);
    }
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.order.clear();
        self.arena.clear();
    }
    pub fn reposition_nodes(&mut self) {
        let roots: Vec<_> = self
            .order
            .iter()
            .filter_map(|id| self.nodes.get(id).copied())
            .filter(|&index| {
                self.arena[index].parent_node.is_none()
                    && self.arena[index].holder.value.display.is_some()
            })
            .collect();
        for root in roots {
            TreeNodePosition::run(self, root);
        }
    }
    /// Replay captured vanilla identity-set traversal. Repeated identifiers retain
    /// their relative identity order; the supplied multiset must match exactly.
    pub fn set_child_order(&mut self, id: &Identifier, order: Vec<Identifier>) -> bool {
        let Some(&index) = self.nodes.get(id) else {
            return false;
        };
        let node = &mut self.arena[index];
        let mut a = node.children.clone();
        a.sort();
        let mut b = order.clone();
        b.sort();
        if a != b {
            return false;
        }
        let mut remaining: Vec<_> = node
            .children
            .iter()
            .cloned()
            .zip(node.child_nodes.iter().copied())
            .collect();
        node.child_nodes = order
            .iter()
            .map(|id| {
                let index = remaining
                    .iter()
                    .position(|(key, _)| key == id)
                    .expect("validated multiset");
                remaining.remove(index).1
            })
            .collect();
        node.children = order;
        true
    }
    /// Port of AdvancementVisibilityEvaluator. Completed descendants expose their
    /// ancestors even through hidden/displayless nodes; output is postorder.
    pub fn evaluate_visibility(
        &self,
        root: &Identifier,
        done: impl Fn(&Identifier) -> bool,
    ) -> Vec<(Identifier, bool)> {
        fn walk(
            tree: &AdvancementTree,
            index: usize,
            stack: &mut Vec<i8>,
            done: &impl Fn(&Identifier) -> bool,
            output: &mut Vec<(Identifier, bool)>,
        ) -> bool {
            let node = &tree.arena[index];
            let id = &node.holder.id;
            let is_done = done(id);
            let rule = match &node.holder.value.display {
                None => -1,
                Some(_) if is_done => 1,
                Some(display) if display.hidden => -1,
                Some(_) => 0,
            };
            stack.push(rule);
            let mut descendant_done = is_done;
            for &child in &node.child_nodes {
                descendant_done |= walk(tree, child, stack, done, output);
            }
            let inherited = stack
                .iter()
                .rev()
                .take(3)
                .find(|&&rule| rule != 0)
                .is_some_and(|&rule| rule == 1);
            stack.pop();
            output.push((id.clone(), descendant_done || inherited));
            descendant_done
        }
        let mut output = Vec::new();
        if let Some(&index) = self.nodes.get(root) {
            walk(
                self,
                self.root_index(index),
                &mut vec![0; 3],
                &done,
                &mut output,
            );
        }
        output
    }
}

/// Port of TreeNodePosition's tidy-tree layout, using arena indices for threads.
struct TreeNodePosition {
    node: usize,
    parent: Option<usize>,
    previous: Option<usize>,
    index: usize,
    children: Vec<usize>,
    ancestor: usize,
    thread: Option<usize>,
    x: usize,
    y: f32,
    modifier: f32,
    change: f32,
    shift: f32,
}
impl TreeNodePosition {
    fn run(tree: &mut AdvancementTree, root: usize) {
        let mut arena = Vec::new();
        Self::build(tree, root, None, None, 1, 0, &mut arena);
        Self::first(&mut arena, 0);
        let y = arena[0].y;
        let min = Self::second(&mut arena, 0, 0.0, 0, y);
        for position in arena {
            let node = &mut tree.arena[position.node];
            node.x = position.x as f32;
            node.y = position.y + if min < 0.0 { -min } else { 0.0 };
        }
    }
    fn build(
        tree: &AdvancementTree,
        node: usize,
        parent: Option<usize>,
        previous: Option<usize>,
        index: usize,
        x: usize,
        arena: &mut Vec<Self>,
    ) -> usize {
        let i = arena.len();
        arena.push(Self {
            node,
            parent,
            previous,
            index,
            x,
            children: Vec::new(),
            ancestor: i,
            thread: None,
            y: -1.0,
            modifier: 0.0,
            change: 0.0,
            shift: 0.0,
        });
        fn add(
            tree: &AdvancementTree,
            node: usize,
            parent: usize,
            arena: &mut Vec<TreeNodePosition>,
        ) {
            if tree.arena[node].holder.value.display.is_some() {
                let child = TreeNodePosition::build(
                    tree,
                    node,
                    Some(parent),
                    arena[parent].children.last().copied(),
                    arena[parent].children.len() + 1,
                    arena[parent].x + 1,
                    arena,
                );
                arena[parent].children.push(child);
            } else {
                for &child in &tree.arena[node].child_nodes {
                    add(tree, child, parent, arena);
                }
            }
        }
        for &child in &tree.arena[node].child_nodes {
            add(tree, child, i, arena);
        }
        i
    }
    fn first(a: &mut [Self], i: usize) {
        if a[i].children.is_empty() {
            a[i].y = a[i].previous.map_or(0.0, |p| a[p].y + 1.0);
            return;
        }
        let mut ancestor = None;
        for child in a[i].children.clone() {
            Self::first(a, child);
            ancestor = Some(Self::apportion(a, child, ancestor.unwrap_or(child)));
        }
        let mut shift = 0.0;
        let mut change = 0.0;
        for child in a[i].children.clone().into_iter().rev() {
            a[child].y += shift;
            a[child].modifier += shift;
            change += a[child].change;
            shift += a[child].shift + change;
        }
        let first = a[i].children[0];
        let last = *a[i].children.last().expect("children");
        let midpoint = (a[first].y + a[last].y) / 2.0;
        if let Some(previous) = a[i].previous {
            a[i].y = a[previous].y + 1.0;
            a[i].modifier = a[i].y - midpoint;
        } else {
            a[i].y = midpoint;
        }
    }
    fn left(&self) -> Option<usize> {
        self.thread.or_else(|| self.children.first().copied())
    }
    fn right(&self) -> Option<usize> {
        self.thread.or_else(|| self.children.last().copied())
    }
    fn apportion(a: &mut [Self], i: usize, mut ancestor: usize) -> usize {
        let Some(mut inside_left) = a[i].previous else {
            return ancestor;
        };
        let mut inside_right = i;
        let mut outside_right = i;
        let parent = a[i].parent.expect("sibling parent");
        let mut outside_left = a[parent].children[0];
        let mut sir = a[i].modifier;
        let mut sor = sir;
        let mut sil = a[inside_left].modifier;
        let mut sol = a[outside_left].modifier;
        while let (Some(left), Some(right)) = (a[inside_left].right(), a[inside_right].left()) {
            inside_left = left;
            inside_right = right;
            outside_left = a[outside_left].left().expect("left contour");
            outside_right = a[outside_right].right().expect("right contour");
            a[outside_right].ancestor = i;
            let shift = a[inside_left].y + sil - (a[inside_right].y + sir) + 1.0;
            if shift > 0.0 {
                let candidate = a[inside_left].ancestor;
                let left_ancestor = if a[parent].children.contains(&candidate) {
                    candidate
                } else {
                    ancestor
                };
                let count = (a[i].index as i32 - a[left_ancestor].index as i32) as f32;
                if count != 0.0 {
                    a[i].change -= shift / count;
                    a[left_ancestor].change += shift / count;
                }
                a[i].shift += shift;
                a[i].y += shift;
                a[i].modifier += shift;
                sir += shift;
                sor += shift;
            }
            sil += a[inside_left].modifier;
            sir += a[inside_right].modifier;
            sol += a[outside_left].modifier;
            sor += a[outside_right].modifier;
        }
        if a[inside_left].right().is_some() && a[outside_right].right().is_none() {
            a[outside_right].thread = a[inside_left].right();
            a[outside_right].modifier += sil - sor;
        } else {
            if a[inside_right].left().is_some() && a[outside_left].left().is_none() {
                a[outside_left].thread = a[inside_right].left();
                a[outside_left].modifier += sir - sol;
            }
            ancestor = i;
        }
        ancestor
    }
    fn second(a: &mut [Self], i: usize, sum: f32, depth: usize, mut min: f32) -> f32 {
        a[i].y += sum;
        a[i].x = depth;
        min = min.min(a[i].y);
        for child in a[i].children.clone() {
            min = Self::second(a, child, sum + a[i].modifier, depth + 1, min);
        }
        min
    }
}
