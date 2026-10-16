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
        active_effects:[{id:"minecraft:speed",duration:200,amplifier:1,show_icon:1b}],
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
            r#"{active_effects:[{id:"minecraft:speed",duration:20}]}"#,
            "active_effects",
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
}
