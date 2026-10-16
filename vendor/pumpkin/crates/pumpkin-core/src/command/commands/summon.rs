use pumpkin_data::translation;
use pumpkin_util::PermissionLvl;
use pumpkin_util::difficulty::Difficulty;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::permission::{Permission, PermissionDefault, PermissionRegistry};
use pumpkin_util::text::TextComponent;
use uuid::Uuid;

use crate::command::argument_builder::{ArgumentBuilder, argument, command};
use crate::command::argument_types::coordinates::vec3::Vec3ArgumentType;
use crate::command::argument_types::nbt::NbtCompoundArgumentType;
use crate::command::argument_types::resource::{ENTITY_TYPE_ARGUMENT, ResourceArgument};
use crate::command::context::command_context::CommandContext;
use crate::command::errors::error_types::CommandErrorType;
use crate::command::node::dispatcher::CommandDispatcher;
use crate::command::node::{CommandExecutor, CommandExecutorResult};
use crate::entity::mob::spawn::finalize_spawn;
use crate::entity::r#type::from_type;
use crate::world::World;

const DESCRIPTION: &str = "Spawns an entity at position.";
const PERMISSION: &str = "minecraft:command.summon";

const ERROR_FAILED_PEACEFUL: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_SUMMON_FAILED_PEACEFUL,
    translation::bedrock::COMMANDS_SUMMON_FAILED,
);
const ERROR_DUPLICATE_UUID: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_SUMMON_FAILED_UUID,
    translation::bedrock::COMMANDS_SUMMON_FAILED,
);
const INVALID_POSITION: CommandErrorType<0> = CommandErrorType::new(
    translation::java::COMMANDS_SUMMON_INVALIDPOSITION,
    translation::bedrock::COMMANDS_SUMMON_OUTOFWORLD,
);

struct SummonExecutor {
    has_pos: bool,
    has_nbt: bool,
}

impl CommandExecutor for SummonExecutor {
    fn execute(&self, context: &CommandContext) -> CommandExecutorResult {
        let entity_type = ResourceArgument::get_summonable_entity_type(context, "entity")?;
        let pos = if self.has_pos {
            Vec3ArgumentType::get_coordinates(context, "pos")?.resolve(&context.source)
        } else {
            context.source.position
        };

        let block_pos = BlockPos::floored_v(pos);
        if !World::is_valid(block_pos) {
            return Err(INVALID_POSITION.create_without_context());
        }

        let world = context.source.world();
        let difficulty = world.level_info.load().difficulty;
        if difficulty == Difficulty::Peaceful && !entity_type.category.is_friendly {
            return Err(ERROR_FAILED_PEACEFUL.create_without_context());
        }

        let nbt = if self.has_nbt {
            Some(NbtCompoundArgumentType::get(context, "nbt")?)
        } else {
            None
        };

        let uuid = nbt
            .and_then(|nbt| nbt.get_uuid("UUID"))
            .unwrap_or_else(Uuid::new_v4);

        let entity = from_type(entity_type, pos, world, uuid);
        if let Some(nbt) = nbt {
            super::data::validate_entity_items(entity.as_ref(), nbt)?;
            entity.read_nbt_non_mut(nbt);
            entity.get_entity().set_pos(pos);
        }

        if world
            .entities
            .load()
            .iter()
            .any(|e| e.get_entity().entity_uuid == entity.get_entity().entity_uuid)
        {
            return Err(ERROR_DUPLICATE_UUID.create_without_context());
        }

        let name = entity.get_display_name();
        // Vanilla only finalizes the spawn when no NBT argument was given.
        if !self.has_nbt {
            finalize_spawn(&entity, world, None);
        }
        world.spawn_entity(entity);

        context.source.send_feedback(
            TextComponent::translate_cross(
                translation::java::COMMANDS_SUMMON_SUCCESS,
                translation::bedrock::COMMANDS_SUMMON_SUCCESS,
                [name],
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
        command("summon", DESCRIPTION).requires(PERMISSION).then(
            argument("entity", ENTITY_TYPE_ARGUMENT.clone())
                .executes(SummonExecutor {
                    has_pos: false,
                    has_nbt: false,
                })
                .then(
                    argument("pos", Vec3ArgumentType::Default)
                        .executes(SummonExecutor {
                            has_pos: true,
                            has_nbt: false,
                        })
                        .then(
                            argument("nbt", NbtCompoundArgumentType).executes(SummonExecutor {
                                has_pos: true,
                                has_nbt: true,
                            }),
                        ),
                ),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::commands::fetchprofile;
    use crate::command::context::command_source::CommandSource;
    use std::sync::Arc;

    #[test]
    fn parse_summon_cat() {
        let mut dispatcher = CommandDispatcher::new();
        let registry = PermissionRegistry::default();

        // Register fetchprofile first to ensure its "entity" argument doesn't collide
        fetchprofile::register(&mut dispatcher, &registry);
        register(&mut dispatcher, &registry);

        let source = Arc::new(CommandSource::dummy());
        let result = dispatcher.parse_input("summon cat", &source);
        assert!(result.errors.is_empty());
        let context = result.context.build("summon cat");
        let entity_type = ResourceArgument::get_summonable_entity_type(&context, "entity")
            .expect("Should resolve summonable cat entity type");
        assert_eq!(entity_type.resource_name, "cat");
    }

    #[test]
    fn parse_summon_cat_with_pos_and_nbt() {
        let mut dispatcher = CommandDispatcher::new();
        let registry = PermissionRegistry::default();

        fetchprofile::register(&mut dispatcher, &registry);
        register(&mut dispatcher, &registry);

        let source = Arc::new(CommandSource::dummy());
        let result = dispatcher.parse_input("summon cat ~ ~ ~ {NoAI:1b}", &source);
        assert!(result.errors.is_empty());
        let context = result.context.build("summon cat ~ ~ ~ {NoAI:1b}");
        let entity_type = ResourceArgument::get_summonable_entity_type(&context, "entity")
            .expect("Should resolve summonable cat entity type");
        assert_eq!(entity_type.resource_name, "cat");
        let nbt =
            NbtCompoundArgumentType::get(&context, "nbt").expect("Should resolve NBT argument");
        assert_eq!(nbt.get_byte("NoAI"), Some(1));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn summon_item_preflight_refuses_before_spawn_and_healthy_entity_saves() {
        use crate::{
            plugin::{
                EventHandler, EventPriority, api::events::entity::entity_spawn::EntitySpawnEvent,
            },
            server::Server,
        };
        use futures::future::BoxFuture;
        use pumpkin_data::{
            data_component_impl::{BundleContentsImpl, CustomNameImpl},
            item_stack::ItemStack,
        };
        use pumpkin_nbt::compound::NbtCompound;
        use pumpkin_world::chunk::{
            ChunkEntityData,
            format::anvil::AnvilChunkFile,
            io::{FileIO, LoadedData, file_manager::ChunkFileManager},
        };
        use std::{
            sync::atomic::{AtomicUsize, Ordering},
            time::Duration,
        };

        struct SpawnWitness(AtomicUsize);
        impl EventHandler<EntitySpawnEvent> for SpawnWitness {
            fn handle_blocking<'a>(
                &'a self,
                _server: &'a Arc<Server>,
                _event: &'a mut EntitySpawnEvent,
            ) -> BoxFuture<'a, ()> {
                Box::pin(async move {
                    self.0.fetch_add(1, Ordering::Relaxed);
                })
            }
        }

        let mut fixture = crate::command::commands::data::tests::Fixture::new().await;
        register(&mut fixture.dispatcher, &PermissionRegistry::default());
        let world = fixture.source.world().clone();
        assert!(world.entities.load().is_empty());
        let witness = Arc::new(SpawnWitness(AtomicUsize::new(0)));
        fixture
            .source
            .server()
            .plugin_manager
            .register::<EntitySpawnEvent, _>(witness.clone(), EventPriority::Normal, true);
        let mut source_before = NbtCompound::new();
        fixture
            .source
            .entity
            .as_ref()
            .unwrap()
            .write_nbt(&mut source_before);
        let block_before = fixture.block_data();
        let pending_before = fixture.pending_data();
        for command in [
            "summon item 3 64 3 {Item:{id:stick,count:1,components:{tooltip_display:{hide_tooltip:true}}}}",
            "summon pig 3 64 3 {Health:1f,equipment:{mainhand:{id:stick,components:{tooltip_display:{hide_tooltip:true}}}}}",
            "summon item 3 64 3 {Item:{id:bundle,components:{bundle_contents:[{id:stick,components:{custom_name:'A','minecraft:custom_name':'B'}}]}}}",
        ] {
            let error = fixture
                .execute(command)
                .expect_err("unsafe entity item must refuse before spawn");
            assert_eq!(
                error.message.get_text(),
                "This item cannot currently be persisted without data loss.",
                "{command}"
            );
            assert!(world.entities.load().is_empty(), "{command}");
            assert_eq!(witness.0.load(Ordering::Relaxed), 0, "{command}");
            let mut source_after = NbtCompound::new();
            fixture
                .source
                .entity
                .as_ref()
                .unwrap()
                .write_nbt(&mut source_after);
            assert_eq!(source_after, source_before, "{command}");
            assert_eq!(fixture.block_data(), block_before, "{command}");
            assert_eq!(fixture.pending_data(), pending_before, "{command}");
        }

        assert_eq!(fixture.execute("summon item 3 64 3 {Item:{id:bundle,count:1,components:{bundle_contents:[{id:stick,count:2,components:{custom_name:'A','!tooltip_display':{},custom_data:{components:{custom_name:'opaque A','minecraft:custom_name':'opaque B'}}}}]}}}").unwrap(), 1);
        assert_eq!(witness.0.load(Ordering::Relaxed), 1);
        let entities = world.entities.load_full();
        assert_eq!(entities.len(), 1);
        let entity = &entities[0];
        let uuid = entity.get_entity().entity_uuid;
        let position = entity.get_entity().chunk_pos.load();
        let mut snapshot = NbtCompound::new();
        entity.write_nbt(&mut snapshot);
        let expected_item = snapshot.get_compound("Item").unwrap();
        let stack = ItemStack::read_item_stack(expected_item).unwrap();
        let child = &stack
            .get_data_component::<BundleContentsImpl>()
            .unwrap()
            .items[0];
        assert_eq!(child.item_count, 2);
        assert_eq!(
            child
                .get_data_component::<CustomNameImpl>()
                .unwrap()
                .name
                .clone()
                .get_text(),
            "A"
        );

        // The stopped-worker fixture still uses World::shutdown's real entity
        // snapshot and region flush. Read a fresh region cache to witness disk IO.
        let saved = tokio::time::timeout(Duration::from_secs(10), async {
            world.shutdown().await.unwrap();
            let reader = ChunkFileManager::<AnvilChunkFile<ChunkEntityData>>::new(
                pumpkin_config::chunk::AnvilChunkConfig::default(),
            );
            let (send, mut receive) = tokio::sync::mpsc::channel(1);
            reader
                .fetch_chunks(&world.level.level_folder, &[position], send)
                .await;
            let Some(LoadedData::Loaded(saved)) = receive.recv().await else {
                panic!("healthy summon must save a readable entity region");
            };
            saved
        })
        .await
        .expect("summoned entity save and fresh region read must finish");
        let records = saved.data.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].get_uuid("UUID"), Some(uuid));
        assert_eq!(records[0].get_compound("Item"), Some(expected_item));
    }
}
