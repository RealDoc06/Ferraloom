use super::tests::POS;
use crate::{
    block::{BlockMethod, InteractionResult, UnportedBehavior, UseContext},
    entity::{
        EntityRef, EntityState,
        player::{GameType, ServerPlayer},
    },
    inventory::AbstractContainerMenu,
    level::{Level, effects::LevelEffect},
    stats::{Stat, custom},
};
use ferraloom_data::BlockState;
use ferraloom_protocol::{
    chat::Component,
    play::{BlockHitResult, InteractionHand, clientbound::Packet},
};
use ferraloom_util::{Direction, Uuid, Vec3};

const NATIVE: &str = include_str!("../../../tests/java/blocks/building/workstations.tsv");

fn fixture(state: BlockState) -> (Level, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(UnportedBehavior::Strict);
    crate::entity::tests::install_test_environment(&mut level, false, "minecraft:overworld");
    level.set_block_state(POS, state).unwrap();
    let player = level.next_entity_ref();
    let network_id = level.next_entity_id().unwrap();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "workstation".into(),
                network_id,
                Vec3::new(8.5, 64.5, 8.5),
                GameType::Survival,
            ))),
        )
        .unwrap();
    level.take_effects();
    (level, player)
}
fn title(component: &Component) -> String {
    serde_json::from_str::<serde_json::Value>(&component.to_json().unwrap()).unwrap()["translate"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn describe(title: &Component, menu: &AbstractContainerMenu) -> String {
    let access = menu
        .level_access
        .as_ref()
        .expect("native bound workstation");
    format!(
        "{}\t{}\t{}\t{}@{}, {}, {}",
        self::title(title),
        menu.kind.registry_id().unwrap().0,
        menu.slots.len(),
        access.dimension,
        access.pos.x,
        access.pos.y,
        access.pos.z,
    )
}

#[test]
fn all_workstation_providers_match_native_titles_kinds_slots_and_captured_access() {
    let mut checked = 0;
    for line in NATIVE.lines().filter(|line| line.starts_with("provider\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let (mut level, player) = fixture(state);
        let provider = level
            .required_block_behavior(state.block(), BlockMethod::GetMenuProvider)
            .unwrap()
            .get_menu_provider(state, POS, &level)
            .unwrap()
            .unwrap();
        let menu = provider
            .create_menu(7, player, &mut level)
            .unwrap()
            .unwrap();
        assert_eq!(menu.container_id, 7);
        assert_eq!(
            describe(&provider.display_name(), &menu),
            f[2..].join("\t"),
            "{line}"
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        checked += 1;
    }
    assert_eq!(checked, 35);
}

#[test]
fn all_workstation_clicks_match_native_open_screen_success_and_interaction_stat() {
    let mut checked = 0;
    for line in NATIVE.lines().filter(|line| line.starts_with("use\t")) {
        let f: Vec<_> = line.split('\t').collect();
        let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
        let (mut level, player) = fixture(state);
        let context = UseContext {
            player,
            hand: InteractionHand::MainHand,
            hit: BlockHitResult {
                block_pos: POS,
                direction: Direction::VALUES[f[2].parse::<usize>().unwrap()],
                offset_x: 0.5,
                offset_y: 0.5,
                offset_z: 0.5,
                inside: false,
                world_border_hit: false,
            },
        };
        let result = level
            .required_block_behavior(state.block(), BlockMethod::UseWithoutItem)
            .unwrap()
            .use_without_item(state, POS, &context, &mut level)
            .unwrap();
        assert_eq!(
            result == InteractionResult::SUCCESS,
            f[3] == "true",
            "{line}"
        );
        let screens = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                LevelEffect::PlayerPacket { recipients, packet } => match *packet {
                    Packet::OpenScreen(screen) => {
                        assert_eq!(recipients, [player]);
                        Some(screen)
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(screens.len(), 1, "{line}");
        let actor = level.entity::<ServerPlayer>(player).unwrap();
        let menu = actor.menus.current();
        assert_eq!(screens[0].container_id.0, menu.container_id);
        assert_eq!(screens[0].menu_type, menu.kind.registry_id().unwrap());
        assert_eq!(
            describe(&screens[0].title, menu),
            f[4..8].join("\t"),
            "{line}"
        );
        let stat = custom::Id::from_protocol_id(f[8].parse().unwrap()).unwrap();
        assert_eq!(
            actor.stats.get_value(Stat::Custom(stat)),
            f[9].parse::<i32>().unwrap(),
            "{line}"
        );
        assert_eq!(level.unported_behavior_stats().skipped_calls, 0);
        checked += 1;
    }
    assert_eq!(checked, 210);
}
