//! Canonical simulation eligibility during ordinary ticks and managed blocking.
use super::{
    Level, distance_manager::DistanceManager, simulation::ChunkLevelChange, tickets::TicketStorage,
};
use ferraloom_util::{BlockPos, ChunkPos};

impl Level {
    /// Pure DistanceManager#runAllUpdates graph slots. Return owned holder
    /// changes so every graph/backend guard ends before publication callbacks.
    pub fn run_distance_manager_updates(
        &self,
        manager: &mut DistanceManager,
        tickets: &mut TicketStorage,
    ) -> Vec<ChunkLevelChange> {
        manager.run_all_updates(tickets, &mut self.region().simulation.borrow_mut())
    }

    pub fn simulation_level(&self, pos: ChunkPos) -> i32 {
        self.region().simulation.borrow().level(pos)
    }

    /// Native fastutil traversal, captured before any per-chunk world calls.
    pub fn entity_ticking_chunks(&self) -> Vec<ChunkPos> {
        self.region()
            .simulation
            .borrow()
            .entity_ticking_chunks()
            .collect()
    }

    pub fn block_ticking_chunks(&self) -> Vec<ChunkPos> {
        self.region()
            .simulation
            .borrow()
            .block_ticking_chunks()
            .collect()
    }

    pub fn should_tick_blocks_at(&self, pos: BlockPos) -> bool {
        self.should_tick_blocks_at_chunk(ChunkPos::containing(pos))
    }

    /// Port of Level#shouldTickBlocksAt(long chunkPos), using the same graph as
    /// the BlockPos overload without checking or loading the chunk itself.
    pub fn should_tick_blocks_at_chunk(&self, pos: ChunkPos) -> bool {
        self.simulation_level(pos) <= 32
    }

    /// Simulation eligibility is independent of attached chunks, holder loading
    /// status and the entity section manager's confirmed visibility.
    pub fn is_entity_ticking_chunk(&self, pos: ChunkPos) -> bool {
        self.simulation_level(pos) <= 31
    }

    /// Explicit local graph contents for constructed fixtures, without source
    /// tickets or neighbor propagation. Never used by a production caller.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn set_chunk_block_ticking(
        &mut self,
        pos: ChunkPos,
        ticking: bool,
    ) -> Result<(), super::LevelError> {
        self.chunk(pos)
            .ok_or(super::LevelError::MissingChunk(pos))?;
        let tracker = self.region_mut().simulation.get_mut();
        let level = if ticking {
            tracker.level(pos).min(32)
        } else {
            33
        };
        tracker.set_fixture_level(pos, level);
        Ok(())
    }

    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn set_chunk_entity_ticking(
        &mut self,
        pos: ChunkPos,
        ticking: bool,
    ) -> Result<(), super::LevelError> {
        self.chunk(pos)
            .ok_or(super::LevelError::MissingChunk(pos))?;
        let tracker = self.region_mut().simulation.get_mut();
        let level = if ticking {
            31
        } else {
            tracker.level(pos).max(32)
        };
        tracker.set_fixture_level(pos, level);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        level::{
            LevelContext, Region,
            tickets::{Ticket, TicketType},
        },
    };

    #[test]
    fn shared_graph_pass_publishes_native_levels_and_order_without_attached_chunks() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        let reader = &level;
        let mut manager = DistanceManager::default();
        let mut tickets = TicketStorage::default();
        for (stage, changes) in [
            ("origin", vec![(ChunkPos::ZERO, 30, true)]),
            ("overlap", vec![(ChunkPos::new(2, -1), 30, true)]),
            ("remove-origin", vec![(ChunkPos::ZERO, 30, false)]),
            (
                "move",
                vec![
                    (ChunkPos::new(2, -1), 30, false),
                    (ChunkPos::new(-2, 1), 29, true),
                ],
            ),
            ("empty", vec![(ChunkPos::new(-2, 1), 29, false)]),
        ] {
            for (pos, ticket_level, add) in changes {
                if add {
                    tickets.add(
                        pos,
                        Ticket {
                            kind: TicketType::PlayerSimulation,
                            level: ticket_level,
                            expires: None,
                        },
                    );
                } else {
                    tickets.remove(pos, TicketType::PlayerSimulation, ticket_level);
                }
            }
            reader.run_distance_manager_updates(&mut manager, &mut tickets);
            let expected: Vec<_> = include_str!("../../tests/java/level/simulation.tsv")
                .lines()
                .find_map(|row| {
                    row.strip_prefix(stage)
                        .and_then(|row| row.strip_prefix('\t'))
                })
                .unwrap()
                .split(';')
                .filter(|row| !row.is_empty())
                .map(|row| {
                    let fields: Vec<i32> = row.split(',').map(|n| n.parse().unwrap()).collect();
                    (ChunkPos::new(fields[0], fields[1]), fields[2])
                })
                .collect();
            assert_eq!(
                reader.block_ticking_chunks(),
                expected
                    .iter()
                    .filter(|(_, level)| *level <= 32)
                    .map(|&(pos, _)| pos)
                    .collect::<Vec<_>>(),
                "{stage}"
            );
            assert_eq!(
                reader.entity_ticking_chunks(),
                expected
                    .iter()
                    .filter(|(_, level)| *level <= 31)
                    .map(|&(pos, _)| pos)
                    .collect::<Vec<_>>(),
                "{stage}"
            );
            for (pos, expected_level) in expected {
                assert_eq!(reader.simulation_level(pos), expected_level, "{stage}");
                assert_eq!(
                    reader.should_tick_blocks_at(BlockPos::new(pos.x * 16, 0, pos.z * 16)),
                    expected_level <= 32
                );
                assert_eq!(
                    reader.should_tick_blocks_at_chunk(pos),
                    expected_level <= 32
                );
                assert_eq!(reader.is_entity_ticking_chunk(pos), expected_level <= 31);
                assert!(reader.chunk(pos).is_none());
            }
        }
        assert_eq!(reader.simulation_level(ChunkPos::ZERO), 33);
    }
}
