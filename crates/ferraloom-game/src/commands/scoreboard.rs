//! Port of ScoreboardCommand. Holder selectors are captured synchronously by the server adapter.
use super::{
    PreparedCommand,
    arguments::ArgumentValue,
    builtins::{self, CommandResult, Feedback},
    exceptions::{error, error_with_arguments},
};
use crate::scoreboard::{
    DisplaySlot, NumberFormat, Objective, ObjectiveCriteria, RenderType, Scoreboard,
    ServerScoreboard,
};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_protocol::{
    Reader, Wire,
    chat::{Argument, Component, HoverEvent, TextColor},
};
pub type Result<T> = std::result::Result<T, CommandSyntaxException>;

/// Port of ScoreHolder's command-facing name and optional entity display name.
#[derive(Clone, Debug)]
pub struct Holder {
    /// The exact entity captured by ScoreHolderArgument, never reselected for feedback.
    pub entity: Option<crate::entity::EntityRef>,
    pub name: String,
    pub display: Option<Component>,
}
impl Holder {
    pub fn for_name(name: impl Into<String>) -> Self {
        Self {
            entity: None,
            name: name.into(),
            display: None,
        }
    }
    /// Port of ScoreHolder#getFeedbackDisplayName for a captured display name.
    pub fn feedback(&self) -> Component {
        self.display.clone().map_or_else(
            || Component::literal(&self.name),
            |display| self.feedback_with_display(display),
        )
    }
    /// Port of ScoreHolder#getFeedbackDisplayName after a live entity-name read.
    pub fn feedback_with_display(&self, mut display: Component) -> Component {
        display.style.hover_event = Some(HoverEvent::ShowText(Box::new(Component::literal(
            &self.name,
        ))));
        display
    }
}
pub(super) fn success(
    value: i32,
    key: &str,
    args: Vec<Argument>,
    broadcast: bool,
) -> CommandResult {
    CommandResult::success(value, Component::translatable(key, args), broadcast)
}
pub(super) fn empty(value: i32) -> CommandResult {
    CommandResult {
        value,
        feedback: Vec::new(),
        rule_change: None,
    }
}
pub(super) fn c(value: Component) -> Argument {
    builtins::component(value)
}
pub(super) fn n(value: usize) -> Argument {
    builtins::integer(value as i32)
}
pub(super) fn s(value: impl Into<String>) -> Argument {
    builtins::string(value)
}
pub(super) fn list(values: impl IntoIterator<Item = Component>) -> Component {
    let values: Vec<_> = values.into_iter().collect();
    if values.len() == 1 {
        return values.into_iter().next().unwrap();
    }
    let mut result = Component::default();
    for (index, value) in values.into_iter().enumerate() {
        if index != 0 {
            let mut separator = Component::literal(", ");
            separator.style.color = Some(TextColor::parse("gray").unwrap());
            result.extra.push(separator);
        }
        result.extra.push(value);
    }
    result
}
pub(super) fn component(command: &PreparedCommand, name: &str) -> Option<Component> {
    if let Some(ArgumentValue::Component(value)) = command.argument(name) {
        Some(value.clone())
    } else {
        None
    }
}
pub(super) fn boolean(command: &PreparedCommand, name: &str) -> bool {
    matches!(command.argument(name), Some(ArgumentValue::Boolean(true)))
}
pub(super) fn enum_value<T: Wire>(value: &str, names: &[&str]) -> T {
    let id = names
        .iter()
        .position(|&name| name == value)
        .expect("parsed enum");
    T::decode(&mut Reader::new(&[id as u8])).expect("enum ordinal")
}
pub(super) fn checked<T>(value: crate::scoreboard::Result<T>) -> Result<T> {
    value.map_err(|error_value| match error_value {
        crate::scoreboard::Error::UnknownObjective(name) => {
            error("arguments.objective.notFound", &[name])
        }
        crate::scoreboard::Error::ReadOnly(name) => error("arguments.objective.readonly", &[name]),
        crate::scoreboard::Error::UnknownTeam(name) => error("team.notFound", &[name]),
        other => error("ferraloom.scoreboard.invalid", &[other.to_string()]),
    })
}
pub fn objective<'a>(board: &'a Scoreboard, name: &str, writable: bool) -> Result<&'a Objective> {
    let objective = board
        .get_objective(name)
        .ok_or_else(|| error("arguments.objective.notFound", &[name.to_owned()]))?;
    if writable && objective.criteria.read_only {
        return Err(error("arguments.objective.readonly", &[name.to_owned()]));
    }
    Ok(objective)
}
pub(super) fn formatted_objective(board: &Scoreboard, name: &str) -> Component {
    let mut display = board.get_objective(name).unwrap().display_name.clone();
    display.style.hover_event = Some(HoverEvent::ShowText(Box::new(Component::literal(name))));
    Component::translatable("chat.square_brackets", vec![c(display)])
}
fn number_format(command: &PreparedCommand) -> Option<NumberFormat> {
    if command.literals.iter().any(|v| v == "blank") {
        Some(NumberFormat::Blank {})
    } else if command.literals.iter().any(|v| v == "fixed") {
        Some(NumberFormat::Fixed {
            value: component(command, "contents").unwrap(),
        })
    } else if let Some(ArgumentValue::Style(style)) = command.argument("style") {
        Some(NumberFormat::Styled {
            style: style.clone(),
        })
    } else {
        None
    }
}
/// Port of ComponentArgument#getResolvedComponent. Each argument has its own
/// ResolutionContext; the source entity supplies vanilla's default score holder.
pub fn resolve_components(
    command: &PreparedCommand,
    resolver: &mut impl super::components::ComponentResolver,
) -> std::result::Result<PreparedCommand, super::components::ResolutionError> {
    let mut resolved = command.clone();
    for argument in resolved.arguments.values_mut() {
        if let ArgumentValue::Component(component) = argument {
            let mut context = super::components::ResolutionContext::new(Some(&command.source));
            *component = super::components::resolve(&mut context, component, resolver)?;
        }
    }
    Ok(resolved)
}
/// Port of ScoreboardCommand. targets/sources retain selector/wildcard order and display information.
/// Component arguments have already passed ComponentArgument resolution in the adapter.
pub fn execute(
    board: &mut ServerScoreboard,
    command: &PreparedCommand,
    targets: &[Holder],
    sources: &[Holder],
) -> Result<CommandResult> {
    if command.literals[1] == "objectives" {
        objectives(board, command)
    } else {
        players(board, command, targets, sources)
    }
}
fn objectives(board: &mut ServerScoreboard, command: &PreparedCommand) -> Result<CommandResult> {
    let verb = command.literals[2].as_str();
    if verb == "list" {
        let names: Vec<_> = board
            .scoreboard()
            .objectives()
            .map(|(name, _)| formatted_objective(board.scoreboard(), name))
            .collect();
        return Ok(if names.is_empty() {
            success(
                0,
                "commands.scoreboard.objectives.list.empty",
                vec![],
                false,
            )
        } else {
            success(
                names.len() as i32,
                "commands.scoreboard.objectives.list.success",
                vec![n(names.len()), c(list(names))],
                false,
            )
        });
    }
    if verb == "setdisplay" {
        let Some(ArgumentValue::DisplaySlot(index)) = command.argument("slot") else {
            unreachable!()
        };
        let slot: DisplaySlot =
            DisplaySlot::decode(&mut Reader::new(&[*index])).expect("display slot");
        let name = command.string("objective");
        if let Some(name) = name {
            objective(board.scoreboard(), name, false)?;
        }
        let old = board.scoreboard().get_display_objective(slot);
        if old == name {
            return Err(error(
                if name.is_some() {
                    "commands.scoreboard.objectives.display.alreadySet"
                } else {
                    "commands.scoreboard.objectives.display.alreadyEmpty"
                },
                &[],
            ));
        }
        let display = name.map(|name| {
            board
                .scoreboard()
                .get_objective(name)
                .unwrap()
                .display_name
                .clone()
        });
        checked(board.set_display_objective(slot, name.map(str::to_owned)))?;
        let mut args = vec![s(
            super::arguments::scoreboard::DISPLAY_SLOTS[*index as usize]
        )];
        if let Some(display) = display {
            args.push(c(display));
        }
        return Ok(success(
            0,
            if name.is_some() {
                "commands.scoreboard.objectives.display.set"
            } else {
                "commands.scoreboard.objectives.display.cleared"
            },
            args,
            true,
        ));
    }
    let name = command.string("objective").unwrap();
    if verb == "add" {
        if board.scoreboard().get_objective(name).is_some() {
            return Err(error("commands.scoreboard.objectives.add.duplicate", &[]));
        }
        let Some(ArgumentValue::Criteria(criteria)) = command.argument("criteria") else {
            unreachable!()
        };
        let criteria = ObjectiveCriteria::by_name(&criteria.name).expect("parsed criteria");
        checked(board.add_objective(
            name.to_owned(),
            Objective::new(
                criteria,
                component(command, "displayName").unwrap_or_else(|| Component::literal(name)),
            ),
        ))?;
        return Ok(success(
            board.scoreboard().objectives().count() as i32,
            "commands.scoreboard.objectives.add.success",
            vec![c(formatted_objective(board.scoreboard(), name))],
            true,
        ));
    }
    objective(board.scoreboard(), name, false)?;
    let formatted = formatted_objective(board.scoreboard(), name);
    if verb == "remove" {
        checked(board.remove_objective(name))?;
        return Ok(success(
            board.scoreboard().objectives().count() as i32,
            "commands.scoreboard.objectives.remove.success",
            vec![c(formatted)],
            true,
        ));
    }
    let option = command.literals[3].as_str();
    let current = board.scoreboard().get_objective(name).unwrap();
    let key = match option {
        "displayname" => {
            let display = component(command, "displayName").unwrap();
            if current.display_name == display {
                return Ok(empty(0));
            }
            checked(board.update_objective(name, |obj| obj.display_name = display))?;
            "commands.scoreboard.objectives.modify.displayname"
        }
        "displayautoupdate" => {
            let value = boolean(command, "value");
            if current.display_auto_update == value {
                return Ok(empty(0));
            }
            checked(board.update_objective(name, |obj| obj.display_auto_update = value))?;
            if value {
                "commands.scoreboard.objectives.modify.displayAutoUpdate.enable"
            } else {
                "commands.scoreboard.objectives.modify.displayAutoUpdate.disable"
            }
        }
        "rendertype" => {
            let render_type = if command.literals.last().unwrap() == "hearts" {
                RenderType::Hearts
            } else {
                RenderType::Integer
            };
            if current.render_type == render_type {
                return Ok(empty(0));
            }
            checked(board.update_objective(name, |obj| obj.render_type = render_type))?;
            return Ok(success(
                0,
                "commands.scoreboard.objectives.modify.rendertype",
                vec![c(formatted)],
                true,
            ));
        }
        "numberformat" => {
            let format = number_format(command);
            let present = format.is_some();
            checked(board.update_objective(name, |obj| obj.number_format = format))?;
            return Ok(success(
                0,
                if present {
                    "commands.scoreboard.objectives.modify.objectiveFormat.set"
                } else {
                    "commands.scoreboard.objectives.modify.objectiveFormat.clear"
                },
                vec![s(name)],
                true,
            ));
        }
        _ => unreachable!("native scoreboard objective option"),
    };
    Ok(success(
        0,
        key,
        vec![s(name), c(formatted_objective(board.scoreboard(), name))],
        true,
    ))
}
fn players(
    board: &mut ServerScoreboard,
    command: &PreparedCommand,
    targets: &[Holder],
    sources: &[Holder],
) -> Result<CommandResult> {
    let verb = command.literals[2].as_str();
    if verb == "list" {
        return list_players(board.scoreboard(), targets.first());
    }
    let operation = verb == "operation";
    let name = command.string(if operation {
        "targetObjective"
    } else {
        "objective"
    });
    let writable = matches!(verb, "set" | "add" | "remove" | "operation");
    if let Some(name) = name {
        objective(board.scoreboard(), name, writable)?;
    }
    if verb == "get" {
        let name = name.unwrap();
        let holder = &targets[0];
        let score = board
            .scoreboard()
            .get_player_score(&holder.name, name)
            .ok_or_else(|| {
                error_with_arguments(
                    "commands.scoreboard.players.get.null",
                    vec![s(name), c(holder.feedback())],
                )
            })?;
        return Ok(success(
            score.value,
            "commands.scoreboard.players.get.success",
            vec![
                c(holder.feedback()),
                builtins::integer(score.value),
                c(formatted_objective(board.scoreboard(), name)),
            ],
            false,
        ));
    }
    if operation {
        return perform_operation(board, command, targets, sources);
    }
    let mut changed: Vec<&Holder> = Vec::new();
    let mut total = 0i32;
    let amount = command.integer("score").unwrap_or(0);
    if verb == "enable"
        && board
            .scoreboard()
            .get_objective(name.unwrap())
            .unwrap()
            .criteria
            .name
            != "trigger"
    {
        return Err(error("commands.scoreboard.players.enable.invalid", &[]));
    }
    for holder in targets {
        match verb {
            "set" | "add" | "remove" => {
                let name = name.unwrap();
                let old = board
                    .scoreboard()
                    .get_player_score(&holder.name, name)
                    .map_or(0, |score| score.value);
                let value = match verb {
                    "add" => old.wrapping_add(amount),
                    "remove" => old.wrapping_sub(amount),
                    _ => amount,
                };
                checked(board.set_score(
                    &holder.name,
                    name,
                    value,
                    holder.display.as_ref(),
                    false,
                ))?;
                total = total.wrapping_add(value);
                changed.push(holder);
            }
            "reset" => {
                board.reset_score(&holder.name, name);
                changed.push(holder);
                total = total.wrapping_add(1);
            }
            "enable" => {
                let name = name.unwrap();
                if board
                    .scoreboard()
                    .get_player_score(&holder.name, name)
                    .is_none_or(|score| score.locked)
                {
                    checked(board.set_score_locked(&holder.name, name, false))?;
                    changed.push(holder);
                    total += 1;
                }
            }
            "display" => {
                let name = name.unwrap();
                if command.literals[3] == "name" {
                    let display = component(command, "name");
                    checked(board.update_score(
                        &holder.name,
                        name,
                        |score| score.display = display,
                        false,
                    ))?;
                } else {
                    let format = number_format(command);
                    checked(board.update_score(
                        &holder.name,
                        name,
                        |score| score.number_format = format,
                        true,
                    ))?;
                }
                changed.push(holder);
                total += 1;
            }
            _ => unreachable!("native scoreboard player verb"),
        }
    }
    if verb == "enable" && changed.is_empty() {
        return Err(error("commands.scoreboard.players.enable.failed", &[]));
    }
    let single = changed.len() == 1;
    // set uses NON_ZERO feedback, although its result still sums all assigned values.
    let feedback_targets: Vec<_> = if verb == "set" && amount == 0 {
        Vec::new()
    } else {
        changed
    };
    let single = if verb == "set" {
        feedback_targets.len() == 1
    } else {
        single
    };
    let holder = feedback_targets.first().copied();
    let suffix = if single { "single" } else { "multiple" };
    let objective_display = name.map(|name| formatted_objective(board.scoreboard(), name));
    let count = feedback_targets.len();
    let holder_or_count = || {
        if let Some(holder) = holder.filter(|_| single) {
            c(holder.feedback())
        } else {
            n(count)
        }
    };
    let (prefix, args) = match verb {
        "set" => (
            "commands.scoreboard.players.set.success",
            vec![
                c(objective_display.unwrap()),
                holder_or_count(),
                builtins::integer(amount),
            ],
        ),
        "add" | "remove" => {
            let mut args = vec![
                builtins::integer(amount),
                c(objective_display.unwrap()),
                holder_or_count(),
            ];
            if single {
                args.push(builtins::integer(total));
            }
            (
                if verb == "add" {
                    "commands.scoreboard.players.add.success"
                } else {
                    "commands.scoreboard.players.remove.success"
                },
                args,
            )
        }
        "enable" => (
            "commands.scoreboard.players.enable.success",
            vec![c(objective_display.unwrap()), holder_or_count()],
        ),
        "reset" => {
            if let Some(objective) = objective_display {
                (
                    "commands.scoreboard.players.reset.specific",
                    vec![c(objective), holder_or_count()],
                )
            } else {
                (
                    "commands.scoreboard.players.reset.all",
                    vec![holder_or_count()],
                )
            }
        }
        "display" => {
            let display = component(command, "name");
            let format = number_format(command);
            let kind = if command.literals[3] == "name" {
                "name"
            } else {
                "numberFormat"
            };
            let set = if kind == "name" {
                display.is_some()
            } else {
                format.is_some()
            };
            let mut args = Vec::new();
            if let Some(display) = display {
                args.push(c(display));
            }
            args.push(holder_or_count());
            args.push(c(objective_display.unwrap()));
            return Ok(success(
                total,
                &format!(
                    "commands.scoreboard.players.display.{kind}.{}.success.{suffix}",
                    if set { "set" } else { "clear" }
                ),
                args,
                true,
            ));
        }
        _ => unreachable!(),
    };
    Ok(success(total, &format!("{prefix}.{suffix}"), args, true))
}
fn list_players(board: &Scoreboard, target: Option<&Holder>) -> Result<CommandResult> {
    if let Some(holder) = target {
        let scores: Vec<_> = board.player_scores(&holder.name).collect();
        let count = scores.len();
        if count == 0 {
            return Ok(success(
                0,
                "commands.scoreboard.players.list.entity.empty",
                vec![c(holder.feedback())],
                false,
            ));
        }
        let mut result = success(
            count as i32,
            "commands.scoreboard.players.list.entity.success",
            vec![c(holder.feedback()), n(count)],
            false,
        );
        for (name, score) in scores {
            result.feedback.push(Feedback {
                message: Component::translatable(
                    "commands.scoreboard.players.list.entity.entry",
                    vec![
                        c(formatted_objective(board, name)),
                        builtins::integer(score.value),
                    ],
                ),
                broadcast: false,
            });
        }
        Ok(result)
    } else {
        let holders: Vec<_> = board.tracked_players().map(Component::literal).collect();
        let count = holders.len();
        Ok(if count == 0 {
            success(0, "commands.scoreboard.players.list.empty", vec![], false)
        } else {
            success(
                count as i32,
                "commands.scoreboard.players.list.success",
                vec![n(count), c(list(holders))],
                false,
            )
        })
    }
}
fn perform_operation(
    board: &mut ServerScoreboard,
    command: &PreparedCommand,
    targets: &[Holder],
    sources: &[Holder],
) -> Result<CommandResult> {
    let target = command.string("targetObjective").unwrap();
    let source = command.string("sourceObjective").unwrap();
    objective(board.scoreboard(), source, false)?;
    let Some(ArgumentValue::Operation(operation)) = command.argument("operation") else {
        unreachable!()
    };
    let mut total = 0i32;
    for holder in targets {
        let mut target_new = board
            .scoreboard()
            .get_player_score(&holder.name, target)
            .is_none();
        checked(
            board
                .scoreboard_mut()
                .get_or_create_score(&holder.name, target),
        )?;
        for from in sources {
            let source_new = board
                .scoreboard()
                .get_player_score(&from.name, source)
                .is_none();
            checked(
                board
                    .scoreboard_mut()
                    .get_or_create_score(&from.name, source),
            )?;
            let mut a = board
                .scoreboard()
                .get_player_score(&holder.name, target)
                .unwrap()
                .value;
            let mut b = board
                .scoreboard()
                .get_player_score(&from.name, source)
                .unwrap()
                .value;
            operation.apply(&mut a, &mut b)?;
            checked(board.set_score_with_sync(
                &holder.name,
                target,
                a,
                holder.display.as_ref(),
                false,
                target_new,
            ))?;
            target_new = false;
            if *operation == super::arguments::operation::Operation::Swap {
                let result = board.set_score_with_sync(
                    &from.name,
                    source,
                    b,
                    from.display.as_ref(),
                    false,
                    source_new,
                );
                if matches!(result, Err(crate::scoreboard::Error::ReadOnly(_))) {
                    // Port of Commands#performCommand's runtime-exception feedback after the target's write.
                    let mut message = super::exceptions::translated("command.failed", &[]);
                    message.component.style.hover_event = Some(HoverEvent::ShowText(Box::new(
                        Component::literal("Cannot modify read-only score"),
                    )));
                    return Err(CommandSyntaxException::new(
                        ferraloom_brigadier::ExceptionKind::Custom("command.failed".into()),
                        std::sync::Arc::new(message),
                    ));
                }
                checked(result)?;
            }
        }
        total = total.wrapping_add(
            board
                .scoreboard()
                .get_player_score(&holder.name, target)
                .unwrap()
                .value,
        );
    }
    let single = targets.len() == 1;
    Ok(success(
        total,
        if single {
            "commands.scoreboard.players.operation.success.single"
        } else {
            "commands.scoreboard.players.operation.success.multiple"
        },
        vec![
            c(formatted_objective(board.scoreboard(), target)),
            if single {
                c(targets[0].feedback())
            } else {
                n(targets.len())
            },
            builtins::integer(total),
        ]
        .into_iter()
        .take(if single { 3 } else { 2 })
        .collect(),
        true,
    ))
}
