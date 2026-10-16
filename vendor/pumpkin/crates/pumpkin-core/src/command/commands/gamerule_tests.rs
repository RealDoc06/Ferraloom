use super::*;
use crate::command::commands::data::tests::Fixture;
use std::collections::BTreeSet;

// Independent spelling witness: review17-parity.json, servers.vanilla's
// network.command_tree.detail.literal_paths, official 26.3 / protocol 777.
// Report SHA256: 9220e3bf51cd7dbd502e0a7a9a386c4224e4d5238531e35f30592bf6326bcc1d
// Packet payload SHA256: e7c5ca4934ec9cefd076192f1402416810267e04981f656c68694c61cd760647
// These 58 bare names AND their minecraft: spellings occur in that packet.
// This witnesses spelling, not defaults, numeric bounds, persistence or gameplay.
const VANILLA_DEFAULT_RULES: &[(GameRule, &str)] = &[
    (GameRule::AdvanceTime, "advance_time"),
    (GameRule::AdvanceWeather, "advance_weather"),
    (
        GameRule::AllowEnteringNetherUsingPortals,
        "allow_entering_nether_using_portals",
    ),
    (GameRule::BlockDrops, "block_drops"),
    (
        GameRule::BlockExplosionDropDecay,
        "block_explosion_drop_decay",
    ),
    (GameRule::CommandBlockOutput, "command_block_output"),
    (GameRule::CommandBlocksWork, "command_blocks_work"),
    (GameRule::DrowningDamage, "drowning_damage"),
    (GameRule::ElytraMovementCheck, "elytra_movement_check"),
    (
        GameRule::EnderPearlsVanishOnDeath,
        "ender_pearls_vanish_on_death",
    ),
    (GameRule::EntityDrops, "entity_drops"),
    (GameRule::FallDamage, "fall_damage"),
    (GameRule::FireDamage, "fire_damage"),
    (
        GameRule::FireSpreadRadiusAroundPlayer,
        "fire_spread_radius_around_player",
    ),
    (GameRule::ForgiveDeadPlayers, "forgive_dead_players"),
    (GameRule::FreezeDamage, "freeze_damage"),
    (GameRule::GlobalSoundEvents, "global_sound_events"),
    (GameRule::ImmediateRespawn, "immediate_respawn"),
    (GameRule::KeepInventory, "keep_inventory"),
    (GameRule::LavaSourceConversion, "lava_source_conversion"),
    (GameRule::LimitedCrafting, "limited_crafting"),
    (GameRule::LocatorBar, "locator_bar"),
    (GameRule::LogAdminCommands, "log_admin_commands"),
    (GameRule::MaxBlockModifications, "max_block_modifications"),
    (GameRule::MaxCommandForks, "max_command_forks"),
    (
        GameRule::MaxCommandSequenceLength,
        "max_command_sequence_length",
    ),
    (GameRule::MaxEntityCramming, "max_entity_cramming"),
    (
        GameRule::MaxSnowAccumulationHeight,
        "max_snow_accumulation_height",
    ),
    (GameRule::MobDrops, "mob_drops"),
    (GameRule::MobExplosionDropDecay, "mob_explosion_drop_decay"),
    (GameRule::MobGriefing, "mob_griefing"),
    (
        GameRule::NaturalHealthRegeneration,
        "natural_health_regeneration",
    ),
    (GameRule::PlayerMovementCheck, "player_movement_check"),
    (
        GameRule::PlayersNetherPortalCreativeDelay,
        "players_nether_portal_creative_delay",
    ),
    (
        GameRule::PlayersNetherPortalDefaultDelay,
        "players_nether_portal_default_delay",
    ),
    (
        GameRule::PlayersSleepingPercentage,
        "players_sleeping_percentage",
    ),
    (
        GameRule::ProjectilesCanBreakBlocks,
        "projectiles_can_break_blocks",
    ),
    (GameRule::Pvp, "pvp"),
    (GameRule::Raids, "raids"),
    (GameRule::RandomTickSpeed, "random_tick_speed"),
    (GameRule::ReducedDebugInfo, "reduced_debug_info"),
    (GameRule::RespawnRadius, "respawn_radius"),
    (GameRule::SendCommandFeedback, "send_command_feedback"),
    (
        GameRule::ShowAdvancementMessages,
        "show_advancement_messages",
    ),
    (GameRule::ShowDeathMessages, "show_death_messages"),
    (GameRule::SpawnMobs, "spawn_mobs"),
    (GameRule::SpawnMonsters, "spawn_monsters"),
    (GameRule::SpawnPatrols, "spawn_patrols"),
    (GameRule::SpawnPhantoms, "spawn_phantoms"),
    (GameRule::SpawnWanderingTraders, "spawn_wandering_traders"),
    (GameRule::SpawnWardens, "spawn_wardens"),
    (GameRule::SpawnerBlocksWork, "spawner_blocks_work"),
    (
        GameRule::SpectatorsGenerateChunks,
        "spectators_generate_chunks",
    ),
    (GameRule::SpreadVines, "spread_vines"),
    (GameRule::TntExplodes, "tnt_explodes"),
    (GameRule::TntExplosionDropDecay, "tnt_explosion_drop_decay"),
    (GameRule::UniversalAnger, "universal_anger"),
    (GameRule::WaterSourceConversion, "water_source_conversion"),
];

async fn fixture() -> Fixture {
    let mut fixture = Fixture::new().await;
    register(&mut fixture.dispatcher, &PermissionRegistry::default());
    // The real server registered gamerule's level-two default permission.
    assert_eq!(fixture.source.output.permission_lvl(), PermissionLvl::Two);
    assert!(fixture.source.has_permission(PERMISSION));
    fixture
}

#[tokio::test]
async fn boolean_set_and_query_share_live_state_and_results_across_spellings() {
    let fixture = fixture().await;
    let server = fixture.source.server();
    let world = fixture.source.world();

    assert_eq!(fixture.execute("gamerule keep_inventory true").unwrap(), 1);
    assert_eq!(
        fixture
            .execute("gamerule minecraft:keep_inventory")
            .unwrap(),
        1
    );
    assert!(server.level_info.load().game_rules.keep_inventory);
    assert!(world.level_info.load().game_rules.keep_inventory);

    assert_eq!(
        fixture
            .execute("gamerule minecraft:keep_inventory false")
            .unwrap(),
        0
    );
    assert_eq!(fixture.execute("gamerule keep_inventory").unwrap(), 0);
    assert!(!server.level_info.load().game_rules.keep_inventory);
    assert!(!world.level_info.load().game_rules.keep_inventory);
}

#[tokio::test]
async fn integer_set_and_query_share_live_state_and_results_across_spellings() {
    let fixture = fixture().await;
    let server = fixture.source.server();
    let world = fixture.source.world();

    assert_eq!(
        fixture.execute("gamerule random_tick_speed 11").unwrap(),
        11
    );
    assert_eq!(
        fixture
            .execute("gamerule minecraft:random_tick_speed")
            .unwrap(),
        11
    );
    assert_eq!(server.level_info.load().game_rules.random_tick_speed, 11);
    assert_eq!(world.level_info.load().game_rules.random_tick_speed, 11);

    assert_eq!(
        fixture
            .execute("gamerule minecraft:random_tick_speed 5")
            .unwrap(),
        5
    );
    assert_eq!(fixture.execute("gamerule random_tick_speed").unwrap(), 5);
    assert_eq!(server.level_info.load().game_rules.random_tick_speed, 5);
    assert_eq!(world.level_info.load().game_rules.random_tick_speed, 5);
}

#[tokio::test]
async fn invalid_types_and_unknown_names_leave_live_rules_untouched() {
    let fixture = fixture().await;
    let before = fixture.source.server().level_info.load_full();

    for (rule, name) in VANILLA_DEFAULT_RULES {
        let invalid = match before.game_rules.get(rule) {
            GameRuleValue::Int(_) => "true",
            GameRuleValue::Bool(_) => "1",
        };
        for spelling in [(*name).to_owned(), format!("minecraft:{name}")] {
            let input = format!("gamerule {spelling} {invalid}");
            assert!(fixture.execute(&input).is_err(), "{input}");
            assert_eq!(
                fixture.source.server().level_info.load().game_rules,
                before.game_rules
            );
        }
    }

    for input in [
        "gamerule other:keep_inventory true",
        "gamerule minecraft:missing_rule true",
    ] {
        assert!(fixture.execute(input).is_err(), "{input}");
    }
    assert!(Arc::ptr_eq(
        &before,
        &fixture.source.server().level_info.load_full()
    ));
}

#[tokio::test]
async fn retained_default_names_bind_real_rules_and_are_advertised() {
    let fixture = fixture().await;
    let tree = &fixture.dispatcher.tree;
    let command = tree.get("gamerule").unwrap();
    let children = &tree[command].children;
    let before = fixture.source.server().level_info.load_full();
    assert_eq!(VANILLA_DEFAULT_RULES.len(), 58);
    let mut expected_names = BTreeSet::new();

    for (rule, name) in VANILLA_DEFAULT_RULES {
        let expected_result = match before.game_rules.get(rule) {
            GameRuleValue::Int(value) => i32::try_from(*value).unwrap(),
            GameRuleValue::Bool(value) => i32::from(*value),
        };
        for spelling in [(*name).to_owned(), format!("minecraft:{name}")] {
            // The Java command packet copies these non-root child edges.
            // Check advertisement AND execution; neither alone proves mechanics.
            let node = &tree[*children.get(&spelling).unwrap()];
            assert!(node.command().is_some(), "{spelling}");
            let input = format!("gamerule {spelling}");
            assert_eq!(fixture.execute(&input).unwrap(), expected_result, "{input}");
            expected_names.insert(spelling);
        }

        // A full registry comparison catches a spelling bound to the wrong rule,
        // including booleans whose default query results happen to be identical.
        let mut updated_rules = before.game_rules.clone();
        let updated_result = match updated_rules.get_mut(rule) {
            GameRuleValue::Int(value) => {
                *value += 1;
                i32::try_from(*value).unwrap()
            }
            GameRuleValue::Bool(value) => {
                *value = !*value;
                i32::from(*value)
            }
        };
        let input = format!("gamerule minecraft:{name} {}", updated_rules.get(rule));
        assert_eq!(fixture.execute(&input).unwrap(), updated_result, "{input}");
        assert_eq!(
            fixture.source.server().level_info.load().game_rules,
            updated_rules
        );
        assert_eq!(
            fixture.execute(&format!("gamerule {name}")).unwrap(),
            updated_result
        );

        let input = format!("gamerule {name} {}", before.game_rules.get(rule));
        assert_eq!(fixture.execute(&input).unwrap(), expected_result, "{input}");
        assert_eq!(
            fixture.source.server().level_info.load().game_rules,
            before.game_rules
        );
        assert_eq!(
            fixture
                .execute(&format!("gamerule minecraft:{name}"))
                .unwrap(),
            expected_result,
        );
    }

    // Existing Rust-only default extra: official 26.3 enables it only with
    // MINECART_IMPROVEMENTS. This alias patch leaves its bare form unchanged;
    // feature-aware registration/advertisement requires a separate change.
    expected_names.insert("max_minecart_speed".to_owned());
    assert_eq!(
        children.keys().cloned().collect::<BTreeSet<_>>(),
        expected_names
    );
}
