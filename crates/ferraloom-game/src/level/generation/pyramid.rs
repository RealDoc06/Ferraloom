use super::ChunkStatus;
use std::sync::OnceLock;

/// Port of ChunkStatus#getStatusList. 26.3 folds noise/surface/carvers into TERRAIN.
pub const STATUSES: [ChunkStatus; 10] = [
    ChunkStatus::Empty,
    ChunkStatus::StructureStarts,
    ChunkStatus::StructureReferences,
    ChunkStatus::Biomes,
    ChunkStatus::Terrain,
    ChunkStatus::Features,
    ChunkStatus::InitializeLight,
    ChunkStatus::Light,
    ChunkStatus::Spawn,
    ChunkStatus::Full,
];
pub fn status_name(status: ChunkStatus) -> &'static str {
    match status {
        ChunkStatus::Empty => "minecraft:empty",
        ChunkStatus::StructureStarts => "minecraft:structure_starts",
        ChunkStatus::StructureReferences => "minecraft:structure_references",
        ChunkStatus::Biomes => "minecraft:biomes",
        ChunkStatus::Terrain => "minecraft:terrain",
        ChunkStatus::Features => "minecraft:features",
        ChunkStatus::InitializeLight => "minecraft:initialize_light",
        ChunkStatus::Light => "minecraft:light",
        ChunkStatus::Spawn => "minecraft:spawn",
        ChunkStatus::Full => "minecraft:full",
    }
}
pub fn parse_status(name: &str) -> Option<ChunkStatus> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    STATUSES
        .into_iter()
        .find(|&status| status_name(status).strip_prefix("minecraft:") == Some(name))
}

/// Port of ChunkDependencies. The index is chessboard distance, not a square size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkDependencies(Vec<ChunkStatus>);
impl ChunkDependencies {
    pub fn by_radius(&self) -> &[ChunkStatus] {
        &self.0
    }
    pub fn get(&self, distance: usize) -> Option<ChunkStatus> {
        self.0.get(distance).copied()
    }
    pub fn radius_of(&self, status: ChunkStatus) -> Option<usize> {
        self.0.iter().rposition(|&required| required >= status)
    }
    pub fn radius(&self) -> Option<usize> {
        self.0.len().checked_sub(1)
    }
}

/// Port of ChunkStatusTasks. Loading deliberately repeats only the runtime hooks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkStatusTask {
    PassThrough,
    GenerateStructureStarts,
    LoadStructureStarts,
    GenerateStructureReferences,
    GenerateBiomes,
    BuildTerrain,
    GenerateFeatures,
    InitializeLight,
    Light,
    GenerateSpawn,
    Full,
}
#[derive(Clone, Debug)]
pub struct ChunkStep {
    pub target: ChunkStatus,
    pub direct: ChunkDependencies,
    pub accumulated: ChunkDependencies,
    pub block_state_write_radius: i32,
    pub task: ChunkStatusTask,
}
impl ChunkStep {
    pub fn accumulated_radius_of(&self, status: ChunkStatus) -> Option<usize> {
        if status == self.target {
            Some(0)
        } else {
            self.accumulated.radius_of(status)
        }
    }
}

/// Port of ChunkPyramid and ChunkStep.Builder#buildAccumulatedDependencies.
#[derive(Clone, Debug)]
pub struct ChunkPyramid {
    steps: Vec<ChunkStep>,
}
impl ChunkPyramid {
    pub fn generation() -> &'static Self {
        static PYRAMID: OnceLock<ChunkPyramid> = OnceLock::new();
        PYRAMID.get_or_init(|| Self::build(true))
    }
    pub fn loading() -> &'static Self {
        static PYRAMID: OnceLock<ChunkPyramid> = OnceLock::new();
        PYRAMID.get_or_init(|| Self::build(false))
    }
    pub fn step_to(&self, status: ChunkStatus) -> &ChunkStep {
        &self.steps[status as usize]
    }
    fn build(generation: bool) -> Self {
        use ChunkStatus::*;
        use ChunkStatusTask as Task;
        let mut steps: Vec<ChunkStep> = Vec::new();
        for (index, target) in STATUSES.into_iter().enumerate() {
            let mut direct = index
                .checked_sub(1)
                .map(|i| vec![STATUSES[i]])
                .unwrap_or_default();
            let mut require = |status, radius: usize| {
                direct.resize(direct.len().max(radius + 1), status);
                for dependency in &mut direct[..=radius] {
                    *dependency = (*dependency).max(status);
                }
            };
            let mut write_radius = -1;
            let task = match target {
                Empty => Task::PassThrough,
                StructureStarts if generation => Task::GenerateStructureStarts,
                StructureStarts => Task::LoadStructureStarts,
                StructureReferences if generation => {
                    require(StructureStarts, 8);
                    Task::GenerateStructureReferences
                }
                Biomes if generation => {
                    require(StructureStarts, 8);
                    Task::GenerateBiomes
                }
                Terrain if generation => {
                    require(StructureStarts, 8);
                    require(Biomes, 1);
                    write_radius = 0;
                    Task::BuildTerrain
                }
                Features if generation => {
                    require(StructureStarts, 8);
                    require(Terrain, 1);
                    write_radius = 1;
                    Task::GenerateFeatures
                }
                InitializeLight => Task::InitializeLight,
                Light => {
                    require(InitializeLight, 1);
                    Task::Light
                }
                Spawn if generation => {
                    require(Biomes, 1);
                    Task::GenerateSpawn
                }
                Full => Task::Full,
                _ => Task::PassThrough,
            };
            let accumulated = if let Some(parent) = steps.last() {
                let radius = direct
                    .iter()
                    .rposition(|&s| s >= parent.target)
                    .unwrap_or(0);
                (0..direct.len().max(radius + parent.accumulated.0.len()))
                    .map(|distance| {
                        let inherited = distance
                            .checked_sub(radius)
                            .and_then(|i| parent.accumulated.get(i));
                        match (direct.get(distance).copied(), inherited) {
                            (Some(a), Some(b)) => a.max(b),
                            (Some(a), None) | (None, Some(a)) => a,
                            (None, None) => unreachable!("continuous dependency radii"),
                        }
                    })
                    .collect()
            } else {
                direct.clone()
            };
            steps.push(ChunkStep {
                target,
                direct: ChunkDependencies(direct),
                accumulated: ChunkDependencies(accumulated),
                block_state_write_radius: write_radius,
                task,
            });
        }
        Self { steps }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vanilla_pyramids_match_every_direct_and_accumulated_dependency() {
        for row in include_str!("pyramids.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            let pyramid = if fields[0] == "generation" {
                ChunkPyramid::generation()
            } else {
                ChunkPyramid::loading()
            };
            let step = pyramid.step_to(parse_status(fields[1]).unwrap());
            let encode = |dependencies: &ChunkDependencies| {
                dependencies
                    .by_radius()
                    .iter()
                    .map(|&s| status_name(s))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            assert_eq!(encode(&step.direct), fields[2], "direct {}", fields[1]);
            assert_eq!(
                encode(&step.accumulated),
                fields[3],
                "accumulated {}",
                fields[1]
            );
            assert_eq!(step.block_state_write_radius.to_string(), fields[4]);
        }
        assert_eq!(
            ChunkPyramid::generation()
                .step_to(ChunkStatus::Full)
                .accumulated_radius_of(ChunkStatus::Empty),
            Some(11)
        );
    }
}
