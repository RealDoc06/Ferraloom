//! The native constructor request, shared by loading and spawning call sites.

/// Port of EntitySpawnReason. Current factories do not consume this value;
/// retaining it in the request avoids inventing a different load/spawn contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntitySpawnReason {
    Natural,
    ChunkGeneration,
    Spawner,
    Structure,
    Breeding,
    MobSummoned,
    Jockey,
    Event,
    Conversion,
    Reinforcement,
    Triggered,
    Bucket,
    SpawnItemUse,
    Command,
    Dispenser,
    Patrol,
    TrialSpawner,
    Load,
    DimensionTravel,
}

/// Port of EntitySpawnRequest, including the native canSpawn bypass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntitySpawnRequest {
    pub reason: EntitySpawnReason,
    pub ignore_checks: bool,
}
