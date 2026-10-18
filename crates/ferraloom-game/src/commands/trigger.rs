//! Port of TriggerCommand: only a primed trigger score may be consumed by its player.
use super::{
    PreparedCommand, builtins,
    exceptions::error,
    scoreboard::{Holder, Result, c, checked, formatted_objective, objective, success},
};
use crate::scoreboard::ServerScoreboard;
pub fn execute(
    board: &mut ServerScoreboard,
    command: &PreparedCommand,
    holder: &Holder,
) -> Result<builtins::CommandResult> {
    let name = command.string("objective").unwrap();
    if objective(board.scoreboard(), name, false)?.criteria.name != "trigger" {
        return Err(error("commands.trigger.failed.invalid", &[]));
    }
    let score = board
        .scoreboard()
        .get_player_score(&holder.name, name)
        .ok_or_else(|| error("commands.trigger.failed.unprimed", &[]))?;
    if score.locked {
        return Err(error("commands.trigger.failed.unprimed", &[]));
    }
    let old = score.value;
    let verb = command.literals.get(1).map(String::as_str);
    let amount = command.integer("value").unwrap_or(1);
    let value = if verb == Some("set") {
        amount
    } else {
        old.wrapping_add(amount)
    };
    checked(board.set_score_locked(&holder.name, name, true))?;
    checked(board.set_score(&holder.name, name, value, holder.display.as_ref(), false))?;
    let mut args = vec![c(formatted_objective(board.scoreboard(), name))];
    if verb.is_some() {
        args.push(builtins::integer(amount));
    }
    Ok(success(
        value,
        match verb {
            Some("set") => "commands.trigger.set.success",
            Some("add") => "commands.trigger.add.success",
            _ => "commands.trigger.simple.success",
        },
        args,
        true,
    ))
}
