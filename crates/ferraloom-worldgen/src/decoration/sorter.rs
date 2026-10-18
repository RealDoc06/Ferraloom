//! Port of FeatureSorter. Encounter-order indices identify canonical placed
//! features; edges also cross step boundaries. Sorted DFS is reversed once.
use crate::Error;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct StepFeatureData<F> {
    pub features: Vec<F>,
    pub indices: BTreeMap<F, usize>,
}
/// Sources must be supplied in BiomeSource.possibleBiomes encounter order.
/// F denotes canonical feature identity, never configuration equality.
pub fn build_features_per_step<F: Clone + Ord>(
    sources: &[Vec<Vec<F>>],
) -> Result<Vec<StepFeatureData<F>>, Error> {
    let mut feature_indices = BTreeMap::new();
    let mut features = Vec::new();
    let mut edges = BTreeMap::<(usize, usize), BTreeSet<(usize, usize)>>::new();
    let mut max_step = 0;
    for source in sources {
        max_step = max_step.max(source.len());
        let mut nodes = Vec::new();
        for (step, entries) in source.iter().enumerate() {
            for feature in entries {
                let index = *feature_indices.entry(feature.clone()).or_insert_with(|| {
                    let index = features.len();
                    features.push(feature.clone());
                    index
                });
                nodes.push((step, index));
            }
        }
        for (i, &node) in nodes.iter().enumerate() {
            let outgoing = edges.entry(node).or_default();
            if let Some(&next) = nodes.get(i + 1) {
                outgoing.insert(next);
            }
        }
    }
    let mut visited = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    let mut sorted = Vec::new();
    for &node in edges.keys() {
        visit(node, &edges, &mut visited, &mut visiting, &mut sorted)?;
    }
    sorted.reverse();
    Ok((0..max_step)
        .map(|step| {
            let features = sorted
                .iter()
                .filter(|n| n.0 == step)
                .map(|n| features[n.1].clone())
                .collect::<Vec<_>>();
            let indices = features
                .iter()
                .cloned()
                .enumerate()
                .map(|(i, f)| (f, i))
                .collect();
            StepFeatureData { features, indices }
        })
        .collect())
}
fn visit(
    node: (usize, usize),
    edges: &BTreeMap<(usize, usize), BTreeSet<(usize, usize)>>,
    visited: &mut BTreeSet<(usize, usize)>,
    visiting: &mut BTreeSet<(usize, usize)>,
    sorted: &mut Vec<(usize, usize)>,
) -> Result<(), Error> {
    if visited.contains(&node) {
        return Ok(());
    }
    if !visiting.insert(node) {
        return Err(Error("feature order cycle found".into()));
    }
    if let Some(outgoing) = edges.get(&node) {
        for &next in outgoing {
            visit(next, edges, visited, visiting, sorted)?;
        }
    }
    visiting.remove(&node);
    visited.insert(node);
    sorted.push(node);
    Ok(())
}
