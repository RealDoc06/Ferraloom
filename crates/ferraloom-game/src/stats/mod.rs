//! Vanilla statistic identities, counters and player JSON. Gameplay hooks apply
//! the returned values to scoreboard objectives in the owning player adapter.
mod live;
use crate::loot::{Identifier, LootError, Result, codec};
use ferraloom_data::{Block, EntityType, Item, registries::custom_stat};
use ferraloom_protocol::{
    codec::VarInt,
    play::{
        ClientCommandAction,
        clientbound::{AwardStats, StatValue},
    },
};
use ferraloom_util::collections::{
    HashMap as JavaHashMap, HashSet as JavaHashSet, JavaHash, Object2IntOpenHashMap,
};
pub use live::request_stats;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub use ferraloom_data::registries::custom_stat as custom;
/// Port of Stats' registered StatType objects, preserving registry protocol IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatType {
    Mined,
    Crafted,
    Used,
    Broken,
    PickedUp,
    Dropped,
    Killed,
    KilledBy,
    Custom,
}
impl StatType {
    pub const ALL: &'static [Self] = &[
        Self::Mined,
        Self::Crafted,
        Self::Used,
        Self::Broken,
        Self::PickedUp,
        Self::Dropped,
        Self::Killed,
        Self::KilledBy,
        Self::Custom,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Mined => "minecraft:mined",
            Self::Crafted => "minecraft:crafted",
            Self::Used => "minecraft:used",
            Self::Broken => "minecraft:broken",
            Self::PickedUp => "minecraft:picked_up",
            Self::Dropped => "minecraft:dropped",
            Self::Killed => "minecraft:killed",
            Self::KilledBy => "minecraft:killed_by",
            Self::Custom => "minecraft:custom",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.name() == name)
    }
    pub fn protocol_id(self) -> i32 {
        ferraloom_data::registry_entries("minecraft:stat_type")
            .expect("stat type registry")
            .iter()
            .position(|&name| name == self.name())
            .expect("stat type") as i32
    }
    pub fn registry(self) -> &'static str {
        match self {
            Self::Mined => "minecraft:block",
            Self::Killed | Self::KilledBy => "minecraft:entity_type",
            Self::Custom => "minecraft:custom_stat",
            _ => "minecraft:item",
        }
    }
    pub fn decode_stat(self, value: &Identifier) -> Result<Stat> {
        let missing = || LootError::MissingReference {
            registry: self.registry(),
            id: value.clone(),
        };
        Ok(match self {
            Self::Mined => Stat::Mined(Block::from_name(value.as_str()).ok_or_else(missing)?),
            Self::Killed => {
                Stat::Killed(EntityType::from_name(value.as_str()).ok_or_else(missing)?)
            }
            Self::KilledBy => {
                Stat::KilledBy(EntityType::from_name(value.as_str()).ok_or_else(missing)?)
            }
            Self::Custom => {
                Stat::Custom(custom_stat::Id::from_name(value.as_str()).ok_or_else(missing)?)
            }
            kind => {
                let item = Item::from_name(value.as_str()).ok_or_else(missing)?;
                match kind {
                    Self::Crafted => Stat::Crafted(item),
                    Self::Used => Stat::Used(item),
                    Self::Broken => Stat::Broken(item),
                    Self::PickedUp => Stat::PickedUp(item),
                    Self::Dropped => Stat::Dropped(item),
                    _ => unreachable!(),
                }
            }
        })
    }
}
/// Port of Stat. Variants prevent assigning items to block/entity stat types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stat {
    Mined(Block),
    Crafted(Item),
    Used(Item),
    Broken(Item),
    PickedUp(Item),
    Dropped(Item),
    Killed(EntityType),
    KilledBy(EntityType),
    Custom(custom_stat::Id),
}
impl Stat {
    pub fn stat_type(self) -> StatType {
        match self {
            Self::Mined(_) => StatType::Mined,
            Self::Crafted(_) => StatType::Crafted,
            Self::Used(_) => StatType::Used,
            Self::Broken(_) => StatType::Broken,
            Self::PickedUp(_) => StatType::PickedUp,
            Self::Dropped(_) => StatType::Dropped,
            Self::Killed(_) => StatType::Killed,
            Self::KilledBy(_) => StatType::KilledBy,
            Self::Custom(_) => StatType::Custom,
        }
    }
    pub fn value_name(self) -> &'static str {
        match self {
            Self::Mined(v) => v.name(),
            Self::Crafted(v)
            | Self::Used(v)
            | Self::Broken(v)
            | Self::PickedUp(v)
            | Self::Dropped(v) => v.name(),
            Self::Killed(v) | Self::KilledBy(v) => v.name(),
            Self::Custom(v) => v.name(),
        }
    }
    pub fn protocol_id(self) -> i32 {
        match self {
            Self::Mined(v) => i32::from(v.protocol_id()),
            Self::Crafted(v)
            | Self::Used(v)
            | Self::Broken(v)
            | Self::PickedUp(v)
            | Self::Dropped(v) => i32::from(v.protocol_id()),
            Self::Killed(v) | Self::KilledBy(v) => i32::from(v.protocol_id()),
            Self::Custom(v) => i32::from(v.protocol_id()),
        }
    }
    pub fn objective_name(self) -> String {
        format!(
            "{}:{}",
            self.stat_type().name().replace(':', "."),
            self.value_name().replace(':', ".")
        )
    }
}
impl JavaHash for Stat {
    fn java_hash_code(&self) -> i32 {
        self.stat_type()
            .name()
            .bytes()
            .map(|c| if c == b':' { b'.' } else { c })
            .chain(std::iter::once(b':'))
            .chain(
                self.value_name()
                    .bytes()
                    .map(|c| if c == b':' { b'.' } else { c }),
            )
            .fold(0i32, |hash, c| {
                hash.wrapping_mul(31).wrapping_add(i32::from(c))
            })
    }
}
/// Port of StatsCounter; increment clamps the upper bound only. Negative
/// underflow follows Java's narrowing long-to-int conversion, rather than clamp.
#[derive(Clone, Debug, Default)]
pub struct StatsCounter {
    pub stats: Object2IntOpenHashMap<Stat>,
}
impl StatsCounter {
    pub fn get_value(&self, stat: Stat) -> i32 {
        self.stats.get(&stat).copied().unwrap_or(0)
    }
    pub fn set_value(&mut self, stat: Stat, count: i32) {
        self.stats.insert(stat, count);
    }
    pub fn increment(&mut self, stat: Stat, count: i32) -> i32 {
        let value =
            (i64::from(self.get_value(stat)) + i64::from(count)).min(i64::from(i32::MAX)) as i32;
        self.set_value(stat, value);
        value
    }
}
/// Port of ServerStatsCounter, including same-value writes marking a stat dirty.
#[derive(Clone, Debug, Default)]
pub struct ServerStatsCounter {
    pub counter: StatsCounter,
    dirty: JavaHashSet<Stat>,
}
impl ServerStatsCounter {
    pub fn get_value(&self, stat: Stat) -> i32 {
        self.counter.get_value(stat)
    }
    pub fn set_value(&mut self, stat: Stat, count: i32) {
        self.counter.set_value(stat, count);
        self.dirty.insert(stat);
    }
    pub fn increment(&mut self, stat: Stat, count: i32) -> i32 {
        let value = self.counter.increment(stat, count);
        self.dirty.insert(stat);
        value
    }
    pub fn mark_all_dirty(&mut self) {
        for (stat, _) in self.counter.stats.iter() {
            self.dirty.insert(*stat);
        }
    }
    /// Port of ServerStatsCounter#sendStats. REQUEST_STATS sends only dirty
    /// entries and consumes that set; it does not implicitly mark all stats dirty.
    pub fn send_stats(&mut self) -> AwardStats {
        // JDK 25 HashSet(Collection) uses HashMap.newHashMap(max(size, 12)).
        let mut copied = JavaHashSet::with_capacity((self.dirty.len().max(12) * 4).div_ceil(3));
        for stat in self.dirty.iter() {
            copied.insert(*stat);
        }
        self.dirty.clear();
        let mut result = Object2IntOpenHashMap::new();
        for stat in copied.iter() {
            result.insert(*stat, self.get_value(*stat));
        }
        AwardStats {
            stats: result
                .iter()
                .map(|(stat, count)| StatValue {
                    stat_type: VarInt(stat.stat_type().protocol_id()),
                    value: VarInt(stat.protocol_id()),
                    count: VarInt(*count),
                })
                .collect(),
        }
    }
    pub fn to_json(&self) -> Value {
        let mut grouped = serde_json::Map::new();
        for (stat, count) in self.counter.stats.iter() {
            let entries = grouped
                .entry(stat.stat_type().name())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .expect("stat group");
            entries.insert(stat.value_name().into(), json!(count));
        }
        json!({"stats":grouped,"DataVersion":ferraloom_storage::DATA_VERSION})
    }
    /// Port of STATS_CODEC's partial map decoding. Bad entries produce diagnostics
    /// while valid siblings load; parsing never marks loaded entries dirty.
    pub fn parse(&mut self, v: &Value) -> Result<Vec<String>> {
        let version = v
            .get("DataVersion")
            .map(codec::int)
            .transpose()?
            .unwrap_or(1343);
        if version < ferraloom_storage::DATA_VERSION {
            return Err(LootError::Unsupported(format!("stats.datafix.{version}")));
        }
        let Some(stats) = v.get("stats").filter(|v| !v.is_null()) else {
            return Ok(Vec::new());
        };
        let mut diagnostics = Vec::new();
        let Some(groups) = stats.as_object() else {
            diagnostics.push("stats must be a map".into());
            return Ok(diagnostics);
        };
        let mut flattened = JavaHashMap::new();
        for (kind, values) in groups {
            let decoded = (|| {
                let id = Identifier::parse(kind)?;
                let kind = StatType::from_name(id.as_str())
                    .ok_or_else(|| codec::error(format!("unknown stat type {id}")))?;
                let mut entries = Vec::new();
                let mut problems = Vec::new();
                for (value, count) in codec::object(values)? {
                    match (|| {
                        Ok::<_, LootError>((
                            kind.decode_stat(&Identifier::parse(value)?)?,
                            codec::int(count)?,
                        ))
                    })() {
                        Ok(entry) => entries.push(entry),
                        Err(e) => problems.push(e.to_string()),
                    }
                }
                Ok::<_, LootError>((entries, problems))
            })();
            match decoded {
                Ok((entries, problems)) => {
                    flattened.put_all(entries.into_iter());
                    diagnostics.extend(problems);
                }
                Err(e) => diagnostics.push(e.to_string()),
            }
        }
        self.counter
            .stats
            .ensure_capacity(self.counter.stats.len() + flattened.len());
        for (stat, count) in flattened.iter() {
            self.counter.set_value(*stat, *count);
        }
        Ok(diagnostics)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| codec::error(e.to_string()))?;
        }
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&self.to_json()).map_err(|e| codec::error(e.to_string()))?,
        )
        .map_err(|e| codec::error(e.to_string()))
    }
    pub fn load_file(path: &Path) -> Result<(Self, Vec<String>)> {
        let mut result = Self::default();
        if !path.is_file() {
            return Ok((result, Vec::new()));
        }
        let bytes = std::fs::read(path).map_err(|e| codec::error(e.to_string()))?;
        let json = serde_json::from_slice(&bytes).map_err(|e| codec::error(e.to_string()))?;
        let diagnostics = result.parse(&json)?;
        Ok((result, diagnostics))
    }
    pub fn save_path(world: &Path, uuid: ferraloom_protocol::Uuid) -> PathBuf {
        crate::advancements::player_progress_path(
            world,
            uuid,
            crate::advancements::PlayerProgressDirectory::Stats,
        )
    }
}
/// Pure REQUEST_STATS branch of ServerGamePacketListenerImpl#handleClientCommand.
pub fn handle_client_command(
    counter: &mut ServerStatsCounter,
    action: ClientCommandAction,
) -> Option<AwardStats> {
    match action {
        ClientCommandAction::RequestStats => Some(counter.send_stats()),
        _ => None,
    }
}

/// Port of ServerPlayer#awardStat. The canonical counter saturates first;
/// matching scoreboard objectives synchronously add the original delta.
pub fn award_stat(
    level: &mut crate::level::Level,
    player: crate::entity::EntityRef,
    stat: Stat,
    amount: i32,
) -> std::result::Result<(), crate::level::LevelError> {
    level
        .entity_mut::<crate::entity::player::ServerPlayer>(player)?
        .stats
        .increment(stat, amount);
    statistic_scoreboard(level, player, stat, ScoreboardUpdate::Add(amount))
}
/// Port of ServerPlayer#resetStat (ScoreAccess#reset writes zero).
pub fn reset_stat(
    level: &mut crate::level::Level,
    player: crate::entity::EntityRef,
    stat: Stat,
) -> std::result::Result<(), crate::level::LevelError> {
    level
        .entity_mut::<crate::entity::player::ServerPlayer>(player)?
        .stats
        .set_value(stat, 0);
    statistic_scoreboard(level, player, stat, ScoreboardUpdate::Reset)
}
enum ScoreboardUpdate {
    Add(i32),
    Reset,
}
fn statistic_scoreboard(
    level: &mut crate::level::Level,
    player: crate::entity::EntityRef,
    stat: Stat,
    update: ScoreboardUpdate,
) -> std::result::Result<(), crate::level::LevelError> {
    let criterion = stat.objective_name();
    let scoreboard = level.shared()?.scoreboard.scoreboard();
    let needs_display = scoreboard.objectives_for_criterion(&criterion).any(|name| {
        scoreboard
            .get_objective(name)
            .is_some_and(|objective| objective.display_auto_update)
    });
    // ScoreAccess#set only invokes the native display getter for auto-updated objectives.
    let display = needs_display
        .then(|| level.entity_display_name(player))
        .transpose()?;
    let holder = level.entity_scoreboard_name(player)?.into_owned();
    let scoreboard = &mut level.shared_mut()?.scoreboard;
    match update {
        ScoreboardUpdate::Add(amount) => {
            scoreboard.add_criterion_score(&holder, &criterion, amount, display.as_ref())
        }
        ScoreboardUpdate::Reset => {
            scoreboard.set_criterion_score(&holder, &criterion, 0, display.as_ref())
        }
    }
    .map_err(|_| crate::level::LevelError::Inventory("statistic scoreboard objective"))
}

#[cfg(test)]
mod tests;
