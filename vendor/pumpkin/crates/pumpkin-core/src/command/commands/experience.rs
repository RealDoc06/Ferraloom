use std::sync::Arc;

use pumpkin_util::PermissionLvl;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

use crate::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use crate::command::argument_types::core::integer::IntegerArgumentType;
use crate::command::argument_types::entity::EntityArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::entity::EntityBase;
use crate::entity::player::Player;
use crate::entity::player::experience::Experience;

const DESCRIPTION: &str = "Add, set or query player experience.";
const PERMISSION: &str = "minecraft:command.experience";

const ERROR_SET_POINTS_INVALID: CommandErrorType<0> = CommandErrorType::new(
    "commands.experience.set.points.invalid",
    "commands.experience.set.points.invalid",
);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Add,
    Set,
    Query,
}

#[derive(Clone, Copy, PartialEq)]
enum ExpType {
    Points,
    Levels,
}

impl ExpType {
    fn query(self, experience: Experience) -> i32 {
        match self {
            Self::Points => experience.points(),
            Self::Levels => experience.level,
        }
    }

    fn can_set(self, experience: Experience, amount: i32) -> bool {
        match self {
            Self::Points => amount < experience.points_needed(),
            Self::Levels => true,
        }
    }
}

struct ExperienceExecutor {
    mode: Mode,
    exp_type: ExpType,
}

impl ExperienceExecutor {
    fn handle_query(&self, context: &CommandContext, target: &Player) -> i32 {
        let val = self.exp_type.query(target.experience());
        let translation_key = match self.exp_type {
            ExpType::Levels => "commands.experience.query.levels",
            ExpType::Points => "commands.experience.query.points",
        };

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation_key,
                translation_key,
                [
                    target.get_display_name(),
                    TextComponent::text(val.to_string()),
                ],
            ),
            false,
        );

        val
    }

    fn get_success_message(
        &self,
        amount: i32,
        target_count: usize,
        first_target_name: TextComponent,
    ) -> TextComponent {
        let type_str = match self.exp_type {
            ExpType::Points => "points",
            ExpType::Levels => "levels",
        };
        let mode_str = match self.mode {
            Mode::Add => "add",
            Mode::Set => "set",
            Mode::Query => "query",
        };

        let bedrock_key = match self.exp_type {
            ExpType::Points => pumpkin_data::translation::bedrock::COMMANDS_XP_SUCCESS,
            ExpType::Levels => {
                if amount >= 0 {
                    pumpkin_data::translation::bedrock::COMMANDS_XP_SUCCESS_LEVELS
                } else {
                    pumpkin_data::translation::bedrock::COMMANDS_XP_SUCCESS_NEGATIVE_LEVELS
                }
            }
        };

        if target_count > 1 {
            TextComponent::translate_cross(
                format!("commands.experience.{mode_str}.{type_str}.success.multiple"),
                bedrock_key,
                [
                    TextComponent::text(amount.to_string()),
                    TextComponent::text(target_count.to_string()),
                ],
            )
        } else {
            TextComponent::translate_cross(
                format!("commands.experience.{mode_str}.{type_str}.success.single"),
                bedrock_key,
                [TextComponent::text(amount.to_string()), first_target_name],
            )
        }
    }

    fn handle_modify(&self, target: &Arc<Player>, amount: i32) -> bool {
        if self.mode == Mode::Set && !self.exp_type.can_set(target.experience(), amount) {
            return false;
        }
        match self.exp_type {
            ExpType::Levels => {
                if self.mode == Mode::Add {
                    target.add_experience_levels(amount);
                } else {
                    target.set_experience_level(amount, true);
                }
            }
            ExpType::Points => {
                if self.mode == Mode::Add {
                    target.add_experience_points(amount);
                } else {
                    target.set_experience_points(amount);
                }
            }
        }
        true
    }
}

impl CommandExecutor for ExperienceExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_players(context, "target")?;

        if self.mode == Mode::Query {
            let target = targets[0].clone();
            return Ok(self.handle_query(context, &target));
        }

        let amount = IntegerArgumentType::get(context, "amount")?;

        let mut successes = 0;
        let mut first_success = None;
        for target in &targets {
            if self.handle_modify(target, amount) {
                successes += 1;
                first_success.get_or_insert(target);
            }
        }

        let Some(first_success) = first_success else {
            return Err(ERROR_SET_POINTS_INVALID.create_without_context());
        };

        let first_name = first_success.get_display_name();
        let msg = self.get_success_message(amount, successes, first_name);
        context.source.send_feedback(msg, true);

        Ok(successes as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    let add_node = literal("add").then(
        argument("target", EntityArgumentType::Players).then(
            argument("amount", IntegerArgumentType::any())
                .executes(ExperienceExecutor {
                    mode: Mode::Add,
                    exp_type: ExpType::Points,
                })
                .then(literal("points").executes(ExperienceExecutor {
                    mode: Mode::Add,
                    exp_type: ExpType::Points,
                }))
                .then(literal("levels").executes(ExperienceExecutor {
                    mode: Mode::Add,
                    exp_type: ExpType::Levels,
                })),
        ),
    );

    let set_node = literal("set").then(
        argument("target", EntityArgumentType::Players).then(
            argument("amount", IntegerArgumentType::with_min(0))
                .executes(ExperienceExecutor {
                    mode: Mode::Set,
                    exp_type: ExpType::Points,
                })
                .then(literal("points").executes(ExperienceExecutor {
                    mode: Mode::Set,
                    exp_type: ExpType::Points,
                }))
                .then(literal("levels").executes(ExperienceExecutor {
                    mode: Mode::Set,
                    exp_type: ExpType::Levels,
                })),
        ),
    );

    let query_node = literal("query").then(
        argument("target", EntityArgumentType::Player)
            .then(literal("points").executes(ExperienceExecutor {
                mode: Mode::Query,
                exp_type: ExpType::Points,
            }))
            .then(literal("levels").executes(ExperienceExecutor {
                mode: Mode::Query,
                exp_type: ExpType::Levels,
            })),
    );

    let cmd = command("experience", DESCRIPTION)
        .requires(PERMISSION)
        .then(add_node)
        .then(set_node)
        .then(query_node);

    dispatcher.register_with_aliases(cmd, &["xp"]);
}

#[cfg(test)]
mod tests {
    use super::{ExpType, Experience};

    #[test]
    fn query_uses_the_float_bar_instead_of_total_or_rounded_remainder() {
        // First progress is a retained Java save; continuation/query are derived
        // from the official bytecode and still need paired runtime verification.
        for (total, progress_bits, expected_points) in
            [(223, 0x3e67_39c4, 6), (227, 0x3eb5_ad66, 10)]
        {
            let experience = Experience {
                level: 12,
                progress: f32::from_bits(progress_bits),
                total,
            };
            assert_eq!(ExpType::Points.query(experience), expected_points);
            assert_eq!(ExpType::Levels.query(experience), 12);
        }
    }

    #[test]
    fn set_points_rejects_the_next_level_boundary_before_clamping() {
        let mut experience = Experience {
            level: 12,
            progress: 0.0,
            total: 223,
        };
        assert!(ExpType::Points.can_set(experience, 0));
        assert!(ExpType::Points.can_set(experience, 30));
        assert!(!ExpType::Points.can_set(experience, 31));
        assert!(!ExpType::Points.can_set(experience, i32::MAX));
        assert!(ExpType::Levels.can_set(experience, i32::MAX));
        experience.set_points(7);
        assert_eq!(experience.progress.to_bits(), 0x3e67_39ce);
        assert_eq!(experience.total, 223);
        assert_eq!(ExpType::Points.query(experience), 7);
        experience.set_level(0);
        assert!(!ExpType::Points.can_set(experience, 7));
        assert!(ExpType::Points.can_set(experience, 6));
    }
}
