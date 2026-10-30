//! Canonical dimension state extracted from the active/parked client shell.
use crate::{tick_chunk_membership::TickChunkMembership, ticket_levels::ChunkTickets};
use horizon_world::level::Ticks;

/// The server-owned part of a level, transferred intact on a dimension change.
/// Resident chunks and entities join this owner as their adapters are extracted.
/// The serial shell borrows these fields directly; it keeps no duplicate queues
/// or ticket state. No tick order or phase budget is introduced by this owner.
pub struct ServerLevel {
    pub block_ticks: Ticks,
    pub tick_chunks: TickChunkMembership,
    pub tickets: ChunkTickets,
}

impl ServerLevel {
    pub fn new(random_tick_seed: i32) -> Self {
        Self {
            block_ticks: Ticks::new(random_tick_seed),
            tick_chunks: TickChunkMembership::default(),
            tickets: ChunkTickets::default(),
        }
    }
}
