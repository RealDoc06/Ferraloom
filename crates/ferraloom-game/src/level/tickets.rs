use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_storage::{Error, SavedDataCodec};
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
    fn timeout(self) -> i64 {
        match self {
            Self::PlayerSpawn => 20,
            Self::SpawnSearch | Self::Unknown => 1,
            Self::Portal => 300,
            Self::EnderPearl => 40,
            _ => 0,
        }
    }
    fn persisted(self) -> bool {
        matches!(self, Self::Forced | Self::Portal)
    }
    fn name(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Start => "start",
            Self::Forced => "forced",
            Self::Unknown => "unknown",
            Self::PlayerLoading => "player_loading",
            Self::PlayerSimulation => "player_simulation",
            Self::PlayerSpawn => "player_spawn",
            Self::SpawnSearch => "spawn_search",
            Self::Dragon => "dragon",
            Self::Portal => "portal",
            Self::EnderPearl => "ender_pearl",
        }
    }
    fn by_name(name: &str) -> Option<Self> {
        [
            Self::Forced,
            Self::Unknown,
            Self::PlayerLoading,
            Self::PlayerSimulation,
            Self::PlayerSpawn,
            Self::SpawnSearch,
            Self::Dragon,
            Self::Portal,
            Self::EnderPearl,
        ]
        .into_iter()
        .find(|kind| format!("minecraft:{}", kind.name()) == name)
    }
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
/// Native saved tickets retain signed ticksLeft, including values on non-expiring types.
#[derive(Clone, Copy, Debug)]
struct TicketRecord {
    ticket: Ticket,
    ticks_left: i64,
}
impl std::ops::Deref for TicketRecord {
    type Target = Ticket;
    fn deref(&self) -> &Ticket {
        &self.ticket
    }
}
#[derive(Clone)]
pub struct TicketStorage {
    // Keep the bring-up map lookup-only. The native primitive-key table owns
    // traversal and cursor-removal order, matching Long2ObjectOpenHashMap(4).
    tickets: BTreeMap<i64, Vec<TicketRecord>>,
    deactivated: BTreeMap<i64, Vec<TicketRecord>>,
    deactivated_traversal: LongOpenHashSet,
    forced_chunks: LongOpenHashSet,
    traversal: LongOpenHashSet,
    updates: Vec<TicketSourceUpdate>,
}
impl Default for TicketStorage {
    fn default() -> Self {
        Self {
            tickets: BTreeMap::new(),
            traversal: LongOpenHashSet::with_capacity(4),
            deactivated: BTreeMap::new(),
            deactivated_traversal: LongOpenHashSet::default(),
            forced_chunks: LongOpenHashSet::default(),
            updates: Vec::new(),
        }
    }
}
impl TicketStorage {
    pub const STORAGE_ID: &'static str = "minecraft:chunk_tickets";
    pub fn add(&mut self, pos: ChunkPos, ticket: Ticket) {
        self.add_record(
            pos,
            TicketRecord {
                ticket,
                ticks_left: ticket.kind.timeout(),
            },
        );
    }
    fn add_record(&mut self, pos: ChunkPos, ticket: TicketRecord) -> bool {
        let tickets = self.tickets.entry(pos.pack()).or_insert_with(|| {
            self.traversal.insert(pos.pack());
            Vec::with_capacity(4)
        });
        if let Some(old) = tickets
            .iter_mut()
            .find(|old| old.kind == ticket.kind && old.level == ticket.level)
        {
            old.ticket.expires = ticket.expires;
            old.ticks_left = ticket.kind.timeout();
            false
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
            if ticket.kind == TicketType::Forced {
                self.forced_chunks.insert(pos.pack());
            }
            true
        }
    }
    pub fn remove(&mut self, pos: ChunkPos, kind: TicketType, level: i32) -> bool {
        if let Some(tickets) = self.tickets.get_mut(&pos.pack()) {
            let Some(index) = tickets
                .iter()
                .position(|t| t.kind == kind && t.level == level)
            else {
                return false;
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
            if kind == TicketType::Forced {
                self.rebuild_forced_chunks();
            }
            true
        } else {
            false
        }
    }
    fn rebuild_forced_chunks(&mut self) {
        self.forced_chunks = LongOpenHashSet::default();
        for &key in self.traversal.iter() {
            if self.tickets[&key]
                .iter()
                .any(|ticket| ticket.kind == TicketType::Forced)
            {
                self.forced_chunks.insert(key);
            }
        }
    }
    /// Port of TicketStorage#updateChunkForced; synchronous chunk loading belongs to the caller.
    pub fn set_chunk_forced(&mut self, pos: ChunkPos, forced: bool) -> bool {
        if forced {
            self.add_record(
                pos,
                TicketRecord {
                    ticket: Ticket {
                        kind: TicketType::Forced,
                        level: 31,
                        expires: None,
                    },
                    ticks_left: 0,
                },
            )
        } else {
            self.remove(pos, TicketType::Forced, 31)
        }
    }
    pub fn get_force_loaded_chunks(&self) -> &LongOpenHashSet {
        &self.forced_chunks
    }
    pub fn activate_all_deactivated_tickets(&mut self) {
        let traversal = std::mem::take(&mut self.deactivated_traversal);
        for &key in traversal.iter() {
            for ticket in self.deactivated.remove(&key).unwrap() {
                self.add_record(ChunkPos::unpack(key), ticket);
            }
        }
    }
    pub fn purge_stale(&mut self, tick: u64) {
        let mut removed_forced = false;
        let mut cursor = self.traversal.cursor();
        while let Some((&key, _)) = cursor.next_entry() {
            let tickets = self.tickets.get_mut(&key).unwrap();
            let mut removed_loading = false;
            let mut removed_simulation = false;
            tickets.retain_mut(|t| {
                let retained = if let Some(expires) = t.expires {
                    tick <= expires
                } else if t.kind.timeout() != 0 {
                    t.ticks_left = t.ticks_left.wrapping_sub(1);
                    t.ticks_left >= 0
                } else {
                    true
                };
                if retained {
                    true
                } else {
                    removed_forced |= t.kind == TicketType::Forced;
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
        if removed_forced {
            self.rebuild_forced_chunks();
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
/// Port of TicketStorage.CODEC. Invalid optional ticks_left retains the default as a partial value.
pub struct TicketStorageResult {
    pub value: TicketStorage,
    pub error: Option<String>,
}
impl TicketStorage {
    pub fn decode_partial(tag: &Tag) -> TicketStorageResult {
        let mut value = Self::default();
        let mut errors = Vec::new();
        let Some(map) = tag.as_compound() else {
            return TicketStorageResult {
                value,
                error: Some(format!("Not a map: {tag}")),
            };
        };
        if let Some(tickets) = map.get("tickets") {
            if let Tag::List(tickets) = tickets {
                for tag in &tickets.0 {
                    match decode_ticket(tag) {
                        Ok((position, ticket, error)) => {
                            value.deactivated_traversal.insert(position.pack());
                            value
                                .deactivated
                                .entry(position.pack())
                                .or_default()
                                .push(ticket);
                            errors.extend(error);
                        }
                        Err(error) => errors.push(error),
                    }
                }
            } else {
                errors.push(format!("Not a list: {tickets}"));
            }
        }
        TicketStorageResult {
            value,
            error: (!errors.is_empty()).then(|| errors.join("; ")),
        }
    }
}
fn decode_ticket(tag: &Tag) -> Result<(ChunkPos, TicketRecord, Option<String>), String> {
    let map = tag
        .as_compound()
        .ok_or_else(|| format!("Not a map: {tag}"))?;
    let required = |key: &str| {
        map.get(key)
            .ok_or_else(|| format!("No key {key} in MapLike[{tag}]"))
    };
    let pos = required("chunk_pos")?;
    let numbers = match pos {
        Tag::IntArray(values) => values.clone(),
        Tag::List(values) => values
            .0
            .iter()
            .map(|tag| tag.as_int().ok_or_else(|| "Not a number".to_string()))
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(format!("Not an int stream: {pos}")),
    };
    let [x, z] = numbers.as_slice() else {
        return Err("Input is not a list of 2 ints".into());
    };
    let kind = required("type")?;
    let text = kind
        .as_string()
        .ok_or_else(|| format!("Not a string: {kind}"))?
        .to_string_lossy();
    let id = ferraloom_protocol::Identifier::parse(&text)
        .map_err(|_| format!("Not a valid resource location: {text}"))?;
    let kind = TicketType::by_name(id.as_str()).ok_or_else(|| {
        format!("Unknown registry key in ResourceKey[minecraft:root / minecraft:ticket_type]: {id}")
    })?;
    let level = match required("level")? {
        Tag::Float(value) => *value as i32,
        Tag::Double(value) => *value as i32,
        value => ticket_integer(value)? as i32,
    };
    if level < 0 {
        return Err(format!("Value must be non-negative: {level}"));
    }
    let mut error = None;
    let ticks_left = match map.get("ticks_left").map(ticket_integer).transpose() {
        Ok(value) => value.unwrap_or(0),
        Err(message) => {
            error = Some(message);
            0
        }
    };
    Ok((
        ChunkPos::new(*x, *z),
        TicketRecord {
            ticket: Ticket {
                kind,
                level,
                expires: None,
            },
            ticks_left,
        },
        error,
    ))
}
fn ticket_integer(tag: &Tag) -> Result<i64, String> {
    match tag {
        Tag::Byte(value) => Ok(i64::from(*value)),
        Tag::Short(value) => Ok(i64::from(*value)),
        Tag::Int(value) => Ok(i64::from(*value)),
        Tag::Long(value) => Ok(*value),
        Tag::Float(value) => Ok(*value as i64),
        Tag::Double(value) => Ok(*value as i64),
        _ => Err("Not a number".into()),
    }
}
impl SavedDataCodec for TicketStorage {
    fn decode(tag: &Tag) -> ferraloom_storage::Result<Self> {
        let result = Self::decode_partial(tag);
        match result.error {
            Some(error) => Err(Error::Invalid(error)),
            None => Ok(result.value),
        }
    }
    fn encode(&self) -> ferraloom_storage::Result<Tag> {
        let mut tickets = Vec::new();
        for (traversal, map) in [
            (&self.traversal, &self.tickets),
            (&self.deactivated_traversal, &self.deactivated),
        ] {
            for &key in traversal.iter() {
                for ticket in &map[&key] {
                    if !ticket.kind.persisted() {
                        continue;
                    }
                    if ticket.level < 0 {
                        return Err(Error::Invalid(format!(
                            "Value must be non-negative: {}",
                            ticket.level
                        )));
                    }
                    let position = ChunkPos::unpack(key);
                    let mut entry = CompoundTag::new();
                    entry.insert("chunk_pos", Tag::IntArray(vec![position.x, position.z]));
                    entry.insert(
                        "type",
                        Tag::String(format!("minecraft:{}", ticket.kind.name()).into()),
                    );
                    entry.insert("level", Tag::Int(ticket.level));
                    if ticket.ticks_left != 0 {
                        entry.insert("ticks_left", Tag::Long(ticket.ticks_left));
                    }
                    tickets.push(Tag::Compound(entry));
                }
            }
        }
        let mut map = CompoundTag::new();
        if !tickets.is_empty() {
            map.insert("tickets", Tag::List(ListTag(tickets)));
        }
        Ok(Tag::Compound(map))
    }
}

fn minimum_level(tickets: &[TicketRecord], tracker: TicketTracker) -> i32 {
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
    fn native_saved_ticket_codec_activation_and_mutation_order_match_java() {
        #[derive(serde::Deserialize)]
        struct Parsing {
            input: String,
            success: bool,
            error: Option<String>,
            before: String,
            after: String,
            forced_before: Vec<i64>,
            forced_after: Vec<i64>,
        }
        #[derive(serde::Deserialize)]
        struct Mutation {
            operation: String,
            encoded: String,
            forced: Vec<i64>,
        }
        #[derive(serde::Deserialize)]
        struct Vectors {
            parsing: Vec<Parsing>,
            mutations: Vec<Mutation>,
        }
        let vectors: Vectors = serde_json::from_str(include_str!(
            "../commands/vanilla/ticket-storage-vectors.json"
        ))
        .unwrap();
        let forced = |storage: &TicketStorage| {
            storage
                .get_force_loaded_chunks()
                .iter()
                .copied()
                .collect::<Vec<_>>()
        };
        for vector in vectors.parsing {
            let result =
                TicketStorage::decode_partial(&ferraloom_nbt::parse(&vector.input).unwrap());
            assert_eq!(result.error, vector.error, "{}", vector.input);
            assert_eq!(result.error.is_none(), vector.success, "{}", vector.input);
            let mut storage = result.value;
            assert_eq!(
                storage.encode().unwrap().to_string(),
                vector.before,
                "{}",
                vector.input
            );
            assert_eq!(forced(&storage), vector.forced_before, "{}", vector.input);
            storage.activate_all_deactivated_tickets();
            assert_eq!(
                storage.encode().unwrap().to_string(),
                vector.after,
                "{}",
                vector.input
            );
            assert_eq!(forced(&storage), vector.forced_after, "{}", vector.input);
        }
        let mut storage = TicketStorage::default();
        for vector in vectors.mutations {
            match vector.operation.as_str() {
                "add-24" => {
                    for i in 0..24 {
                        assert!(
                            storage.set_chunk_forced(ChunkPos::new(i % 6 - 3, i / 6 - 2), true)
                        );
                    }
                }
                "duplicate" => assert!(!storage.set_chunk_forced(ChunkPos::new(-3, -2), true)),
                "remove-zero" => assert!(storage.set_chunk_forced(ChunkPos::new(0, 0), false)),
                "remove-negative" => {
                    assert!(storage.set_chunk_forced(ChunkPos::new(-3, -2), false))
                }
                "persistent-types" => {
                    storage.add(
                        ChunkPos::new(6, 7),
                        Ticket {
                            kind: TicketType::Portal,
                            level: 30,
                            expires: None,
                        },
                    );
                    storage.add(
                        ChunkPos::new(8, 9),
                        Ticket {
                            kind: TicketType::PlayerLoading,
                            level: 31,
                            expires: None,
                        },
                    );
                }
                _ => unreachable!(),
            }
            assert_eq!(
                storage.encode().unwrap().to_string(),
                vector.encoded,
                "{}",
                vector.operation
            );
            assert_eq!(forced(&storage), vector.forced, "{}", vector.operation);
        }
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
