use crate::command::commands::data::tests::Fixture;
use crate::entity::{EntityBase, NBTStorage, player::Player};
use crate::net::{
    ClientPlatform, GameProfile, PacketRateLimiter, PlayerConfig,
    java::{JavaClient, pending::PendingConnection},
};
use crate::world::World;
use arc_swap::ArcSwap;
use pumpkin_command::{snbt::SnbtParser, string_reader::StringReader};
use pumpkin_data::item_stack::ItemStack;
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::GameMode;
use pumpkin_world::{
    data::player_data::{PlayerDataError, PlayerDataStorage},
    world_info::MAXIMUM_SUPPORTED_WORLD_DATA_VERSION,
};
use std::sync::{Arc, atomic::Ordering};
use uuid::Uuid;

fn compound(snbt: &str) -> NbtCompound {
    let NbtTag::Compound(value) =
        SnbtParser::parse_for_commands(&mut StringReader::new(snbt)).unwrap()
    else {
        panic!("expected compound");
    };
    value
}

// Same stopped-worker fixture as the container tests. Player::new needs a live
// Server, but no packet tasks, listeners or tick loop are started for the player.
async fn player(world: &Arc<World>, uuid: Uuid) -> Player {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (peer, accepted) = tokio::join!(tokio::net::TcpStream::connect(address), listener.accept());
    let _peer = peer.unwrap();
    let (socket, address) = accepted.unwrap();
    let profile = GameProfile {
        id: uuid,
        name: "PreservedPlayer".into(),
        properties: ArcSwap::from_pointee(Vec::new()),
        profile_actions: None,
    };
    let pending = PendingConnection::new(
        socket,
        address,
        0,
        PacketRateLimiter::new(false, 0.0, 0.0),
        world.server.clone(),
    );
    let client = Arc::new(ClientPlatform::Java(JavaClient::from_pending(
        pending,
        profile.clone(),
        PlayerConfig::default(),
    )));
    Player::new(
        client,
        profile,
        PlayerConfig::default(),
        world,
        GameMode::Survival,
    )
}

fn save(storage: &PlayerDataStorage, player: &Player) -> NbtCompound {
    let snapshot = storage.snapshot_player_data(player.gameprofile.id, |nbt| {
        // Exercise the actual trait object used by persistence, not a test overlay.
        EntityBase::write_nbt(player as &dyn EntityBase, nbt);
    });
    storage.save_player_data(snapshot).unwrap();
    storage.load_player_data(&player.gameprofile.id).unwrap().1
}

fn seed(storage: &PlayerDataStorage, uuid: Uuid, data: NbtCompound) {
    let snapshot = storage.snapshot_player_data(uuid, |nbt| *nbt = data);
    storage.save_player_data(snapshot).unwrap();
}

fn assert_refused_unchanged(snbt: &str, field: &str) {
    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path(), true);
    let uuid = Uuid::from_u128(513);
    seed(&storage, uuid, compound("{opaque:42}"));
    seed(&storage, uuid, compound(snbt));
    let path = storage.get_player_data_path(&uuid);
    let backup_path = path.with_extension("dat_old");
    let primary = std::fs::read(&path).unwrap();
    let backup = std::fs::read(&backup_path).unwrap();
    assert!(
        matches!(storage.load_player_data(&uuid), Err(PlayerDataError::Unsupported { field: actual, .. }) if actual == field),
        "{snbt}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), primary);
    assert_eq!(std::fs::read(&backup_path).unwrap(), backup);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn player_dat_preserves_vanilla_remainder_across_snapshots_and_reloads() {
    let fixture = Fixture::new().await;
    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path().join("world/players/data"), true);
    let uuid = Uuid::from_u128(510);
    // These opaque vanilla shapes were read from the retained official 26.3
    // ZeroCommand 86c9d97e-03b2-3290-b077-4e678de3881c.dat in
    // chat-throttle-wuoqvsb3/vanilla/production-defaults/world/players/data.
    // Extensions/arrays below are deliberate sentinels, not claimed Java gameplay.
    let mut original = compound(
        r#"{
        Brain:{memories:{}}, post_effects:[],
        recipeBook:{recipes:["minecraft:crafting_table"],toBeDisplayed:["minecraft:crafting_table"]},
        attributes:[{id:"minecraft:movement_speed",base:0.10000000149011612d}],
        current_impulse_context_reset_grace_time:0,
        Inventory:[],EnderItems:[],fall_distance:3.5d,FallDistance:99.0f,
        Health:17.0f,Dimension:"minecraft:overworld",playerGameType:0,
        abilities:{walkSpeed:0.1f,flySpeed:0.05f,extension:{values:[I;1,-2,3]}},
        warden_spawn_tracker:{ticks_since_last_warning:25,extension:[{value:4L}]},
        respawn:{dimension:"minecraft:overworld",pos:[I;1,64,2],yaw:42.0f,pitch:0.0f,forced:1b,extension:{keep:7}},
        equipment:{future_slot:{keep:[I;5,6]}},
        CustomName:{text:"Saved name",extension:{keep:9}},
        opaque:{nested:[{ints:[I;1,-2,2147483647],longs:[L;1L,-2L],bytes:[B;1b,-2b]}]}
    }"#,
    );
    original.put_int("DataVersion", MAXIMUM_SUPPORTED_WORLD_DATA_VERSION);
    seed(&storage, uuid, original.clone());

    for level in 1..=3 {
        let mut current = player(fixture.source.world(), uuid).await;
        let (exists, mut loaded) = storage.load_player_data(&uuid).unwrap();
        assert!(exists);
        NBTStorage::read_nbt(&mut current, &mut loaded);
        assert_eq!(current.living_entity.fall_distance.load(), 3.5);
        assert_eq!(
            current.respawn_point.lock().unwrap().as_ref().unwrap().yaw,
            42.0
        );
        current.experience_level.store(level, Ordering::Relaxed);
        current.abilities.lock().unwrap().walk_speed = 0.25;
        let saved = save(&storage, &current);
        for key in [
            "Brain",
            "post_effects",
            "recipeBook",
            "attributes",
            "current_impulse_context_reset_grace_time",
            "opaque",
            "CustomName",
        ] {
            assert_eq!(saved.get(key), original.get(key), "{key}");
        }
        for (parent, child) in [
            ("abilities", "extension"),
            ("warden_spawn_tracker", "extension"),
            ("respawn", "extension"),
            ("equipment", "future_slot"),
        ] {
            assert_eq!(
                saved.get_compound(parent).unwrap().get(child),
                original.get_compound(parent).unwrap().get(child)
            );
        }
        assert_eq!(saved.get_int("XpLevel"), Some(level));
        assert_eq!(
            saved
                .get_compound("abilities")
                .unwrap()
                .get_float("walkSpeed"),
            Some(0.25)
        );
        assert_eq!(saved.get_double("fall_distance"), Some(3.5));
        assert!(saved.get("FallDistance").is_none());
        assert_eq!(
            saved.get_int("DataVersion"),
            Some(MAXIMUM_SUPPORTED_WORLD_DATA_VERSION)
        );
    }
    assert!(
        storage
            .get_player_data_path(&uuid)
            .starts_with(folder.path().join("world/players/data"))
    );
    assert!(!folder.path().join("world/playerdata").exists());
}

#[tokio::test]
async fn player_dat_removed_modeled_fields_do_not_return_from_the_remainder() {
    let fixture = Fixture::new().await;
    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path(), true);
    let uuid = Uuid::from_u128(511);
    seed(
        &storage,
        uuid,
        compound(
            r#"{
        Inventory:[{Slot:0b,id:"minecraft:stone",count:37},{Slot:100b,id:"minecraft:diamond_boots",count:1}],
        equipment:{offhand:{id:"minecraft:shield",count:1},head:{id:"minecraft:diamond_helmet",count:1}},
        EnderItems:[{Slot:2b,id:"minecraft:diamond",count:7}],
        active_effects:[{id:"minecraft:speed",duration:200,amplifier:1b,show_icon:1b}],
        respawn:{dimension:"minecraft:overworld",pos:[I;1,64,2],yaw:42.0f,pitch:0.0f,extension:7},
        RootVehicle:{Attach:[I;0,0,0,42]},
        CustomName:{text:"Old",extension:9},HasVisualFire:1b,Tags:[old],
        PumpkinCustomData:{test:{flag:1}},previousPlayerGameType:1,DeathTime:12s,
        opaque:{keep:[I;42]}
    }"#,
        ),
    );
    let mut current = player(fixture.source.world(), uuid).await;
    let (_, mut loaded) = storage.load_player_data(&uuid).unwrap();
    NBTStorage::read_nbt(&mut current, &mut loaded);
    assert_eq!(
        current.inventory.main_inventory.read().unwrap()[0].item_count,
        37
    );
    assert_eq!(
        current
            .living_entity
            .entity_equipment
            .lock()
            .unwrap()
            .equipment
            .len(),
        3
    );
    assert!(
        !current
            .living_entity
            .active_effects
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(current.root_vehicle_uuid.load().is_some());

    current
        .inventory
        .main_inventory
        .write()
        .unwrap()
        .fill(ItemStack::EMPTY.clone());
    current
        .living_entity
        .entity_equipment
        .lock()
        .unwrap()
        .clear();
    current
        .ender_chest_inventory
        .items
        .write()
        .unwrap()
        .fill(ItemStack::EMPTY.clone());
    current.living_entity.active_effects.lock().unwrap().clear();
    *current.respawn_point.lock().unwrap() = None;
    current.root_vehicle_uuid.store(None);
    current.previous_gamemode.store(None);
    current.living_entity.death_time.store(0, Ordering::Relaxed);
    current.living_entity.fall_distance.store(2.25);
    let entity = &current.living_entity.entity;
    entity.custom_name.store(Arc::new(None));
    entity.has_visual_fire.store(false, Ordering::Relaxed);
    entity.scoreboard_tags.lock().unwrap().clear();
    entity.remove_custom_data("test", "flag");

    for _ in 0..2 {
        let saved = save(&storage, &current);
        for key in [
            "RootVehicle",
            "respawn",
            "active_effects",
            "CustomName",
            "HasVisualFire",
            "Tags",
            "PumpkinCustomData",
            "previousPlayerGameType",
        ] {
            assert!(saved.get(key).is_none(), "resurrected {key}");
        }
        assert!(saved.get_list("Inventory").unwrap().is_empty());
        assert!(saved.get_list("EnderItems").unwrap().is_empty());
        assert!(saved.get_compound("equipment").unwrap().is_empty());
        assert_eq!(saved.get_short("DeathTime"), Some(0));
        assert_eq!(saved.get_double("fall_distance"), Some(2.25));
        assert_eq!(
            saved.get_compound("opaque").unwrap().get_int_array("keep"),
            Some([42].as_slice())
        );
    }
    let mut reloaded = player(fixture.source.world(), uuid).await;
    let (_, mut loaded) = storage.load_player_data(&uuid).unwrap();
    NBTStorage::read_nbt(&mut reloaded, &mut loaded);
    assert!(reloaded.root_vehicle_uuid.load().is_none());
    assert!(reloaded.respawn_point.lock().unwrap().is_none());
    assert!(
        reloaded
            .living_entity
            .active_effects
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(
        save(&storage, &reloaded)
            .get_list("Inventory")
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn player_dat_new_player_and_recognized_legacy_aliases_write_pinned_schema() {
    let fixture = Fixture::new().await;
    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path(), true);
    let uuid = Uuid::from_u128(512);
    let mut current = player(fixture.source.world(), uuid).await;
    let fresh = save(&storage, &current);
    assert_eq!(
        fresh.get_int("DataVersion"),
        Some(MAXIMUM_SUPPORTED_WORLD_DATA_VERSION)
    );
    assert!(fresh.get("opaque").is_none());
    assert!(fresh.get("recipeBook").is_none());
    assert!(fresh.get("RootVehicle").is_none());
    assert!(fresh.get("FallDistance").is_none());
    assert_eq!(fresh.get_double("fall_distance"), Some(0.0));

    seed(
        &storage,
        uuid,
        compound(
            r#"{
        DataVersion:4903,HasPlayedBefore:1b,AirSupply:300,DrowningTick:0,
        FallDistance:7.5f,SpawnX:1,SpawnY:64,SpawnZ:2,SpawnAngle:15.0f,
        SpawnDimension:"minecraft:overworld",SpawnForced:1b
    }"#,
        ),
    );
    let (_, mut legacy) = storage.load_player_data(&uuid).unwrap();
    NBTStorage::read_nbt(&mut current, &mut legacy);
    let saved = save(&storage, &current);
    for key in [
        "FallDistance",
        "SpawnX",
        "SpawnY",
        "SpawnZ",
        "SpawnAngle",
        "SpawnDimension",
        "SpawnForced",
    ] {
        assert!(saved.get(key).is_none(), "legacy alias {key}");
    }
    assert_eq!(saved.get_double("fall_distance"), Some(7.5));
    let respawn = saved.get_compound("respawn").unwrap();
    assert_eq!(respawn.get_float("yaw"), Some(15.0));
    assert_eq!(respawn.get_float("pitch"), Some(0.0));
    assert!(respawn.get("angle").is_none());
    assert_eq!(
        saved.get_int("DataVersion"),
        Some(MAXIMUM_SUPPORTED_WORLD_DATA_VERSION)
    );

    // The earlier Pumpkin nested alias is read, but never written alongside yaw.
    legacy = compound(r#"{respawn:{dimension:"minecraft:overworld",pos:[I;1,64,2],angle:-45.0f}}"#);
    NBTStorage::read_nbt(&mut current, &mut legacy);
    let saved = save(&storage, &current);
    assert_eq!(
        saved.get_compound("respawn").unwrap().get_float("yaw"),
        Some(-45.0)
    );
    assert!(
        saved
            .get_compound("respawn")
            .unwrap()
            .get("angle")
            .is_none()
    );
}

#[test]
fn player_dat_unsupported_partial_state_refuses_without_backup_fallback_or_replacement() {
    for (snbt, field) in [
        ("{DataVersion:5024}", "DataVersion"),
        ("{DataVersion:4903}", "DataVersion"),
        (
            "{RootVehicle:{Attach:[I;0,0,0,42],Entity:{id:boat}}}",
            "RootVehicle",
        ),
        (
            r#"{respawn:{dimension:"minecraft:overworld",pos:[I;1,64,2],yaw:0.0f,pitch:30.0f}}"#,
            "respawn.pitch",
        ),
        (
            r#"{active_effects:[{id:"minecraft:speed",show_icon:1b,hidden_effect:{duration:20}}]}"#,
            "active_effects.hidden_effect",
        ),
        (
            r#"{active_effects:[{id:"minecraft:speed",duration:20,show_icon:2b}]}"#,
            "active_effects.show_icon",
        ),
        (
            r#"{active_effects:[{id:"minecraft:speed",duration:20,show_particles:2b}]}"#,
            "active_effects.show_particles",
        ),
        (
            r#"{Inventory:[{Slot:0b,id:"minecraft:stone",count:1,extension:42}]}"#,
            "Inventory",
        ),
        (
            r#"{equipment:{body:{id:"minecraft:stone",count:1}}}"#,
            "equipment.body",
        ),
    ] {
        assert_refused_unchanged(snbt, field);
    }
}

#[tokio::test]
async fn player_dat_effect_amplifier_loads_unsigned_bytes_and_legacy_ints() {
    let fixture = Fixture::new().await;
    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path(), true);
    let uuid = Uuid::from_u128(514);
    // Bytecode-derived shapes: Details uses UNSIGNED_BYTE with default zero.
    // The int cases exercise old Pumpkin output, not an observed Java save.
    for (amplifier, expected) in [
        (None, 0),
        (Some(NbtTag::Byte(0)), 0),
        (Some(NbtTag::Byte(1)), 1),
        (Some(NbtTag::Byte(-1)), 255),
        (Some(NbtTag::Int(1)), 1),
        (Some(NbtTag::Int(255)), 255),
    ] {
        let mut effect = compound(r#"{id:"minecraft:regeneration",duration:200,show_icon:1b}"#);
        if let Some(amplifier) = amplifier {
            effect.put("amplifier", amplifier);
        }
        let mut data = NbtCompound::new();
        data.put_list("active_effects", vec![NbtTag::Compound(effect)]);
        seed(&storage, uuid, data);
        for _ in 0..2 {
            let (_, mut loaded) = storage.load_player_data(&uuid).unwrap();
            let mut current = player(fixture.source.world(), uuid).await;
            NBTStorage::read_nbt(&mut current, &mut loaded);
            let effect = current
                .living_entity
                .get_effect(&pumpkin_data::effect::StatusEffect::REGENERATION)
                .unwrap();
            assert_eq!(effect.amplifier, expected);
            assert_eq!(effect.duration, 200);
            assert!(!effect.ambient);
            assert!(effect.show_particles);
            assert!(effect.show_icon);
            let saved = save(&storage, &current);
            let effects = saved.get_list("active_effects").unwrap();
            assert_eq!(effects.len(), 1);
            let effect = effects[0].extract_compound().unwrap();
            assert_eq!(
                effect.get("amplifier"),
                (expected != 0)
                    .then_some(NbtTag::Byte(expected as i8))
                    .as_ref()
            );
        }
    }
}

#[test]
fn player_dat_effect_amplifier_outside_supported_range_refuses_unchanged() {
    for amplifier in ["-1", "256", "1s"] {
        assert_refused_unchanged(
            &format!(
                r#"{{active_effects:[{{id:"minecraft:regeneration",amplifier:{amplifier},show_icon:1b}}]}}"#
            ),
            "active_effects.amplifier",
        );
    }
}

#[tokio::test]
async fn player_dat_effect_visibility_defaults_survive_actual_player_save_and_reload() {
    let fixture = Fixture::new().await;
    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path(), true);
    let uuid = Uuid::from_u128(517);
    // Bytecode-derived Details shapes, not a captured gameplay witness. The
    // writer omits default ambient/particles but always supplies show_icon;
    // the reader also accepts its absence, inheriting the particles flag.
    for (flags, show_particles, show_icon) in [
        (",show_icon:1b", true, true),
        ("", true, true),
        (",show_particles:0b", false, false),
        (",show_icon:0b", true, false),
        (",show_particles:0b,show_icon:1b", false, true),
        (",ambient:0b,show_particles:0b,show_icon:0b", false, false),
    ] {
        let effect = compound(&format!(
            r#"{{id:"minecraft:regeneration",amplifier:1b,duration:200{flags}}}"#
        ));
        let mut original = NbtCompound::new();
        original.put_list("active_effects", vec![NbtTag::Compound(effect)]);
        seed(&storage, uuid, original);
        let mut expected = compound(r#"{id:"minecraft:regeneration",amplifier:1b,duration:200}"#);
        if !show_particles {
            expected.put_bool("show_particles", false);
        }
        expected.put_bool("show_icon", show_icon);
        for _ in 0..2 {
            let (_, mut loaded) = storage.load_player_data(&uuid).unwrap();
            let mut current = player(fixture.source.world(), uuid).await;
            NBTStorage::read_nbt(&mut current, &mut loaded);
            let effect = current
                .living_entity
                .get_effect(&pumpkin_data::effect::StatusEffect::REGENERATION)
                .unwrap();
            assert_eq!(effect.amplifier, 1);
            assert_eq!(effect.duration, 200);
            assert!(!effect.ambient);
            assert_eq!(effect.show_particles, show_particles, "{flags}");
            assert_eq!(effect.show_icon, show_icon, "{flags}");
            let saved = save(&storage, &current);
            let effects = saved.get_list("active_effects").unwrap();
            assert_eq!(effects.len(), 1);
            assert_eq!(effects[0].extract_compound(), Some(&expected), "{flags}");
        }
    }
}

#[tokio::test]
async fn player_dat_absorption_survives_load_before_attributes_and_effects() {
    let fixture = Fixture::new().await;
    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path(), true);
    let uuid = Uuid::from_u128(515);
    // The official 26.3 load path calls the unclamped internal absorption setter.
    // Attributes remain opaque here; this checks preservation, not their gameplay.
    let original = compound(
        r#"{
        AbsorptionAmount:4.0f,
        attributes:[{id:"minecraft:max_absorption",base:4.0d}],
        active_effects:[{id:"minecraft:absorption",duration:200,show_icon:1b}]
    }"#,
    );
    seed(&storage, uuid, original.clone());
    for _ in 0..2 {
        let (_, mut loaded) = storage.load_player_data(&uuid).unwrap();
        let mut current = player(fixture.source.world(), uuid).await;
        assert_eq!(
            current
                .living_entity
                .get_attribute_value(&pumpkin_data::attributes::Attributes::MAX_ABSORPTION),
            0.0
        );
        NBTStorage::read_nbt(&mut current, &mut loaded);
        assert_eq!(current.get_absorption(), 4.0);
        assert!(
            current
                .living_entity
                .get_effect(&pumpkin_data::effect::StatusEffect::ABSORPTION)
                .is_some()
        );
        let saved = save(&storage, &current);
        assert_eq!(saved.get_float("AbsorptionAmount"), Some(4.0));
        assert_eq!(saved.get("attributes"), original.get("attributes"));
    }
}

#[test]
fn player_dat_lossy_books_and_nested_items_refuse_unchanged() {
    // These refusals describe Rust codec capacity, not Java source validity.
    for (snbt, field) in [
        (
            r#"{Inventory:[{Slot:0b,id:"minecraft:written_book",count:1,components:{"minecraft:written_book_content":{title:"Copy",author:"Author",pages:[],generation:1}}}]}"#,
            "Inventory",
        ),
        (
            r#"{EnderItems:[{Slot:0b,id:"minecraft:written_book",count:1,components:{"minecraft:written_book_content":{title:{raw:"Copy",filtered:"Filtered"},author:"Author",pages:[],resolved:1b}}}]}"#,
            "EnderItems",
        ),
        (
            r#"{Inventory:[{Slot:0b,id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[{slot:0,item:{id:"minecraft:stick",count:1,components:{"minecraft:custom_name":{text:"Keep",color:"red"}}}}]}}]}"#,
            "Inventory",
        ),
        (
            r#"{EnderItems:[{Slot:0b,id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":[{id:"minecraft:stick",count:1,components:{"minecraft:custom_name":{text:"Keep",color:"red"}}}]}}]}"#,
            "EnderItems",
        ),
        (
            r#"{equipment:{offhand:{id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[{slot:0,item:{id:"minecraft:stone",count:1,components:{},extension:42}}]}}}}"#,
            "equipment",
        ),
        (
            r#"{Inventory:[{Slot:0b,id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":[{id:"minecraft:stone",count:257,components:{}}]}}]}"#,
            "Inventory",
        ),
        (
            r#"{Inventory:[{Slot:0b,id:"minecraft:stick",count:1,components:{"!minecraft:custom_name":{extension:42}}}]}"#,
            "Inventory",
        ),
    ] {
        assert_refused_unchanged(snbt, field);
    }
}

#[tokio::test]
async fn player_dat_literal_names_and_lossless_containers_survive_actual_player_save() {
    let fixture = Fixture::new().await;
    let folder = tempfile::tempdir().unwrap();
    let storage = PlayerDataStorage::new(folder.path(), true);
    let uuid = Uuid::from_u128(516);
    // The first three slots match the Java-shaped interchange fixture's literal
    // names and ordinary count. Nested positive cases keep a represented patch.
    let original = compound(
        r#"{
        Inventory:[
            {Slot:0b,id:"minecraft:stick",count:1,components:{"minecraft:item_name":"Interchange item"}},
            {Slot:1b,id:"minecraft:gold_ingot",count:37},
            {Slot:2b,id:"minecraft:stick",count:1,components:{"minecraft:custom_name":"Interchange custom"}},
            {Slot:3b,id:"minecraft:shulker_box",count:1,components:{"minecraft:container":[{slot:0,item:{id:"minecraft:stone",count:5,components:{"minecraft:custom_name":"Nested stone"}}}]}}
        ],
        EnderItems:[{Slot:2b,id:"minecraft:bundle",count:1,components:{"minecraft:bundle_contents":[{id:"minecraft:stick",count:2,components:{"minecraft:item_name":"Nested stick"}}]}}],
        equipment:{offhand:{id:"minecraft:shield",count:1,components:{"minecraft:custom_name":"Shield"}}}
    }"#,
    );
    seed(&storage, uuid, original.clone());
    let mut expected = original.clone();
    let Some(NbtTag::List(inventory)) = expected.child_tags.get_mut("Inventory") else {
        panic!("expected inventory");
    };
    let NbtTag::Compound(gold) = &mut inventory[1] else {
        panic!("expected gold stack");
    };
    gold.put_compound("components", NbtCompound::new());
    let mut offhand = original
        .get_compound("equipment")
        .unwrap()
        .get_compound("offhand")
        .unwrap()
        .clone();
    offhand.put_byte("Slot", -106);
    inventory.push(NbtTag::Compound(offhand));
    for _ in 0..2 {
        let (_, mut loaded) = storage.load_player_data(&uuid).unwrap();
        let mut current = player(fixture.source.world(), uuid).await;
        NBTStorage::read_nbt(&mut current, &mut loaded);
        let saved = save(&storage, &current);
        for key in ["Inventory", "EnderItems", "equipment"] {
            assert_eq!(saved.get(key), expected.get(key), "{key}");
        }
    }
}
