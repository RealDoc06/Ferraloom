use ferraloom_util::ChunkPos;
/// ChunkLevel.MAX_LEVEL for the 26.3 generation pyramid's radius of 11.
/// Checked against the real LoadingChunkTracker Java vectors. A future
/// generation-pipeline descriptor supplies this radius to the loading tracker.
pub const MAX_LOADED_LEVEL: i32 = 44;
/// Port of ChunkLevel's full, block-ticking and entity-ticking thresholds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FullChunkStatus {
    Inaccessible,
    Full,
    BlockTicking,
    EntityTicking,
}
pub fn full_status(level: i32) -> FullChunkStatus {
    match level {
        i32::MIN..=31 => FullChunkStatus::EntityTicking,
        32 => FullChunkStatus::BlockTicking,
        33 => FullChunkStatus::Full,
        _ => FullChunkStatus::Inaccessible,
    }
}
/// Port of ChunkTrackingView#isWithinDistance, including the neighbor buffer.
pub fn in_view(center: ChunkPos, distance: i32, pos: ChunkPos, neighbors: bool) -> bool {
    let buffer = if neighbors { 2 } else { 1 };
    let x = i64::from((pos.x - center.x).abs().saturating_sub(buffer).max(0));
    let z = i64::from((pos.z - center.z).abs().saturating_sub(buffer).max(0));
    x * x + z * z < i64::from(distance) * i64::from(distance)
}
pub fn tracking_view(center: ChunkPos, distance: i32) -> Vec<ChunkPos> {
    let mut chunks = Vec::new();
    for x in center.x - distance - 1..=center.x + distance + 1 {
        for z in center.z - distance - 1..=center.z + distance + 1 {
            let pos = ChunkPos::new(x, z);
            if in_view(center, distance, pos, true) {
                chunks.push(pos);
            }
        }
    }
    chunks
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rounded_view_and_ticking_levels() {
        let center = ChunkPos::new(0, 0);
        assert!(in_view(center, 2, ChunkPos::new(3, 3), true));
        assert!(!in_view(center, 2, ChunkPos::new(4, 0), true));
        assert!(!in_view(center, 2, ChunkPos::new(3, 3), false));
        assert_eq!(full_status(31), FullChunkStatus::EntityTicking);
        assert_eq!(full_status(34), FullChunkStatus::Inaccessible);
    }
}

/// Single-region DistanceManager state. Lookup maps never determine graph or
/// chunk traversal order; every player section transition is applied at its call.
pub struct DistanceManager {
    pub natural_spawn: super::simulation::FixedPlayerDistanceChunkTracker,
    pub loading: super::simulation::LoadingChunkTracker,
    player_tickets: super::simulation::PlayerTicketTracker,
    dispatcher: super::chunk_task_dispatcher::ThrottlingChunkTaskDispatcher<
        super::simulation::PlayerLoadingTask,
    >,
    players_per_chunk:
        std::collections::HashMap<ChunkPos, std::collections::HashSet<crate::entity::EntityRef>>,
    player_sections:
        std::collections::HashMap<crate::entity::EntityRef, ferraloom_util::SectionPos>,
    tickets_to_release: ferraloom_util::collections::LongOpenHashSet,
    simulation_distance: i32,
}
impl Default for DistanceManager {
    fn default() -> Self {
        Self {
            natural_spawn: Default::default(),
            loading: super::simulation::LoadingChunkTracker::new(MAX_LOADED_LEVEL),
            player_tickets: Default::default(),
            dispatcher: super::chunk_task_dispatcher::ThrottlingChunkTaskDispatcher::new(4),
            players_per_chunk: Default::default(),
            player_sections: Default::default(),
            tickets_to_release: Default::default(),
            simulation_distance: 10,
        }
    }
}
impl DistanceManager {
    pub fn set_view_distance(&mut self, distance: i32) {
        self.player_tickets
            .update_view_distance(distance.clamp(2, 32), &mut self.dispatcher);
    }
    pub fn set_simulation_distance(
        &mut self,
        distance: i32,
        tickets: &mut super::tickets::TicketStorage,
    ) {
        if self.simulation_distance == distance {
            return;
        }
        self.simulation_distance = distance;
        tickets.replace_ticket_level_of_type(
            (31 - distance).max(0),
            super::tickets::TicketType::PlayerSimulation,
        );
    }
    pub fn update_player(
        &mut self,
        entity: crate::entity::EntityRef,
        section: Option<ferraloom_util::SectionPos>,
        tickets: &mut super::tickets::TicketStorage,
    ) {
        let old = self.player_sections.get(&entity).copied();
        if old == section {
            return;
        }
        let level = (31 - self.simulation_distance).max(0);
        if let Some(old) = old {
            let pos = ChunkPos::new(old.x, old.z);
            let players = self
                .players_per_chunk
                .get_mut(&pos)
                .expect("registered player chunk");
            players.remove(&entity);
            if players.is_empty() {
                self.players_per_chunk.remove(&pos);
                self.natural_spawn.update_player_chunk(pos, false);
                self.player_tickets.update_player_chunk(pos, false);
                tickets.remove(pos, super::tickets::TicketType::PlayerSimulation, level);
            }
            self.player_sections.remove(&entity);
        }
        if let Some(section) = section {
            let pos = ChunkPos::new(section.x, section.z);
            self.players_per_chunk
                .entry(pos)
                .or_default()
                .insert(entity);
            self.player_sections.insert(entity, section);
            self.natural_spawn.update_source_level(pos, 0, true);
            self.player_tickets.update_player_chunk(pos, true);
            tickets.add(
                pos,
                super::tickets::Ticket {
                    kind: super::tickets::TicketType::PlayerSimulation,
                    level,
                    expires: None,
                },
            );
        }
    }
    /// Port of runAllUpdates' graph slots. The runtime applies returned holder
    /// changes, then polls owned dispatcher work at its main-executor slot.
    pub fn run_all_updates(
        &mut self,
        tickets: &mut super::tickets::TicketStorage,
        simulation: &mut super::simulation::SimulationChunkTracker,
    ) -> Vec<super::simulation::ChunkLevelChange> {
        for update in tickets.take_source_updates() {
            match update.tracker {
                super::tickets::TicketTracker::Loading => self.loading.update_source_level(
                    update.pos,
                    update.level,
                    update.only_decreased,
                ),
                super::tickets::TicketTracker::Simulation => {
                    simulation.update_source_level(update.pos, update.level, update.only_decreased)
                }
            }
        }
        self.natural_spawn.run_all_updates();
        simulation.run_all_updates();
        self.player_tickets.run_all_updates(&mut self.dispatcher);
        self.loading.run_all_updates();
        self.loading.take_level_changes()
    }
    pub fn poll_player_loading_tasks(&mut self, tickets: &mut super::tickets::TicketStorage) {
        use super::{
            chunk_task_dispatcher::ChunkTaskDispatch,
            simulation::PlayerLoadingTask,
            tickets::{Ticket, TicketType},
        };
        while let Some(dispatch) = self.dispatcher.next_dispatch() {
            let (tasks, batch) = match dispatch {
                ChunkTaskDispatch::Released(task) => (vec![task], false),
                ChunkTaskDispatch::Batch { tasks, .. } => (tasks, true),
            };
            for task in tasks {
                match task {
                    PlayerLoadingTask::Add(pos) if self.player_tickets.has_ticket(pos) => {
                        tickets.add(
                            pos,
                            Ticket {
                                kind: TicketType::PlayerLoading,
                                level: 31,
                                expires: None,
                            },
                        );
                        self.tickets_to_release.insert(pos.pack());
                    }
                    PlayerLoadingTask::Add(pos) => self.dispatcher.release(pos, false, None),
                    PlayerLoadingTask::Remove(pos) => {
                        tickets.remove(pos, TicketType::PlayerLoading, 31);
                    }
                }
            }
            if batch {
                self.dispatcher.complete_batch();
            }
        }
    }
    /// DistanceManager#runAllUpdates registers these future callbacks only
    /// after the holder update pass has settled. Preserve primitive-set order.
    pub fn take_player_loading_waiters(
        &mut self,
        tickets: &super::tickets::TicketStorage,
    ) -> Vec<ChunkPos> {
        let waiters = self
            .tickets_to_release
            .iter()
            .filter_map(|&key| {
                let pos = ChunkPos::unpack(key);
                tickets
                    .contains_type(pos, super::tickets::TicketType::PlayerLoading)
                    .then_some(pos)
            })
            .collect();
        self.tickets_to_release.clear();
        waiters
    }
    pub fn has_pending_player_loading(&self, pos: ChunkPos) -> bool {
        self.tickets_to_release.contains(&pos.pack())
    }
    /// Call only after the registered entity-ticking range future completes
    /// (success or failure). A merely loaded chunk never releases this slot.
    pub fn complete_player_loading(&mut self, pos: ChunkPos) {
        self.tickets_to_release.remove(&pos.pack());
        self.dispatcher.release(pos, false, None);
    }
}

#[cfg(test)]
mod player_tests {
    use super::*;
    use crate::{
        entity::EntityRef,
        level::tickets::{TicketStorage, TicketTracker},
    };
    use ferraloom_util::SectionPos;

    #[test]
    fn player_membership_retains_each_section_transition_and_shared_ticket() {
        let mut manager = DistanceManager::default();
        let mut tickets = TicketStorage::default();
        let mut simulation = super::super::simulation::SimulationChunkTracker::default();
        let a = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        let b = EntityRef::fixture(crate::level::RegionId::FIXTURE, 2);
        let origin = SectionPos::new(0, 4, 0);
        manager.update_player(a, Some(origin), &mut tickets);
        manager.update_player(b, Some(origin), &mut tickets);
        assert_eq!(
            tickets.source_level(ChunkPos::new(0, 0), TicketTracker::Simulation),
            21
        );
        manager.run_all_updates(&mut tickets, &mut simulation);
        manager.update_player(a, None, &mut tickets);
        assert_eq!(
            tickets.source_level(ChunkPos::new(0, 0), TicketTracker::Simulation),
            21
        );
        manager.update_player(b, Some(SectionPos::new(0, 5, 0)), &mut tickets);
        assert_eq!(
            tickets.source_level(ChunkPos::new(0, 0), TicketTracker::Simulation),
            21
        );
        manager.run_all_updates(&mut tickets, &mut simulation);
        // A->B->A during one packet phase must disturb the queued graph, even
        // though a snapshot of only the final player chunk would be unchanged.
        manager.update_player(b, Some(SectionPos::new(1, 5, 0)), &mut tickets);
        manager.update_player(b, Some(origin), &mut tickets);
        assert_eq!(
            tickets.source_level(ChunkPos::new(0, 0), TicketTracker::Simulation),
            21
        );
        assert_eq!(
            tickets.source_level(ChunkPos::new(1, 0), TicketTracker::Simulation),
            45
        );
        manager.run_all_updates(&mut tickets, &mut simulation);
        assert_eq!(simulation.level(ChunkPos::new(0, 0)), 21);
        assert_eq!(simulation.level(ChunkPos::new(1, 0)), 22);
    }
}
