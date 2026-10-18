//! Port of ForceLoadCommand. The host changes canonical chunk tickets synchronously.
use super::{
    PreparedCommand,
    builtins::{self, CommandResult},
    exceptions::error,
};
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_protocol::chat::Component;
use ferraloom_util::ChunkPos;

pub trait ForceLoadAccess {
    fn forced_chunks(&self) -> Vec<ChunkPos>;
    fn set_chunk_forced(
        &mut self,
        position: ChunkPos,
        forced: bool,
    ) -> Result<bool, CommandSyntaxException>;
}
pub struct ForceLoadOutput {
    pub result: CommandResult,
    /// An empty list sends failure feedback while still completing with the successful value zero.
    pub failure: Option<Component>,
}
fn success(
    value: i32,
    key: &str,
    args: Vec<ferraloom_protocol::chat::Argument>,
    broadcast: bool,
) -> ForceLoadOutput {
    ForceLoadOutput {
        result: CommandResult::success(value, Component::translatable(key, args), broadcast),
        failure: None,
    }
}
fn chunk_text(position: ChunkPos) -> String {
    format!("[{}, {}]", position.x, position.z)
}
fn column(command: &PreparedCommand, name: &str) -> [i32; 2] {
    let position = command
        .coordinates(name)
        .expect("column argument")
        .position(&command.source, None);
    [position.x.floor() as i32, position.z.floor() as i32]
}
pub fn run<A: ForceLoadAccess>(
    command: &PreparedCommand,
    access: &mut A,
) -> Result<ForceLoadOutput, CommandSyntaxException> {
    let dimension = &command.source.level.dimension;
    let action = command.literals[1].as_str();
    if action == "query" {
        let forced = access.forced_chunks();
        if command.argument("pos").is_some() {
            let [x, z] = column(command, "pos");
            let position = ChunkPos::new(x >> 4, z >> 4);
            if !forced.contains(&position) {
                return Err(error(
                    "commands.forceload.query.failure",
                    &[chunk_text(position), dimension.clone()],
                ));
            }
            return Ok(success(
                1,
                "commands.forceload.query.success",
                vec![
                    builtins::string(chunk_text(position)),
                    builtins::string(dimension),
                ],
                false,
            ));
        }
        if forced.is_empty() {
            return Ok(ForceLoadOutput {
                result: CommandResult {
                    value: 0,
                    feedback: Vec::new(),
                    rule_change: None,
                },
                failure: Some(Component::translatable(
                    "commands.forceload.added.none",
                    vec![builtins::string(dimension)],
                )),
            });
        }
        let mut chunks = forced.iter().map(|chunk| chunk.pack()).collect::<Vec<_>>();
        chunks.sort();
        let list = chunks
            .into_iter()
            .map(|chunk| chunk_text(ChunkPos::unpack(chunk)))
            .collect::<Vec<_>>()
            .join(", ");
        let count = forced.len() as i32;
        return Ok(if count == 1 {
            success(
                count,
                "commands.forceload.list.single",
                vec![builtins::string(dimension), builtins::string(list)],
                false,
            )
        } else {
            success(
                count,
                "commands.forceload.list.multiple",
                vec![
                    builtins::integer(count),
                    builtins::string(dimension),
                    builtins::string(list),
                ],
                false,
            )
        });
    }
    if action == "remove" && command.literals.iter().any(|literal| literal == "all") {
        for chunk in access.forced_chunks() {
            access.set_chunk_forced(chunk, false)?;
        }
        return Ok(success(
            0,
            "commands.forceload.removed.all",
            vec![builtins::string(dimension)],
            true,
        ));
    }
    let from = column(command, "from");
    let to = if command.argument("to").is_some() {
        column(command, "to")
    } else {
        from
    };
    let min = [from[0].min(to[0]), from[1].min(to[1])];
    let max = [from[0].max(to[0]), from[1].max(to[1])];
    if min.iter().any(|value| *value < -30_000_000) || max.iter().any(|value| *value >= 30_000_000)
    {
        return Err(error("argument.pos.outofworld", &[]));
    }
    let min = ChunkPos::new(min[0] >> 4, min[1] >> 4);
    let max = ChunkPos::new(max[0] >> 4, max[1] >> 4);
    let total =
        (i64::from(max.x) - i64::from(min.x) + 1) * (i64::from(max.z) - i64::from(min.z) + 1);
    if total > 256 {
        return Err(error(
            "commands.forceload.toobig",
            &["256".into(), total.to_string()],
        ));
    }
    let add = action == "add";
    let mut changed = Vec::new();
    for x in min.x..=max.x {
        for z in min.z..=max.z {
            let position = ChunkPos::new(x, z);
            if access.set_chunk_forced(position, add)? {
                changed.push(position);
            }
        }
    }
    let action = if add { "added" } else { "removed" };
    let count = changed.len() as i32;
    if count == 0 {
        return Err(error(&format!("commands.forceload.{action}.failure"), &[]));
    }
    Ok(if count == 1 {
        success(
            count,
            &format!("commands.forceload.{action}.single"),
            vec![
                builtins::string(chunk_text(changed[0])),
                builtins::string(dimension),
            ],
            true,
        )
    } else {
        success(
            count,
            &format!("commands.forceload.{action}.multiple"),
            vec![
                builtins::integer(count),
                builtins::string(dimension),
                builtins::string(chunk_text(min)),
                builtins::string(chunk_text(max)),
            ],
            true,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{CommandSourceStack, Commands};
    use ferraloom_util::{Vec3, collections::LongOpenHashSet};
    #[derive(Default)]
    struct Tickets(LongOpenHashSet);
    impl ForceLoadAccess for Tickets {
        fn forced_chunks(&self) -> Vec<ChunkPos> {
            self.0
                .iter()
                .map(|chunk| ChunkPos::unpack(*chunk))
                .collect()
        }
        fn set_chunk_forced(
            &mut self,
            position: ChunkPos,
            forced: bool,
        ) -> Result<bool, CommandSyntaxException> {
            Ok(if forced {
                self.0.insert(position.pack())
            } else {
                self.0.remove(&position.pack())
            })
        }
    }
    #[derive(serde::Deserialize)]
    struct Vector {
        command: String,
        feedback: serde_json::Value,
        forced: Vec<i64>,
        error: Option<String>,
        value: Option<i32>,
    }
    #[test]
    fn force_load_feedback_limits_and_mutation_sequence_match_java() {
        let commands = Commands::default();
        let mut tickets = Tickets::default();
        let source = CommandSourceStack::server(4, Vec3::ZERO);
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/forceload-vectors.json")).unwrap();
        for vector in vectors {
            let command = commands.prepare(&vector.command, source.clone()).unwrap();
            let result = run(&command, &mut tickets);
            if let Some(expected) = vector.error {
                assert_eq!(
                    result.err().unwrap().raw_message.text(),
                    expected,
                    "{}",
                    vector.command
                );
            } else {
                let output = result.unwrap();
                assert_eq!(
                    output.result.value,
                    vector.value.unwrap(),
                    "{}",
                    vector.command
                );
                let mut feedback = output.result.feedback.into_iter().map(|feedback| serde_json::json!({"failure":false,"broadcast":feedback.broadcast,"message":feedback.message.to_json().unwrap()})).collect::<Vec<_>>();
                if let Some(failure) = output.failure {
                    feedback.push(serde_json::json!({"failure":true,"broadcast":false,"message":failure.to_json().unwrap()}));
                }
                assert_eq!(
                    serde_json::json!(feedback),
                    vector.feedback,
                    "{}",
                    vector.command
                );
            }
            let mut forced = tickets.0.iter().copied().collect::<Vec<_>>();
            forced.sort();
            assert_eq!(forced, vector.forced, "{}", vector.command);
        }
    }
}
