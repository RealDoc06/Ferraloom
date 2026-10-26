use super::*;
use crate::{
    Behaviors,
    entity::{
        player::{GameType, ServerPlayer},
        synched_data,
    },
    level::Region,
    scoreboard::{Scoreboard, ServerScoreboard, TeamColor},
};
use ferraloom_data::Item;
use ferraloom_protocol::{
    chat::ClickEvent,
    component::{DataComponent, DataComponentPatch, ItemStack},
};
use ferraloom_util::{Uuid, Vec3};

fn action_name() -> Component {
    let mut name = Component::literal("named");
    name.style.bold = Some(true);
    name.style.insertion = Some("custom".into());
    name.style.click_event = Some(ClickEvent::RunCommand("/root".into()));
    let mut child = Component::literal("child");
    child.style.click_event = Some(ClickEvent::SuggestCommand("/child".into()));
    name.extra.push(child);
    name
}

#[test]
fn falling_block_names_and_hover_use_native_carried_block_descriptions() {
    use crate::entity::item::FallingBlockEntity;
    use ferraloom_data::BlockState;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut checked = 0;
    for row in include_str!("../../../tests/java/entity/falling_name/vectors.tsv").lines() {
        let fields: Vec<_> = row.split('\t').collect();
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        let mut falling = FallingBlockEntity::new(1, 0);
        falling.block_state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
        falling.parent.uuid = Uuid::new(1, 2);
        if fields[2] == "true" {
            falling.parent.entity_data.set(
                synched_data::CUSTOM_NAME,
                Some(Box::new(Component::literal("custom falling"))),
            );
        }
        let entity = level.next_entity_ref();
        level
            .add_entity(entity, EntityState::FallingBlock(Box::new(falling)))
            .unwrap();
        assert_eq!(
            level.entity_name(entity).unwrap(),
            Component::from_json(fields[3]).unwrap(),
            "name {row}"
        );
        assert_eq!(
            level
                .entity_display_name_with_scoreboard(entity, &Scoreboard::default())
                .unwrap(),
            Component::from_json(fields[4]).unwrap(),
            "display {row}"
        );
        checked += 1;
    }
    assert_eq!(checked, 14);
}

#[test]
fn virtual_names_team_formatting_and_hover_decorations_match_native_components() {
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    let mut count = 0;
    for row in include_str!("../../../tests/java/entity/vectors.tsv")
        .lines()
        .filter(|row| row.starts_with("entity-name\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let scenario = fields[1];
        let mut level = {
            let mut fixture_level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        let mut state = if scenario.starts_with("player") {
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(0, 1),
                "metadata".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            )))
        } else if scenario.starts_with("item") {
            let mut patch = DataComponentPatch::default();
            if scenario == "item-stack-custom" {
                patch.set(DataComponent::CustomName(Box::new(action_name())));
            }
            let mut item = ItemEntity::new(1, 0);
            item.set_item(ItemStack::new(
                Item::STONE,
                if scenario == "item-zero" { 0 } else { 3 },
                patch,
            ));
            EntityState::Item(Box::new(item))
        } else {
            EntityState::Cow(Box::default())
        };
        state.as_entity_mut().uuid = Uuid::new(0, 1);
        if scenario.ends_with("-custom") && scenario != "item-stack-custom" {
            state
                .as_entity_mut()
                .entity_data
                .set(synched_data::CUSTOM_NAME, Some(Box::new(action_name())));
        }
        let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
        level.add_entity(entity, state).unwrap();
        let shared = level.take_shared().unwrap();
        assert_eq!(
            level.entity_display_name(entity),
            Err(LevelError::MissingServerShared)
        );
        level.install_shared(shared);
        let before = level.entity_display_name(entity).unwrap();
        if fields[2] == "true" {
            let mut scoreboard = Scoreboard::default();
            let team = scoreboard.add_player_team("names");
            team.prefix = Component::literal("<");
            team.prefix.style.bold = Some(true);
            team.suffix = Component::literal(">");
            team.suffix.style.italic = Some(false);
            team.color = Some(TeamColor::Red);
            scoreboard
                .add_player_to_team(&level.entity_scoreboard_name(entity).unwrap(), "names")
                .unwrap();
            let current = level
                .entity_display_name_with_scoreboard(entity, &scoreboard)
                .unwrap();
            assert_eq!(
                current,
                Component::from_json(fields[5]).unwrap(),
                "updated team reader"
            );
            assert_eq!(
                level.entity_display_name(entity).unwrap(),
                before,
                "explicit reader does not replace the canonical scoreboard"
            );
            level.shared_mut().unwrap().scoreboard = ServerScoreboard::new(scoreboard);
        }
        assert_eq!(level.entity_scoreboard_name(entity).unwrap(), fields[3]);
        assert_eq!(
            level.entity_name(entity).unwrap(),
            Component::from_json(fields[4]).unwrap(),
            "{scenario} name"
        );
        assert_eq!(
            level.entity_display_name(entity).unwrap(),
            Component::from_json(fields[5]).unwrap(),
            "{scenario} team {} display",
            fields[2]
        );
        count += 1;
    }
    assert_eq!(count, 16);
}
