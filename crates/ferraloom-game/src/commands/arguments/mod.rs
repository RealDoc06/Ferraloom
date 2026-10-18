//! Minecraft argument values retain their parsed type and their network descriptor.
pub mod blocks;
pub mod coordinates;
pub mod items;
pub mod operation;
pub mod range;
pub mod scoreboard;
pub mod selectors;
pub mod slots;
pub mod wire;
use crate::{
    commands::{
        CommandSourceStack,
        exceptions::{error, error_at},
        permissions::Permission,
    },
    entity::player::GameType,
};
use coordinates::{CoordinateKind, Coordinates};
use ferraloom_brigadier::{
    self as brigadier, ArgumentType, CommandContext, CommandSyntaxException, StringReader,
    Suggestions, SuggestionsBuilder,
};
use ferraloom_nbt::{NbtPath, Tag};
use ferraloom_protocol::{
    Identifier, Uuid,
    chat::{Component, Style},
    play::CommandArgument,
};
use operation::Operation;
use range::NumberRange;
use selectors::{EntitySelector, parse_uuid};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum ArgumentValue {
    Slots(slots::SlotRange),
    DisplaySlot(u8),
    Criteria(scoreboard::ObjectiveCriteria),
    Resources(Vec<String>),
    Block(blocks::BlockInput),
    BlockPredicate(blocks::BlockPredicate),
    Item(items::ItemInput),
    Boolean(bool),
    Integer(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    String(String),
    Coordinates(Coordinates),
    Entity(EntitySelector),
    GameMode(GameType),
    Identifier(String),
    Tag(Tag),
    Path(NbtPath),
    Component(Component),
    Style(Style),
    Range(NumberRange),
    Angle { value: f32, relative: bool },
    Swizzle(u8),
    Operation(Operation),
    Uuid(Uuid),
    Message(Message),
    HexColor(u32),
}
#[derive(Clone, Debug)]
pub struct Message {
    pub text: String,
    pub parts: Vec<MessagePart>,
}
#[derive(Clone, Debug)]
pub struct MessagePart {
    pub start: usize,
    pub end: usize,
    pub selector: EntitySelector,
}
impl Message {
    /// Port of MessageArgument.Message#parseText, including literal unknown @ prefixes.
    pub fn parse(
        reader: &mut StringReader,
        allow_selectors: bool,
    ) -> Result<Self, CommandSyntaxException> {
        if reader.remaining_length() > 256 {
            return Err(error(
                "argument.message.too_long",
                &[reader.remaining_length().to_string(), "256".into()],
            ));
        }
        let text = reader.remaining();
        let offset = reader.cursor();
        let mut parts = Vec::new();
        if !allow_selectors {
            reader.set_cursor(reader.total_length());
            return Ok(Self { text, parts });
        }
        while reader.can_read() {
            if reader.peek() != 64 {
                reader.skip();
                continue;
            }
            let start = reader.cursor();
            match EntitySelector::parse(reader, true) {
                Ok(selector) => parts.push(MessagePart {
                    start: start - offset,
                    end: reader.cursor() - offset,
                    selector,
                }),
                Err(e) if matches!(&e.kind,brigadier::ExceptionKind::Custom(key) if key=="argument.entity.selector.missing"||key=="argument.entity.selector.unknown") => {
                    reader.set_cursor(start + 1)
                }
                Err(e) => return Err(e),
            }
        }
        Ok(Self { text, parts })
    }
}
#[derive(Clone, Debug)]
pub struct MinecraftArgument {
    pub context: Arc<crate::commands::context::CommandBuildContext>,
    pub wire: CommandArgument,
    pub examples: Arc<[String]>,
}
impl MinecraftArgument {
    pub fn new(wire: CommandArgument) -> Self {
        Self {
            context: Arc::default(),
            wire,
            examples: Arc::from([]),
        }
    }
    fn parse_impl(
        &self,
        reader: &mut StringReader,
        allow_selectors: bool,
    ) -> Result<ArgumentValue, CommandSyntaxException> {
        use ArgumentValue as V;
        use CommandArgument as A;
        let coordinate = match self.wire {
            A::BlockPos {} => Some(CoordinateKind::BlockPos),
            A::ColumnPos {} => Some(CoordinateKind::ColumnPos),
            A::Vec3 {} => Some(CoordinateKind::Vec3),
            A::Vec2 {} => Some(CoordinateKind::Vec2),
            A::Rotation {} => Some(CoordinateKind::Rotation),
            _ => None,
        };
        if let Some(kind) = coordinate {
            return Coordinates::parse(reader, kind).map(V::Coordinates);
        }
        Ok(match &self.wire {
            A::Bool {} => V::Boolean(brigadier::BoolArgumentType.parse(reader)?),
            A::Integer { bounds } => V::Integer(
                brigadier::IntegerArgumentType::new(
                    bounds.min.unwrap_or(i32::MIN),
                    bounds.max.unwrap_or(i32::MAX),
                )
                .parse(reader)?,
            ),
            A::Long { bounds } => V::Long(
                brigadier::LongArgumentType::new(
                    bounds.min.unwrap_or(i64::MIN),
                    bounds.max.unwrap_or(i64::MAX),
                )
                .parse(reader)?,
            ),
            A::Float { bounds } => V::Float(
                brigadier::FloatArgumentType::new(
                    bounds.min.unwrap_or(-f32::MAX),
                    bounds.max.unwrap_or(f32::MAX),
                )
                .parse(reader)?,
            ),
            A::Double { bounds } => V::Double(
                brigadier::DoubleArgumentType::new(
                    bounds.min.unwrap_or(-f64::MAX),
                    bounds.max.unwrap_or(f64::MAX),
                )
                .parse(reader)?,
            ),
            A::String { string_type } => V::String(match string_type {
                ferraloom_protocol::play::StringArgumentType::SingleWord => {
                    reader.read_unquoted_string()
                }
                ferraloom_protocol::play::StringArgumentType::QuotablePhrase => {
                    reader.read_string()?
                }
                ferraloom_protocol::play::StringArgumentType::GreedyPhrase => {
                    let text = reader.remaining();
                    reader.set_cursor(reader.total_length());
                    text
                }
            }),
            A::Entity { flags } => {
                let selector = EntitySelector::parse(reader, allow_selectors)?;
                if flags.single && selector.max_results > 1 {
                    reader.set_cursor(0);
                    return Err(error_at(
                        reader,
                        if flags.players_only {
                            "argument.player.toomany"
                        } else {
                            "argument.entity.toomany"
                        },
                        &[],
                    ));
                }
                if flags.players_only && selector.includes_entities && !selector.current_entity {
                    reader.set_cursor(0);
                    return Err(error_at(reader, "argument.player.entities", &[]));
                }
                V::Entity(selector)
            }
            A::GameProfile {} => {
                if reader.can_read() && reader.peek() == 64 {
                    let selector = EntitySelector::parse(reader, allow_selectors)?;
                    if selector.includes_entities {
                        return Err(error_at(reader, "argument.player.entities", &[]));
                    }
                    V::Entity(selector)
                } else {
                    V::String(word(reader))
                }
            }
            A::Gamemode {} => {
                let value = reader.read_unquoted_string();
                V::GameMode(
                    game_mode(&value)
                        .ok_or_else(|| error_at(reader, "argument.gamemode.invalid", &[value]))?,
                )
            }
            A::Message {} => V::Message(Message::parse(reader, allow_selectors)?),
            A::ResourceLocation {} | A::Dimension {} | A::ResourceKey { .. } => {
                V::Identifier(identifier(reader)?)
            }
            A::Resource { registry } => {
                let value = identifier(reader)?;
                if !self.context.contains(registry.as_str(), &value) {
                    return Err(error_at(
                        reader,
                        "argument.resource.not_found",
                        &[value, registry.to_string()],
                    ));
                }
                V::Identifier(value)
            }
            A::ResourceOrTag { registry } => {
                let start = reader.cursor();
                if reader.can_read() && reader.peek() == 35 {
                    reader.skip();
                    let parsed = (|| {
                        let value = identifier(reader)?;
                        if self.context.tag(registry.as_str(), &value).is_none() {
                            return Err(error_at(
                                reader,
                                "argument.resource_tag.not_found",
                                &[value, registry.to_string()],
                            ));
                        }
                        Ok(V::Identifier(format!("#{value}")))
                    })();
                    if parsed.is_err() {
                        reader.set_cursor(start);
                    }
                    parsed?
                } else {
                    let value = identifier(reader)?;
                    if !self.context.contains(registry.as_str(), &value) {
                        return Err(error_at(
                            reader,
                            "argument.resource.not_found",
                            &[value, registry.to_string()],
                        ));
                    }
                    V::Identifier(value)
                }
            }
            A::Function {} | A::ResourceOrTagKey { .. } => {
                let tag = reader.can_read() && reader.peek() == 35;
                if tag {
                    reader.skip();
                }
                let value = identifier(reader)?;
                V::Identifier(if tag { format!("#{value}") } else { value })
            }
            A::BlockState {} => V::Block(blocks::parse(reader)?),
            A::BlockPredicate {} => V::BlockPredicate(blocks::predicate(reader, &self.context)?),
            A::ItemStack {} => V::Item(items::parse(reader)?),
            A::ItemSlot {} => V::Integer(slots::parse(reader, true)?.slots[0]),
            A::ItemSlots {} => V::Slots(slots::parse(reader, false)?),
            A::NbtTag {} => V::Tag(nbt(reader, false)?),
            A::NbtCompoundTag {} => V::Tag(nbt(reader, true)?),
            A::NbtPath {} => V::Path(nbt_path(reader)?),
            A::Component {} => {
                let tag = nbt(reader, false)?;
                V::Component(
                    Component::from_nbt(tag)
                        .map_err(|e| error("argument.component.invalid", &[e.to_string()]))?,
                )
            }
            A::Style {} => {
                let tag = nbt(reader, false)?;
                V::Style(
                    Style::from_nbt(tag)
                        .map_err(|e| error("argument.style.invalid", &[e.to_string()]))?,
                )
            }
            A::IntRange {} => V::Range(NumberRange::parse(reader, true, false)?),
            A::FloatRange {} => V::Range(NumberRange::parse(reader, false, false)?),
            A::Angle {} => {
                if !reader.can_read() {
                    return Err(error_at(reader, "argument.angle.incomplete", &[]));
                }
                let relative = reader.peek() == 126;
                if relative {
                    reader.skip();
                }
                let value = if reader.can_read() && reader.peek() != 32 {
                    reader.read_float()?
                } else {
                    0.
                };
                if !value.is_finite() {
                    return Err(error_at(reader, "argument.angle.invalid", &[]));
                }
                V::Angle { value, relative }
            }
            A::Time { min } => {
                let value = reader.read_float()?;
                let unit = reader.read_unquoted_string();
                let factor = match unit.as_str() {
                    "d" => 24000.,
                    "s" => 20.,
                    "t" | "" => 1.,
                    _ => return Err(error_at(reader, "argument.time.invalid_unit", &[])),
                };
                let ticks = (value * factor + 0.5).floor() as i32;
                if ticks < *min {
                    return Err(error_at(
                        reader,
                        "argument.time.tick_count_too_low",
                        &[min.to_string(), ticks.to_string()],
                    ));
                }
                V::Integer(ticks)
            }
            A::Swizzle {} => {
                let mut bits = 0;
                while reader.can_read() && reader.peek() != 32 {
                    let bit = match reader.read() {
                        120 => 1,
                        121 => 2,
                        122 => 4,
                        _ => return Err(error_at(reader, "arguments.swizzle.invalid", &[])),
                    };
                    if bits & bit != 0 {
                        return Err(error_at(reader, "arguments.swizzle.invalid", &[]));
                    }
                    bits |= bit;
                }
                V::Swizzle(bits)
            }
            A::Operation {} => {
                if !reader.can_read() {
                    return Err(error_at(reader, "arguments.operation.invalid", &[]));
                }
                V::Operation(Operation::parse(&word(reader))?)
            }
            A::Uuid {} => {
                let start = reader.cursor();
                while reader.can_read() && matches!(reader.peek(),45|48..=57|65..=70|97..=102) {
                    reader.skip();
                }
                let value = parse_uuid(&reader.slice(start, reader.cursor()));
                if let Some(value) = value {
                    V::Uuid(value)
                } else {
                    reader.set_cursor(start);
                    return Err(error_at(reader, "argument.uuid.invalid", &[]));
                }
            }
            A::HexColor {} => {
                let text = reader.read_unquoted_string();
                let value = u32::from_str_radix(&text, 16)
                    .ok()
                    .filter(|_| matches!(text.len(), 3 | 6))
                    .ok_or_else(|| {
                        error_at(
                            reader,
                            "argument.hexcolor.invalid",
                            std::slice::from_ref(&text),
                        )
                    })?;
                V::HexColor(
                    0xff000000
                        | if text.len() == 3 {
                            ((value >> 8) & 15) * 0x110000
                                + ((value >> 4) & 15) * 0x1100
                                + (value & 15) * 0x11
                        } else {
                            value
                        },
                )
            }
            A::TeamColor {} => V::String(enum_word(
                reader,
                &[
                    "black",
                    "dark_blue",
                    "dark_green",
                    "dark_aqua",
                    "dark_red",
                    "dark_purple",
                    "gold",
                    "gray",
                    "dark_gray",
                    "blue",
                    "green",
                    "aqua",
                    "red",
                    "light_purple",
                    "yellow",
                    "white",
                ],
                "argument.color.invalid",
            )?),
            A::EntityAnchor {} => V::String(enum_word(
                reader,
                &["feet", "eyes"],
                "argument.anchor.invalid",
            )?),
            A::TemplateMirror {} => V::String(enum_word(
                reader,
                &["none", "front_back", "left_right"],
                "argument.enum.invalid",
            )?),
            A::TemplateRotation {} => V::String(enum_word(
                reader,
                &[
                    "none",
                    "clockwise_90",
                    "clockwise_180",
                    "counterclockwise_90",
                ],
                "argument.enum.invalid",
            )?),
            A::Heightmap {} => V::String(enum_word(
                reader,
                &[
                    "world_surface",
                    "motion_blocking",
                    "motion_blocking_no_leaves",
                    "ocean_floor",
                ],
                "argument.enum.invalid",
            )?),
            A::SwingAnimation {} => V::String(enum_word(
                reader,
                &["none", "whack", "stab"],
                "argument.swing_animation.invalid",
            )?),
            A::Objective {} | A::Team {} => V::String(reader.read_unquoted_string()),
            A::ObjectiveCriteria {} => V::Criteria(scoreboard::criteria(reader)?),
            A::ScoreboardSlot {} => V::DisplaySlot(scoreboard::display_slot(reader)?),
            A::ResourceSelector { registry } => {
                let start = reader.cursor();
                while reader.can_read()
                    && matches!(reader.peek(),97..=122|48..=57|95|58|47|46|45|42|63)
                {
                    reader.skip();
                }
                let pattern = reader.slice(start, reader.cursor());
                let pattern = if pattern.contains(':') {
                    pattern
                } else {
                    format!("minecraft:{pattern}")
                };
                let resources = self
                    .context
                    .entries(registry.as_str())
                    .into_iter()
                    .filter(|id| wildcard_match(&pattern, id))
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if resources.is_empty() {
                    return Err(error_at(
                        reader,
                        "argument.resource_selector.not_found",
                        &[pattern, registry.to_string()],
                    ));
                }
                V::Resources(resources)
            }
            A::ScoreHolder { flags } => {
                if reader.can_read() && reader.peek() == 64 {
                    let selector = EntitySelector::parse(reader, allow_selectors)?;
                    if !flags.multiple && selector.max_results > 1 {
                        return Err(error_at(reader, "argument.entity.toomany", &[]));
                    }
                    V::Entity(selector)
                } else {
                    V::String(word(reader))
                }
            }
            _ => {
                return Err(error(
                    "ferraloom.argument.pending",
                    &[format!("{:?}", self.wire)],
                ));
            }
        })
    }
}
impl ArgumentType for MinecraftArgument {
    type Value = ArgumentValue;
    fn parse(&self, reader: &mut StringReader) -> Result<Self::Value, CommandSyntaxException> {
        self.parse_impl(reader, true)
    }
    fn parse_with_source<S: 'static>(
        &self,
        reader: &mut StringReader,
        source: &S,
    ) -> Result<Self::Value, CommandSyntaxException> {
        let allow = (source as &dyn std::any::Any)
            .downcast_ref::<CommandSourceStack>()
            .is_some_and(|s| {
                s.permissions
                    .has(&Permission::Atom(Permission::ENTITY_SELECTORS.into()))
            });
        self.parse_impl(reader, allow)
    }
    fn examples(&self) -> Vec<String> {
        self.examples.to_vec()
    }
    fn list_suggestions<S>(
        &self,
        _context: &CommandContext<S>,
        mut builder: SuggestionsBuilder,
    ) -> Result<Suggestions, CommandSyntaxException> {
        let values: &[&str] = match self.wire {
            CommandArgument::Bool {} => &["true", "false"],
            CommandArgument::Gamemode {} => &["survival", "creative", "adventure", "spectator"],
            CommandArgument::EntityAnchor {} => &["feet", "eyes"],
            CommandArgument::Operation {} => &["=", "+=", "-=", "*=", "/=", "%=", "<", ">", "><"],
            CommandArgument::HexColor {} => &["F00", "FF0000"],
            CommandArgument::TeamColor {} => scoreboard::COLORS,
            CommandArgument::ScoreboardSlot {} => scoreboard::DISPLAY_SLOTS,
            CommandArgument::TemplateMirror {} => &["none", "front_back", "left_right"],
            CommandArgument::TemplateRotation {} => &[
                "none",
                "clockwise_90",
                "clockwise_180",
                "counterclockwise_90",
            ],
            CommandArgument::Heightmap {} => &[
                "world_surface",
                "motion_blocking",
                "motion_blocking_no_leaves",
                "ocean_floor",
            ],
            CommandArgument::SwingAnimation {} => &["none", "whack", "stab"],
            _ => &[],
        };
        for value in values {
            if value
                .to_lowercase()
                .starts_with(&builder.remaining().to_lowercase())
            {
                builder.suggest(*value);
            }
        }
        Ok(builder.build())
    }
}
pub fn word(reader: &mut StringReader) -> String {
    let start = reader.cursor();
    while reader.can_read() && reader.peek() != 32 {
        reader.skip();
    }
    reader.slice(start, reader.cursor())
}
pub fn identifier(reader: &mut StringReader) -> Result<String, CommandSyntaxException> {
    let start = reader.cursor();
    while reader.can_read() && matches!(reader.peek(),97..=122|48..=57|95|58|47|46|45) {
        reader.skip();
    }
    let text = reader.slice(start, reader.cursor());
    Identifier::parse(&text)
        .map(|id| id.to_string())
        .map_err(|_| {
            reader.set_cursor(start);
            error_at(reader, "argument.id.invalid", &[])
        })
}
pub fn game_mode(value: &str) -> Option<GameType> {
    match value {
        "survival" => Some(GameType::Survival),
        "creative" => Some(GameType::Creative),
        "adventure" => Some(GameType::Adventure),
        "spectator" => Some(GameType::Spectator),
        _ => None,
    }
}
fn enum_word(
    reader: &mut StringReader,
    values: &[&str],
    key: &str,
) -> Result<String, CommandSyntaxException> {
    let value = reader.read_unquoted_string();
    if values.contains(&value.as_str()) {
        Ok(value)
    } else {
        Err(error_at(reader, key, &[value]))
    }
}
pub fn nbt(reader: &mut StringReader, compound: bool) -> Result<Tag, CommandSyntaxException> {
    let start = reader.cursor();
    let (tag, used) = ferraloom_nbt::parse_prefix(&reader.remaining())
        .map_err(|e| error(&e.message, &[]).with_context(reader.string(), start + e.cursor))?;
    reader.set_cursor(start + used);
    if compound && !matches!(tag, Tag::Compound(_)) {
        return Err(error_at(reader, "argument.nbt.expected.compound", &[]));
    }
    Ok(tag)
}
/// FilenameUtils#wildcardMatch over resource identifiers (ASCII).
fn wildcard_match(pattern: &str, value: &str) -> bool {
    let mut matched = vec![false; value.len() + 1];
    matched[0] = true;
    for token in pattern.bytes() {
        if token == b'*' {
            for index in 1..matched.len() {
                matched[index] |= matched[index - 1];
            }
        } else {
            for index in (1..matched.len()).rev() {
                matched[index] =
                    matched[index - 1] && (token == b'?' || token == value.as_bytes()[index - 1]);
            }
            matched[0] = false;
        }
    }
    matched[value.len()]
}
/// Port of NbtPathArgument#parse; retain the reader's position independently of the error cursor.
fn nbt_path(reader: &mut StringReader) -> Result<NbtPath, CommandSyntaxException> {
    let start = reader.cursor();
    let mut first = true;
    while reader.can_read() && reader.peek() != 32 {
        match reader.peek() {
            91 => {
                reader.skip();
                match if reader.can_read() { reader.peek() } else { 0 } {
                    123 => {
                        nbt(reader, true)?;
                        reader.expect(93)?;
                    }
                    93 => reader.skip(),
                    _ => {
                        reader.read_int()?;
                        reader.expect(93)?;
                    }
                }
            }
            123 => {
                if !first {
                    return Err(error_at(reader, "arguments.nbtpath.node.invalid", &[]));
                }
                nbt(reader, true)?;
            }
            _ => {
                let name = if matches!(reader.peek(), 34 | 39) {
                    reader.read_string()?
                } else {
                    let start = reader.cursor();
                    while reader.can_read()
                        && !matches!(reader.peek(), 32 | 34 | 39 | 91 | 93 | 46 | 123 | 125)
                    {
                        reader.skip();
                    }
                    reader.slice(start, reader.cursor())
                };
                if name.is_empty() {
                    return Err(error_at(reader, "arguments.nbtpath.node.invalid", &[]));
                }
                if reader.can_read() && reader.peek() == 123 {
                    nbt(reader, true)?;
                }
            }
        }
        first = false;
        if reader.can_read() && !matches!(reader.peek(), 32 | 91 | 123) {
            reader.expect(46)?;
        }
    }
    NbtPath::parse(&reader.slice(start, reader.cursor())).map_err(|error| {
        super::exceptions::error(&error.message(), &[])
            .with_context(reader.string(), start + error.cursor().max(0) as usize)
    })
}

#[cfg(test)]
mod tests;

impl Message {
    /// Port of MessageArgument.Message#toComponent. Selectors capture live matches here.
    pub fn to_component<Q: selectors::EntityQuery>(
        &self,
        source: &CommandSourceStack,
        query: &mut Q,
    ) -> Result<Component, CommandSyntaxException> {
        if self.parts.is_empty()
            || !source
                .permissions
                .has(&Permission::Atom(Permission::ENTITY_SELECTORS.into()))
        {
            return Ok(Component::literal(&self.text));
        }
        let text = self.text.encode_utf16().collect::<Vec<_>>();
        let mut result = Component::literal(String::from_utf16_lossy(&text[..self.parts[0].start]));
        let mut read_to = self.parts[0].start;
        for part in &self.parts {
            if read_to < part.start {
                result
                    .extra
                    .push(Component::literal(String::from_utf16_lossy(
                        &text[read_to..part.start],
                    )));
            }
            let matches = part.selector.find(source, query, false)?;
            result.extra.push(crate::commands::builtins::format_list(
                matches.into_iter().map(|entity| query.display_name(entity)),
            ));
            read_to = part.end;
        }
        if read_to < text.len() {
            result
                .extra
                .push(Component::literal(String::from_utf16_lossy(
                    &text[read_to..],
                )));
        }
        Ok(result)
    }
}
