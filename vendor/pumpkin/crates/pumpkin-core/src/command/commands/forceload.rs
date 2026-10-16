use crate::command::argument_builder::{ArgumentBuilder, argument, command, literal};
use crate::command::argument_types::coordinates::Coordinates;
use crate::command::argument_types::coordinates::block_pos::OUT_OF_WORLD_ERROR_TYPE;
use crate::command::argument_types::coordinates::column_pos::ColumnPosArgumentType;
use crate::command::context::command_context::CommandContext;
use crate::command::errors::command_syntax_error::CommandSyntaxError;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;
use std::future::Future;
use tokio::runtime::{Handle, RuntimeFlavor};

const DESCRIPTION: &str = "Constantly load chunks in the world.";
const PERMISSION: &str = "minecraft:command.forceload";

static ERROR_FAILED_ADD: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_ADDED_FAILURE,
    "No world in source",
);
static ERROR_FAILED_REMOVE: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_REMOVED_FAILURE,
    "No world in source",
);
static ERROR_FAILED_QUERY: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_QUERY_FAILURE,
    "No world in source",
);
static ERROR_TOO_MANY: CommandErrorType<2> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_TOOBIG,
    "Too many chunks",
);

const ERROR_LOADING: CommandErrorType<0> = CommandErrorType::new(
    "ferraloom.forceload.loading",
    "Unable to load forced chunks during server shutdown or without a multithread server runtime",
);

const ERROR_STORAGE: CommandErrorType<0> = CommandErrorType::new(
    "ferraloom.forceload.storage",
    "Unable to load forced chunks: chunk storage could not be read",
);

#[derive(Debug)]
struct ChunkRectangle {
    min_x: i32,
    max_x: i32,
    min_z: i32,
    max_z: i32,
}

impl ChunkRectangle {
    fn new(from: Vector2<i32>, to: Vector2<i32>) -> Result<Self, CommandSyntaxError> {
        // Vanilla checks block coordinates before converting the inclusive rectangle to chunks.
        if [from.x, from.y, to.x, to.y]
            .into_iter()
            .any(|coordinate| !(-30_000_000..30_000_000).contains(&coordinate))
        {
            return Err(OUT_OF_WORLD_ERROR_TYPE.create_without_context());
        }
        let rectangle = Self {
            min_x: (from.x >> 4).min(to.x >> 4),
            max_x: (from.x >> 4).max(to.x >> 4),
            min_z: (from.y >> 4).min(to.y >> 4),
            max_z: (from.y >> 4).max(to.y >> 4),
        };
        let width = i64::from(rectangle.max_x) - i64::from(rectangle.min_x) + 1;
        let depth = i64::from(rectangle.max_z) - i64::from(rectangle.min_z) + 1;
        let count = width * depth;
        if count > 256 {
            return Err(ERROR_TOO_MANY.create_without_context(
                TextComponent::text("256"),
                TextComponent::text(count.to_string()),
            ));
        }
        Ok(rectangle)
    }

    fn positions(&self) -> Vec<Vector2<i32>> {
        (self.min_x..=self.max_x)
            .flat_map(|x| (self.min_z..=self.max_z).map(move |z| Vector2::new(x, z)))
            .collect()
    }
}

/// Complete each newly admitted chunk before admitting the next, as ServerLevel.setChunkForced does.
async fn add_chunks_in_order<F, Fut>(
    positions: Vec<Vector2<i32>>,
    mut add_and_load: F,
) -> Result<Vec<Vector2<i32>>, CommandSyntaxError>
where
    F: FnMut(Vector2<i32>) -> Fut,
    Fut: Future<Output = Result<bool, CommandSyntaxError>>,
{
    let mut changed = Vec::new();
    for pos in positions {
        if add_and_load(pos).await? {
            changed.push(pos);
        }
    }
    Ok(changed)
}

/// The command API is synchronous; release a Tokio worker while the server's runtime drives loading.
fn wait_for_loading<F, T>(runtime: &Handle, future: F) -> Result<T, CommandSyntaxError>
where
    F: Future<Output = Result<T, CommandSyntaxError>> + Send,
    T: Send,
{
    if runtime.runtime_flavor() != RuntimeFlavor::MultiThread {
        return Err(ERROR_LOADING.create_without_context());
    }
    match Handle::try_current() {
        Ok(current) if current.runtime_flavor() == RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| runtime.block_on(future))
        }
        // A current-thread caller cannot use block_in_place. The separate server runtime
        // remains live while this helper thread waits; no temporary runtime is created.
        Ok(_) => std::thread::scope(|scope| {
            scope
                .spawn(|| runtime.block_on(future))
                .join()
                .unwrap_or_else(|_| Err(ERROR_LOADING.create_without_context()))
        }),
        Err(_) => runtime.block_on(future),
    }
}

fn changed_count(changed: &[Vector2<i32>], add: bool) -> Result<i32, CommandSyntaxError> {
    if changed.is_empty() {
        return Err(if add {
            &ERROR_FAILED_ADD
        } else {
            &ERROR_FAILED_REMOVE
        }
        .create_without_context());
    }
    Ok(changed.len() as i32)
}

#[expect(
    clippy::too_many_lines,
    reason = "Keep force-load bounds, ticket changes and command feedback together."
)]
fn change_force_load(context: &CommandContext, add: bool) -> CommandExecutorResult {
    let from = ColumnPosArgumentType::get_column_pos(context, "from")?;
    let to = if context.get_argument::<Coordinates>("to").is_ok() {
        ColumnPosArgumentType::get_column_pos(context, "to")?
    } else {
        from
    };
    let rectangle = ChunkRectangle::new(from.0, to.0)?;
    let failure = if add {
        &ERROR_FAILED_ADD
    } else {
        &ERROR_FAILED_REMOVE
    };
    let world = context
        .source
        .world
        .as_ref()
        .ok_or_else(|| failure.create_without_context())?;
    let changed = if add {
        let server = world
            .server
            .upgrade()
            .ok_or_else(|| ERROR_LOADING.create_without_context())?;
        wait_for_loading(
            &server.runtime,
            add_chunks_in_order(rectangle.positions(), |pos| {
                let world = world.clone();
                async move {
                    if crate::STOP_INTERRUPT.is_cancelled() {
                        return Err(ERROR_LOADING.create_without_context());
                    }
                    let added = world
                        .forced_chunks
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .insert(pos);
                    if !added {
                        return Ok(false);
                    }
                    world.update_active_chunks();
                    world.refresh_forced_chunk_tickets();
                    tokio::select! {
                        biased;
                        () = crate::STOP_INTERRUPT.cancelled() => Err(ERROR_LOADING.create_without_context()),
                        result = world.level.get_or_fetch_chunk(pos, |_| ()) => {
                            result.map(|()| true).map_err(|error| {
                                tracing::error!("Cannot force-load chunk: {error}");
                                ERROR_STORAGE.create_without_context()
                            })
                        },
                    }
                }
            }),
        )?
    } else {
        let changed = {
            let mut forced = world
                .forced_chunks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            rectangle
                .positions()
                .into_iter()
                .filter(|pos| forced.remove(pos))
                .collect::<Vec<_>>()
        };
        if !changed.is_empty() {
            world.update_active_chunks();
            world.refresh_forced_chunk_tickets();
        }
        changed
    };
    let count = changed_count(&changed, add)?;
    let dimension = TextComponent::text(world.dimension.minecraft_name.to_string());
    let (translation_key, arguments) = if changed.len() == 1 {
        let pos = changed[0];
        (
            if add {
                translation::java::COMMANDS_FORCELOAD_ADDED_SINGLE
            } else {
                translation::java::COMMANDS_FORCELOAD_REMOVED_SINGLE
            },
            vec![
                TextComponent::text(format!("[{}, {}]", pos.x, pos.y)),
                dimension,
            ],
        )
    } else {
        (
            if add {
                translation::java::COMMANDS_FORCELOAD_ADDED_MULTIPLE
            } else {
                translation::java::COMMANDS_FORCELOAD_REMOVED_MULTIPLE
            },
            vec![
                TextComponent::text(changed.len().to_string()),
                dimension,
                TextComponent::text(format!("[{}, {}]", rectangle.min_x, rectangle.min_z)),
                TextComponent::text(format!("[{}, {}]", rectangle.max_x, rectangle.max_z)),
            ],
        )
    };
    context.source.send_feedback(
        TextComponent::translate_cross(translation_key, translation_key, arguments),
        true,
    );
    Ok(count)
}

struct ForceloadAddExecutor;

impl CommandExecutor for ForceloadAddExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        change_force_load(context, true)
    }
}

struct ForceloadRemoveExecutor;

impl CommandExecutor for ForceloadRemoveExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        change_force_load(context, false)
    }
}

struct ForceloadRemoveAllExecutor;

impl CommandExecutor for ForceloadRemoveAllExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let world = context
            .source
            .world
            .as_ref()
            .ok_or_else(|| ERROR_FAILED_REMOVE.create_without_context())?;

        {
            let mut forced = world
                .forced_chunks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            forced.clear();
        };

        world.update_active_chunks();
        world.refresh_forced_chunk_tickets();

        let dimension_name = world.dimension.minecraft_name.to_string();

        let text = TextComponent::translate_cross(
            translation::java::COMMANDS_FORCELOAD_REMOVED_ALL,
            translation::java::COMMANDS_FORCELOAD_REMOVED_ALL,
            [TextComponent::text(dimension_name)],
        );
        context.source.send_feedback(text, true);

        // Vanilla removeAll reports success even for an empty set and returns zero.
        Ok(0)
    }
}

static ERROR_NOT_FORCED: CommandErrorType<2> = CommandErrorType::new(
    translation::java::COMMANDS_FORCELOAD_QUERY_FAILURE,
    "Chunk is not force loaded",
);

struct ForceloadQueryExecutor;

impl CommandExecutor for ForceloadQueryExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let world = context
            .source
            .world
            .as_ref()
            .ok_or_else(|| ERROR_FAILED_QUERY.create_without_context())?;
        let pos = ColumnPosArgumentType::get_column_pos(context, "pos")?;
        let chunk_pos = Vector2::new(pos.0.x >> 4, pos.0.y >> 4);
        let is_forced = world
            .forced_chunks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&chunk_pos);
        let position = TextComponent::text(format!("[{}, {}]", chunk_pos.x, chunk_pos.y));
        let dimension = TextComponent::text(world.dimension.minecraft_name.to_string());
        if !is_forced {
            return Err(ERROR_NOT_FORCED.create_without_context(position, dimension));
        }
        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_FORCELOAD_QUERY_SUCCESS,
                translation::java::COMMANDS_FORCELOAD_QUERY_SUCCESS,
                [position, dimension],
            ),
            false,
        );
        Ok(1)
    }
}

struct ForceloadListExecutor;

impl CommandExecutor for ForceloadListExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let world = context
            .source
            .world
            .as_ref()
            .ok_or_else(|| ERROR_FAILED_QUERY.create_without_context())?;
        let mut forced = world
            .forced_chunks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .copied()
            .collect::<Vec<_>>();
        // Vanilla sorts the signed packed ChunkPos longs, rather than X/Z pairs.
        forced.sort_unstable_by_key(|pos| (i64::from(pos.y) << 32) | i64::from(pos.x as u32));
        let count = forced.len() as i32;
        let dimension = TextComponent::text(world.dimension.minecraft_name.to_string());
        if forced.is_empty() {
            context.source.send_error(TextComponent::translate_cross(
                translation::java::COMMANDS_FORCELOAD_ADDED_NONE,
                translation::java::COMMANDS_FORCELOAD_ADDED_NONE,
                [dimension],
            ));
        } else {
            let list = TextComponent::text(
                forced
                    .iter()
                    .map(|pos| format!("[{}, {}]", pos.x, pos.y))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            let (key, arguments) = if count == 1 {
                (
                    translation::java::COMMANDS_FORCELOAD_LIST_SINGLE,
                    vec![dimension, list],
                )
            } else {
                (
                    translation::java::COMMANDS_FORCELOAD_LIST_MULTIPLE,
                    vec![TextComponent::text(count.to_string()), dimension, list],
                )
            };
            context
                .source
                .send_feedback(TextComponent::translate_cross(key, key, arguments), false);
        }
        // An empty list sends failure text but still completes with result zero.
        Ok(count)
    }
}

pub fn register(dispatcher: &mut CommandDispatcher, registry: &PermissionRegistry) {
    registry.register_permission_or_panic(Permission::new(
        PERMISSION,
        DESCRIPTION,
        PermissionDefault::Op(PermissionLvl::Two),
    ));

    let builder = command("forceload", DESCRIPTION)
        .requires(PERMISSION)
        .then(
            literal("add").then(
                argument("from", ColumnPosArgumentType)
                    .executes(ForceloadAddExecutor)
                    .then(argument("to", ColumnPosArgumentType).executes(ForceloadAddExecutor)),
            ),
        )
        .then(
            literal("remove")
                .then(literal("all").executes(ForceloadRemoveAllExecutor))
                .then(
                    argument("from", ColumnPosArgumentType)
                        .executes(ForceloadRemoveExecutor)
                        .then(
                            argument("to", ColumnPosArgumentType).executes(ForceloadRemoveExecutor),
                        ),
                ),
        )
        .then(
            literal("query")
                .executes(ForceloadListExecutor)
                .then(argument("pos", ColumnPosArgumentType).executes(ForceloadQueryExecutor)),
        );

    dispatcher.register(builder);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn rectangles_use_inclusive_chunks_and_x_then_z_order() {
        let rectangle = ChunkRectangle::new(Vector2::new(31, 15), Vector2::new(-16, -1)).unwrap();
        assert_eq!(
            rectangle.positions(),
            vec![
                Vector2::new(-1, -1),
                Vector2::new(-1, 0),
                Vector2::new(0, -1),
                Vector2::new(0, 0),
                Vector2::new(1, -1),
                Vector2::new(1, 0),
            ]
        );
        let maximum = ChunkRectangle::new(Vector2::new(0, 0), Vector2::new(255, 255)).unwrap();
        assert_eq!(maximum.positions().len(), 256);
        assert!(ChunkRectangle::new(Vector2::new(0, 0), Vector2::new(4096, 0)).is_err());
    }

    #[test]
    fn rectangles_reject_world_bounds_and_large_areas_before_iteration() {
        for coordinate in [i32::MIN, -30_000_001, 30_000_000, i32::MAX] {
            for pos in [Vector2::new(coordinate, 0), Vector2::new(0, coordinate)] {
                let error = ChunkRectangle::new(pos, Vector2::new(0, 0)).unwrap_err();
                assert_eq!(
                    serde_json::to_value(error.message).unwrap()["translate"],
                    translation::java::ARGUMENT_POS_OUTOFWORLD
                );
            }
        }
        for coordinate in [-30_000_000, 29_999_999] {
            let point = Vector2::new(coordinate, coordinate);
            assert_eq!(
                ChunkRectangle::new(point, point).unwrap().positions().len(),
                1
            );
        }
        let error = ChunkRectangle::new(
            Vector2::new(-30_000_000, -30_000_000),
            Vector2::new(29_999_999, 29_999_999),
        )
        .unwrap_err();
        assert_eq!(
            serde_json::to_value(error.message).unwrap()["translate"],
            translation::java::COMMANDS_FORCELOAD_TOOBIG
        );
    }

    #[test]
    fn additions_finish_loading_before_the_next_admission_and_count_only_changes() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let changed = wait_for_loading(
            runtime.handle(),
            add_chunks_in_order(
                vec![Vector2::new(-1, 0), Vector2::new(0, 0), Vector2::new(1, 0)],
                |pos| {
                    let events = events.clone();
                    async move {
                        events.lock().unwrap().push(("admitted", pos));
                        if pos == Vector2::new(0, 0) {
                            return Ok(false);
                        }
                        tokio::task::yield_now().await;
                        events.lock().unwrap().push(("full", pos));
                        Ok(true)
                    }
                },
            ),
        )
        .unwrap();
        assert_eq!(changed, vec![Vector2::new(-1, 0), Vector2::new(1, 0)]);
        assert_eq!(changed_count(&changed, true).unwrap(), 2);
        assert_eq!(changed_count(&changed, false).unwrap(), 2);
        assert_eq!(
            *events.lock().unwrap(),
            vec![
                ("admitted", Vector2::new(-1, 0)),
                ("full", Vector2::new(-1, 0)),
                ("admitted", Vector2::new(0, 0)),
                ("admitted", Vector2::new(1, 0)),
                ("full", Vector2::new(1, 0)),
            ]
        );
        for (add, key) in [
            (true, translation::java::COMMANDS_FORCELOAD_ADDED_FAILURE),
            (false, translation::java::COMMANDS_FORCELOAD_REMOVED_FAILURE),
        ] {
            let error = changed_count(&[], add).unwrap_err();
            assert_eq!(
                serde_json::to_value(error.message).unwrap()["translate"],
                key
            );
        }
    }

    #[test]
    fn synchronous_bridge_supports_worker_and_current_thread_callers() {
        let server_runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        for current_thread in [false, true] {
            let caller = if current_thread {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
            } else {
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .unwrap()
            };
            caller.block_on(async {
                let (send, receive) = tokio::sync::oneshot::channel();
                server_runtime.spawn(async move {
                    tokio::task::yield_now().await;
                    send.send(37).unwrap();
                });
                let value = wait_for_loading(server_runtime.handle(), async {
                    receive
                        .await
                        .map_err(|_| ERROR_LOADING.create_without_context())
                })
                .unwrap();
                assert_eq!(value, 37);
            });
        }
        let unsupported = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let polled = std::sync::atomic::AtomicBool::new(false);
        let result = wait_for_loading(unsupported.handle(), async {
            polled.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        });
        assert!(result.is_err());
        assert!(!polled.load(std::sync::atomic::Ordering::Relaxed));
    }
}
