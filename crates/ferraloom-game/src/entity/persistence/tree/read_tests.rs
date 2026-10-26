use super::*;
use crate::{
    Behaviors,
    entity::{
        RemovalReason,
        animal::Cow,
        item::{ItemEntity, PrimedTnt},
        player::{GameType, ServerPlayer},
    },
    level::Region,
};
use ferraloom_data::Item;
use ferraloom_protocol::component::ItemStack;
use ferraloom_util::Uuid;

#[test]
fn comparison_nbt_reads_subclass_data_selected_item_and_ineligible_objects() {
    let mut count = 0;
    for row in include_str!("../../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|row| row.starts_with("entity-nbt\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let scenario = fields[1];
        let mut level = crate::entity::tests::persistence_level();
        let mut state = if scenario.starts_with("player") {
            let mut player = ServerPlayer::new(
                Uuid::new(0, 1),
                "metadata".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            );
            if scenario != "player-empty" {
                player.inventory.set_selected_item(ItemStack::new(
                    Item::STONE,
                    if scenario == "player-zero" { 0 } else { 3 },
                    Default::default(),
                ));
            }
            EntityState::Player(Box::new(player))
        } else if scenario.starts_with("item") {
            let mut item = ItemEntity::new(1, 0);
            item.set_item(ItemStack::new(Item::STONE, 3, Default::default()));
            item.pickup_delay = 7;
            EntityState::Item(Box::new(item))
        } else {
            let mut cow = Cow::default();
            cow.as_living_mut().set_health(7.5);
            cow.parent.parent.parent.age = -17;
            EntityState::Cow(Box::new(cow))
        };
        let base = state.as_entity_mut();
        base.uuid = Uuid::new(0, 1);
        base.set_pos(Vec3::new(1.0, 64.0, 2.0));
        base.set_y_rot(0.0);
        base.set_x_rot(0.0);
        base.velocity = Vec3::ZERO;
        let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        level.add_entity(entity, state).unwrap();
        if scenario == "item-riding" {
            let mut vehicle = PrimedTnt::new(2, 0);
            vehicle.parent.uuid = Uuid::new(0, 2);
            vehicle.parent.set_pos(Vec3::new(10.0, 70.0, -20.0));
            level
                .add_entity(
                    EntityRef::fixture(crate::level::RegionId::FIXTURE, 2),
                    EntityState::PrimedTnt(Box::new(vehicle)),
                )
                .unwrap();
            level
                .entity::<EntityData>(entity)
                .unwrap()
                .vehicle
                .set(Some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 2)));
        } else if scenario == "item-removed" {
            level
                .remove_entity_with_reason(entity, RemovalReason::Discarded)
                .unwrap();
        }
        let actual = level.entity_nbt_to_compare(entity).unwrap();
        assert_eq!(actual.contains_key("id"), fields[2] == "true");
        assert_eq!(actual.contains_key("DataVersion"), fields[3] == "true");
        let expected = ferraloom_nbt::parse_compound(fields[4]).unwrap();
        for (key, expected) in expected.iter() {
            assert_eq!(
                actual.get(&key.to_string_lossy()),
                Some(expected),
                "{scenario}: {key:?}"
            );
        }
        assert_eq!(
            actual.contains_key("SelectedItem"),
            scenario == "player-selected"
        );
        if scenario == "item-riding" || scenario == "item-removed" || scenario.starts_with("player")
        {
            assert!(level.save_entity(entity).unwrap().is_none());
        }
        count += 1;
    }
    assert_eq!(count, 7);
}

#[test]
fn player_save_and_comparison_nbt_require_the_current_dimension_instead_of_saved_dimension() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut level = Level::new(
        Region::new(&BEHAVIORS),
        crate::level::LevelContext::fixture(0, 63),
    );
    level.install_shared(Box::default());
    let entity = level.next_entity_ref();
    let mut player = ServerPlayer::new(
        Uuid::new(9, 10),
        "dimension".into(),
        level.next_entity_id().unwrap(),
        Vec3::ZERO,
        GameType::Survival,
    );
    player
        .saved
        .insert("Dimension", Tag::String("stale:dimension".into()));
    level
        .add_entity(entity, EntityState::Player(Box::new(player)))
        .unwrap();
    assert_eq!(
        level.entity::<ServerPlayer>(entity).unwrap().save(&level),
        Err(LevelError::MissingEnvironmentContext),
    );
    assert_eq!(
        level.entity_nbt_to_compare(entity),
        Err(LevelError::MissingEnvironmentContext),
    );
    for dimension in [
        "minecraft:overworld",
        "minecraft:the_nether",
        "minecraft:the_end",
        "test:custom",
    ] {
        crate::entity::tests::install_test_environment(&mut level, false, dimension);
        let saved = level
            .entity::<ServerPlayer>(entity)
            .unwrap()
            .save(&level)
            .unwrap();
        assert_eq!(
            saved.get_string("Dimension").unwrap().to_string_lossy(),
            dimension
        );
        assert_eq!(level.entity_nbt_to_compare(entity).unwrap(), saved);
    }
}
