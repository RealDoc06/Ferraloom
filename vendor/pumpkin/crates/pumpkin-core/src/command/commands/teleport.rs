use std::sync::Arc;

use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::{Axis, Vector3};
use pumpkin_util::math::wrap_degrees;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;

use crate::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use crate::command::argument_types::coordinates::rotation::RotationArgumentType;
use crate::command::argument_types::coordinates::vec3::Vec3ArgumentType;
use crate::command::argument_types::entity::EntityArgumentType;
use crate::command::argument_types::entity_anchor::{
    EntityAnchor, EntityAnchorArgumentType, EntityAnchorExt,
};
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::entity::EntityBase;
use crate::world::World;

const DESCRIPTION: &str = "Teleports entities, including players.";
const PERMISSION: &str = "minecraft:command.teleport";

const ERROR_INVALID_POSITION: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_TELEPORT_INVALIDPOSITION,
    translation::java::COMMANDS_TELEPORT_INVALIDPOSITION,
);

fn yaw_pitch_facing_position(
    looking_from: &Vector3<f64>,
    looking_towards: &Vector3<f64>,
) -> (f32, f32) {
    let direction_vector = (looking_towards.sub(looking_from)).normalize();

    let yaw_radians = -direction_vector.x.atan2(direction_vector.z);
    let pitch_radians = (-direction_vector.y).asin();

    let yaw_degrees = yaw_radians.to_degrees();
    let pitch_degrees = pitch_radians.to_degrees();

    (yaw_degrees as f32, pitch_degrees as f32)
}

fn success_key_and_arg(
    targets: &[Arc<dyn EntityBase>],
    single_key: &'static str,
    multiple_key: &'static str,
) -> (&'static str, TextComponent) {
    if targets.len() == 1 {
        (single_key, targets[0].get_display_name())
    } else {
        (multiple_key, TextComponent::text(targets.len().to_string()))
    }
}

struct SelfToPosExecutor;

impl CommandExecutor for SelfToPosExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let entity = context.source.entity_or_err()?;

        let pos = Vec3ArgumentType::get_coordinates(context, "location")?.resolve(&context.source);
        if !World::is_valid(BlockPos(pos.floor_to_i32())) {
            return Err(ERROR_INVALID_POSITION.create_without_context());
        }

        let yaw = entity.get_entity().yaw.load();
        let pitch = entity.get_entity().pitch.load();
        let world = context.source.world();
        entity.teleport(pos, Some(yaw), Some(pitch), world.clone());

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TELEPORT_SUCCESS_LOCATION_SINGLE,
                translation::bedrock::COMMANDS_TP_SUCCESS_COORDINATES,
                [
                    entity.get_display_name(),
                    TextComponent::text(pos.x.to_string()),
                    TextComponent::text(pos.y.to_string()),
                    TextComponent::text(pos.z.to_string()),
                ],
            ),
            true,
        );

        Ok(1)
    }
}

struct SelfToEntityExecutor;

impl CommandExecutor for SelfToEntityExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let entity = context.source.entity_or_err()?;

        let destination = EntityArgumentType::get_entity(context, "destination")?;
        let destination_entity = destination.get_entity();
        let pos = destination_entity.pos.load();
        let yaw = destination_entity.yaw.load();
        let pitch = destination_entity.pitch.load();
        let world = destination_entity.world.load_full();

        if !World::is_valid(BlockPos(pos.floor_to_i32())) {
            return Err(ERROR_INVALID_POSITION.create_without_context());
        }

        entity.teleport(pos, Some(yaw), Some(pitch), world);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_TELEPORT_SUCCESS_ENTITY_SINGLE,
                translation::bedrock::COMMANDS_TP_SUCCESSVICTIM,
                [
                    entity.get_display_name(),
                    destination_entity.get_display_name(),
                ],
            ),
            true,
        );

        Ok(1)
    }
}

struct EntitiesToEntityExecutor;

impl CommandExecutor for EntitiesToEntityExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_entities(context, "targets")?;
        let destination = EntityArgumentType::get_entity(context, "destination")?;
        let destination_entity = destination.get_entity();
        let pos = destination_entity.pos.load();
        let yaw = destination_entity.yaw.load();
        let pitch = destination_entity.pitch.load();
        let world = destination_entity.world.load_full();

        if !World::is_valid(BlockPos(pos.floor_to_i32())) {
            return Err(ERROR_INVALID_POSITION.create_without_context());
        }

        for target in &targets {
            target.teleport(pos, Some(yaw), Some(pitch), world.clone());
        }

        let (key, target_arg) = success_key_and_arg(
            &targets,
            translation::java::COMMANDS_TELEPORT_SUCCESS_ENTITY_SINGLE,
            translation::java::COMMANDS_TELEPORT_SUCCESS_ENTITY_MULTIPLE,
        );
        context.source.send_feedback(
            TextComponent::translate_cross(
                key,
                translation::bedrock::COMMANDS_TP_SUCCESSVICTIM,
                [target_arg, destination_entity.get_display_name()],
            ),
            true,
        );

        Ok(targets.len() as i32)
    }
}

struct EntitiesToPosExecutor;

impl CommandExecutor for EntitiesToPosExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_entities(context, "targets")?;
        let pos = Vec3ArgumentType::get_coordinates(context, "location")?.resolve(&context.source);

        if !World::is_valid(BlockPos(pos.floor_to_i32())) {
            return Err(ERROR_INVALID_POSITION.create_without_context());
        }

        let world = context.source.world();
        for target in &targets {
            let yaw = target.get_entity().yaw.load();
            let pitch = target.get_entity().pitch.load();
            target.teleport(pos, Some(yaw), Some(pitch), world.clone());
        }

        let (key, target_arg) = success_key_and_arg(
            &targets,
            translation::java::COMMANDS_TELEPORT_SUCCESS_LOCATION_SINGLE,
            translation::java::COMMANDS_TELEPORT_SUCCESS_LOCATION_MULTIPLE,
        );
        context.source.send_feedback(
            TextComponent::translate_cross(
                key,
                translation::bedrock::COMMANDS_TP_SUCCESS_COORDINATES,
                [
                    target_arg,
                    TextComponent::text(pos.x.to_string()),
                    TextComponent::text(pos.y.to_string()),
                    TextComponent::text(pos.z.to_string()),
                ],
            ),
            true,
        );

        Ok(targets.len() as i32)
    }
}

struct EntitiesToPosWithRotationExecutor;

impl CommandExecutor for EntitiesToPosWithRotationExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_entities(context, "targets")?;
        let pos = Vec3ArgumentType::get_coordinates(context, "location")?.resolve(&context.source);

        if !World::is_valid(BlockPos(pos.floor_to_i32())) {
            return Err(ERROR_INVALID_POSITION.create_without_context());
        }

        let coordinates = RotationArgumentType::get(context, "rotation")?;
        let rotation = coordinates.rotation(&context.source);
        let world = context.source.world();

        for target in &targets {
            let entity = target.get_entity();
            // Java 26.3 performTeleport wraps each victim's relative delta;
            // calculateAbsolute adds its current angle without wrapping again.
            let yaw = if coordinates.is_relative(Axis::Y) {
                let current = entity.yaw.load();
                current + wrap_degrees(rotation.y - current)
            } else {
                wrap_degrees(rotation.y)
            };
            let pitch = if coordinates.is_relative(Axis::X) {
                let current = entity.pitch.load();
                current + wrap_degrees(rotation.x - current)
            } else {
                wrap_degrees(rotation.x)
            }
            .clamp(-90.0, 90.0);
            target.teleport(pos, Some(yaw), Some(pitch), world.clone());
        }

        let (key, target_arg) = success_key_and_arg(
            &targets,
            translation::java::COMMANDS_TELEPORT_SUCCESS_LOCATION_SINGLE,
            translation::java::COMMANDS_TELEPORT_SUCCESS_LOCATION_MULTIPLE,
        );
        context.source.send_feedback(
            TextComponent::translate_cross(
                key,
                translation::bedrock::COMMANDS_TP_SUCCESS_COORDINATES,
                [
                    target_arg,
                    TextComponent::text(pos.x.to_string()),
                    TextComponent::text(pos.y.to_string()),
                    TextComponent::text(pos.z.to_string()),
                ],
            ),
            true,
        );

        Ok(targets.len() as i32)
    }
}

struct EntitiesToPosFacingPosExecutor;

impl CommandExecutor for EntitiesToPosFacingPosExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_entities(context, "targets")?;
        let pos = Vec3ArgumentType::get_coordinates(context, "location")?.resolve(&context.source);

        if !World::is_valid(BlockPos(pos.floor_to_i32())) {
            return Err(ERROR_INVALID_POSITION.create_without_context());
        }

        let facing_pos =
            Vec3ArgumentType::get_coordinates(context, "facingLocation")?.resolve(&context.source);
        let world = context.source.world();

        for target in &targets {
            let eye_offset = match context.source.entity_anchor {
                EntityAnchor::Feet => 0.0,
                EntityAnchor::Eyes => target.get_entity().get_eye_height(),
            };
            let looking_from = Vector3::new(pos.x, pos.y + eye_offset, pos.z);
            let (yaw, pitch) = yaw_pitch_facing_position(&looking_from, &facing_pos);
            target.teleport(pos, Some(yaw), Some(pitch), world.clone());
        }

        let (key, target_arg) = success_key_and_arg(
            &targets,
            translation::java::COMMANDS_TELEPORT_SUCCESS_LOCATION_SINGLE,
            translation::java::COMMANDS_TELEPORT_SUCCESS_LOCATION_MULTIPLE,
        );
        context.source.send_feedback(
            TextComponent::translate_cross(
                key,
                translation::bedrock::COMMANDS_TP_SUCCESS_COORDINATES,
                [
                    target_arg,
                    TextComponent::text(pos.x.to_string()),
                    TextComponent::text(pos.y.to_string()),
                    TextComponent::text(pos.z.to_string()),
                ],
            ),
            true,
        );

        Ok(targets.len() as i32)
    }
}

struct EntitiesToPosFacingEntityExecutor {
    has_anchor: bool,
}

impl CommandExecutor for EntitiesToPosFacingEntityExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let targets = EntityArgumentType::get_entities(context, "targets")?;
        let pos = Vec3ArgumentType::get_coordinates(context, "location")?.resolve(&context.source);

        if !World::is_valid(BlockPos(pos.floor_to_i32())) {
            return Err(ERROR_INVALID_POSITION.create_without_context());
        }

        let facing_entity = EntityArgumentType::get_entity(context, "facingEntity")?;
        let anchor = if self.has_anchor {
            EntityAnchorArgumentType::get(context, "facingAnchor")?
        } else {
            EntityAnchor::Feet
        };

        let facing_pos = anchor.position_at_entity(facing_entity.get_entity());
        let world = context.source.world();

        for target in &targets {
            let eye_offset = match context.source.entity_anchor {
                EntityAnchor::Feet => 0.0,
                EntityAnchor::Eyes => target.get_entity().get_eye_height(),
            };
            let looking_from = Vector3::new(pos.x, pos.y + eye_offset, pos.z);
            let (yaw, pitch) = yaw_pitch_facing_position(&looking_from, &facing_pos);
            target.teleport(pos, Some(yaw), Some(pitch), world.clone());
        }

        let (key, target_arg) = success_key_and_arg(
            &targets,
            translation::java::COMMANDS_TELEPORT_SUCCESS_LOCATION_SINGLE,
            translation::java::COMMANDS_TELEPORT_SUCCESS_LOCATION_MULTIPLE,
        );
        context.source.send_feedback(
            TextComponent::translate_cross(
                key,
                translation::bedrock::COMMANDS_TP_SUCCESS_COORDINATES,
                [
                    target_arg,
                    TextComponent::text(pos.x.to_string()),
                    TextComponent::text(pos.y.to_string()),
                    TextComponent::text(pos.z.to_string()),
                ],
            ),
            true,
        );

        Ok(targets.len() as i32)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    let cmd = command("teleport", DESCRIPTION)
        .requires(PERMISSION)
        .then(argument("location", Vec3ArgumentType::Default).executes(SelfToPosExecutor))
        .then(argument("destination", EntityArgumentType::Entity).executes(SelfToEntityExecutor))
        .then(
            argument("targets", EntityArgumentType::Entities)
                .then(
                    argument("location", Vec3ArgumentType::Default)
                        .executes(EntitiesToPosExecutor)
                        .then(
                            argument("rotation", RotationArgumentType)
                                .executes(EntitiesToPosWithRotationExecutor),
                        )
                        .then(
                            literal("facing")
                                .then(
                                    literal("entity").then(
                                        argument("facingEntity", EntityArgumentType::Entity)
                                            .executes(EntitiesToPosFacingEntityExecutor {
                                                has_anchor: false,
                                            })
                                            .then(
                                                argument("facingAnchor", EntityAnchorArgumentType)
                                                    .executes(EntitiesToPosFacingEntityExecutor {
                                                        has_anchor: true,
                                                    }),
                                            ),
                                    ),
                                )
                                .then(
                                    argument("facingLocation", Vec3ArgumentType::Default)
                                        .executes(EntitiesToPosFacingPosExecutor),
                                ),
                        ),
                )
                .then(
                    argument("destination", EntityArgumentType::Entity)
                        .executes(EntitiesToEntityExecutor),
                ),
        );

    dispatcher.register_with_aliases(cmd, &["tp"]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{CommandSender, test::CommandFixture};
    use crate::entity::r#type::from_type;
    use pumpkin_data::entity::EntityType;
    use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};

    fn assert_rotation(target: &dyn EntityBase, yaw: f32, pitch: f32) {
        let entity = target.get_entity();
        assert_eq!(entity.yaw.load().to_bits(), yaw.to_bits());
        assert_eq!(entity.pitch.load().to_bits(), pitch.to_bits());
        let mut nbt = NbtCompound::new();
        target.write_nbt(&mut nbt);
        assert_eq!(
            nbt.get_list("Rotation").unwrap(),
            &[NbtTag::Float(yaw), NbtTag::Float(pitch)]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn absolute_teleport_yaw_uses_java_half_open_range() {
        let fixture = CommandFixture::new().await;
        let cases: [(f32, f32); 9] = [
            (180.0, -180.0),
            (-180.0, -180.0),
            (540.0, -180.0),
            (-540.0, -180.0),
            (720.0, 0.0),
            (-720.0, -0.0),
            (-181.0, 179.0),
            (181.0, -179.0),
            (90.0, 90.0),
        ];
        for (requested, expected) in cases {
            assert_eq!(
                fixture.execute(
                    CommandSender::Console,
                    &format!("tp RotationPlayer 0 64 0 {requested} -10")
                ),
                1
            );
            assert_rotation(fixture.player.as_ref(), expected, -10.0);
        }
        fixture.execute(CommandSender::Console, "tp RotationPlayer 0 64 0 0 200");
        assert_rotation(fixture.player.as_ref(), 0.0, -90.0);
        fixture.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn player_source_relative_teleport_retains_positive_180() {
        let fixture = CommandFixture::new().await;
        let entity = fixture.player.get_entity();
        entity.set_pos(Vector3::new(0.0, 64.0, 0.0));
        entity.set_rotation(170.0, 15.0);
        assert_eq!(
            fixture.execute(
                CommandSender::Player(fixture.player.clone()),
                "tp @s ~ ~ ~ ~10 ~"
            ),
            1
        );
        assert_rotation(fixture.player.as_ref(), 180.0, 15.0);
        assert_eq!(entity.pos.load(), Vector3::new(0.0, 64.0, 0.0));
        fixture.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn player_source_local_teleport_uses_yaw_as_forward_direction() {
        let fixture = CommandFixture::new().await;
        let entity = fixture.player.get_entity();
        entity.set_pos(Vector3::new(0.0, 64.0, 0.0));
        entity.set_rotation(-90.0, 0.0);
        assert_eq!(
            fixture.execute(
                CommandSender::Player(fixture.player.clone()),
                "tp @s ^ ^ ^1"
            ),
            1
        );
        assert_eq!(entity.pos.load(), Vector3::new(1.0, 64.0, 0.0));
        fixture.close().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn relative_teleport_wraps_each_victims_delta_before_adding_and_clamping() {
        let fixture = CommandFixture::new().await;
        let world = fixture.player.world();
        let mut targets = Vec::new();
        for (id, yaw, pitch) in [(104, 90.0, 40.0), (105, -170.0, -40.0)] {
            let target = from_type(
                &EntityType::PIG,
                Vector3::new(0.0, 64.0, 0.0),
                &world,
                uuid::Uuid::from_u128(id),
            );
            target.get_entity().set_rotation(yaw, pitch);
            world.add_entity_silent(target.clone());
            targets.push(target);
        }
        assert_eq!(
            fixture.execute(
                CommandSender::Console,
                "tp @e[type=minecraft:pig] 0 64 0 ~190 ~200"
            ),
            2
        );
        assert_rotation(targets[0].as_ref(), 190.0, 90.0);
        assert_rotation(targets[1].as_ref(), -170.0, -90.0);
        fixture.close().await;
    }
}
