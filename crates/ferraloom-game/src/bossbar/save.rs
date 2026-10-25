//! Port of CustomBossEvent.Packed.CODEC and CustomBossEvents' unbounded-map codec.
use super::*;
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_storage::{Error, Result, SavedDataCodec};

const COLORS: [BossBarColor; 7] = [
    BossBarColor::Pink,
    BossBarColor::Blue,
    BossBarColor::Red,
    BossBarColor::Green,
    BossBarColor::Yellow,
    BossBarColor::Purple,
    BossBarColor::White,
];
const COLOR_NAMES: [&str; 7] = ["pink", "blue", "red", "green", "yellow", "purple", "white"];
const OVERLAYS: [BossBarOverlay; 5] = [
    BossBarOverlay::Progress,
    BossBarOverlay::Notched6,
    BossBarOverlay::Notched10,
    BossBarOverlay::Notched12,
    BossBarOverlay::Notched20,
];
const OVERLAY_NAMES: [&str; 5] = [
    "progress",
    "notched_6",
    "notched_10",
    "notched_12",
    "notched_20",
];

/// Immutable save representation. Runtime UUIDs and online player handles are not persisted.
#[derive(Clone, Debug)]
pub struct PackedBossEvent {
    name: Component,
    visible: bool,
    value: i32,
    max: i32,
    color: BossBarColor,
    overlay: BossBarOverlay,
    darken_screen: bool,
    play_boss_music: bool,
    create_world_fog: bool,
    players: HashSet<Uuid>,
}
/// Codec payload only; SavedData supplies the gzip envelope and current DataVersion.
#[derive(Clone, Debug, Default)]
pub struct BossEventsSaveData(HashMap<Key, PackedBossEvent>);

impl CustomBossEvent {
    /// Port of CustomBossEvent#pack. UUID membership has set semantics.
    pub fn pack(&self) -> PackedBossEvent {
        let properties = self.server.properties();
        PackedBossEvent {
            name: self.server.name().clone(),
            visible: self.server.is_visible(),
            value: self.value,
            max: self.max,
            color: self.server.color(),
            overlay: self.server.overlay(),
            darken_screen: properties.darken_screen,
            play_boss_music: properties.play_music,
            create_world_fog: properties.create_world_fog,
            players: self.players.clone(),
        }
    }
    /// Port of CustomBossEvent#load, including value-before-max setter order.
    fn load(id: Uuid, custom_id: Identifier, packed: PackedBossEvent) -> Self {
        let mut event = Self::new(id, custom_id, packed.name);
        let mut packets = Vec::new();
        event.update(BossEventUpdate::Visible(packed.visible), &mut packets);
        event.set_value(packed.value, &mut packets);
        event.set_max(packed.max, &mut packets);
        for update in [
            BossEventUpdate::Color(packed.color),
            BossEventUpdate::Overlay(packed.overlay),
            BossEventUpdate::DarkenScreen(packed.darken_screen),
            BossEventUpdate::PlayBossMusic(packed.play_boss_music),
            BossEventUpdate::CreateWorldFog(packed.create_world_fog),
        ] {
            event.update(update, &mut packets);
        }
        event.players = packed.players;
        event.dirty = false;
        debug_assert!(
            packets.is_empty(),
            "loaded bossbar has no connected viewers"
        );
        event
    }
}
impl CustomBossEvents {
    pub fn pack(&self) -> BossEventsSaveData {
        let mut events = HashMap::new();
        for (id, event) in self.events.iter() {
            events.insert(id.clone(), event.pack());
        }
        BossEventsSaveData(events)
    }
    /// Native loading assigns fresh UUID.randomUUID identities, supplied by the server adapter.
    pub fn load(data: BossEventsSaveData, mut next_uuid: impl FnMut() -> Uuid) -> Self {
        let mut events = HashMap::new();
        for (key, packed) in data.0 {
            events.insert(
                key.clone(),
                CustomBossEvent::load(next_uuid(), key.0, packed),
            );
        }
        let dirty = !events.is_empty(); // CustomBossEvent#load invokes its dirty callback while restoring setters.
        Self {
            events,
            packets: Vec::new(),
            dirty,
        }
    }
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(format!("custom boss events: {}", message.into()))
}
fn protocol<T>(result: ferraloom_protocol::Result<T>) -> Result<T> {
    result.map_err(|e| invalid(e.to_string()))
}
fn integer(tag: &Tag) -> Result<i32> {
    match tag {
        Tag::Byte(v) => Ok(i32::from(*v)),
        Tag::Short(v) => Ok(i32::from(*v)),
        Tag::Int(v) => Ok(*v),
        Tag::Long(v) => Ok(*v as i32),
        Tag::Float(v) => Ok(*v as i32),
        Tag::Double(v) => Ok(*v as i32),
        _ => Err(invalid("expected number")),
    }
}
fn boolean(map: &CompoundTag, key: &str) -> Result<bool> {
    map.get(key)
        .map(|tag| {
            tag.as_double()
                .map(|value| value != 0.0)
                .ok_or_else(|| invalid(key))
        })
        .transpose()
        .map(|value| value.unwrap_or(false))
}
fn enum_index(map: &CompoundTag, key: &str, names: &[&str], default: usize) -> Result<usize> {
    map.get(key)
        .map(|tag| {
            let name = tag
                .as_string()
                .ok_or_else(|| invalid(key))?
                .to_string_lossy();
            names
                .iter()
                .position(|candidate| *candidate == name)
                .ok_or_else(|| invalid(format!("unknown {key}: {name}")))
        })
        .transpose()
        .map(|value| value.unwrap_or(default))
}
fn uuid(tag: &Tag) -> Result<Uuid> {
    let words: Vec<i32> = match tag {
        Tag::IntArray(words) => words.clone(),
        Tag::ByteArray(words) => words.iter().map(|value| i32::from(*value)).collect(),
        Tag::LongArray(words) => words.iter().map(|value| *value as i32).collect(),
        Tag::List(values) => values.0.iter().map(integer).collect::<Result<_>>()?,
        _ => return Err(invalid("expected UUID int stream")),
    };
    Ok(Uuid::from_int_array(
        words
            .try_into()
            .map_err(|_| invalid("UUID requires four numbers"))?,
    ))
}
impl SavedDataCodec for BossEventsSaveData {
    fn decode(tag: &Tag) -> Result<Self> {
        let map = tag
            .as_compound()
            .ok_or_else(|| invalid("expected boss-event map"))?;
        let mut events = HashMap::new();
        for (id, tag) in map.iter() {
            let id = protocol(Identifier::parse(&id.to_string_lossy()))?;
            let map = tag
                .as_compound()
                .ok_or_else(|| invalid("expected packed boss event"))?;
            let name = protocol(Component::from_nbt(
                map.get("Name")
                    .ok_or_else(|| invalid("missing Name"))?
                    .clone(),
            ))?;
            let mut players = HashSet::new();
            if let Some(list) = map.get("Players") {
                for tag in &list
                    .as_list()
                    .ok_or_else(|| invalid("Players must be a list"))?
                    .0
                {
                    players.insert(uuid(tag)?);
                }
            }
            let packed = PackedBossEvent {
                name,
                visible: boolean(map, "Visible")?,
                value: map.get("Value").map(integer).transpose()?.unwrap_or(0),
                max: map.get("Max").map(integer).transpose()?.unwrap_or(100),
                color: COLORS[enum_index(map, "Color", &COLOR_NAMES, 6)?],
                overlay: OVERLAYS[enum_index(map, "Overlay", &OVERLAY_NAMES, 0)?],
                darken_screen: boolean(map, "DarkenScreen")?,
                play_boss_music: boolean(map, "PlayBossMusic")?,
                create_world_fog: boolean(map, "CreateWorldFog")?,
                players,
            };
            events.insert(Key(id), packed);
        }
        Ok(Self(events))
    }
    fn encode(&self) -> Result<Tag> {
        let mut map = CompoundTag::new();
        for (id, event) in self.0.iter() {
            let mut packed = CompoundTag::new();
            packed.insert("Name", protocol(event.name.to_nbt())?);
            for (key, value) in [
                ("Visible", event.visible),
                ("DarkenScreen", event.darken_screen),
                ("PlayBossMusic", event.play_boss_music),
                ("CreateWorldFog", event.create_world_fog),
            ] {
                if value {
                    packed.insert(key, Tag::Byte(1));
                }
            }
            if event.value != 0 {
                packed.insert("Value", Tag::Int(event.value));
            }
            if event.max != 100 {
                packed.insert("Max", Tag::Int(event.max));
            }
            if event.color != BossBarColor::White {
                packed.insert(
                    "Color",
                    Tag::String(COLOR_NAMES[event.color as usize].into()),
                );
            }
            if event.overlay != BossBarOverlay::Progress {
                packed.insert(
                    "Overlay",
                    Tag::String(OVERLAY_NAMES[event.overlay as usize].into()),
                );
            }
            if !event.players.is_empty() {
                packed.insert(
                    "Players",
                    Tag::List(ListTag(
                        event
                            .players
                            .iter()
                            .map(|uuid| Tag::IntArray(uuid.to_int_array().to_vec()))
                            .collect(),
                    )),
                );
            }
            map.insert(id.0.as_str(), Tag::Compound(packed));
        }
        Ok(Tag::Compound(map))
    }
}
