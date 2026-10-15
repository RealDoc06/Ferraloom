use crate::command::{
    CommandSource,
    argument_types::argument_type::{ArgumentType, JavaClientArgumentType},
    context::command_context::CommandContext,
    errors::command_syntax_error::CommandSyntaxError,
    errors::error_types::CommandErrorType,
    string_reader::StringReader,
    suggestion::suggestions::{Suggestions, SuggestionsBuilder},
};
use crate::world::scoreboard::DISPLAY_SLOTS;
use pumpkin_data::{scoreboard::ScoreboardDisplaySlot, translation};
use pumpkin_util::text::TextComponent;

const INVALID_SLOT: CommandErrorType<1> = CommandErrorType::new(
    translation::java::ARGUMENT_SCOREBOARDDISPLAYSLOT_INVALID,
    translation::java::ARGUMENT_SCOREBOARDDISPLAYSLOT_INVALID,
);

pub struct ScoreboardSlotArgumentType;

impl ArgumentType<CommandSource> for ScoreboardSlotArgumentType {
    type Item = ScoreboardDisplaySlot;

    fn parse(&self, reader: &mut StringReader) -> Result<Self::Item, CommandSyntaxError> {
        let name = reader.read_unquoted_string();
        DISPLAY_SLOTS
            .iter()
            .find_map(|(slot, candidate)| (*candidate == name).then_some(*slot))
            .ok_or_else(|| INVALID_SLOT.create(reader, TextComponent::text(name)))
    }

    fn client_side_parser(&self) -> JavaClientArgumentType {
        JavaClientArgumentType::ScoreboardSlot
    }

    fn list_suggestions(
        &self,
        _context: &CommandContext,
        mut builder: SuggestionsBuilder,
    ) -> Suggestions {
        for (_, name) in DISPLAY_SLOTS {
            builder = builder.filter_and_suggest_one(name);
        }
        builder.build()
    }

    fn examples(&self) -> Vec<String> {
        vec!["list".into(), "sidebar".into(), "sidebar.team.red".into()]
    }
}

impl ScoreboardSlotArgumentType {
    pub fn get(
        context: &CommandContext,
        name: &str,
    ) -> Result<ScoreboardDisplaySlot, CommandSyntaxError> {
        Ok(*context.get_argument(name)?)
    }

    pub fn name(slot: ScoreboardDisplaySlot) -> &'static str {
        DISPLAY_SLOTS
            .iter()
            .find_map(|(candidate, name)| (*candidate == slot).then_some(*name))
            .expect("all display slots have vanilla names")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_slot_parser_accepts_team_slots_and_rejects_unknown_names() {
        for (slot, name) in DISPLAY_SLOTS {
            assert_eq!(
                ScoreboardSlotArgumentType.parse(&mut StringReader::new(name)),
                Ok(slot)
            );
        }
        for name in [
            "sidebar.team.reset",
            "Sidebar",
            "sidebar.team.unknown",
            "belowname",
            "0",
        ] {
            let error = ScoreboardSlotArgumentType
                .parse(&mut StringReader::new(name))
                .unwrap_err();
            assert!(error.is(&INVALID_SLOT));
        }
        assert!(matches!(
            ScoreboardSlotArgumentType.client_side_parser(),
            JavaClientArgumentType::ScoreboardSlot
        ));
    }
}
