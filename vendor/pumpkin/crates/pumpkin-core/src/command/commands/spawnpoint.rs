use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::wrap_degrees;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

use crate::command::argument_builder::{ArgumentBuilder, argument, command};
use crate::command::argument_types::coordinates::block_pos::BlockPosArgumentType;
use crate::command::argument_types::coordinates::rotation::RotationArgumentType;
use crate::command::argument_types::entity::EntityArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::command_syntax_error::CommandSyntaxError;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};

const DESCRIPTION: &str = "Sets the spawn point for a player.";
const PERMISSION: &str = "minecraft:command.spawnpoint";

const ERROR_NOT_PLAYER: CommandErrorType<0> = CommandErrorType::new(
    translation::java::PERMISSIONS_REQUIRES_PLAYER,
    translation::java::PERMISSIONS_REQUIRES_PLAYER,
);

enum SpawnpointMode {
    SelfDefault,
    TargetsDefault,
    TargetsPos,
    TargetsPosRotation,
}

struct SpawnpointExecutor(SpawnpointMode);

impl SpawnpointExecutor {
    fn rotation(context: &CommandContext) -> Result<(f32, f32), CommandSyntaxError> {
        let rotation = RotationArgumentType::get(context, "rotation")?.rotation(&context.source);
        Ok((wrap_degrees(rotation.y), rotation.x.clamp(-90.0, 90.0)))
    }
}

impl CommandExecutor for SpawnpointExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = match self.0 {
            SpawnpointMode::SelfDefault => {
                let player = context
                    .source
                    .output
                    .as_player()
                    .ok_or_else(|| ERROR_NOT_PLAYER.create_without_context())?;
                vec![player]
            }
            _ => EntityArgumentType::get_players(context, "targets")?,
        };

        let (pos, (yaw, pitch)) = match self.0 {
            SpawnpointMode::SelfDefault | SpawnpointMode::TargetsDefault => {
                let block_pos = BlockPos::floored_v(context.source.position);
                (block_pos, (0.0, 0.0))
            }
            SpawnpointMode::TargetsPos => {
                let block_pos = BlockPosArgumentType::get_block_pos(context, "pos")?;
                (block_pos, (0.0, 0.0))
            }
            SpawnpointMode::TargetsPosRotation => {
                let block_pos = BlockPosArgumentType::get_block_pos(context, "pos")?;
                (block_pos, Self::rotation(context)?)
            }
        };

        let world = context.source.world();
        let dimension = world.dimension.clone();
        let dimension_name = dimension.minecraft_name.to_string();

        for target in &targets {
            target.set_respawn_point(dimension.clone(), pos, yaw, pitch, true);
        }

        if targets.len() == 1 {
            context.source.send_feedback(
                TextComponent::translate_cross(
                    translation::java::COMMANDS_SPAWNPOINT_SUCCESS_SINGLE,
                    translation::bedrock::COMMANDS_SPAWNPOINT_SUCCESS_SINGLE,
                    [
                        TextComponent::text(pos.0.x.to_string()),
                        TextComponent::text(pos.0.y.to_string()),
                        TextComponent::text(pos.0.z.to_string()),
                        TextComponent::text(yaw.to_string()),
                        TextComponent::text(pitch.to_string()),
                        TextComponent::text(dimension_name),
                        TextComponent::text(targets[0].gameprofile.name.clone()),
                    ],
                ),
                true,
            );
        } else {
            context.source.send_feedback(
                TextComponent::translate_cross(
                    translation::java::COMMANDS_SPAWNPOINT_SUCCESS_MULTIPLE,
                    translation::java::COMMANDS_SPAWNPOINT_SUCCESS_MULTIPLE,
                    [
                        TextComponent::text(pos.0.x.to_string()),
                        TextComponent::text(pos.0.y.to_string()),
                        TextComponent::text(pos.0.z.to_string()),
                        TextComponent::text(yaw.to_string()),
                        TextComponent::text(pitch.to_string()),
                        TextComponent::text(dimension_name),
                        TextComponent::text(targets.len().to_string()),
                    ],
                ),
                true,
            );
        }

        Ok(targets.len() as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("spawnpoint", DESCRIPTION)
            .requires(PERMISSION)
            .executes(SpawnpointExecutor(SpawnpointMode::SelfDefault))
            .then(
                argument("targets", EntityArgumentType::Players)
                    .executes(SpawnpointExecutor(SpawnpointMode::TargetsDefault))
                    .then(
                        argument("pos", BlockPosArgumentType)
                            .executes(SpawnpointExecutor(SpawnpointMode::TargetsPos))
                            .then(
                                argument("rotation", RotationArgumentType).executes(
                                    SpawnpointExecutor(SpawnpointMode::TargetsPosRotation),
                                ),
                            ),
                    ),
            ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CommandSender, test::CommandFixture};
    use crate::entity::EntityBase;
    use pumpkin_nbt::compound::NbtCompound;
    use std::sync::Arc;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spawnpoint_normalizes_resolved_rotation_before_setting_real_players_spawn() {
        let fixture = CommandFixture::new().await;
        fixture.player.get_entity().set_rotation(170.0, 15.0);
        for (rotation, yaw, pitch) in [
            ("90 10", 90.0, 10.0),
            ("540 200", -180.0, 90.0),
            ("~10 ~-200", -180.0, -90.0),
        ] {
            let input = format!("spawnpoint @s 1 64 2 {rotation}");
            let source = Arc::new(
                CommandSender::Player(fixture.player.clone()).into_source(&fixture.server),
            );
            let dispatcher = fixture.server.command_dispatcher.load();
            let parsed = dispatcher.parse_input(&input, &source);
            assert!(parsed.errors.is_empty(), "{input}: {:?}", parsed.errors);
            assert_eq!(parsed.reader.remaining_length(), 0, "{input}");
            let context = parsed.context.build(&input);
            // The production argument path must clamp pitch before the setter.
            // RespawnPoint does not yet retain pitch, so saved pitch is not asserted.
            assert_eq!(
                SpawnpointExecutor::rotation(context.get_last_child()).unwrap(),
                (yaw, pitch)
            );
            assert_eq!(dispatcher.execute_input(&input, &source).unwrap(), 1);
            let spawn = fixture
                .player
                .respawn_point
                .lock()
                .unwrap()
                .clone()
                .unwrap();
            assert_eq!(spawn.position, BlockPos::new(1, 64, 2));
            assert_eq!(spawn.yaw, yaw);
            assert!(spawn.force);
            assert_eq!(spawn.dimension, fixture.player.world().dimension);
            let mut nbt = NbtCompound::new();
            fixture.player.write_nbt(&mut nbt);
            assert_eq!(
                nbt.get_compound("respawn").unwrap().get_float("yaw"),
                Some(yaw)
            );
        }
        fixture.close().await;
    }
}
