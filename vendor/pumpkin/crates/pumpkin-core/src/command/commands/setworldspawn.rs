use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

use crate::command::argument_builder::{ArgumentBuilder, argument, command};
use crate::command::argument_types::coordinates::block_pos::BlockPosArgumentType;
use crate::command::argument_types::coordinates::rotation::RotationArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::plugin::world::spawn_change::SpawnChangeEvent;
use crate::world::level_info::update_level_info;

const DESCRIPTION: &str = "Sets the world spawn point.";
const PERMISSION: &str = "minecraft:command.setworldspawn";

enum SetWorldSpawnMode {
    SelfPos,
    PosOnly,
    PosAndRotation,
}

struct SetWorldSpawnExecutor(SetWorldSpawnMode);

impl CommandExecutor for SetWorldSpawnExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let (pos, (yaw, pitch)) = match self.0 {
            SetWorldSpawnMode::SelfPos => {
                let block_pos = BlockPos::floored_v(context.source.position);
                (block_pos, (0.0, 0.0))
            }
            SetWorldSpawnMode::PosOnly => {
                let block_pos = BlockPosArgumentType::get_block_pos(context, "pos")?;
                (block_pos, (0.0, 0.0))
            }
            SetWorldSpawnMode::PosAndRotation => {
                let block_pos = BlockPosArgumentType::get_block_pos(context, "pos")?;
                let rot = RotationArgumentType::get(context, "rotation")?.rotation(&context.source);
                (block_pos, (rot.y, rot.x))
            }
        };

        let world = context.source.world();
        let server = context.source.server();
        let current_info = server.level_info.load();
        let previous_position = BlockPos::new(
            current_info.spawn_x,
            current_info.spawn_y,
            current_info.spawn_z,
        );
        let new_position = pos;
        let previous_yaw = current_info.spawn_yaw;
        let previous_pitch = current_info.spawn_pitch;
        let new_yaw = yaw;
        let new_pitch = pitch;
        let mut event = SpawnChangeEvent::new(
            world.clone(),
            previous_position,
            previous_yaw,
            previous_pitch,
            new_position,
            new_yaw,
            new_pitch,
        );
        if let Some(server_arc) = world.server.upgrade() {
            server_arc
                .plugin_manager
                .fire_blocking(&server_arc, &mut event);
        }

        update_level_info(&server.level_info, |info| {
            info.spawn_x = new_position.0.x;
            info.spawn_y = new_position.0.y;
            info.spawn_z = new_position.0.z;
            info.spawn_yaw = new_yaw;
            info.spawn_pitch = new_pitch;
        });

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_SETWORLDSPAWN_SUCCESS,
                translation::bedrock::COMMANDS_SETWORLDSPAWN_SUCCESS,
                [
                    TextComponent::text(new_position.0.x.to_string()),
                    TextComponent::text(new_position.0.y.to_string()),
                    TextComponent::text(new_position.0.z.to_string()),
                    TextComponent::text(new_yaw.to_string()),
                    TextComponent::text(new_pitch.to_string()),
                    TextComponent::text(world.dimension.minecraft_name),
                ],
            ),
            true,
        );

        Ok(1)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    dispatcher.register(
        command("setworldspawn", DESCRIPTION)
            .requires(PERMISSION)
            .executes(SetWorldSpawnExecutor(SetWorldSpawnMode::SelfPos))
            .then(
                argument("pos", BlockPosArgumentType)
                    .executes(SetWorldSpawnExecutor(SetWorldSpawnMode::PosOnly))
                    .then(
                        argument("rotation", RotationArgumentType)
                            .executes(SetWorldSpawnExecutor(SetWorldSpawnMode::PosAndRotation)),
                    ),
            ),
    );
}

#[cfg(test)]
mod tests {
    use crate::command::{CommandSender, test::CommandFixture};
    use crate::entity::EntityBase;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn world_spawn_preserves_yaw_pitch_order_for_player_source() {
        let fixture = CommandFixture::new().await;
        fixture.player.get_entity().set_rotation(170.0, 15.0);
        for (rotation, yaw, pitch) in [("90 10", 90.0, 10.0), ("~10 ~-5", 180.0, 10.0)] {
            assert_eq!(
                fixture.execute(
                    CommandSender::Player(fixture.player.clone()),
                    &format!("setworldspawn 1 64 2 {rotation}")
                ),
                1
            );
            let info = fixture.server.level_info.load();
            assert_eq!((info.spawn_x, info.spawn_y, info.spawn_z), (1, 64, 2));
            assert_eq!((info.spawn_yaw, info.spawn_pitch), (yaw, pitch));
        }
        fixture.close().await;
    }
}
