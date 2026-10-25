//! Complete per-dimension ServerLevel ticks, driven in server insertion order.
use super::*;

impl MinecraftServer {
    /// Port of one complete ServerLevel#tick, including its exclusive chunk
    /// continuations. Shared services return before the next dimension begins.
    pub(super) fn tick_level(
        &mut self,
        dimension: &protocol::Identifier,
        paused: bool,
    ) -> Result<()> {
        let lent = self.lend_server_shared_to(dimension)?;
        let result = self.tick_level_inner(dimension, paused);
        self.return_server_shared(lent);
        result
    }
    fn tick_level_inner(&mut self, dimension: &protocol::Identifier, paused: bool) -> Result<()> {
        if paused {
            return self.integrate_jobs_in(dimension);
        }
        let runs = self.tick_rate_manager.runs_normally();
        self.live_level_in_mut(dimension)?
            .level
            .set_handling_tick(true);
        let clocks = command_runtime::environment_clock_snapshot(&self.clocks);
        self.live_level_in_mut(dimension)?
            .level
            .refresh_environment_attributes(clocks)
            .map_err(game_error)?;
        if runs {
            self.live_level_in_mut(dimension)?.level.tick_world_border();
            self.live_level_in_mut(dimension)?
                .level
                .advance_weather_cycle(true)
                .map_err(game_error)?;
            self.apply_level_effects_in(dimension)?;
        }
        self.live_level_in_mut(dimension)?
            .level
            .update_sky_brightness()
            .map_err(game_error)?;
        if runs {
            if dimension.as_str() == "minecraft:overworld" {
                // Only primary data advances tickTime. Every other value is an
                // input published from it before a callback can read that level.
                let game_time = self.level.game_time.wrapping_add(1);
                for live in self.live_levels_mut() {
                    live.game_time = game_time;
                    live.level.set_game_time(game_time);
                }
            }
            let live = self.live_level_in_mut(dimension)?;
            if !live.level.rules().debug {
                live.level
                    .tick_block_phase(live.game_time, 65536)
                    .map_err(game_error)?;
                live.level
                    .tick_fluid_phase(live.game_time, 65536)
                    .map_err(game_error)?;
            }
            self.live_level_in(dimension)?
                .generation()
                .purge_stale_tickets(self.tick);
        }
        self.integrate_jobs_in(dimension)?;
        self.update_chunk_tickets_in(dimension)?;
        if !self.live_level_in(dimension)?.level.rules().debug {
            if runs {
                self.tick_live_chunks_in(dimension)?;
            }
            self.broadcast_block_changes_in(dimension)?;
        }
        self.track_players_in(dimension)?;
        self.live_level_in_mut(dimension)?
            .level
            .tick_entity_tracker()
            .map_err(anyhow::Error::new)?;
        self.apply_level_effects_in(dimension)?;
        if runs {
            self.live_level_in_mut(dimension)?
                .level
                .run_block_events()
                .map_err(game_error)?;
        }
        self.live_level_in_mut(dimension)?
            .level
            .set_handling_tick(false);
        let active = self
            .live_level_in(dimension)?
            .generation()
            .with_tickets(|tickets| tickets.should_keep_dimension_active());
        if self
            .live_level_in_mut(dimension)?
            .level
            .update_empty_time(active, runs)
        {
            // Borrow the concrete level and the sole tick-rate manager disjointly.
            let live = std::iter::once(&mut self.level)
                .chain(self.additional_levels.iter_mut())
                .find(|live| live.dimension == dimension.as_str())
                .expect("selected dimension");
            live.level
                .tick_entities_with_tick_rate(&self.tick_rate_manager)
                .map_err(game_error)?;
            self.broadcast_container_changes_in(dimension)?;
            self.live_level_in_mut(dimension)?
                .level
                .tick_block_entities(runs)
                .map_err(game_error)?;
        }
        self.apply_level_effects_in(dimension)?;
        let live = self.live_level_in(dimension)?;
        live.generation().poll_entity_io(&live.level)?;
        self.live_level_in_mut(dimension)?
            .level
            .tick_entity_management()?;
        let requests = self
            .live_level_in(dimension)?
            .level
            .take_entity_storage_requests();
        self.live_level_in(dimension)?
            .generation()
            .with_world_mut(|world| world.submit_entity_requests(requests, &self.properties))?;
        self.live_level_in_mut(dimension)?
            .level
            .collect_detached_block_entities();
        if self.tick.is_multiple_of(1200) {
            let live = self.live_level_in(dimension)?;
            eprintln!(
                "Live level {}: unported={:?}, pending={:?}, precipitation={}",
                dimension,
                live.level.unported_behavior_stats(),
                self.pending_level_hooks,
                live.level.pending_precipitation_calls()
            );
        }
        Ok(())
    }
    pub(super) fn request_area_in(
        &mut self,
        dimension: &protocol::Identifier,
        center: ChunkPos,
        radius: i32,
    ) -> Result<()> {
        ensure!(radius <= 34, "chunk request radius too large");
        let mut chunks = Vec::new();
        for x in center.x - radius..=center.x + radius {
            for z in center.z - radius..=center.z + radius {
                let pos = ChunkPos::new(x, z);
                let dx = x - center.x;
                let dz = z - center.z;
                chunks.push((dx * dx + dz * dz, pos));
            }
        }
        chunks.sort_by_key(|(distance, pos)| (*distance, pos.pack()));
        for (_, pos) in chunks {
            self.live_level_in_mut(dimension)?
                .request_full(pos)
                .map_err(game_error)?;
        }
        Ok(())
    }
    /// Port of ServerChunkCache's spawning shuffle and random-tick slots.
    pub(super) fn tick_live_chunks_in(&mut self, dimension: &protocol::Identifier) -> Result<()> {
        self.pending_level_hooks.natural_spawn_state += 1;
        let mut spawning = Vec::new();
        for pos in self
            .live_level_in(dimension)?
            .generation()
            .natural_spawn_chunks()
        {
            if !self
                .live_level_in(dimension)?
                .level
                .chunk(pos)
                .is_some_and(|chunk| chunk.ticking_ready())
            {
                continue;
            }
            let mut close = false;
            for connection in self.player_connections_in(dimension)? {
                let c = &self.connections[&connection];
                let Phase::Play(entity) = c.phase else {
                    continue;
                };
                let player = self.player(entity)?;
                if player.game_mode != GameType::Spectator {
                    let dx = f64::from(pos.x * 16 + 8) - player.position.x;
                    let dz = f64::from(pos.z * 16 + 8) - player.position.z;
                    if dx * dx + dz * dz < 16384.0 {
                        close = true;
                        break;
                    }
                }
            }
            if close {
                spawning.push(pos);
            }
        }
        self.live_level_in_mut(dimension)?
            .level
            .shuffle_spawning_chunks(&mut spawning);
        for pos in spawning {
            self.live_level_in_mut(dimension)?
                .level
                .increment_chunk_inhabited_time(pos)
                .map_err(game_error)?;
            if self
                .live_level_in(dimension)?
                .level
                .is_entity_ticking_chunk(pos)
            {
                self.live_level_in_mut(dimension)?
                    .level
                    .tick_thunder(pos)
                    .map_err(game_error)?;
            }
            // SPAWN_MOBS defaults true until the commands GameRules adapter lands.
            self.pending_level_hooks.natural_spawn_chunks += 1;
        }
        let live = self.live_level_in(dimension)?;
        let ticking: Vec<_> = live
            .level
            .entity_ticking_chunks()
            .into_iter()
            .filter(|&pos| {
                live.level
                    .chunk(pos)
                    .is_some_and(|chunk| chunk.ticking_ready())
            })
            .collect();
        for pos in ticking {
            self.live_level_in_mut(dimension)?
                .level
                .tick_random_chunk(pos)
                .map_err(game_error)?;
        }
        if dimension.as_str() == "minecraft:overworld" {
            self.pending_level_hooks.custom_spawners += 1;
        }
        Ok(())
    }
    /// Broadcast precedes block events; their later changes wait one tick.
    pub(super) fn broadcast_block_changes_in(
        &mut self,
        dimension: &protocol::Identifier,
    ) -> Result<()> {
        let mut positions = self
            .live_level_in_mut(dimension)?
            .level
            .take_block_update_chunks();
        let mut seen: HashSet<_> = positions.iter().copied().collect();
        while let Some(key) = self
            .live_level_in(dimension)?
            .generation()
            .with_world_mut(|world| world.light_chunks_to_broadcast.pop_first())
        {
            let pos = ChunkPos::unpack(key);
            if seen.insert(pos) {
                positions.push(pos);
            }
        }
        for pos in positions {
            if !self
                .live_level_in(dimension)?
                .level
                .chunk(pos)
                .is_some_and(|chunk| chunk.ticking_ready())
            {
                continue;
            }
            let packets = self
                .live_level_in_mut(dimension)?
                .level
                .take_block_updates(pos)
                .map_err(game_error)?;
            let light = self
                .live_level_in(dimension)?
                .generation()
                .with_world_mut(|world| {
                    world.pending_light_updates.remove(&pos).map(|mask| {
                        cb::Packet::LightUpdate(cb::LightUpdate {
                            x: VarInt(pos.x),
                            z: VarInt(pos.z),
                            light_data: light_packet_with_masks(
                                world.lights.get(&pos.pack()),
                                mask.sky,
                                mask.block,
                            ),
                        })
                    })
                });
            for connection in self.player_connections_in(dimension)? {
                let Phase::Play(entity) = self.connections[&connection].phase else {
                    continue;
                };
                let sent = self.player(entity)?.chunk_sender.sent.contains(&pos);
                let c = self
                    .connections
                    .get_mut(&connection)
                    .expect("selected connection");
                if !c.close && sent {
                    if let Some(light) = &light {
                        c.push(light.clone());
                    }
                    for packet in &packets {
                        c.push(packet.clone());
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn apply_level_effects_in(
        &mut self,
        dimension: &protocol::Identifier,
    ) -> Result<()> {
        use ferraloom_game::level::effects::LevelEffect;
        let mut lighting = Vec::new();
        for effect in self.live_level_in_mut(dimension)?.level.take_effects() {
            match effect {
                effect @ (LevelEffect::CheckLight(_) | LevelEffect::SectionEmptiness { .. }) => {
                    lighting.push(effect)
                }
                LevelEffect::WeatherEvent { event, param } => self.broadcast_in_level(
                    dimension,
                    cb::Packet::GameEvent(cb::GameEvent { event, param }),
                )?,
                LevelEffect::LevelEvent { pos, id, data } => self.broadcast_in_level(
                    dimension,
                    cb::Packet::LevelEvent(cb::LevelEvent {
                        event_type: id,
                        pos,
                        data,
                        global_event: false,
                    }),
                )?,
                LevelEffect::PoiChanged { .. } => self.pending_level_hooks.poi += 1,
                LevelEffect::BlockUpdated { .. } => self.pending_level_hooks.navigation += 1,
                LevelEffect::DestroyParticles { .. } => self.pending_level_hooks.particles += 1,
                LevelEffect::BlockDestroyed { .. } => self.pending_level_hooks.game_events += 1,
                LevelEffect::PendingPrecipitation(_) => {}
                LevelEffect::BroadcastAll(packet) => self.broadcast(*packet),
                LevelEffect::PlayerPacket { recipients, packet } => {
                    for entity in recipients {
                        for &connection in &self.players {
                            let Some(c) = self.connections.get_mut(&connection) else {
                                continue;
                            };
                            if matches!(c.phase, Phase::Play(id) if id == entity) && !c.close {
                                c.push(*packet.clone());
                                break;
                            }
                        }
                    }
                }
                LevelEffect::PlayerSectionChange { player, section } => {
                    self.live_level_in_mut(dimension)?
                        .level
                        .update_player_tracking_section(player, section)
                        .map_err(game_error)?;
                }
            }
        }
        if let Some(changes) =
            World::capture_block_light_changes(&self.live_level_in(dimension)?.level, lighting)?
        {
            self.live_level_in(dimension)?
                .generation()
                .with_world(|world| world.submit_block_light_changes(changes))?;
        }
        Ok(())
    }
    pub(super) fn update_chunk_tickets_in(
        &mut self,
        dimension: &protocol::Identifier,
    ) -> Result<()> {
        self.live_level_in_mut(dimension)?
            .update_chunk_tickets()
            .map_err(game_error)?;
        Ok(())
    }
    pub(super) fn integrate_jobs_in(&mut self, dimension: &protocol::Identifier) -> Result<()> {
        self.live_level_in_mut(dimension)?.progress_jobs()?;
        self.live_level_in_mut(dimension)?.finish_promotions()?;
        self.live_level_in_mut(dimension)?.evict_unloaded_chunks()?;
        self.live_level_in_mut(dimension)?
            .level
            .collect_chunk_structure_versions();
        self.live_level_in_mut(dimension)?
            .level
            .finish_chunk_source_tick();
        Ok(())
    }
    pub(super) fn track_players_in(&mut self, dimension: &protocol::Identifier) -> Result<()> {
        let players = self.player_connections_in(dimension)?;
        for connection in players {
            let Some(c) = self.connections.get(&connection) else {
                continue;
            };
            if c.close {
                continue;
            }
            let Phase::Play(entity) = c.phase else {
                continue;
            };
            let player = self.player(entity)?;
            let center = player.chunk_position();
            let distance = player
                .view_distance
                .clamp(2, self.properties.int("view-distance").clamp(2, 32));
            self.live_level_in_mut(dimension)?
                .level
                .update_player_chunk_tracking(entity)
                .map_err(game_error)?;
            if !self
                .live_level_in(dimension)?
                .level
                .player_ignored_by_chunk_map(entity)
                .map_err(game_error)?
            {
                self.request_area_in(dimension, center, distance + 2)?;
            }
        }
        Ok(())
    }
    /// Route ordered menu packets after the current entity phase.
    pub(super) fn broadcast_container_changes_in(
        &mut self,
        dimension: &protocol::Identifier,
    ) -> Result<()> {
        for connection in self.player_connections_in(dimension)? {
            let Some(c) = self.connections.get(&connection) else {
                continue;
            };
            let Phase::Play(entity) = c.phase else {
                continue;
            };
            let player = self
                .live_level_in_mut(dimension)?
                .level
                .entity_mut::<ServerPlayer>(entity)
                .map_err(game_error)?;
            let packets = std::mem::take(&mut player.menus.pending_packets);
            for packet in packets {
                self.connections.get_mut(&connection).unwrap().push(packet);
            }
        }
        Ok(())
    }
}
