//! Port of LevelData.RespawnData and SetSpawnCommand's persisted respawn config.
use super::{
    PreparedCommand, arguments::coordinates::Coordinates, builtins, exceptions::error,
    source::Rotation,
};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Identifier,
    chat::{Argument, Component, Numeric},
    codec::GlobalPos,
    play::RespawnData,
};
use ferraloom_util::{BlockPos, mth};

#[derive(Clone, Debug, PartialEq)]
pub struct SpawnData {
    pub dimension: String,
    pub position: BlockPos,
    pub rotation: Rotation,
}
impl SpawnData {
    pub fn command(command: &PreparedCommand) -> Result<Self, CommandSyntaxException> {
        let position = command
            .coordinates("pos")
            .map_or(command.source.position, |coordinates| {
                coordinates.position(&command.source, None)
            });
        let position = BlockPos::new(
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        );
        if command.argument("pos").is_some()
            && (!(-30_000_000..30_000_000).contains(&position.x)
                || !(-20_000_000..20_000_000).contains(&position.y)
                || !(-30_000_000..30_000_000).contains(&position.z))
        {
            return Err(error("argument.pos.outofbounds", &[]));
        }
        let rotation = command
            .coordinates("rotation")
            .map_or(Rotation::default(), |coordinates: Coordinates| {
                coordinates.rotation(&command.source)
            });
        Ok(Self {
            dimension: command.source.level.dimension.clone(),
            position,
            rotation: Rotation {
                yaw: mth::wrap_degrees_f32(rotation.yaw),
                pitch: rotation.pitch.clamp(-90., 90.),
            },
        })
    }
    pub fn to_nbt(&self) -> CompoundTag {
        let mut tag = CompoundTag::new();
        tag.insert("dimension", Tag::String(self.dimension.clone().into()));
        tag.insert(
            "pos",
            Tag::IntArray(vec![self.position.x, self.position.y, self.position.z]),
        );
        tag.insert("yaw", Tag::Float(self.rotation.yaw));
        tag.insert("pitch", Tag::Float(self.rotation.pitch));
        tag
    }
    pub fn packet_data(&self) -> RespawnData {
        RespawnData {
            global_pos: GlobalPos {
                dimension: Identifier::parse(&self.dimension).expect("dimension identifier"),
                pos: self.position,
            },
            yaw: self.rotation.yaw,
            pitch: self.rotation.pitch,
        }
    }
    pub fn feedback_arguments(&self) -> Vec<Argument> {
        vec![
            builtins::integer(self.position.x),
            builtins::integer(self.position.y),
            builtins::integer(self.position.z),
            Argument::Number(Numeric::Float(self.rotation.yaw)),
            Argument::Number(Numeric::Float(self.rotation.pitch)),
            builtins::string(&self.dimension),
        ]
    }
    pub fn world_feedback(&self) -> Component {
        Component::translatable("commands.setworldspawn.success", self.feedback_arguments())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{CommandSourceStack, Commands};
    use ferraloom_util::Vec3;
    #[derive(serde::Deserialize)]
    struct Vector {
        rotation: String,
        nbt: String,
        component: serde_json::Value,
    }
    #[test]
    fn spawn_rotation_persistence_and_feedback_match_java() {
        let commands = Commands::default();
        let source = CommandSourceStack::server(4, Vec3::ZERO).with_rotation(Rotation {
            pitch: -60.,
            yaw: 35.,
        });
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/spawn-vectors.json")).unwrap();
        for vector in vectors {
            let command = commands
                .prepare(
                    &format!("setworldspawn 1 80 -2 {}", vector.rotation),
                    source.clone(),
                )
                .unwrap();
            let spawn = SpawnData::command(&command).unwrap();
            assert_eq!(
                Tag::Compound(spawn.to_nbt()),
                ferraloom_nbt::parse(&vector.nbt).unwrap()
            );
            assert_eq!(spawn.world_feedback().to_json().unwrap(), vector.component);
        }
    }
}
