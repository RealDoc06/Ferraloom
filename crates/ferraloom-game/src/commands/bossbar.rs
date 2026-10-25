//! Port of BossBarCommands. Selector and component resolution stay at the live call site.
use super::{
    PreparedCommand,
    builtins::CommandResult,
    exceptions::error,
    scoreboard::{Result, boolean, c, component, list, n, success},
};
use crate::{
    bossbar::{BossEventUpdate, BossPlayer, CustomBossEvents},
    entity::EntityRef,
};
use ferraloom_protocol::{
    Identifier, Uuid,
    chat::Component,
    play::{BossBarColor, BossBarOverlay},
};

/// Native sendSuccess suppliers read player display names after mutations complete.
pub enum BossBarCommandResult {
    Complete(CommandResult),
    Players {
        key: &'static str,
        bar: Box<Component>,
        count: usize,
        value: i32,
        players: Vec<EntityRef>,
    },
}
impl BossBarCommandResult {
    pub fn resolve_feedback(
        self,
        mut display: impl FnMut(EntityRef) -> Result<Component>,
    ) -> Result<CommandResult> {
        Ok(match self {
            Self::Complete(result) => result,
            Self::Players {
                key,
                bar,
                count,
                value,
                players,
            } => success(
                value,
                key,
                vec![
                    c(*bar),
                    n(count),
                    c(list(
                        players
                            .into_iter()
                            .map(&mut display)
                            .collect::<Result<Vec<_>>>()?,
                    )),
                ],
                true,
            ),
        })
    }
}
pub fn id(command: &PreparedCommand) -> Identifier {
    Identifier::parse(command.string("id").expect("IdentifierArgument"))
        .expect("validated IdentifierArgument")
}
pub fn require(events: &CustomBossEvents, id: &Identifier) -> Result<()> {
    if events.get(id).is_some() {
        Ok(())
    } else {
        Err(error("commands.bossbar.unknown", &[id.to_string()]))
    }
}
/// Component arguments have already passed both native resolution calls. new_uuid is drawn from
/// the live level RNG only for creation, after resolution and the duplicate-name check.
pub fn execute(
    events: &mut CustomBossEvents,
    command: &PreparedCommand,
    targets: &[BossPlayer],
    new_uuid: Option<Uuid>,
) -> Result<BossBarCommandResult> {
    let verb = command.literals[1].as_str();
    let complete = |result| Ok(BossBarCommandResult::Complete(result));
    if verb == "list" {
        let bars: Vec<_> = events.events().map(|event| event.display_name()).collect();
        return complete(if bars.is_empty() {
            success(0, "commands.bossbar.list.bars.none", vec![], false)
        } else {
            success(
                bars.len() as i32,
                "commands.bossbar.list.bars.some",
                vec![n(bars.len()), c(list(bars))],
                false,
            )
        });
    }
    let id = id(command);
    if verb == "add" {
        if events.get(&id).is_some() {
            return Err(error("commands.bossbar.create.failed", &[id.to_string()]));
        }
        events.create(
            new_uuid.expect("live level RNG UUID"),
            id.clone(),
            component(command, "name").expect("resolved name"),
        );
        return complete(success(
            events.events().len() as i32,
            "commands.bossbar.create.success",
            vec![c(events.get(&id).unwrap().display_name())],
            true,
        ));
    }
    require(events, &id)?;
    if verb == "remove" {
        let display = events.get(&id).unwrap().display_name();
        events.remove(&id);
        return complete(success(
            events.events().len() as i32,
            "commands.bossbar.remove.success",
            vec![c(display)],
            true,
        ));
    }
    let field = command.literals[2].as_str();
    let event = events.get(&id).unwrap();
    if verb == "get" {
        let display = event.display_name();
        return match field {
            "value" => complete(success(
                event.value(),
                "commands.bossbar.get.value",
                vec![c(display), super::builtins::integer(event.value())],
                true,
            )),
            "max" => complete(success(
                event.max(),
                "commands.bossbar.get.max",
                vec![c(display), super::builtins::integer(event.max())],
                true,
            )),
            "visible" => complete(success(
                i32::from(event.server().is_visible()),
                if event.server().is_visible() {
                    "commands.bossbar.get.visible.visible"
                } else {
                    "commands.bossbar.get.visible.hidden"
                },
                vec![c(display)],
                true,
            )),
            "players" => {
                let players: Vec<_> = event
                    .server()
                    .players()
                    .map(|player| player.entity)
                    .collect();
                if players.is_empty() {
                    complete(success(
                        0,
                        "commands.bossbar.get.players.none",
                        vec![c(display)],
                        true,
                    ))
                } else {
                    Ok(BossBarCommandResult::Players {
                        key: "commands.bossbar.get.players.some",
                        bar: Box::new(display),
                        count: players.len(),
                        value: players.len() as i32,
                        players,
                    })
                }
            }
            _ => unreachable!("registered bossbar getter"),
        };
    }
    let (key, value, extra) = match field {
        "players" => {
            if !events
                .update(&id, |event, packets| event.set_players(targets, packets))
                .unwrap()
            {
                return Err(error("commands.bossbar.set.players.unchanged", &[]));
            }
            let event = events.get(&id).unwrap();
            let count = event.server().players().len();
            return if count == 0 {
                complete(success(
                    0,
                    "commands.bossbar.set.players.success.none",
                    vec![c(event.display_name())],
                    true,
                ))
            } else {
                Ok(BossBarCommandResult::Players {
                    key: "commands.bossbar.set.players.success.some",
                    bar: Box::new(event.display_name()),
                    count: targets.len(),
                    value: count as i32,
                    players: targets.iter().map(|player| player.entity).collect(),
                })
            };
        }
        "value" | "max" => {
            let value = command.integer(field).unwrap();
            if value
                == if field == "value" {
                    event.value()
                } else {
                    event.max()
                }
            {
                return Err(error(
                    &format!("commands.bossbar.set.{field}.unchanged"),
                    &[],
                ));
            }
            events.update(&id, |event, packets| {
                if field == "value" {
                    event.set_value(value, packets)
                } else {
                    event.set_max(value, packets)
                }
            });
            (
                if field == "value" {
                    "commands.bossbar.set.value.success"
                } else {
                    "commands.bossbar.set.max.success"
                },
                value,
                vec![super::builtins::integer(value)],
            )
        }
        "visible" => {
            let value = boolean(command, "visible");
            if value == event.server().is_visible() {
                return Err(error(
                    if value {
                        "commands.bossbar.set.visibility.unchanged.visible"
                    } else {
                        "commands.bossbar.set.visibility.unchanged.hidden"
                    },
                    &[],
                ));
            }
            events.update(&id, |event, packets| {
                event.update(BossEventUpdate::Visible(value), packets)
            });
            (
                if value {
                    "commands.bossbar.set.visible.success.visible"
                } else {
                    "commands.bossbar.set.visible.success.hidden"
                },
                0,
                vec![],
            )
        }
        "name" => {
            let name = component(command, "name").expect("twice-resolved component");
            if event.server().name() == &name {
                return Err(error("commands.bossbar.set.name.unchanged", &[]));
            }
            events.update(&id, |event, packets| {
                event.update(BossEventUpdate::Name(Box::new(name)), packets)
            });
            ("commands.bossbar.set.name.success", 0, vec![])
        }
        "color" => {
            let color = match command.literals[3].as_str() {
                "pink" => BossBarColor::Pink,
                "blue" => BossBarColor::Blue,
                "red" => BossBarColor::Red,
                "green" => BossBarColor::Green,
                "yellow" => BossBarColor::Yellow,
                "purple" => BossBarColor::Purple,
                "white" => BossBarColor::White,
                _ => unreachable!("registered color"),
            };
            if event.server().color() == color {
                return Err(error("commands.bossbar.set.color.unchanged", &[]));
            }
            events.update(&id, |event, packets| {
                event.update(BossEventUpdate::Color(color), packets)
            });
            ("commands.bossbar.set.color.success", 0, vec![])
        }
        "style" => {
            let overlay = match command.literals[3].as_str() {
                "progress" => BossBarOverlay::Progress,
                "notched_6" => BossBarOverlay::Notched6,
                "notched_10" => BossBarOverlay::Notched10,
                "notched_12" => BossBarOverlay::Notched12,
                "notched_20" => BossBarOverlay::Notched20,
                _ => unreachable!("registered overlay"),
            };
            if event.server().overlay() == overlay {
                return Err(error("commands.bossbar.set.style.unchanged", &[]));
            }
            events.update(&id, |event, packets| {
                event.update(BossEventUpdate::Overlay(overlay), packets)
            });
            ("commands.bossbar.set.style.success", 0, vec![])
        }
        _ => unreachable!("registered bossbar setter"),
    };
    let mut args = vec![c(events.get(&id).unwrap().display_name())];
    args.extend(extra);
    complete(success(value, key, args, true))
}
