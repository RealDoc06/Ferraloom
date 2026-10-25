#[cfg(test)]
mod tests {
    use crate::entity::player::GameType;
    use crate::{
        Behaviors,
        entity::{EntityRef, EntityState, player::ServerPlayer},
        level::{Level, LevelContext, Region},
    };
    use ferraloom_data::Item;
    use ferraloom_nbt::{Tag, compare_nbt};
    use ferraloom_protocol::component::ItemStack;
    use ferraloom_util::{Uuid, Vec3};
    #[derive(serde::Deserialize)]
    struct Vector {
        count: i32,
        pattern: String,
        inverted: bool,
        matches: bool,
    }
    #[test]
    fn selected_stack_and_player_metadata_match_java_selectors() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("../../vanilla/player-nbt-vectors.json")).unwrap();
        for vector in vectors {
            let mut player = ServerPlayer::new(
                Uuid::NIL,
                "SelectorPlayer".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            );
            if vector.count > 0 {
                player.inventory.set_selected_item(ItemStack::new(
                    Item::STONE,
                    vector.count,
                    Default::default(),
                ));
            }
            static BEHAVIORS: Behaviors = Behaviors::EMPTY;
            let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
            crate::entity::tests::install_test_environment(
                &mut level,
                false,
                "minecraft:overworld",
            );
            let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
            level
                .add_entity(entity, EntityState::Player(Box::new(player)))
                .unwrap();
            let actual = Tag::Compound(level.entity_nbt_to_compare(entity).unwrap());
            let predicate = super::super::SelectorPredicate::Nbt {
                value: ferraloom_nbt::parse(&vector.pattern).unwrap(),
                inverted: vector.inverted,
            };
            assert_eq!(
                super::super::live_entity_matches(
                    &level,
                    &crate::scoreboard::Scoreboard::default(),
                    &Default::default(),
                    entity,
                    &predicate
                )
                .unwrap(),
                vector.matches
            );
            let pattern = ferraloom_nbt::parse(&vector.pattern).unwrap();
            assert_eq!(
                compare_nbt(Some(&pattern), Some(&actual), true) != vector.inverted,
                vector.matches,
                "{}, count {}, inverted {}",
                vector.pattern,
                vector.count,
                vector.inverted
            );
        }
    }
}
