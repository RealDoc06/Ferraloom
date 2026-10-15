use super::*;
use crate::block::entities::chest::ChestBlockEntity;
use crate::command::CommandSource;
use crate::command::argument_types::nbt_path::{
    ERROR_EXPECTED_LIST, ERROR_INVALID_INDEX, ERROR_NOTHING_FOUND,
};
use crate::command::errors::error_types::AnyCommandErrorType;
use crate::data::VanillaData;
use crate::entity::r#type::from_type;
use pumpkin_command::{snbt::SnbtParser, string_reader::StringReader};
use pumpkin_config::{AdvancedConfiguration, BasicConfiguration, TelemetryConfig};
use pumpkin_data::{Block, entity::EntityType, item::Item, item_stack::ItemStack};
use pumpkin_inventory::Inventory;
use pumpkin_util::{math::vector2::Vector2, math::vector3::Vector3, world_seed::Seed};
use pumpkin_world::{
    chunk::ChunkData,
    world_info::{LevelData, WorldInfoWriter, anvil::AnvilLevelInfo},
};

const BLOCK_POS: BlockPos = BlockPos::new(0, 64, 0);
const SOURCES: [&str; 3] = ["block 0 64 0", "entity @s", "storage repair:source"];

fn compound(snbt: &str) -> NbtCompound {
    let NbtTag::Compound(value) =
        SnbtParser::parse_for_commands(&mut StringReader::new(snbt)).unwrap()
    else {
        panic!("expected a compound fixture");
    };
    value
}

struct Fixture {
    source: CommandSource,
    dispatcher: CommandDispatcher,
    _folder: tempfile::TempDir,
}

impl Fixture {
    /// No listeners, ticks, external authentication, or cwd configuration loads.
    /// Stop chunk workers before manually loading the command's Full chunk.
    async fn new() -> Self {
        let folder = tempfile::tempdir().unwrap();
        AnvilLevelInfo
            .write_world_info(&LevelData::default(Seed(0)), folder.path())
            .unwrap();
        let basic = BasicConfiguration {
            default_level_name: folder.path().to_string_lossy().into_owned(),
            seed: Seed(0),
            allow_nether: false,
            allow_end: false,
            allow_chat_reports: false,
            use_favicon: false,
            ..BasicConfiguration::default()
        };
        let mut advanced = AdvancedConfiguration::default();
        advanced.networking.java.online_mode = false;
        advanced.networking.bedrock.online_mode = false;
        let data = VanillaData {
            banned_ip_list: Default::default(),
            banned_player_list: Default::default(),
            operator_config: Default::default(),
            user_cache: Default::default(),
            whitelist_config: Default::default(),
        };
        let server = Server::new(
            basic,
            advanced,
            TelemetryConfig::default(),
            data,
            Vec::new(),
        )
        .await
        .unwrap();
        server.finish_tasks().await;
        let world = server.worlds.load()[0].clone();
        world.level.shutdown().await.unwrap();
        world
            .level
            .loaded_chunks
            .insert(Vector2::new(0, 0), ChunkData::empty_sync(0, 0));
        world
            .level
            .read_chunk_sync(&BLOCK_POS.chunk_position(), |chunk| {
                chunk.set_block_absolute_y(0, 64, 0, Block::CHEST.default_state.id);
            })
            .unwrap();
        let chest = Arc::new(ChestBlockEntity::new(BLOCK_POS));
        chest.set_stack(0, ItemStack::new(3, &Item::DIAMOND));
        world.add_block_entity(chest);
        let entity = from_type(
            &EntityType::PIG,
            Vector3::new(2.0, 64.0, 0.0),
            &world,
            uuid::Uuid::from_u128(1),
        );
        entity.read_nbt_non_mut(&compound("{Tags:[original]}"));
        let source = CommandSource {
            server: Some(server),
            ..CommandSource::dummy().with_world(world).with_entity(entity)
        };
        let mut dispatcher = CommandDispatcher::new();
        register(&mut dispatcher, &PermissionRegistry::default());
        crate::command::commands::execute::register(
            &mut dispatcher,
            &PermissionRegistry::default(),
        );
        let fixture = Self {
            source,
            dispatcher,
            _folder: folder,
        };
        fixture.put_storage(
            "repair:source",
            compound("{id:'minecraft:storage',numbers:[3,4],number:42,text:abcdef,object:{key:7}}"),
        );
        fixture
    }

    fn execute(&self, input: &str) -> CommandExecutorResult {
        self.dispatcher.execute_input(input, &self.source)
    }

    fn put_storage(&self, id: &str, data: NbtCompound) {
        StorageDataAccessor::new(id.into(), self.source.server().clone())
            .set_data(&data)
            .unwrap();
    }

    fn storage(&self, id: &str) -> NbtCompound {
        StorageDataAccessor::new(id.into(), self.source.server().clone())
            .get_data()
            .unwrap()
    }

    fn sources(&self) -> [NbtCompound; 3] {
        [
            BlockDataAccessor::new(BLOCK_POS, self.source.world().clone())
                .unwrap()
                .get_data()
                .unwrap(),
            EntityDataAccessor::new(self.source.entity.clone().unwrap())
                .get_data()
                .unwrap(),
            self.storage("repair:source"),
        ]
    }
}

#[tokio::test]
async fn modify_from_retains_every_provider_and_copies_without_mutating_sources() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    let mut expected = NbtCompound::new();
    for (index, provider) in SOURCES.iter().enumerate() {
        let whole = format!("whole{index}");
        let id = format!("id{index}");
        assert_eq!(
            fixture
                .execute(&format!(
                    "data modify storage repair:target {whole} set from {provider}"
                ))
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .execute(&format!(
                    "data modify storage repair:target {id} set from {provider} id"
                ))
                .unwrap(),
            1
        );
        expected.put_compound(&whole, before[index].clone());
        expected.put_string(&id, before[index].get_string("id").unwrap().to_owned());
        assert_eq!(fixture.storage("repair:target"), expected);
    }

    // One list changes even when a source path selects multiple values; set uses
    // the final selected value. Neither operation consumes the source values.
    assert_eq!(
        fixture
            .execute("data modify storage repair:target copied append from storage repair:source numbers[]")
            .unwrap(),
        1
    );
    assert_eq!(
        fixture
            .execute(
                "data modify storage repair:target last set from storage repair:source numbers[]"
            )
            .unwrap(),
        1
    );
    expected.put("copied", NbtTag::List(vec![NbtTag::Int(3), NbtTag::Int(4)]));
    expected.put_int("last", 4);
    assert_eq!(fixture.storage("repair:target"), expected);
    assert_eq!(fixture.sources(), before);
}

#[tokio::test]
async fn modify_insert_requires_index_before_value_from_and_string_sources() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    fixture.put_storage("repair:target", compound("{items:[left,right]}"));
    assert_eq!(
        fixture
            .execute("data modify storage repair:target items insert 1 value literal")
            .unwrap(),
        1
    );
    let mut expected = vec!["left", "literal", "right"];
    for (index, provider) in SOURCES.iter().enumerate() {
        assert_eq!(
            fixture
                .execute(&format!(
                    "data modify storage repair:target items insert 1 from {provider} id"
                ))
                .unwrap(),
            1
        );
        expected.insert(1, before[index].get_string("id").unwrap());
        assert_eq!(
            fixture
                .execute(&format!(
                    "data modify storage repair:target items insert -1 string {provider} id 10"
                ))
                .unwrap(),
            1
        );
        expected.push(&before[index].get_string("id").unwrap()[10..]);
        let expected_tags: Vec<_> = expected
            .iter()
            .map(|value| NbtTag::String((*value).into()))
            .collect();
        assert_eq!(
            fixture.storage("repair:target").get_list("items"),
            Some(expected_tags.as_slice())
        );
    }

    let target = fixture.storage("repair:target");
    for suffix in [
        "insert value ignored",
        "insert from storage repair:source id",
        "insert string storage repair:source id",
        "insert 1",
        "insert value ignored 1",
    ] {
        assert!(
            fixture
                .execute(&format!("data modify storage repair:target items {suffix}"))
                .is_err(),
            "{suffix}"
        );
        assert_eq!(fixture.storage("repair:target"), target);
    }
    assert_eq!(fixture.sources(), before);
}

#[tokio::test]
async fn modify_string_keeps_provider_siblings_and_optional_bounds() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    let mut expected = NbtCompound::new();
    for (index, provider) in SOURCES.iter().enumerate() {
        let id = before[index].get_string("id").unwrap();
        for (name, bounds, text) in [
            ("full", "", id),
            ("tail", " 10", &id[10..]),
            ("middle", " 10 -1", &id[10..id.len() - 1]),
        ] {
            let path = format!("{name}{index}");
            assert_eq!(
                fixture
                    .execute(&format!(
                        "data modify storage repair:target {path} set string {provider} id{bounds}"
                    ))
                    .unwrap(),
                1
            );
            expected.put_string(&path, text.to_owned());
            assert_eq!(fixture.storage("repair:target"), expected);
        }
    }
    assert_eq!(
        fixture
            .execute(
                "data modify storage repair:target numeric set string storage repair:source number"
            )
            .unwrap(),
        1
    );
    expected.put_string("numeric", "42".to_owned());
    assert_eq!(
        fixture
            .execute("data modify storage repair:target negative set string storage repair:source text -3")
            .unwrap(),
        1
    );
    expected.put_string("negative", "def".to_owned());
    assert_eq!(fixture.storage("repair:target"), expected);
    assert_eq!(fixture.sources(), before);
}

#[tokio::test]
async fn modify_value_modes_and_entity_target_return_affected_counts() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    fixture.put_storage("repair:target", compound("{items:[2],rows:[{n:0},{n:0}]}"));
    for (suffix, expected_count) in [
        ("items append value 3", 1),
        ("items prepend value 1", 1),
        ("items insert -2 value 9", 1),
        ("items[0] set value 8", 1),
        // A missing compound is created before merging its contents.
        ("created merge value {key:7}", 1),
        ("rows[].n set value 5", 2),
    ] {
        assert_eq!(
            fixture
                .execute(&format!("data modify storage repair:target {suffix}"))
                .unwrap(),
            expected_count,
            "{suffix}"
        );
    }
    assert_eq!(
        fixture.storage("repair:target"),
        compound("{items:[8,2,9,3],rows:[{n:5},{n:5}],created:{key:7}}")
    );
    assert_eq!(fixture.sources(), before);

    assert_eq!(
        fixture
            .execute("data modify entity @s Tags insert 0 value added")
            .unwrap(),
        1
    );
    let mut after = fixture.sources();
    // Entity scoreboard tags are a set, so their serialized order is unspecified.
    let mut tags: Vec<_> = after[1]
        .get_list("Tags")
        .unwrap()
        .iter()
        .map(|tag| tag.extract_string().unwrap())
        .collect();
    tags.sort_unstable();
    assert_eq!(tags, ["added", "original"]);
    after[1].put("Tags", before[1].get("Tags").unwrap().clone());
    assert_eq!(after[1], before[1]);
    assert_eq!(after[0], before[0]);
    assert_eq!(after[2], before[2]);
}

#[tokio::test]
async fn modify_unchanged_and_errors_preserve_nbt_through_dispatcher() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    let target = compound("{id:'minecraft:storage',items:[1,2],scalar:7,object:{key:7}}");
    fixture.put_storage("repair:target", target.clone());

    // Each target provider reaches its executor, including block and entity
    // targets, and rejects a set of its current value without a write.
    for provider in SOURCES {
        let error = fixture
            .execute(&format!("data modify {provider} id set from {provider} id"))
            .unwrap_err();
        assert!(error.is(&ERROR_MERGE_UNCHANGED), "{provider}: {error:?}");
    }
    for provider in SOURCES {
        let error = fixture
            .execute(&format!(
                "data modify storage repair:target scalar set from {provider} missing"
            ))
            .unwrap_err();
        assert!(error.is(&ERROR_NOTHING_FOUND), "{provider}: {error:?}");
        // A whole source compound cannot be converted to a primitive string.
        let error = fixture
            .execute(&format!(
                "data modify storage repair:target scalar set string {provider}"
            ))
            .unwrap_err();
        assert!(error.is(&ERROR_EXPECTED_VALUE), "{provider}: {error:?}");
        assert_eq!(fixture.storage("repair:target"), target);
        assert_eq!(fixture.sources(), before);
    }

    let failures: [(&str, &'static dyn AnyCommandErrorType); 6] = [
        ("scalar set value 7", &ERROR_MERGE_UNCHANGED),
        ("object merge value {key:7}", &ERROR_MERGE_UNCHANGED),
        ("items insert 99 value 3", &ERROR_INVALID_INDEX),
        ("scalar append value 3", &ERROR_EXPECTED_LIST),
        ("object merge value 3", &ERROR_EXPECTED_OBJECT),
        (
            "scalar set string storage repair:source text 4 2",
            &ERROR_INVALID_SUBSTRING,
        ),
    ];
    for (suffix, expected_error) in failures {
        let error = fixture
            .execute(&format!("data modify storage repair:target {suffix}"))
            .unwrap_err();
        assert!(error.is(expected_error), "{suffix}: {error:?}");
        assert_eq!(fixture.storage("repair:target"), target);
        assert_eq!(fixture.sources(), before);
    }
}

#[tokio::test]
async fn modify_merge_recursively_retains_siblings_and_combines_sources_in_order() {
    let fixture = Fixture::new().await;
    fixture.put_storage(
        "repair:source",
        compound(
            "{parts:[{nested:{first:3,replace:4},list:[2],scalar:{inside:7},object:2},\
             {nested:{last:5,replace:6}}]}",
        ),
    );
    let sources = fixture.sources();
    fixture.put_storage(
        "repair:target",
        compound(
            "{untouched:99,object:{sibling:2,nested:{keep:1,replace:0},\
             list:[1],scalar:0,object:{old:1}}}",
        ),
    );

    assert_eq!(
        fixture
            .execute(
                "data modify storage repair:target object merge from storage repair:source parts[]"
            )
            .unwrap(),
        1
    );
    assert_eq!(
        fixture.storage("repair:target"),
        compound(
            "{untouched:99,object:{sibling:2,nested:{keep:1,first:3,last:5,replace:6},\
             list:[2],scalar:{inside:7},object:2}}"
        )
    );
    assert_eq!(fixture.sources(), sources);
}

#[tokio::test]
async fn modify_merge_counts_changed_compounds_and_rejects_unchanged_results() {
    let fixture = Fixture::new().await;
    fixture.put_storage(
        "repair:source",
        compound("{parts:[{nested:{value:1,extra:5}},{nested:{value:2}}]}"),
    );
    let sources = fixture.sources();
    fixture.put_storage(
        "repair:target",
        compound(
            "{rows:[{kind:hit,payload:{keep:1,nested:{value:2,extra:5}}},\
             {kind:hit,payload:{keep:2,nested:{value:0}}},\
             {kind:skip,payload:{keep:3,nested:{value:0}}}]}",
        ),
    );

    // Three compounds match, but the first already equals the final source
    // merge. Intermediate source changes do not increase the result count.
    assert_eq!(
        fixture
            .execute("data modify storage repair:target rows[].payload merge from storage repair:source parts[]")
            .unwrap(),
        2
    );
    assert_eq!(
        fixture.storage("repair:target"),
        compound(
            "{rows:[{kind:hit,payload:{keep:1,nested:{value:2,extra:5}}},\
             {kind:hit,payload:{keep:2,nested:{value:2,extra:5}}},\
             {kind:skip,payload:{keep:3,nested:{value:2,extra:5}}}]}"
        )
    );
    assert_eq!(
        fixture
            .execute(
                "data modify storage repair:target rows[{kind:hit}].payload merge value {added:8}"
            )
            .unwrap(),
        2
    );
    let expected = compound(
        "{rows:[{kind:hit,payload:{keep:1,nested:{value:2,extra:5},added:8}},\
         {kind:hit,payload:{keep:2,nested:{value:2,extra:5},added:8}},\
         {kind:skip,payload:{keep:3,nested:{value:2,extra:5}}}]}",
    );
    assert_eq!(fixture.storage("repair:target"), expected);
    for suffix in [
        "rows[].payload merge from storage repair:source parts[]",
        "rows[{kind:hit}].payload merge value {added:8}",
        "rows[-1].payload merge value {}",
        "rows[0].payload{keep:99} merge value {added:8}",
    ] {
        let error = fixture
            .execute(&format!("data modify storage repair:target {suffix}"))
            .unwrap_err();
        assert!(error.is(&ERROR_MERGE_UNCHANGED), "{suffix}: {error:?}");
        assert_eq!(fixture.storage("repair:target"), expected);
    }
    assert_eq!(fixture.sources(), sources);
}

#[tokio::test]
async fn modify_merge_type_errors_validate_sources_before_changing_targets() {
    let fixture = Fixture::new().await;
    fixture.put_storage(
        "repair:source",
        compound("{mixed:[{value:{added:2}},{value:7}],number:42}"),
    );
    let sources = fixture.sources();
    let target = compound("{object:{keep:1},scalar:7,numbers:[I;3,4]}");
    fixture.put_storage("repair:target", target.clone());

    for (suffix, offending_tag) in [
        ("object merge from storage repair:source mixed[].value", "7"),
        (
            "missing.path merge from storage repair:source mixed[].value",
            "7",
        ),
        ("scalar merge value {added:2}", "7"),
        ("numbers[] merge value {added:2}", "3"),
        ("numbers[-1] merge value {added:2}", "4"),
        ("object merge string storage repair:source number", "\"42\""),
    ] {
        let error = fixture
            .execute(&format!("data modify storage repair:target {suffix}"))
            .unwrap_err();
        assert_eq!(
            error,
            ERROR_EXPECTED_OBJECT
                .create_without_context(TextComponent::text(offending_tag.to_owned())),
            "{suffix}"
        );
        assert_eq!(fixture.storage("repair:target"), target);
        assert_eq!(fixture.sources(), sources);
    }
    let error = fixture
        .execute("data modify storage repair:target numbers[].child merge value {added:2}")
        .unwrap_err();
    assert_eq!(
        error,
        ERROR_NOTHING_FOUND.create_without_context(TextComponent::text("numbers[].child"))
    );
    for (provider, target_path, offending_tag) in [
        ("entity @s", "Tags[]", "\"original\""),
        ("block 0 64 0", "id", "\"minecraft:chest\""),
    ] {
        let error = fixture
            .execute(&format!(
                "data modify {provider} {target_path} merge value {{added:2}}"
            ))
            .unwrap_err();
        assert_eq!(
            error,
            ERROR_EXPECTED_OBJECT
                .create_without_context(TextComponent::text(offending_tag.to_owned())),
            "{provider}"
        );
    }
    assert_eq!(fixture.storage("repair:target"), target);
    assert_eq!(fixture.sources(), sources);
}

#[tokio::test]
async fn modify_merge_preserves_vanilla_storage_side_effects_on_failure() {
    let fixture = Fixture::new().await;
    let sources = fixture.sources();
    fixture.put_storage(
        "repair:target",
        compound("{rows:[{value:{keep:1}},{value:7},{}],keep:9}"),
    );

    let error = fixture
        .execute("data modify storage repair:target rows[].value merge value {added:2}")
        .unwrap_err();
    assert_eq!(
        error,
        ERROR_EXPECTED_OBJECT.create_without_context(TextComponent::text("7"))
    );
    // Official storage returns live tags: all target paths are created first,
    // then the first compound is merged before the scalar aborts the command.
    assert_eq!(
        fixture.storage("repair:target"),
        compound("{rows:[{value:{keep:1,added:2}},{value:7},{value:{}}],keep:9}")
    );

    fixture.put_storage("repair:target", compound("{keep:9,rows:[]}"));
    for (suffix, expected) in [
        (
            "created.nested merge value {}",
            "{keep:9,rows:[],created:{nested:{}}}",
        ),
        (
            "rows[{kind:new}] merge value {kind:new}",
            "{keep:9,rows:[{kind:new}],created:{nested:{}}}",
        ),
    ] {
        let error = fixture
            .execute(&format!("data modify storage repair:target {suffix}"))
            .unwrap_err();
        assert!(error.is(&ERROR_MERGE_UNCHANGED), "{suffix}: {error:?}");
        assert_eq!(fixture.storage("repair:target"), compound(expected));
    }
    assert_eq!(
        fixture
            .execute("data modify storage repair:target rows[{kind:other}] merge value {added:2}")
            .unwrap(),
        1
    );
    assert_eq!(
        fixture.storage("repair:target"),
        compound("{keep:9,rows:[{kind:new},{kind:other,added:2}],created:{nested:{}}}")
    );

    // Empty roots supplied for absent storage are not attached until success.
    let error = fixture
        .execute("data modify storage repair:absent created.nested merge value {}")
        .unwrap_err();
    assert!(error.is(&ERROR_MERGE_UNCHANGED));
    assert_eq!(fixture.storage("repair:absent"), NbtCompound::new());
    assert_eq!(fixture.sources(), sources);
}

#[tokio::test]
async fn removing_last_storage_tag_removes_the_live_key() {
    let fixture = Fixture::new().await;
    fixture
        .execute("data modify storage repair:removed value set value 9")
        .unwrap();
    assert_eq!(
        fixture
            .execute("data remove storage repair:removed value")
            .unwrap(),
        1
    );
    assert!(fixture.storage("repair:removed").is_empty());
    assert!(
        !fixture
            .source
            .server()
            .command_storage
            .lock()
            .unwrap()
            .contains_key("repair:removed")
    );
}

#[tokio::test]
async fn execute_data_conditions_treat_absent_paths_as_zero_matches_for_every_provider() {
    let fixture = Fixture::new().await;
    let before = fixture.sources();
    for provider in SOURCES {
        for (condition, path, expected) in [
            ("if", "id", 42),
            ("unless", "id", 0),
            ("if", "missing.nested", 0),
            ("unless", "missing.nested", 42),
        ] {
            let command = format!(
                "execute {condition} data {provider} {path} run data get storage repair:source number"
            );
            assert_eq!(fixture.execute(&command).unwrap(), expected, "{command}");
        }
    }
    assert_eq!(fixture.sources(), before);
}
