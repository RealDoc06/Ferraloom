use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI8, Ordering},
};

use super::{BlockEntity, BlockEntityLoadError, NbtEditDecision};
use crate::world::World;
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::text::{TextComponent, click::ClickEvent};
use pumpkin_util::version::JavaMinecraftVersion;

pub use pumpkin_data::dye_color::DyeColor;

pub struct SignBlockEntity {
    state: SignState,
    position: BlockPos,
}

pub type Text = SignText;

/// A retained, read-only face handle. Persisted writes go through its owning sign.
#[derive(Clone)]
pub struct SignText {
    has_glowing_text: Arc<AtomicBool>,
    color: Arc<AtomicI8>,
    messages: Arc<Mutex<[SignLine; 4]>>,
    filtered_messages: Arc<Mutex<[SignLine; 4]>>,
}

/// Unknown loaded forms stay in their original line and on disk. They are never
/// interpreted as commands, and live NBT edits refuse them before committing.
#[derive(Clone, Debug, PartialEq)]
enum SignLine {
    Known { component: TextComponent },
    Unsupported(NbtTag),
}

impl SignLine {
    fn literal(text: Box<str>) -> Self {
        Self::Known {
            component: TextComponent::text(text.to_string()),
        }
    }

    fn from_nbt(nbt: &NbtTag) -> Self {
        // The upstream infallible from_nbt silently defaults on decode failure
        // and treats byte booleans as JSON numbers. Validate a bounded subset
        // and use its fallible TextComponent decoder with typed booleans instead.
        component_json(nbt, 0)
            .and_then(|json| serde_json::from_value(json).ok())
            .map_or_else(
                || Self::Unsupported(nbt.clone()),
                |component| Self::Known { component },
            )
    }

    fn text(&self) -> Box<str> {
        match self {
            Self::Known { component } => component.clone().get_text().into(),
            Self::Unsupported(_) => "[Unsupported sign text]".into(),
        }
    }

    fn raw_nbt(&self) -> NbtTag {
        match self {
            Self::Known { component } => component
                .0
                .to_nbt_tag_for_version(&JavaMinecraftVersion::V_26_3),
            Self::Unsupported(nbt) => nbt.clone(),
        }
    }

    fn client_nbt(&self) -> NbtTag {
        match self {
            Self::Known { .. } => self.raw_nbt(),
            Self::Unsupported(_) => NbtTag::String(self.text()),
        }
    }

    fn run_command(&self) -> Option<String> {
        if let Self::Known { component } = self
            && let Some(ClickEvent::RunCommand { command }) = &component.0.style.click_event
        {
            Some(command.to_string())
        } else {
            None
        }
    }

    /// Packet strings are literal contents; only an already trusted style survives.
    fn with_literal_text(&self, text: Box<str>) -> Self {
        let Self::Known { component } = self else {
            return self.clone();
        };
        if component.0.style.is_empty() {
            return Self::literal(text);
        }
        let mut replacement = TextComponent::text(text.to_string());
        replacement.0.style = component.0.style.clone();
        Self::Known {
            component: replacement,
        }
    }
}

/// Ordinary text, styling, extras and run-command events are understood here.
/// Other content/action forms are preserved on load and refused by live edits.
fn component_json(tag: &NbtTag, depth: usize) -> Option<serde_json::Value> {
    if depth > 64 {
        return None;
    }
    if let NbtTag::String(text) = tag {
        return Some(serde_json::Value::String(text.to_string()));
    }
    let NbtTag::Compound(nbt) = tag else {
        return None;
    };
    nbt.get_string("text")?;
    let mut json = serde_json::Map::new();
    for (key, value) in &nbt.child_tags {
        let value = match key.as_ref() {
            "text" | "insertion" => serde_json::Value::String(value.extract_string()?.into()),
            "type" if value.extract_string()? == "text" => continue,
            "color" => {
                let color = value.extract_string()?;
                let valid = matches!(
                    color,
                    "black"
                        | "dark_blue"
                        | "dark_green"
                        | "dark_aqua"
                        | "dark_red"
                        | "dark_purple"
                        | "gold"
                        | "gray"
                        | "dark_gray"
                        | "blue"
                        | "green"
                        | "aqua"
                        | "red"
                        | "light_purple"
                        | "yellow"
                        | "white"
                ) || (color.len() == 7
                    && color.starts_with('#')
                    && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit));
                if !valid {
                    return None;
                }
                serde_json::Value::String(color.into())
            }
            "bold" | "italic" | "underlined" | "strikethrough" | "obfuscated" => {
                let NbtTag::Byte(value) = value else {
                    return None;
                };
                serde_json::Value::Bool(*value != 0)
            }
            "extra" => {
                let NbtTag::List(children) = value else {
                    return None;
                };
                if children.is_empty() {
                    return None;
                }
                // The upstream extra decoder expects objects rather than strings.
                let children = children
                    .iter()
                    .map(|child| {
                        let json = component_json(child, depth + 1)?;
                        Some(match json {
                            serde_json::Value::String(text) => serde_json::json!({"text": text}),
                            object => object,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                serde_json::Value::Array(children)
            }
            "click_event" => {
                let NbtTag::Compound(event) = value else {
                    return None;
                };
                if event.child_tags.len() != 2 || event.get_string("action")? != "run_command" {
                    return None;
                }
                let command = event.get_string("command")?;
                // RunCommand.CODEC uses ExtraCodecs.CHAT_STRING in 26.3.
                if command
                    .chars()
                    .any(|character| character < ' ' || matches!(character, '\u{7f}' | '\u{a7}'))
                {
                    return None;
                }
                serde_json::json!({"action": "run_command", "command": command})
            }
            _ => return None,
        };
        json.insert(key.to_string(), value);
    }
    Some(serde_json::Value::Object(json))
}

/// Known components use the 26.3 codec's compact strings. The binary NBT writer
/// supplies physical wrappers for heterogeneous lists; semantic tags stay mixed.
fn encode_lines(lines: &[SignLine; 4], for_client: bool) -> Vec<NbtTag> {
    lines
        .iter()
        .map(|line| {
            if for_client {
                line.client_nbt()
            } else {
                line.raw_nbt()
            }
        })
        .collect()
}

struct FaceData {
    messages: [SignLine; 4],
    filtered_messages: [SignLine; 4],
    color: DyeColor,
    has_glowing_text: bool,
}

impl Default for FaceData {
    fn default() -> Self {
        let messages = std::array::from_fn(|_| SignLine::literal("".into()));
        Self {
            filtered_messages: messages.clone(),
            messages,
            color: DyeColor::Black,
            has_glowing_text: false,
        }
    }
}

impl FaceData {
    fn decode_lines(tag: Option<&NbtTag>) -> Option<[SignLine; 4]> {
        let NbtTag::List(lines) = tag? else {
            return None;
        };
        if lines.len() != 4 {
            return None;
        }
        // The binary NBT reader already unwraps exactly one empty-key transport
        // wrapper at each list-element boundary. Remaining compounds are authored
        // values (including protected nested wrappers), never another wrapper to
        // peel here. The SNBT parser, like Java NbtOps.createList, keeps them intact.
        Some(std::array::from_fn(|index| {
            SignLine::from_nbt(&lines[index])
        }))
    }

    fn from_nbt(tag: Option<&NbtTag>) -> Self {
        let Some(NbtTag::Compound(nbt)) = tag else {
            return Self::default();
        };
        // 26.3 requires exactly four raw lines. Its filtered field is lenient:
        // a missing or invalid list uses the raw messages, never shifts lines.
        let Some(messages) = Self::decode_lines(nbt.get("messages")) else {
            return Self::default();
        };
        let Some(color) = DyeColor::by_name(nbt.get_string("color").unwrap_or("black")) else {
            return Self::default();
        };
        Self {
            filtered_messages: Self::decode_lines(nbt.get("filtered_messages"))
                .unwrap_or_else(|| messages.clone()),
            messages,
            color,
            has_glowing_text: nbt.get_bool("has_glowing_text").unwrap_or(false),
        }
    }

    fn supported(&self) -> bool {
        self.messages
            .iter()
            .chain(&self.filtered_messages)
            .all(|line| matches!(line, SignLine::Known { .. }))
    }

    fn to_nbt(&self, for_client: bool) -> NbtTag {
        let mut nbt = NbtCompound::new();
        nbt.put_bool("has_glowing_text", self.has_glowing_text);
        nbt.put_string("color", self.color.name().to_string());
        nbt.put_list("messages", encode_lines(&self.messages, for_client));
        if self.messages != self.filtered_messages {
            nbt.put_list(
                "filtered_messages",
                encode_lines(&self.filtered_messages, for_client),
            );
        }
        NbtTag::Compound(nbt)
    }
}

impl Default for SignText {
    fn default() -> Self {
        Self::from_data(FaceData::default())
    }
}

impl SignText {
    pub const LINES: usize = 4;

    fn from_data(data: FaceData) -> Self {
        Self {
            has_glowing_text: Arc::new(AtomicBool::new(data.has_glowing_text)),
            color: Arc::new(AtomicI8::new(data.color.id() as i8)),
            messages: Arc::new(Mutex::new(data.messages)),
            filtered_messages: Arc::new(Mutex::new(data.filtered_messages)),
        }
    }

    fn snapshot_unlocked(&self) -> FaceData {
        let messages = self
            .messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let filtered = self
            .filtered_messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        FaceData {
            messages: messages.clone(),
            filtered_messages: filtered.clone(),
            color: self.get_color(),
            has_glowing_text: self.has_glowing_text(),
        }
    }

    fn apply_unlocked(&self, data: FaceData) {
        let mut messages = self
            .messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut filtered = self
            .filtered_messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *messages = data.messages;
        *filtered = data.filtered_messages;
        self.color.store(data.color.id() as i8, Ordering::Relaxed);
        self.has_glowing_text
            .store(data.has_glowing_text, Ordering::Relaxed);
    }

    #[must_use]
    pub fn empty_messages() -> [Box<str>; 4] {
        std::array::from_fn(|_| "".into())
    }

    #[must_use]
    pub fn from_messages(messages: [Box<str>; 4]) -> Self {
        let messages = messages.map(SignLine::literal);
        Self::from_data(FaceData {
            filtered_messages: messages.clone(),
            messages,
            ..FaceData::default()
        })
    }

    #[must_use]
    pub fn has_glowing_text(&self) -> bool {
        self.has_glowing_text.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn get_color(&self) -> DyeColor {
        DyeColor::by_id(self.color.load(Ordering::Relaxed) as u8).unwrap_or(DyeColor::Black)
    }

    #[must_use]
    pub fn get_message(&self, index: usize, should_filter: bool) -> Box<str> {
        self.get_messages(should_filter)
            .get(index)
            .cloned()
            .unwrap_or_else(|| "".into())
    }

    #[must_use]
    pub fn get_messages(&self, should_filter: bool) -> [Box<str>; 4] {
        let messages = if should_filter {
            &self.filtered_messages
        } else {
            &self.messages
        }
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::array::from_fn(|index| messages[index].text())
    }

    #[must_use]
    pub fn has_message(&self, should_filter: bool) -> bool {
        self.get_messages(should_filter)
            .iter()
            .any(|msg| !msg.is_empty())
    }

    #[must_use]
    pub fn has_editable_text(&self) -> bool {
        self.messages
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .all(|line| matches!(line, SignLine::Known { .. }))
    }

    #[cfg(test)]
    pub(crate) fn shares_message_storage(&self, other: &Self, filtered: bool) -> bool {
        if filtered {
            Arc::ptr_eq(&self.filtered_messages, &other.filtered_messages)
        } else {
            Arc::ptr_eq(&self.messages, &other.messages)
        }
    }
}

struct SignData {
    front: FaceData,
    back: FaceData,
    waxed: bool,
    allow_op_features: bool,
}

impl SignData {
    fn from_nbt(nbt: &NbtCompound) -> Self {
        Self {
            front: FaceData::from_nbt(nbt.get("front_text")),
            back: FaceData::from_nbt(nbt.get("back_text")),
            waxed: nbt.get_bool("is_waxed").unwrap_or(false),
            allow_op_features: nbt.get_bool("allow_op_features").unwrap_or(false),
        }
    }

    fn validate(&self) -> Result<(), BlockEntityLoadError> {
        if self.front.supported() && self.back.supported() {
            Ok(())
        } else {
            Err(BlockEntityLoadError::Unsupported("sign component form"))
        }
    }
}

/// One gate covers both faces, wax and privileged actions. Inner message Arcs and
/// editor ownership survive reloads. No external callback is made under this gate.
pub(super) struct SignState {
    gate: Mutex<()>,
    front: SignText,
    back: SignText,
    waxed: AtomicBool,
    allow_op_features: AtomicBool,
    editor: Arc<Mutex<Option<uuid::Uuid>>>,
    #[cfg(test)]
    edit_hook: Mutex<Option<Box<dyn FnOnce() + Send + Sync>>>,
}

impl SignState {
    pub(super) fn from_nbt(nbt: &NbtCompound) -> Self {
        let data = SignData::from_nbt(nbt);
        Self {
            gate: Mutex::new(()),
            front: SignText::from_data(data.front),
            back: SignText::from_data(data.back),
            waxed: AtomicBool::new(data.waxed),
            allow_op_features: AtomicBool::new(data.allow_op_features),
            editor: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            edit_hook: Mutex::new(None),
        }
    }

    pub(super) fn new(is_front: bool, messages: [Box<str>; 4]) -> Self {
        let mut state = Self::from_nbt(&NbtCompound::new());
        if is_front {
            state.front = SignText::from_messages(messages);
        } else {
            state.back = SignText::from_messages(messages);
        }
        state
    }

    fn apply_unlocked(&self, data: SignData) {
        self.front.apply_unlocked(data.front);
        self.back.apply_unlocked(data.back);
        self.waxed.store(data.waxed, Ordering::Relaxed);
        self.allow_op_features
            .store(data.allow_op_features, Ordering::Relaxed);
    }

    fn write_unlocked(&self, nbt: &mut NbtCompound, for_client: bool) {
        nbt.put(
            "front_text",
            self.front.snapshot_unlocked().to_nbt(for_client),
        );
        nbt.put(
            "back_text",
            self.back.snapshot_unlocked().to_nbt(for_client),
        );
        nbt.put_bool("is_waxed", self.waxed.load(Ordering::Relaxed));
        if self.allow_op_features.load(Ordering::Relaxed) {
            nbt.put_bool("allow_op_features", true);
        } else {
            nbt.child_tags.remove("allow_op_features");
        }
    }

    fn full_nbt_unlocked(&self, id: &str, position: BlockPos) -> NbtCompound {
        let mut nbt = NbtCompound::new();
        nbt.put_string("id", id.to_string());
        nbt.put_int("x", position.0.x);
        nbt.put_int("y", position.0.y);
        nbt.put_int("z", position.0.z);
        self.write_unlocked(&mut nbt, false);
        nbt
    }

    pub(super) fn load_nbt(&self, nbt: &NbtCompound) -> Result<(), BlockEntityLoadError> {
        let data = SignData::from_nbt(nbt);
        data.validate()?;
        let _gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.apply_unlocked(data);
        Ok(())
    }

    pub(super) fn edit_nbt(
        &self,
        id: &str,
        position: BlockPos,
        edit: &mut dyn FnMut(&mut NbtCompound) -> NbtEditDecision,
        publish: &mut dyn FnMut(NbtCompound),
    ) -> Result<bool, BlockEntityLoadError> {
        let _gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut nbt = self.full_nbt_unlocked(id, position);
        #[cfg(test)]
        {
            let hook = self
                .edit_hook
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            if let Some(hook) = hook {
                hook();
            }
        }
        if edit(&mut nbt) == NbtEditDecision::Abort {
            return Ok(false);
        }
        let data = SignData::from_nbt(&nbt);
        data.validate()?;
        self.apply_unlocked(data);
        publish(self.full_nbt_unlocked(id, position));
        Ok(true)
    }

    pub(super) fn snapshot_nbt(
        &self,
        id: &str,
        position: BlockPos,
        publish: &mut dyn FnMut(NbtCompound),
    ) {
        let _gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        publish(self.full_nbt_unlocked(id, position));
    }

    pub(super) fn write_nbt(&self, nbt: &mut NbtCompound, for_client: bool) {
        let _gate = self
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.write_unlocked(nbt, for_client);
    }

    fn item_editor_matches(&self, player: uuid::Uuid) -> bool {
        self.editor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_none_or(|editor| editor == player)
    }

    pub(super) fn tick(&self, world: &World, position: &BlockPos) {
        let editor = *self
            .editor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(id) = editor {
            // Official 26.3 playerIsTooFarAwayToEdit uses interaction range + 4.
            let valid = world
                .get_player_by_uuid(id)
                .is_some_and(|player| player.can_interact_with_block_at(position, 4.0));
            if !valid {
                let mut current = self
                    .editor
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if *current == Some(id) {
                    *current = None;
                }
            }
        }
    }
}

/// Pure client projection for chunk packets, which start from pending disk NBT.
/// Unsupported loaded lines retain their index and remain unchanged on disk.
#[must_use]
pub(crate) fn client_nbt_from_persisted(nbt: &NbtCompound) -> NbtCompound {
    let data = SignData::from_nbt(nbt);
    let mut client = nbt.clone();
    client.put("front_text", data.front.to_nbt(true));
    client.put("back_text", data.back.to_nbt(true));
    client.put_bool("is_waxed", data.waxed);
    if data.allow_op_features {
        client.put_bool("allow_op_features", true);
    } else {
        client.child_tags.remove("allow_op_features");
    }
    client
}

impl BlockEntity for SignBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }
    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &NbtCompound, position: BlockPos) -> Self {
        Self {
            position,
            state: SignState::from_nbt(nbt),
        }
    }
    fn load_nbt(&self, nbt: &NbtCompound) -> Result<(), BlockEntityLoadError> {
        self.state.load_nbt(nbt)
    }
    fn edit_nbt(
        &self,
        edit: &mut dyn FnMut(&mut NbtCompound) -> NbtEditDecision,
        publish: &mut dyn FnMut(NbtCompound),
    ) -> Result<bool, BlockEntityLoadError> {
        self.state.edit_nbt(Self::ID, self.position, edit, publish)
    }
    fn snapshot_nbt(&self, publish: &mut dyn FnMut(NbtCompound)) {
        self.state.snapshot_nbt(Self::ID, self.position, publish);
    }
    fn write_nbt(&self, nbt: &mut NbtCompound) {
        self.state.write_nbt(nbt, false);
    }
    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.state.write_nbt(&mut nbt, true);
        Some(nbt)
    }
    fn tick(&self, world: &Arc<World>) {
        self.state.tick(world, &self.position);
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SignBlockEntity {
    pub const ID: &'static str = "minecraft:sign";
    #[must_use]
    pub fn new(position: BlockPos, is_front: bool, messages: [Box<str>; 4]) -> Self {
        Self {
            position,
            state: SignState::new(is_front, messages),
        }
    }
    #[must_use]
    pub fn empty(position: BlockPos) -> Self {
        Self::new(position, true, SignText::empty_messages())
    }
}

pub enum SignEntityRef<'a> {
    Sign(&'a SignBlockEntity),
    Hanging(&'a super::hanging_sign::HangingSignBlockEntity),
}

impl<'a> SignEntityRef<'a> {
    pub fn from_block_entity(entity: &'a dyn BlockEntity) -> Option<Self> {
        entity
            .as_any()
            .downcast_ref::<SignBlockEntity>()
            .map(Self::Sign)
            .or_else(|| {
                entity
                    .as_any()
                    .downcast_ref::<super::hanging_sign::HangingSignBlockEntity>()
                    .map(Self::Hanging)
            })
    }

    const fn state(&self) -> &'a SignState {
        match self {
            Self::Sign(sign) => &sign.state,
            Self::Hanging(sign) => &sign.state,
        }
    }
    #[must_use]
    pub const fn front_text(&self) -> &'a SignText {
        &self.state().front
    }
    #[must_use]
    pub const fn back_text(&self) -> &'a SignText {
        &self.state().back
    }
    #[must_use]
    pub const fn get_text(&self, is_front: bool) -> &'a SignText {
        if is_front {
            self.front_text()
        } else {
            self.back_text()
        }
    }
    #[must_use]
    pub fn is_waxed(&self) -> bool {
        self.state().waxed.load(Ordering::Relaxed)
    }
    #[must_use]
    pub const fn currently_editing_player(&self) -> &'a Arc<Mutex<Option<uuid::Uuid>>> {
        &self.state().editor
    }

    /// Atomically recheck the designated editor/wax after the plugin event and
    /// replace all raw/filtered lines together. Returns before any world update.
    #[must_use]
    pub fn update_player_text(
        &self,
        editor: uuid::Uuid,
        is_front: bool,
        should_filter: bool,
        messages: [Box<str>; 4],
    ) -> bool {
        let state = self.state();
        let _gate = state
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut current = state
            .editor
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.waxed.load(Ordering::Relaxed) || *current != Some(editor) {
            return false;
        }
        let face = self.get_text(is_front);
        let mut data = face.snapshot_unlocked();
        if !data.supported() {
            return false;
        }
        let previous = if should_filter {
            &data.filtered_messages
        } else {
            &data.messages
        };
        data.messages =
            std::array::from_fn(|index| previous[index].with_literal_text(messages[index].clone()));
        data.filtered_messages = data.messages.clone();
        face.apply_unlocked(data);
        *current = None;
        true
    }

    /// Item preconditions and persisted changes share the command mutation gate.
    #[must_use]
    pub fn apply_color(&self, player: uuid::Uuid, is_front: bool, color: DyeColor) -> bool {
        let state = self.state();
        let _gate = state
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let face = self.get_text(is_front);
        if state.waxed.load(Ordering::Relaxed)
            || !state.item_editor_matches(player)
            || face.get_color() == color
        {
            return false;
        }
        face.color.store(color.id() as i8, Ordering::Relaxed);
        true
    }
    #[must_use]
    pub fn apply_glowing(&self, player: uuid::Uuid, is_front: bool, glowing: bool) -> bool {
        let state = self.state();
        let _gate = state
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let face = self.get_text(is_front);
        if state.waxed.load(Ordering::Relaxed)
            || !state.item_editor_matches(player)
            || face.has_glowing_text() == glowing
        {
            return false;
        }
        face.has_glowing_text.store(glowing, Ordering::Relaxed);
        true
    }
    #[must_use]
    pub fn apply_wax(&self, player: uuid::Uuid) -> bool {
        let state = self.state();
        let _gate = state
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.item_editor_matches(player) {
            return false;
        }
        !state.waxed.swap(true, Ordering::Relaxed)
    }

    /// Clone the complete dispatch input, then release every sign lock. A command
    /// can now read or edit this exact sign without reentering a held message gate.
    #[must_use]
    pub fn click_commands(&self, is_front: bool, should_filter: bool) -> (bool, Vec<String>) {
        let state = self.state();
        let _gate = state
            .gate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let face = self.get_text(is_front);
        let data = face.snapshot_unlocked();
        let lines = if should_filter {
            data.filtered_messages
        } else {
            data.messages
        };
        (
            state.allow_op_features.load(Ordering::Relaxed),
            lines.iter().filter_map(SignLine::run_command).collect(),
        )
    }

    #[cfg(test)]
    pub(crate) fn on_next_edit(&self, hook: impl FnOnce() + Send + Sync + 'static) {
        *self
            .state()
            .edit_hook
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(crate) fn mutation_is_locked(&self) -> bool {
        matches!(
            self.state().gate.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        )
    }
}

#[cfg(test)]
#[path = "sign_tests.rs"]
mod tests;
