use super::*;
use crate::{block::entity, entity::EntityState};
use ferraloom_data::registries::attribute::Id as Attribute;
use ferraloom_protocol::play::clientbound::Packet;
use ferraloom_util::{Aabb, Uuid};
use std::io::Read;

const POS: BlockPos = BlockPos::new(8, 64, 8);
fn vectors() -> String {
    let mut data = String::new();
    flate2::read::GzDecoder::new(
        include_bytes!("../../../../tests/java/blocks/entities/chest-opener-query.tsv.gz")
            .as_slice(),
    )
    .read_to_string(&mut data)
    .unwrap();
    data
}
fn level() -> (Level, BlockEntityRef) {
    let mut level = crate::entity::tests::level(false);
    level.set_unported_behavior(crate::block::UnportedBehavior::Strict);
    level
        .set_block_state(POS, Block::COPPER_CHEST.default_state())
        .unwrap();
    level
        .set_block_entity(entity::create(POS, Block::COPPER_CHEST.default_state()).unwrap())
        .unwrap();
    let handle = level.block_entity_ref(POS).unwrap().unwrap();
    (level, handle)
}

#[test]
fn native_chest_opener_query_uses_counter_radius_spectator_and_container_identity() {
    let mut checked = 0;
    for row in vectors()
        .lines()
        .filter(|row| row.starts_with("chest-opener-query\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        let (mut level, original) = level();
        level
            .block_entity_mut::<ChestBlockEntity>(original)
            .unwrap()
            .max_interaction_range = f[1].parse().unwrap();
        level.retain_block_entity(original).unwrap();
        // A new object at the identical position is not the original container.
        level
            .set_block_entity(entity::create(POS, Block::COPPER_CHEST.default_state()).unwrap())
            .unwrap();
        let replacement = level.block_entity_ref(POS).unwrap().unwrap();
        let x = f64::from(POS.x) + f[2].parse::<f64>().unwrap();
        let mut player = ServerPlayer::new(
            Uuid::new(1, 2),
            "opener".into(),
            level.next_entity_id().unwrap(),
            Vec3::new(x, 64.0, 8.5),
            if f[4] == "true" {
                GameType::Spectator
            } else {
                GameType::Survival
            },
        );
        player.bounding_box = Aabb::new(x - 0.5, 64.0, 8.0, x + 0.5, 65.0, 9.0);
        let kind = if f[3] == "wrong_menu" {
            MenuKind::Hopper
        } else {
            MenuKind::Chest {
                rows: if f[3].starts_with("compound_") { 6 } else { 3 },
            }
        };
        let mut menu = AbstractContainerMenu::new(kind, 7).unwrap();
        menu.block_entities = match f[3] {
            "same" | "wrong_menu" => vec![original],
            "compound_first" => vec![original, replacement],
            "compound_second" => vec![replacement, original],
            "other" => vec![replacement],
            _ => panic!("unknown identity {row}"),
        };
        player.menus.install_container_menu(menu);
        let id = level.next_entity_ref();
        level
            .add_entity(id, EntityState::Player(Box::new(player)))
            .unwrap();
        level.take_effects();
        assert_eq!(
            has_entities_with_container_open(original, &level).unwrap(),
            f[5] == "1",
            "{row}"
        );
        assert!(
            level.take_effects().is_empty(),
            "query must remain read-only: {row}"
        );
        level.release_block_entity(original);
        checked += 1;
    }
    assert_eq!(checked, 180);
}

#[test]
fn native_chest_counter_range_tracks_increments_and_final_close() {
    let (mut level, receiver) = level();
    // Sound dispatch for the counter uses the same inherited ChestBlock path.
    let player = level.next_entity_ref();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "counter".into(),
                level.next_entity_id().unwrap(),
                Vec3::new(8.5, 64.0, 8.5),
                GameType::Survival,
            ))),
        )
        .unwrap();
    let mut checked = 0;
    for row in vectors()
        .lines()
        .filter(|row| row.starts_with("chest-opener-counter\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        if f[2] == "start" {
            level
                .entity_mut::<ServerPlayer>(player)
                .unwrap()
                .attributes
                .get_instance(Attribute::BLOCK_INTERACTION_RANGE)
                .unwrap()
                .set_base_value(f[3].parse().unwrap());
            start_open(receiver, player, &mut level).unwrap();
        } else {
            stop_open(receiver, player, &mut level).unwrap();
        }
        let chest = level.block_entity::<ChestBlockEntity>(receiver).unwrap();
        assert_eq!(chest.open_count, f[4].parse::<i32>().unwrap(), "{row}");
        assert_eq!(
            chest.max_interaction_range.to_bits(),
            f[5].parse::<f64>().unwrap().to_bits(),
            "{row}"
        );
        level.run_block_events().unwrap();
        let events: Vec<_> = level
            .take_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                crate::level::effects::LevelEffect::PlayerPacket { packet, .. } => match *packet {
                    Packet::BlockEvent(event) => Some(format!("event:{}:{}", event.b0, event.b1)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert_eq!(events.join(";"), f[6], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 6);
}
