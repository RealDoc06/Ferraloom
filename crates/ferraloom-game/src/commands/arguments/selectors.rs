//! Port of EntitySelectorParser and EntitySelectorOptions. Evaluation remains at the caller.
mod live;
pub use live::{EntityReadError, live_entity_matches, selector_read_error};
#[cfg(test)]
mod player;
use super::{identifier, nbt, range::NumberRange};
use crate::{
    commands::{
        CommandSourceStack,
        exceptions::{error, error_at},
        permissions::Permission,
    },
    entity::{EntityRef, player::GameType},
};
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_data::EntityType;
use ferraloom_nbt::Tag;
use ferraloom_util::{Aabb, Uuid, Vec3, mth};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectorSort {
    Arbitrary,
    Nearest,
    Furthest,
    Random,
}
#[derive(Clone, Debug)]
pub enum AdvancementRequirement {
    Done(bool),
    Criteria(BTreeMap<String, bool>),
}
#[derive(Clone, Debug)]
pub enum SelectorPredicate {
    Alive,
    Name {
        value: String,
        inverted: bool,
    },
    GameMode {
        value: GameType,
        inverted: bool,
    },
    Team {
        value: String,
        inverted: bool,
    },
    Type {
        value: String,
        tag: bool,
        inverted: bool,
    },
    Tag {
        value: String,
        inverted: bool,
    },
    Nbt {
        value: Tag,
        inverted: bool,
    },
    Scores(BTreeMap<String, NumberRange>),
    Advancements(BTreeMap<String, AdvancementRequirement>),
    Predicate {
        id: String,
        inverted: bool,
    },
    Level(NumberRange),
    Pitch(NumberRange),
    Yaw(NumberRange),
}
/// Port of EntitySelector; predicates are data, evaluated against live canonical state.
#[derive(Clone, Debug)]
pub struct EntitySelector {
    pub max_results: usize,
    pub includes_entities: bool,
    pub world_limited: bool,
    pub distance: Option<NumberRange>,
    pub position: [Option<f64>; 3],
    pub delta: [Option<f64>; 3],
    pub sort: SelectorSort,
    pub current_entity: bool,
    pub player_name: Option<String>,
    pub uuid: Option<Uuid>,
    pub uses_selectors: bool,
    pub predicates: Vec<SelectorPredicate>,
}
#[derive(Clone, Copy, Default)]
enum Limitation {
    #[default]
    None,
    Single,
    Multiple,
}
#[derive(Default)]
struct Invertible {
    limitation: Limitation,
    tags: BTreeSet<String>,
}
#[derive(Clone, Debug, Default)]
pub(crate) enum CompletionMode {
    #[default]
    NamesOrSelector,
    Names,
    Selector,
    OpenOptions,
    OptionKey,
    NextOrClose,
    GameMode {
        positive: bool,
        negative: bool,
    },
    Type {
        positive: bool,
        negative: bool,
        used_tags: Vec<String>,
    },
    Sort,
    None,
}
#[derive(Default)]
pub(crate) struct CompletionState {
    pub enabled: bool,
    pub mode: CompletionMode,
    pub options: Vec<&'static str>,
}
const OPTION_NAMES: [&str; 21] = [
    "name",
    "distance",
    "level",
    "x",
    "y",
    "z",
    "dx",
    "dy",
    "dz",
    "x_rotation",
    "y_rotation",
    "limit",
    "sort",
    "gamemode",
    "team",
    "type",
    "tag",
    "nbt",
    "scores",
    "advancements",
    "predicate",
];
impl CompletionState {
    fn options(
        &mut self,
        once: &BTreeSet<String>,
        invertibles: &BTreeMap<String, Invertible>,
        current: bool,
    ) {
        if self.enabled {
            self.options = OPTION_NAMES
                .into_iter()
                .filter(|key| {
                    (!current || !matches!(*key, "limit" | "sort"))
                        && (!once.contains(*key) || matches!(*key, "tag" | "nbt" | "predicate"))
                        && invertibles.get(*key).is_none_or(Invertible::available)
                })
                .collect();
        }
    }
}
impl Invertible {
    fn available(&self) -> bool {
        !matches!(self.limitation, Limitation::Single)
    }
    fn accept(&mut self, inverted: bool) -> bool {
        let allowed = if inverted {
            self.available()
        } else {
            matches!(self.limitation, Limitation::None)
        };
        if allowed {
            self.limitation = if inverted {
                Limitation::Multiple
            } else {
                Limitation::Single
            };
        }
        allowed
    }
    fn tag(&mut self, id: String) -> bool {
        if !self.available() || !self.tags.insert(id) {
            return false;
        }
        self.limitation = Limitation::Multiple;
        true
    }
}
fn invert(reader: &mut StringReader) -> bool {
    reader.skip_whitespace();
    if reader.can_read() && reader.peek() == 33 {
        reader.skip();
        reader.skip_whitespace();
        true
    } else {
        false
    }
}
fn rollback(
    reader: &mut StringReader,
    start: usize,
    key: &str,
    args: &[String],
) -> CommandSyntaxException {
    reader.set_cursor(start);
    error_at(reader, key, args)
}
pub fn parse_uuid(text: &str) -> Option<Uuid> {
    if text.len() > 36 {
        return None;
    }
    let parts = text.split('-').collect::<Vec<_>>();
    if parts.len() != 5 {
        return None;
    }
    let values = parts
        .into_iter()
        .map(|part| u64::from_str_radix(part, 16).ok())
        .collect::<Option<Vec<_>>>()?;
    let most =
        (((values[0] & 0xffffffff) << 16) | (values[1] & 0xffff)) << 16 | (values[2] & 0xffff);
    let least = ((values[3] & 0xffff) << 48) | (values[4] & 0xffffffffffff);
    Some(Uuid::new(most as i64, least as i64))
}
impl EntitySelector {
    pub fn parse(
        reader: &mut StringReader,
        allow_selectors: bool,
    ) -> Result<Self, CommandSyntaxException> {
        Self::parse_suggesting(reader, allow_selectors, &mut CompletionState::default())
    }
    pub(crate) fn parse_suggesting(
        reader: &mut StringReader,
        allow_selectors: bool,
        completion: &mut CompletionState,
    ) -> Result<Self, CommandSyntaxException> {
        let mut selector = Self {
            max_results: 1,
            includes_entities: false,
            world_limited: false,
            distance: None,
            position: [None; 3],
            delta: [None; 3],
            sort: SelectorSort::Arbitrary,
            current_entity: false,
            player_name: None,
            uuid: None,
            uses_selectors: false,
            predicates: Vec::new(),
        };
        if !reader.can_read() || reader.peek() != 64 {
            let start = reader.cursor();
            if reader.can_read() {
                completion.mode = CompletionMode::Names;
            }
            let name = reader.read_string()?;
            if let Some(uuid) = parse_uuid(&name) {
                selector.uuid = Some(uuid);
                selector.includes_entities = true;
            } else if name.is_empty() || name.encode_utf16().count() > 16 {
                return Err(rollback(reader, start, "argument.entity.invalid", &[]));
            } else {
                selector.player_name = Some(name);
            }
            return Ok(selector);
        }
        if !allow_selectors {
            return Err(error_at(
                reader,
                "argument.entity.selector.not_allowed",
                &[],
            ));
        }
        reader.skip();
        selector.uses_selectors = true;
        completion.mode = CompletionMode::Selector;
        if !reader.can_read() {
            return Err(error_at(reader, "argument.entity.selector.missing", &[]));
        }
        let start = reader.cursor();
        let kind = reader.read();
        match kind {
            112 => selector.sort = SelectorSort::Nearest,
            97 => selector.max_results = i32::MAX as usize,
            114 => selector.sort = SelectorSort::Random,
            115 => {
                selector.current_entity = true;
                selector.includes_entities = true;
            }
            101 | 110 => {
                selector.includes_entities = true;
                selector.predicates.push(SelectorPredicate::Alive);
                if kind == 101 {
                    selector.max_results = i32::MAX as usize;
                } else {
                    selector.sort = SelectorSort::Nearest;
                }
            }
            _ => {
                return Err(rollback(
                    reader,
                    start,
                    "argument.entity.selector.unknown",
                    &[format!("@{}", String::from_utf16_lossy(&[kind]))],
                ));
            }
        }
        if !selector.includes_entities {
            selector.predicates.push(SelectorPredicate::Type {
                value: "minecraft:player".into(),
                tag: false,
                inverted: false,
            });
        }
        completion.mode = CompletionMode::OpenOptions;
        if !reader.can_read() || reader.peek() != 91 {
            return Ok(selector);
        }
        reader.skip();
        reader.skip_whitespace();
        let mut once = BTreeSet::new();
        let mut invertibles: BTreeMap<String, Invertible> = BTreeMap::new();
        completion.mode = CompletionMode::OptionKey;
        completion.options(&once, &invertibles, selector.current_entity);
        let mut level = None;
        let mut pitch = None;
        let mut yaw = None;
        while reader.can_read() && reader.peek() != 93 {
            reader.skip_whitespace();
            let option_start = reader.cursor();
            let key = reader.read_string()?;
            let repeat = matches!(key.as_str(), "tag" | "nbt" | "predicate");
            let invertible = matches!(key.as_str(), "name" | "gamemode" | "team" | "type");
            let known = repeat
                || invertible
                || matches!(
                    key.as_str(),
                    "distance"
                        | "level"
                        | "x"
                        | "y"
                        | "z"
                        | "dx"
                        | "dy"
                        | "dz"
                        | "x_rotation"
                        | "y_rotation"
                        | "limit"
                        | "sort"
                        | "scores"
                        | "advancements"
                );
            if !known {
                return Err(rollback(
                    reader,
                    option_start,
                    "argument.entity.options.unknown",
                    &[key],
                ));
            }
            let available = if invertible {
                invertibles.entry(key.clone()).or_default().available()
            } else {
                repeat || !once.contains(&key)
            };
            if !available || selector.current_entity && matches!(key.as_str(), "limit" | "sort") {
                return Err(rollback(
                    reader,
                    option_start,
                    "argument.entity.options.inapplicable",
                    &[key],
                ));
            }
            reader.skip_whitespace();
            if !reader.can_read() || reader.peek() != 61 {
                return Err(rollback(
                    reader,
                    option_start,
                    "argument.entity.options.valueless",
                    &[key],
                ));
            }
            reader.skip();
            reader.skip_whitespace();
            let value_start = reader.cursor();
            completion.mode = CompletionMode::None;
            match key.as_str() {
                "name" | "gamemode" | "team" | "type" => {
                    if completion.enabled && matches!(key.as_str(), "gamemode" | "type") {
                        let previous = invertibles.get(&key);
                        let positive = previous
                            .is_none_or(|state| matches!(state.limitation, Limitation::None));
                        let negative = previous.is_none_or(Invertible::available);
                        completion.mode = if key == "gamemode" {
                            CompletionMode::GameMode { positive, negative }
                        } else {
                            CompletionMode::Type {
                                positive,
                                negative,
                                used_tags: previous
                                    .map(|state| state.tags.iter().cloned().collect())
                                    .unwrap_or_default(),
                            }
                        };
                    }
                    let inverted = invert(reader);
                    let state = invertibles.entry(key.clone()).or_default();
                    let tag = key == "type" && reader.can_read() && reader.peek() == 35;
                    if tag {
                        reader.skip();
                        reader.skip_whitespace();
                    }
                    let value = match key.as_str() {
                        "name" => reader.read_string()?,
                        "type" => identifier(reader)?,
                        _ => reader.read_unquoted_string(),
                    };
                    if !(if tag {
                        state.tag(value.clone())
                    } else {
                        state.accept(inverted)
                    }) {
                        return Err(rollback(
                            reader,
                            value_start,
                            "argument.entity.options.inapplicable",
                            &[key],
                        ));
                    }
                    let predicate = match key.as_str() {
                        "name" => SelectorPredicate::Name { value, inverted },
                        "team" => SelectorPredicate::Team { value, inverted },
                        "gamemode" => {
                            let mode = super::game_mode(&value).ok_or_else(|| {
                                rollback(
                                    reader,
                                    value_start,
                                    "argument.entity.options.mode.invalid",
                                    std::slice::from_ref(&value),
                                )
                            })?;
                            selector.includes_entities = false;
                            SelectorPredicate::GameMode {
                                value: mode,
                                inverted,
                            }
                        }
                        _ => {
                            if !tag && EntityType::from_name(&value).is_none() {
                                return Err(rollback(
                                    reader,
                                    value_start,
                                    "argument.entity.options.type.invalid",
                                    &[value],
                                ));
                            }
                            if !tag && !inverted && value == "minecraft:player" {
                                selector.includes_entities = false;
                            }
                            SelectorPredicate::Type {
                                value,
                                tag,
                                inverted,
                            }
                        }
                    };
                    selector.predicates.push(predicate);
                }
                "distance" | "level" | "x_rotation" | "y_rotation" => {
                    let value =
                        NumberRange::parse(reader, key == "level", key.ends_with("rotation"))?;
                    if matches!(key.as_str(), "distance" | "level")
                        && (value.min.is_some_and(|v| v < 0.) || value.max.is_some_and(|v| v < 0.))
                    {
                        return Err(rollback(
                            reader,
                            value_start,
                            if key == "level" {
                                "argument.entity.options.level.negative"
                            } else {
                                "argument.entity.options.distance.negative"
                            },
                            &[],
                        ));
                    }
                    match key.as_str() {
                        "distance" => {
                            selector.distance = Some(value);
                            selector.world_limited = true;
                        }
                        "level" => {
                            level = Some(value);
                            selector.includes_entities = false;
                        }
                        "x_rotation" => pitch = Some(value),
                        _ => yaw = Some(value),
                    }
                }
                "x" | "y" | "z" | "dx" | "dy" | "dz" => {
                    let value = reader.read_double()?;
                    let i = match key.as_bytes().last() {
                        Some(b'x') => 0,
                        Some(b'y') => 1,
                        _ => 2,
                    };
                    if key.len() == 1 {
                        selector.position[i] = Some(value);
                    } else {
                        selector.delta[i] = Some(value);
                    }
                    selector.world_limited = true;
                }
                "limit" => {
                    let limit = reader.read_int()?;
                    if limit < 1 {
                        return Err(rollback(
                            reader,
                            value_start,
                            "argument.entity.options.limit.toosmall",
                            &[],
                        ));
                    }
                    selector.max_results = limit as usize;
                }
                "sort" => {
                    completion.mode = CompletionMode::Sort;
                    let value = reader.read_unquoted_string();
                    selector.sort = match value.as_str() {
                        "nearest" => SelectorSort::Nearest,
                        "furthest" => SelectorSort::Furthest,
                        "random" => SelectorSort::Random,
                        "arbitrary" => SelectorSort::Arbitrary,
                        _ => {
                            return Err(rollback(
                                reader,
                                value_start,
                                "argument.entity.options.sort.irreversible",
                                &[value],
                            ));
                        }
                    };
                }
                "tag" => {
                    let inverted = invert(reader);
                    selector.predicates.push(SelectorPredicate::Tag {
                        value: reader.read_unquoted_string(),
                        inverted,
                    });
                }
                "nbt" => {
                    let inverted = invert(reader);
                    selector.predicates.push(SelectorPredicate::Nbt {
                        value: nbt(reader, true)?,
                        inverted,
                    });
                }
                "predicate" => {
                    let inverted = invert(reader);
                    selector.predicates.push(SelectorPredicate::Predicate {
                        id: identifier(reader)?,
                        inverted,
                    });
                }
                "scores" => {
                    let values = parse_map(
                        reader,
                        |r| Ok(r.read_unquoted_string()),
                        |r| NumberRange::parse(r, true, false),
                    )?;
                    if !values.is_empty() {
                        selector.predicates.push(SelectorPredicate::Scores(values));
                    }
                }
                "advancements" => {
                    let values = parse_map(reader, identifier, |r| {
                        if r.can_read() && r.peek() == 123 {
                            parse_map(
                                r,
                                |r| Ok(r.read_unquoted_string()),
                                StringReader::read_boolean,
                            )
                            .map(AdvancementRequirement::Criteria)
                        } else {
                            r.read_boolean().map(AdvancementRequirement::Done)
                        }
                    })?;
                    if !values.is_empty() {
                        selector.includes_entities = false;
                        selector
                            .predicates
                            .push(SelectorPredicate::Advancements(values));
                    }
                }
                _ => unreachable!(),
            }
            if !repeat && !invertible {
                once.insert(key);
            }
            reader.skip_whitespace();
            completion.mode = CompletionMode::NextOrClose;
            completion.options(&once, &invertibles, selector.current_entity);
            if reader.can_read() {
                if reader.peek() == 44 {
                    reader.skip();
                    completion.mode = CompletionMode::OptionKey;
                } else if reader.peek() == 93 {
                    break;
                } else {
                    return Err(error_at(
                        reader,
                        "argument.entity.options.unterminated",
                        &[],
                    ));
                }
            }
        }
        if !reader.can_read() {
            return Err(error_at(
                reader,
                "argument.entity.options.unterminated",
                &[],
            ));
        }
        reader.skip();
        completion.mode = CompletionMode::None;
        if let Some(range) = pitch {
            selector.predicates.push(SelectorPredicate::Pitch(range));
        }
        if let Some(range) = yaw {
            selector.predicates.push(SelectorPredicate::Yaw(range));
        }
        if let Some(range) = level {
            selector.predicates.push(SelectorPredicate::Level(range));
        }
        Ok(selector)
    }
    /// Captures ordered handles now. The caller executes branches sequentially in the exclusive lane.
    pub fn find<Q: EntityQuery>(
        &self,
        source: &CommandSourceStack,
        query: &mut Q,
        players_only: bool,
    ) -> Result<Vec<EntityRef>, CommandSyntaxException> {
        if self.uses_selectors
            && !source
                .permissions
                .has(&Permission::Atom(Permission::ENTITY_SELECTORS.into()))
        {
            return Err(error("argument.entity.selector.not_allowed", &[]));
        }
        let players_only = players_only || !self.includes_entities;
        if let Some(uuid) = self.uuid {
            let entities = query
                .entity_by_uuid(uuid, players_only)
                .into_iter()
                .collect::<Vec<_>>();
            for entity in &entities {
                query.validate_candidate(*entity)?;
            }
            return Ok(entities);
        }
        let origin = Vec3::new(
            self.position[0].unwrap_or(source.position.x),
            self.position[1].unwrap_or(source.position.y),
            self.position[2].unwrap_or(source.position.z),
        );
        let bounds = if self.delta.iter().any(Option::is_some) {
            let [x, y, z] = self.delta.map(|delta| delta.unwrap_or(0.));
            Some(Aabb::new(
                origin.x + x.min(0.),
                origin.y + y.min(0.),
                origin.z + z.min(0.),
                origin.x + x.max(0.) + 1.,
                origin.y + y.max(0.) + 1.,
                origin.z + z.max(0.) + 1.,
            ))
        } else {
            self.distance.and_then(|range| range.max).map(|max| {
                Aabb::new(
                    origin.x - max,
                    origin.y - max,
                    origin.z - max,
                    origin.x + max + 1.,
                    origin.y + max + 1.,
                    origin.z + max + 1.,
                )
            })
        };
        let candidates = if self.current_entity {
            source.entity.into_iter().collect()
        } else if players_only && self.world_limited {
            query.players_in_level(&source.level.dimension)
        } else {
            query.candidates_in(players_only, bounds)
        };
        let mut result = Vec::new();
        'candidate: for entity in candidates {
            query.validate_candidate(entity)?;
            let Some(info) = query.info(entity) else {
                continue;
            };
            if players_only && !info.player
                || self.world_limited
                    && !self.current_entity
                    && info.dimension != source.level.dimension
                || self
                    .player_name
                    .as_ref()
                    .is_some_and(|name| !name.eq_ignore_ascii_case(info.name.as_ref()))
                || self.uuid.is_some_and(|uuid| uuid != info.uuid)
            {
                continue;
            }
            for predicate in &self.predicates {
                if !query.matches(entity, predicate)? {
                    continue 'candidate;
                }
            }
            let Some(info) = query.info(entity) else {
                continue;
            };
            let dx = info.position.x - origin.x;
            let dy = info.position.y - origin.y;
            let dz = info.position.z - origin.z;
            let distance = dx * dx + dy * dy + dz * dz;
            if let Some(bounds) = bounds
                && !query
                    .bounding_box(entity)
                    .is_some_and(|entity_box| entity_box.intersects(bounds))
            {
                continue;
            }
            if self.distance.is_some_and(|range| {
                range.min.is_some_and(|v| distance < v * v)
                    || range.max.is_some_and(|v| distance > v * v)
            }) {
                continue;
            }
            result.push((entity, distance));
            if self.sort == SelectorSort::Arbitrary && result.len() >= self.max_results {
                break;
            }
        }
        if result.len() > 1 {
            match self.sort {
                SelectorSort::Nearest => result.sort_by(|a, b| a.1.total_cmp(&b.1)),
                SelectorSort::Furthest => result.sort_by(|a, b| b.1.total_cmp(&a.1)),
                SelectorSort::Random => query.shuffle(&mut result),
                SelectorSort::Arbitrary => {}
            }
        }
        result.truncate(self.max_results);
        Ok(result.into_iter().map(|(entity, _)| entity).collect())
    }
}
fn parse_map<K: Ord, V>(
    reader: &mut StringReader,
    mut key: impl FnMut(&mut StringReader) -> Result<K, CommandSyntaxException>,
    mut value: impl FnMut(&mut StringReader) -> Result<V, CommandSyntaxException>,
) -> Result<BTreeMap<K, V>, CommandSyntaxException> {
    reader.expect(123)?;
    reader.skip_whitespace();
    let mut result = BTreeMap::new();
    while reader.can_read() && reader.peek() != 125 {
        reader.skip_whitespace();
        let k = key(reader)?;
        reader.skip_whitespace();
        reader.expect(61)?;
        reader.skip_whitespace();
        let v = value(reader)?;
        result.insert(k, v);
        reader.skip_whitespace();
        if reader.can_read() && reader.peek() == 44 {
            reader.skip();
        }
    }
    reader.expect(125)?;
    Ok(result)
}
/// Immutable facts fetched at evaluation time, not stored in a command source.
pub struct SelectorEntity<'a> {
    pub name: Cow<'a, str>,
    pub uuid: Uuid,
    pub player: bool,
    pub dimension: &'a str,
    pub position: Vec3,
    pub width: f64,
    pub height: f64,
}
/// Live entity queries capture owned handles in native lookup/section order.
pub trait EntityQuery {
    fn candidates(&self, players_only: bool) -> Vec<EntityRef>;
    /// Unbounded entity queries use the linked lookup; bounded ones use the
    /// section index. Global player queries retain PlayerList order.
    fn candidates_in(&self, players_only: bool, _bounds: Option<Aabb>) -> Vec<EntityRef> {
        self.candidates(players_only)
    }
    /// Port of ServerLevel#getPlayers: world-limited player selectors use the
    /// level's player insertion order, independently of global PlayerList order.
    fn players_in_level(&self, dimension: &str) -> Vec<EntityRef> {
        self.candidates(true)
            .into_iter()
            .filter(|entity| {
                self.info(*entity)
                    .is_some_and(|info| info.dimension == dimension)
            })
            .collect()
    }
    fn entity_by_uuid(&self, uuid: Uuid, players_only: bool) -> Option<EntityRef> {
        self.candidates(players_only).into_iter().find(|entity| {
            self.info(*entity)
                .is_some_and(|info| info.uuid == uuid && (!players_only || info.player))
        })
    }
    /// Required live metadata may fail explicitly while its native API is unported.
    fn validate_candidate(&self, _entity: EntityRef) -> Result<(), CommandSyntaxException> {
        Ok(())
    }
    fn info(&self, entity: EntityRef) -> Option<SelectorEntity<'_>>;
    fn bounding_box(&self, entity: EntityRef) -> Option<Aabb> {
        let info = self.info(entity)?;
        Some(Aabb::new(
            info.position.x - info.width / 2.,
            info.position.y,
            info.position.z - info.width / 2.,
            info.position.x + info.width / 2.,
            info.position.y + info.height,
            info.position.z + info.width / 2.,
        ))
    }
    fn display_name(
        &self,
        entity: EntityRef,
    ) -> Result<ferraloom_protocol::chat::Component, CommandSyntaxException> {
        self.info(entity)
            .map(|info| ferraloom_protocol::chat::Component::literal(info.name))
            .ok_or_else(|| error("argument.entity.notfound.entity", &[]))
    }
    fn matches(
        &mut self,
        entity: EntityRef,
        predicate: &SelectorPredicate,
    ) -> Result<bool, CommandSyntaxException>;
    fn shuffle(&mut self, entities: &mut [(EntityRef, f64)]);
}
/// Port of Collections#shuffle: descending Fisher-Yates with java.util.Random's bounded draws.
pub fn shuffle_with_random<T>(
    values: &mut [T],
    random: &mut impl ferraloom_util::random::RandomSource,
) {
    for size in (2..=values.len()).rev() {
        let index = random.next_int_bound(i32::try_from(size).expect("Java list size")) as usize;
        values.swap(size - 1, index);
    }
}
#[cfg(test)]
mod random_tests {
    use super::*;
    use ferraloom_util::random::{LegacyRandomSource, RandomSource};
    #[derive(serde::Deserialize)]
    struct Vector {
        seed: i64,
        size: usize,
        first: Vec<usize>,
        second: Vec<usize>,
        next_long: i64,
    }
    #[test]
    fn random_selector_order_and_consumption_match_java_collections() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("../vanilla/selector-shuffle-vectors.json")).unwrap();
        for vector in vectors {
            let mut random = LegacyRandomSource::new(vector.seed);
            let mut values = (0..vector.size).collect::<Vec<_>>();
            shuffle_with_random(&mut values, &mut random);
            assert_eq!(
                values, vector.first,
                "seed {}, size {}",
                vector.seed, vector.size
            );
            shuffle_with_random(&mut values, &mut random);
            assert_eq!(
                values, vector.second,
                "seed {}, size {}",
                vector.seed, vector.size
            );
            assert_eq!(random.next_long(), vector.next_long);
        }
    }
}
pub fn rotation_matches(range: NumberRange, value: f32) -> bool {
    let min = mth::wrap_degrees_f32(range.min.unwrap_or(0.) as f32);
    let max = mth::wrap_degrees_f32(range.max.unwrap_or(359.) as f32);
    let value = mth::wrap_degrees_f32(value);
    if min > max {
        value >= min || value <= max
    } else {
        value >= min && value <= max
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    #[derive(serde::Deserialize)]
    struct Vector {
        selector: String,
        names: Vec<String>,
        examined: Vec<String>,
        selected: Option<Vec<String>>,
        count: usize,
        current_entity: bool,
    }
    struct Query {
        names: Vec<String>,
        examined: Vec<String>,
        shuffled: usize,
    }
    impl EntityQuery for Query {
        fn candidates(&self, _: bool) -> Vec<EntityRef> {
            (0..self.names.len())
                .map(|id| EntityRef::fixture(crate::level::RegionId::FIXTURE, id as u64))
                .collect()
        }
        fn info(&self, entity: EntityRef) -> Option<SelectorEntity<'_>> {
            let index = entity.serial() as usize;
            Some(SelectorEntity {
                name: self.names[index].as_str().into(),
                uuid: Uuid::NIL,
                player: true,
                dimension: "minecraft:overworld",
                position: Vec3::new((self.names.len() - index) as f64, 0., 0.),
                width: 0.6,
                height: 1.8,
            })
        }
        fn matches(
            &mut self,
            entity: EntityRef,
            predicate: &SelectorPredicate,
        ) -> Result<bool, CommandSyntaxException> {
            if let SelectorPredicate::Type {
                value,
                tag: false,
                inverted,
            } = predicate
            {
                return Ok((value == "minecraft:player") != *inverted);
            }
            let SelectorPredicate::Name { value, inverted } = predicate else {
                panic!("unexpected predicate: {predicate:?}")
            };
            let name = self.names[entity.serial() as usize].clone();
            self.examined.push(name.clone());
            Ok((name == *value) != *inverted)
        }
        fn shuffle(&mut self, entities: &mut [(EntityRef, f64)]) {
            self.shuffled += 1;
            entities.reverse();
        }
    }
    #[test]
    fn candidate_capture_stops_at_vanillas_limit_and_sort_point() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("../vanilla/selector-capture-vectors.json")).unwrap();
        for vector in vectors {
            let selector =
                EntitySelector::parse(&mut StringReader::new(&vector.selector), true).unwrap();
            let mut query = Query {
                names: vector.names,
                examined: Vec::new(),
                shuffled: 0,
            };
            let mut source = CommandSourceStack::server(4, Vec3::ZERO);
            if vector.current_entity {
                source = source
                    .with_entity(EntityRef::fixture(crate::level::RegionId::FIXTURE, 0))
                    .with_level(crate::commands::source::CommandLevel {
                        dimension: "minecraft:the_nether".into(),
                        coordinate_scale: 8.,
                    });
            }
            let selected = selector.find(&source, &mut query, true).unwrap();
            assert_eq!(query.examined, vector.examined, "{}", vector.selector);
            assert_eq!(selected.len(), vector.count, "{}", vector.selector);
            if let Some(names) = vector.selected {
                assert_eq!(
                    selected
                        .iter()
                        .map(|id| query.names[id.serial() as usize].clone())
                        .collect::<Vec<_>>(),
                    names,
                    "{}",
                    vector.selector
                );
            }
            let expected_shuffle = selector.sort == SelectorSort::Random
                && query
                    .examined
                    .iter()
                    .filter(|name| name.as_str() != "Skip")
                    .count()
                    > 1;
            assert_eq!(
                query.shuffled,
                usize::from(expected_shuffle),
                "{}",
                vector.selector
            );
        }
    }
}

#[cfg(test)]
mod mixed_tests;
