use ferraloom_util::ChunkPos;
use ferraloom_util::collections::LongOpenHashSet;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TicketType {
    Player,
    Start,
    Forced,
    Unknown,
    PlayerLoading,
    PlayerSimulation,
    PlayerSpawn,
    SpawnSearch,
    Dragon,
    Portal,
    EnderPearl,
}
impl TicketType {
    pub fn does_load(self) -> bool {
        !matches!(self, Self::PlayerSimulation)
    }
    pub fn does_simulate(self) -> bool {
        matches!(
            self,
            Self::PlayerSimulation | Self::Forced | Self::Dragon | Self::Portal | Self::EnderPearl
        )
    }
    pub fn should_keep_dimension_active(self) -> bool {
        matches!(
            self,
            Self::PlayerSimulation | Self::Forced | Self::Portal | Self::EnderPearl
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketTracker {
    Loading,
    Simulation,
}
/// Port of TicketStorage.ChunkUpdated: preserve every listener call, including
/// removal of a ticket that did not supply the minimum level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TicketSourceUpdate {
    pub tracker: TicketTracker,
    pub pos: ChunkPos,
    pub level: i32,
    pub only_decreased: bool,
}
/// Port of TicketStorage's minimum-level and expiration semantics.
#[derive(Clone, Copy, Debug)]
pub struct Ticket {
    pub kind: TicketType,
    pub level: i32,
    pub expires: Option<u64>,
}
pub struct TicketStorage {
    // Keep the bring-up map lookup-only. The native primitive-key table owns
    // traversal and cursor-removal order, matching Long2ObjectOpenHashMap(4).
    tickets: BTreeMap<i64, Vec<Ticket>>,
    traversal: LongOpenHashSet,
    updates: Vec<TicketSourceUpdate>,
}
impl Default for TicketStorage {
    fn default() -> Self {
        Self {
            tickets: BTreeMap::new(),
            traversal: LongOpenHashSet::with_capacity(4),
            updates: Vec::new(),
        }
    }
}
impl TicketStorage {
    pub fn add(&mut self, pos: ChunkPos, ticket: Ticket) {
        let tickets = self.tickets.entry(pos.pack()).or_insert_with(|| {
            self.traversal.insert(pos.pack());
            Vec::with_capacity(4)
        });
        if let Some(old) = tickets
            .iter_mut()
            .find(|old| old.kind == ticket.kind && old.level == ticket.level)
        {
            *old = ticket;
        } else {
            let old_simulation = minimum_level(tickets, TicketTracker::Simulation);
            let old_loading = minimum_level(tickets, TicketTracker::Loading);
            tickets.push(ticket);
            if ticket.kind.does_simulate() && ticket.level < old_simulation {
                self.updates.push(TicketSourceUpdate {
                    tracker: TicketTracker::Simulation,
                    pos,
                    level: ticket.level,
                    only_decreased: true,
                });
            }
            if ticket.kind.does_load() && ticket.level < old_loading {
                self.updates.push(TicketSourceUpdate {
                    tracker: TicketTracker::Loading,
                    pos,
                    level: ticket.level,
                    only_decreased: true,
                });
            }
        }
    }
    pub fn remove(&mut self, pos: ChunkPos, kind: TicketType, level: i32) {
        if let Some(tickets) = self.tickets.get_mut(&pos.pack()) {
            let Some(index) = tickets
                .iter()
                .position(|t| t.kind == kind && t.level == level)
            else {
                return;
            };
            tickets.remove(index);
            if kind.does_simulate() {
                self.updates.push(TicketSourceUpdate {
                    tracker: TicketTracker::Simulation,
                    pos,
                    level: minimum_level(tickets, TicketTracker::Simulation),
                    only_decreased: false,
                });
            }
            if kind.does_load() {
                self.updates.push(TicketSourceUpdate {
                    tracker: TicketTracker::Loading,
                    pos,
                    level: minimum_level(tickets, TicketTracker::Loading),
                    only_decreased: false,
                });
            }
            if tickets.is_empty() {
                self.tickets.remove(&pos.pack());
                self.traversal.remove(&pos.pack());
            }
        }
    }
    pub fn purge_stale(&mut self, tick: u64) {
        let mut cursor = self.traversal.cursor();
        while let Some((&key, _)) = cursor.next_entry() {
            let tickets = self.tickets.get_mut(&key).unwrap();
            let mut removed_loading = false;
            let mut removed_simulation = false;
            tickets.retain(|t| {
                if t.expires.is_none_or(|expires| tick <= expires) {
                    true
                } else {
                    removed_loading |= t.kind.does_load();
                    removed_simulation |= t.kind.does_simulate();
                    false
                }
            });
            // removeTicketIf notifies loading before simulation; removeTicket
            // and addTicket notify simulation before loading.
            for (tracker, removed) in [
                (TicketTracker::Loading, removed_loading),
                (TicketTracker::Simulation, removed_simulation),
            ] {
                if removed {
                    self.updates.push(TicketSourceUpdate {
                        tracker,
                        pos: ChunkPos::unpack(key),
                        level: minimum_level(tickets, tracker),
                        only_decreased: false,
                    });
                }
            }
            if tickets.is_empty() {
                self.tickets.remove(&key);
                cursor.remove();
            }
        }
    }
    pub fn level(&self, pos: ChunkPos) -> Option<i32> {
        self.tickets.get(&pos.pack())?.iter().map(|t| t.level).min()
    }
    pub fn source_level(&self, pos: ChunkPos, tracker: TicketTracker) -> i32 {
        minimum_level(
            self.tickets.get(&pos.pack()).map_or(&[], Vec::as_slice),
            tracker,
        )
    }
    pub fn take_source_updates(&mut self) -> Vec<TicketSourceUpdate> {
        std::mem::take(&mut self.updates)
    }
    pub fn should_keep_dimension_active(&self) -> bool {
        self.tickets.values().any(|tickets| {
            tickets
                .iter()
                .any(|ticket| ticket.kind.should_keep_dimension_active())
        })
    }
}
fn minimum_level(tickets: &[Ticket], tracker: TicketTracker) -> i32 {
    tickets
        .iter()
        .filter(|t| match tracker {
            TicketTracker::Loading => t.kind.does_load(),
            TicketTracker::Simulation => t.kind.does_simulate(),
        })
        .map(|t| t.level)
        .min()
        .unwrap_or(super::distance_manager::MAX_LOADED_LEVEL + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vector(name: &str) -> &'static str {
        include_str!("../../tests/java/level/loading.tsv")
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once('\t')?;
                (key == name).then_some(value)
            })
            .unwrap()
    }
    fn updates(storage: &mut TicketStorage) -> String {
        storage
            .take_source_updates()
            .iter()
            .map(|update| {
                let kind = match update.tracker {
                    TicketTracker::Loading => "loading",
                    TicketTracker::Simulation => "simulation",
                };
                format!(
                    "{kind},{},{},{},{}",
                    update.pos.x, update.pos.z, update.level, update.only_decreased
                )
            })
            .collect::<Vec<_>>()
            .join(";")
    }
    #[test]
    fn native_source_listener_order_includes_nonminimal_removal_and_iterator_purge() {
        use TicketType::{Forced, PlayerLoading, PlayerSimulation};
        let mut storage = TicketStorage::default();
        let pos = ChunkPos::new(0, 0);
        for (kind, level) in [
            (PlayerSimulation, 30),
            (PlayerLoading, 31),
            (Forced, 29),
            (Forced, 29),
            (Forced, 32),
        ] {
            storage.add(
                pos,
                Ticket {
                    kind,
                    level,
                    expires: None,
                },
            );
        }
        for (kind, level) in [
            (Forced, 32),
            (Forced, 29),
            (Forced, 29),
            (PlayerSimulation, 30),
            (PlayerLoading, 31),
        ] {
            storage.remove(pos, kind, level);
        }
        assert_eq!(updates(&mut storage), vector("ticket-add-remove"));
        for i in 0..24 {
            let pos = ChunkPos::new(i % 6 - 3, i / 6 - 2);
            storage.add(
                pos,
                Ticket {
                    kind: Forced,
                    level: 30,
                    expires: Some(0),
                },
            );
            storage.add(
                pos,
                Ticket {
                    kind: PlayerLoading,
                    level: 31,
                    expires: Some(0),
                },
            );
            if i % 3 == 0 {
                storage.add(
                    pos,
                    Ticket {
                        kind: PlayerSimulation,
                        level: 32,
                        expires: None,
                    },
                );
            }
        }
        storage.take_source_updates();
        storage.purge_stale(1);
        assert_eq!(updates(&mut storage), vector("ticket-purge"));
        assert_eq!(
            storage.source_level(ChunkPos::new(-3, -2), TicketTracker::Simulation),
            32
        );
        assert_eq!(
            storage.source_level(ChunkPos::new(-3, -2), TicketTracker::Loading),
            45
        );
    }
}
