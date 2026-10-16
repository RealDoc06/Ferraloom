#[allow(clippy::wildcard_imports)]
use super::*;
use pumpkin_data::game_rules::GameRule;
use pumpkin_protocol::java::client::play::CGameRuleValues;

impl JavaClient {
    pub(crate) fn handle_client_status(
        &self,
        player: &Arc<Player>,
        client_status: &SClientCommand,
    ) -> Option<crate::net::java::DeferredPacket> {
        player.update_last_action_time();
        match client_status.action_id.0 {
            SClientCommand::PERFORM_RESPAWN => {
                // Perform respawn
                if player.living_entity.health.load() > 0.0 {
                    return None;
                }
                let player_c = player.clone();
                let server = player.world().server.upgrade()?;
                let hardcore = server.basic_config.hardcore;
                return Some(Box::pin(async move {
                    if player_c.world().respawn_player(&player_c, false).await
                        != crate::world::RespawnOutcome::Committed
                    {
                        return;
                    }
                    if hardcore {
                        player_c.set_gamemode(GameMode::Spectator);
                    }

                    {
                        let screen_handler = player_c
                            .current_screen_handler
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let mut screen_handler = screen_handler
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        screen_handler.sync_state();
                    };

                    // Restore abilities based on gamemode after respawn
                    {
                        let mut abilities = player_c
                            .abilities
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        abilities.set_for_gamemode(player_c.gamemode.load());
                    };
                    player_c.send_abilities_update();
                }));
            }
            SClientCommand::REQUEST_STATS => {
                // Request stats
                player.send_stats();
            }
            SClientCommand::REQUEST_GAMERULE_VALUES => {
                self.send_game_rule_values(player);
            }
            _ => {
                self.try_kick(&TextComponent::text("Invalid client status"));
            }
        }
        None
    }

    pub fn send_game_rule_values(&self, player: &Player) {
        if player.permission_lvl.load() < PermissionLvl::Two {
            warn!(
                "Player {} tried to request game rule values without required permissions",
                player.gameprofile.name
            );
            return;
        }

        let world = player.world();
        let level_info = world.level_info.load();
        let minecart_improvements_enabled = world.server.upgrade().map_or_else(
            || {
                level_info
                    .data_packs
                    .enabled
                    .iter()
                    .any(|p| p == "minecart_improvements" || p == "file/minecart_improvements")
            },
            |s| s.is_feature_enabled("minecraft:minecart_improvements"),
        );

        let rules: Vec<(String, String)> = GameRule::all()
            .iter()
            .filter(|rule| match rule {
                GameRule::MaxMinecartSpeed => minecart_improvements_enabled,
                _ => true,
            })
            .map(|rule| {
                (
                    rule.to_string(),
                    level_info.game_rules.get(rule).to_string(),
                )
            })
            .collect();
        let rules_ref: Vec<(&str, &str)> = rules
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        self.try_send_packet(&CGameRuleValues::new(&rules_ref));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::player_server::test::{
        close_fixture, player_fixture_with_hardcore, stop_chunk_workers,
    };
    use crate::entity::NBTStorage;
    use crate::server::Server;
    use pumpkin_nbt::NbtCompound;
    use pumpkin_util::math::{vector2::Vector2, vector3::Vector3};
    use pumpkin_world::{chunk::ChunkData, chunk_system::ChunkLoading, level::ChunkUnavailable};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn refused_hardcore_respawn_owned_tail_preserves_mode_abilities_tickets_and_save() {
        hardcore_respawn_control(true).await;
    }

    #[tokio::test]
    async fn committed_hardcore_respawn_owned_tail_sets_spectator_and_saves_it() {
        hardcore_respawn_control(false).await;
    }

    /// Poll the same production runner through both the parent and descendant drain.
    async fn drive_owned<F: Future>(server: &Arc<Server>, work: F) -> F::Output {
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::pin!(work);
            loop {
                if let std::task::Poll::Ready(value) = futures::poll!(&mut work) {
                    return value;
                }
                let server = server.clone();
                tokio::task::spawn_blocking(move || server.packet_runner.run_ready(&server, None))
                    .await
                    .unwrap();
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }

    #[expect(
        clippy::too_many_lines,
        reason = "Keep paired real-handler controls with ready source, owned continuation and final player.dat assertions."
    )]
    async fn hardcore_respawn_control(refuse_destination: bool) {
        let fixture = player_fixture_with_hardcore(true).await;
        let player = &fixture.player;
        let world = player.world();
        let destination = Vector2::new(16, 16);
        let path = world.level.level_folder.region_folder.join("r.0.0.mca");
        if refuse_destination {
            std::fs::write(&path, b"refused hardcore respawn region").unwrap();
            let refusal = tokio::time::timeout(
                Duration::from_secs(10),
                world.level.get_or_fetch_chunk(destination, |_| ()),
            )
            .await
            .unwrap()
            .unwrap_err();
            assert!(matches!(refusal, ChunkUnavailable::ReadFailed { .. }));
        }
        stop_chunk_workers(&fixture).await;
        let source = Vector2::new(0, 0);
        let terrain = ChunkData::empty_sync(0, 0);
        terrain.post_processing_ready.store(true, Ordering::Release);
        world.level.loaded_chunks.insert(source, terrain);
        {
            let mut loading = world.level.chunk_loading.lock().unwrap();
            loading.add_ticket(source, ChunkLoading::FULL_CHUNK_LEVEL);
            loading.send_change();
        }
        world.level_info.rcu(|current| {
            let mut info = (**current).clone();
            info.spawn_x = if refuse_destination { 256 } else { 0 };
            info.spawn_z = if refuse_destination { 256 } else { 0 };
            info.spawn_y = 65;
            info.game_rules.spectators_generate_chunks = false;
            info
        });
        player.get_entity().set_pos(Vector3::new(0.5, 65.0, 0.5));
        player.get_entity().last_pos.store(player.position());
        player.living_entity.health.store(0.0);
        player.config.rcu(|current| {
            let mut config = (**current).clone();
            config.view_distance = std::num::NonZero::new(2).unwrap();
            config
        });
        // A non-default field catches an unconditional ability reset as well as a mode change.
        player.abilities.lock().unwrap().allow_flying = true;
        let mut abilities_before = NbtCompound::new();
        player
            .abilities
            .lock()
            .unwrap()
            .write_nbt(&mut abilities_before);
        let tickets_before = world.level.chunk_loading.lock().unwrap().ticket.clone();
        fixture
            .server
            .player_data_storage
            .save_all_players(&fixture.server)
            .unwrap();
        let uuid = player.gameprofile.id;
        let saved_before = fixture
            .server
            .player_data_storage
            .load_data(&uuid)
            .unwrap()
            .unwrap();
        assert_eq!(
            saved_before.get_int("playerGameType"),
            Some(GameMode::Survival as i32)
        );
        let java = fixture.client.java().unwrap();
        java.set_player(player);
        assert!(world.enter_packet_callbacks(Some(source)).is_some());
        let activity = player.packet_activity.enter_mutation().unwrap();
        let deferred = activity
            .clone()
            .sync_scope(|| {
                java.handle_client_status(
                    player,
                    &SClientCommand::new(SClientCommand::PERFORM_RESPAWN.into()),
                )
            })
            .unwrap();
        let completed = Arc::new(AtomicBool::new(false));
        let completed_tail = completed.clone();
        let task = java.spawn_activity_task(activity, async move {
            deferred.await;
            // This accepted parent must still run with a ready source after client close.
            completed_tail.store(true, Ordering::Release);
        });
        drive_owned(&fixture.server, task).await.unwrap();
        assert!(completed.load(Ordering::Acquire));
        assert!(world.enter_packet_callbacks(Some(source)).is_some());
        let mut abilities_after = NbtCompound::new();
        player
            .abilities
            .lock()
            .unwrap()
            .write_nbt(&mut abilities_after);
        if refuse_destination {
            assert!(java.is_closed());
            assert_eq!(player.gamemode.load(), GameMode::Survival);
            assert_eq!(player.living_entity.health.load(), 0.0);
            assert_eq!(abilities_after, abilities_before);
            assert_eq!(
                world.level.chunk_loading.lock().unwrap().ticket,
                tickets_before
            );
        } else {
            assert!(!java.is_closed());
            assert_eq!(player.gamemode.load(), GameMode::Spectator);
            assert!(player.living_entity.health.load() > 0.0);
            let abilities = player.abilities.lock().unwrap();
            assert!(abilities.flying && abilities.allow_flying && abilities.invulnerable);
            assert!(!abilities.allow_modify_world);
        }
        drive_owned(
            &fixture.server,
            crate::finish_player_session(&fixture.server, player),
        )
        .await
        .unwrap();
        let saved_after = fixture
            .server
            .player_data_storage
            .load_data(&uuid)
            .unwrap()
            .unwrap();
        assert_eq!(
            saved_after.get_int("playerGameType"),
            Some(player.gamemode.load() as i32)
        );
        assert_eq!(
            saved_after.get_compound("abilities"),
            abilities_after.get_compound("abilities")
        );
        if refuse_destination {
            assert_eq!(
                saved_after.get_int("playerGameType"),
                saved_before.get_int("playerGameType")
            );
            assert_eq!(
                saved_after.get_compound("abilities"),
                saved_before.get_compound("abilities")
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                b"refused hardcore respawn region"
            );
        }
        close_fixture(&fixture).await;
    }
}
