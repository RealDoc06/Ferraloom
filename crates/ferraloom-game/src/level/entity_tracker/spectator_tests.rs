use super::*;
use crate::{
    Behaviors,
    entity::player::GameType,
    gamerules::{GameRule, GameRuleValue, GameRules},
    level::{
        LevelContext, Region,
        acquisition::{
            AcquireDriver, AcquireHandle, AcquireProgress, AcquireResult, ChunkReference,
        },
        chunk_holder::ChunkStatus,
    },
};
use ferraloom_util::{ChunkPos, Uuid, Vec3};
use std::cell::Cell;

/// Observe only the synchronous ticket receiver; unrelated acquisition is forbidden.
#[derive(Default)]
struct TrackingDriver(Cell<Option<SectionPos>>);
impl AcquireDriver for TrackingDriver {
    fn update_player_tracking_status(
        &self,
        _: EntityRef,
        section: Option<SectionPos>,
    ) -> Result<(), LevelError> {
        self.0.set(section);
        Ok(())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn is_owner_thread(&self) -> bool {
        true
    }
    fn visible_ticket_level(&self, _: ChunkPos) -> Option<i32> {
        unreachable!()
    }
    fn add_unknown_ticket(&self, _: ChunkPos, _: ChunkStatus) -> Result<(), LevelError> {
        unreachable!()
    }
    fn run_distance_manager_updates(&self, _: &Level) -> Result<bool, LevelError> {
        unreachable!()
    }
    fn request_status(
        &self,
        _: &Level,
        _: ChunkPos,
        _: ChunkStatus,
    ) -> Result<AcquireHandle, LevelError> {
        unreachable!()
    }
    fn acquisition_result(&self, _: AcquireHandle) -> Result<AcquireResult, LevelError> {
        unreachable!()
    }
    fn poll_acquisition_step(&self, _: &Level) -> Result<AcquireProgress, LevelError> {
        unreachable!()
    }
    fn wait_for_progress(&self) -> Result<(), LevelError> {
        unreachable!()
    }
    fn chunk_if_present(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        unreachable!()
    }
    fn chunk_if_present_unchecked(&self, _: ChunkPos, _: ChunkStatus) -> Option<ChunkReference> {
        unreachable!()
    }
    fn acquire_on_owner(
        &self,
        _: ChunkPos,
        _: ChunkStatus,
        _: bool,
    ) -> Result<Option<ChunkReference>, LevelError> {
        unreachable!()
    }
}

#[test]
fn native_spectator_rule_and_same_section_mode_changes_update_only_generation_tickets() {
    let mut level = Level::new(Region::new(&Behaviors::EMPTY), LevelContext::fixture(0, 63));
    let mut rules = GameRules::default();
    assert_eq!(
        level.rules().spectators_generate_chunks,
        rules.boolean(GameRule::SpectatorsGenerateChunks)
    );
    level.rules_mut().spectators_generate_chunks = false;
    let player = level.next_entity_ref();
    level
        .add_entity(
            player,
            EntityState::Player(Box::new(ServerPlayer::new(
                Uuid::new(1, 2),
                "spectator".into(),
                7,
                Vec3::new(0.0, 64.0, 0.0),
                GameType::Spectator,
            ))),
        )
        .unwrap();
    level.remove_tracked_entity(player).unwrap();
    level
        .install_acquire_driver(Box::<TrackingDriver>::default(), 0..256)
        .unwrap();
    let mut count = 0;
    for row in include_str!("../../../tests/java/entity/tracker/vectors.tsv")
        .lines()
        .filter_map(|row| row.strip_prefix("spectator\t"))
    {
        let fields: Vec<_> = row.split('\t').collect();
        let input: Vec<_> = fields[0].split(',').collect();
        rules
            .set(
                GameRule::SpectatorsGenerateChunks,
                GameRuleValue::Boolean(input[2].parse().unwrap()),
            )
            .unwrap();
        rules.update_level_rules(level.rules_mut());
        let section = SectionPos::new(
            input[3].parse().unwrap(),
            input[4].parse().unwrap(),
            input[5].parse().unwrap(),
        );
        let state = level.entity_mut::<ServerPlayer>(player).unwrap();
        state.game_mode = match input[1] {
            "creative" => GameType::Creative,
            "survival" => GameType::Survival,
            "spectator" => GameType::Spectator,
            _ => unreachable!(),
        };
        state.set_pos(Vec3::new(
            f64::from(section.x * 16 + 1),
            f64::from(section.y * 16 + 1),
            f64::from(section.z * 16 + 1),
        ));
        match input[0] {
            "add" => level.add_tracked_entity(player).unwrap(),
            "move" => {
                level.move_tracked_player(player).unwrap();
                level
                    .update_player_tracking_section(player, section)
                    .unwrap();
                level.update_player_chunk_tracking(player).unwrap();
            }
            "remove" => level.remove_tracked_entity(player).unwrap(),
            _ => unreachable!(),
        }
        assert_eq!(
            level
                .player_ignored_by_chunk_map(player)
                .unwrap()
                .to_string(),
            fields[1],
            "{row}"
        );
        let active = level
            .acquire_driver()
            .unwrap()
            .as_any()
            .downcast_ref::<TrackingDriver>()
            .unwrap()
            .0
            .get();
        assert_eq!(
            active.map_or_else(
                || "-".into(),
                |pos| format!("{},{},{}", pos.x, pos.y, pos.z)
            ),
            fields[2],
            "{row}"
        );
        if input[0] != "remove" {
            let state = level.entity::<ServerPlayer>(player).unwrap();
            assert_eq!(state.chunk_tracking_section, Some(section));
            assert!(!state.chunk_tracking_view.is_empty(), "{row}");
        }
        count += 1;
    }
    assert_eq!(count, 16);
}

#[test]
fn spectator_movement_skips_fall_reads_only_when_touching_unloaded_chunks() {
    use crate::entity::player::{
        MovementChecks, MovementResult, PlayerMovement, move_player_with_checks,
    };
    use crate::level::tick_rate::TickRateManager;

    for loaded in [false, true] {
        let mut level = if loaded {
            crate::entity::tests::level(false)
        } else {
            Level::new(Region::new(&Behaviors::EMPTY), LevelContext::fixture(0, 63))
        };
        let player = level.next_entity_ref();
        let mut state = ServerPlayer::new(
            Uuid::new(1, 2),
            "spectator".into(),
            7,
            Vec3::new(8.5, 80.0, 10.5),
            GameType::Spectator,
        );
        state.no_physics = true;
        state.fall_distance = 3.25;
        level
            .add_entity(player, EntityState::Player(Box::new(state)))
            .unwrap();
        let target = Vec3::new(8.5, 79.0, 10.5);
        let tick_rate = TickRateManager::default();
        assert_eq!(
            move_player_with_checks(
                &mut level,
                player,
                PlayerMovement {
                    position: target,
                    yaw: 0.0,
                    pitch: 0.0,
                    on_ground: false,
                    horizontal_collision: false,
                },
                &MovementChecks::dedicated(&tick_rate),
            )
            .unwrap(),
            MovementResult::Accepted,
        );
        let state = level.entity::<ServerPlayer>(player).unwrap();
        assert_eq!(state.position, target);
        assert_eq!(state.fall_distance, if loaded { 4.25 } else { 3.25 });
    }
}
