use super::*;
use crate::{
    entity::{
        EntityData, EntityState,
        player::{GameType, ServerPlayer},
    },
    inventory::{self, MenuProvider},
    level::{Level, effects::LevelEffect},
};
use ferraloom_data::{BlockState, registries::attribute::Id as Attribute};
use ferraloom_protocol::{chat::Component, play::clientbound as cb};
use ferraloom_util::{Uuid, Vec3};

const POS: BlockPos = BlockPos::new(8, 64, 8);
fn fixture() -> (Level, EntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(crate::block::UnportedBehavior::Strict);
    let player = level.next_entity_ref();
    let entity_id = level.next_entity_id();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "workstation".into(),
                entity_id,
                Vec3::new(8.5, 63.0, 8.5),
                GameType::Survival,
            ))),
        )
        .unwrap();
    (level, player)
}
fn kind(value: &str) -> MenuKind {
    match value {
        "crafting" => MenuKind::Crafting,
        "anvil" => MenuKind::Anvil,
        "loom" => MenuKind::Loom,
        "stonecutter" => MenuKind::Stonecutter,
        "grindstone" => MenuKind::Grindstone,
        "smithing" => MenuKind::Smithing,
        "cartography" => MenuKind::Cartography,
        _ => panic!("unexpected native menu kind {value}"),
    }
}
fn double(value: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(value, 16).unwrap())
}

/// Real menus retain their captured block position across range and block
/// changes, including strict range boundaries and damaged anvil replacements.
#[test]
fn bound_workstation_access_matches_native_range_and_broken_block_queries() {
    let (mut level, player) = fixture();
    let mut scenario = None;
    let mut receiver = None;
    let mut count = 0;
    for line in include_str!("../../../tests/java/blocks/entities/workstation-access.tsv").lines() {
        let f: Vec<_> = line.split('\t').collect();
        let kind = kind(f[1]);
        let bound = f[2] == "true";
        let key = (kind, bound, f[3]);
        if scenario != Some(key) {
            let menu = if bound {
                AbstractContainerMenu::new_bound_workstation(kind, 7, POS, &level).unwrap()
            } else {
                AbstractContainerMenu::new(kind, 7).unwrap()
            };
            receiver = Some(
                level
                    .entity_mut::<ServerPlayer>(player)
                    .unwrap()
                    .menus
                    .install_container_menu(menu),
            );
            scenario = Some(key);
        }
        level
            .entity_mut::<ServerPlayer>(player)
            .unwrap()
            .attributes
            .get_instance(Attribute::BLOCK_INTERACTION_RANGE)
            .unwrap()
            .set_base_value(double(f[3]));
        level
            .entity_mut::<EntityData>(player)
            .unwrap()
            .set_pos(Vec3::new(double(f[4]), 63.0, 8.5));
        level
            .set_block_state(POS, BlockState::from_id(f[5].parse().unwrap()).unwrap())
            .unwrap();
        assert_eq!(
            inventory::live_menu_still_valid(player, receiver.unwrap(), &level).unwrap(),
            f[6] == "true",
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 258);
}

struct Workstation;
impl MenuProvider for Workstation {
    fn display_name(&self) -> Component {
        Component::literal("Workbench")
    }
    fn create_menu(
        &self,
        id: i32,
        _: EntityRef,
        level: &mut Level,
    ) -> Result<Option<AbstractContainerMenu>> {
        AbstractContainerMenu::new_bound_workstation(MenuKind::Crafting, id, POS, level).map(Some)
    }
}
#[test]
fn current_workstation_closes_after_walking_away_or_breaking_the_table() {
    for broken in [false, true] {
        let (mut level, player) = fixture();
        level
            .set_block_state(POS, Block::CRAFTING_TABLE.default_state())
            .unwrap();
        let menu = inventory::open_menu_live(player, &Workstation, &mut level)
            .unwrap()
            .unwrap();
        level.take_effects();
        inventory::check_container_menu_still_valid(player, &mut level).unwrap();
        assert_eq!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .menus
                .current_ref(),
            menu
        );
        if broken {
            level
                .set_block_state(POS, Block::AIR.default_state())
                .unwrap();
        } else {
            level
                .entity_mut::<EntityData>(player)
                .unwrap()
                .set_pos(Vec3::new(100.0, 63.0, 8.5));
        }
        inventory::check_container_menu_still_valid(player, &mut level).unwrap();
        assert_ne!(
            level
                .entity::<ServerPlayer>(player)
                .unwrap()
                .menus
                .current_ref(),
            menu
        );
        assert!(level.take_effects().iter().any(|effect| matches!(effect,
            LevelEffect::PlayerPacket { packet, .. } if matches!(packet.as_ref(), cb::Packet::ContainerClose(_)))));
    }
}

#[test]
fn captured_dimension_mismatch_uses_evaluate_defaults_and_skips_execute() {
    let (mut level, player) = fixture();
    let mut menu =
        AbstractContainerMenu::new_bound_workstation(MenuKind::Crafting, 7, POS, &level).unwrap();
    let access = menu.level_access.as_mut().unwrap();
    access.dimension = "other".parse().unwrap();
    assert_eq!(
        access
            .evaluate(&level, |_, _| panic!("wrong dimension queried"))
            .unwrap(),
        None::<bool>
    );
    access
        .execute(&mut level, |_, _| panic!("wrong dimension callback"))
        .unwrap();
    let receiver = level
        .entity_mut::<ServerPlayer>(player)
        .unwrap()
        .menus
        .install_container_menu(menu);
    assert!(inventory::live_menu_still_valid(player, receiver, &level).unwrap());
}
