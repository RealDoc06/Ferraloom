use ferraloom_util::ChunkPos;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TicketType {
    Player,
    Start,
    Forced,
    Unknown,
}
/// Port of TicketStorage's minimum-level and expiration semantics.
#[derive(Clone, Copy, Debug)]
pub struct Ticket {
    pub kind: TicketType,
    pub level: i32,
    pub expires: Option<u64>,
}
#[derive(Default)]
pub struct TicketStorage {
    tickets: BTreeMap<i64, Vec<Ticket>>,
}
impl TicketStorage {
    pub fn add(&mut self, pos: ChunkPos, ticket: Ticket) {
        let tickets = self.tickets.entry(pos.pack()).or_default();
        if let Some(old) = tickets
            .iter_mut()
            .find(|old| old.kind == ticket.kind && old.level == ticket.level)
        {
            *old = ticket;
        } else {
            tickets.push(ticket);
        }
    }
    pub fn remove(&mut self, pos: ChunkPos, kind: TicketType, level: i32) {
        if let Some(tickets) = self.tickets.get_mut(&pos.pack()) {
            tickets.retain(|t| t.kind != kind || t.level != level);
        }
    }
    pub fn purge_stale(&mut self, tick: u64) {
        self.tickets.retain(|_, tickets| {
            tickets.retain(|t| t.expires.is_none_or(|expires| tick <= expires));
            !tickets.is_empty()
        });
    }
    pub fn level(&self, pos: ChunkPos) -> Option<i32> {
        self.tickets.get(&pos.pack())?.iter().map(|t| t.level).min()
    }
}
