//! PrepareSpawnTask's owned position search, FULL waits and Ready boundary.
use super::*;
use crate::generation::runtime::LiveLevel;
use ferraloom_game::{
    entity::player::SavedPosition,
    level::{
        Level,
        acquisition::{AcquireDriver, AcquireFailure, AcquireHandle, AcquireResult, ticket_level},
        heightmap::Heightmap as LiveHeightmap,
        tickets::{Ticket, TicketType},
    },
};
use ferraloom_util::{Aabb, SingleThreadedRandomSource, shapes};
use ferraloom_worldgen::{
    Error as SpawnError,
    region::Heightmap,
    settings::DimensionType,
    spawn::{PlayerSpawnFinder, SpawnWorld, fixup_spawn_height, level_respawn_position},
};

/// Port of PrepareSpawnTask.Ready's level, position and raw rotation. No player
/// exists yet: its constructor and wire-ID draw belong to Ready#spawn.
#[derive(Clone, Debug)]
pub(super) struct PreparedPlayerSpawn {
    pub dimension: protocol::Identifier,
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

pub(super) struct PrepareSpawnTask {
    state: SpawnState,
}
enum SpawnState {
    Searching {
        dimension: protocol::Identifier,
        yaw: f32,
        pitch: f32,
        search: SpawnSearch,
    },
    Loading {
        spawn: PreparedPlayerSpawn,
        futures: Option<Vec<AcquireHandle>>,
    },
    Ready(PreparedPlayerSpawn),
}
impl PrepareSpawnTask {
    /// Port of PrepareSpawnTask#start. Adventure's height fixup is synchronous;
    /// an ordinary search owns its next candidate until that FULL future settles.
    pub(super) fn new(
        live: &mut LiveLevel,
        dimension_type: DimensionType,
        dimension: protocol::Identifier,
        saved: SavedPosition,
        respawn: &ferraloom_game::commands::spawn::SpawnData,
        game_type: GameType,
        radius: i32,
    ) -> Result<Self> {
        let level = &mut live.level;
        let (yaw, pitch) = saved
            .rotation
            .unwrap_or((respawn.rotation.yaw, respawn.rotation.pitch));
        let position = if let Some(position) = saved.position {
            Some(position)
        } else if game_type == GameType::Adventure {
            Some(with_spawn_world(level, dimension_type, |world| {
                fixup_spawn_height(world, coordinates(respawn.position)).map(vec3)
            })?)
        } else {
            None
        };
        let state = match position {
            Some(position) => SpawnState::Loading {
                spawn: PreparedPlayerSpawn {
                    dimension,
                    position,
                    yaw,
                    pitch,
                },
                futures: None,
            },
            None => {
                // RandomSource#createThreadLocalInstance seeds the existing
                // SingleThreadedRandomSource from a nondeterministic thread-local
                // long. It does not consume Level.random or the entity factory.
                let mut random = SingleThreadedRandomSource::new(rand::random());
                let border = level.entity_world_border().distance_to_border(
                    f64::from(respawn.position.x),
                    f64::from(respawn.position.z),
                );
                SpawnState::Searching {
                    dimension,
                    yaw,
                    pitch,
                    search: SpawnSearch {
                        suggestion: respawn.position,
                        finder: PlayerSpawnFinder::new(
                            coordinates(respawn.position),
                            radius,
                            border,
                            &mut random,
                        ),
                        candidate: None,
                    },
                }
            }
        };
        let mut task = Self { state };
        if let SpawnState::Searching { search, .. } = &mut task.state {
            search.schedule_candidate(live)?;
        }
        Ok(task)
    }
    pub(super) fn dimension(&self) -> &protocol::Identifier {
        match &self.state {
            SpawnState::Searching { dimension, .. } => dimension,
            SpawnState::Loading { spawn, .. } | SpawnState::Ready(spawn) => &spawn.dimension,
        }
    }
    pub(super) fn ready(&self) -> Result<&PreparedPlayerSpawn> {
        match &self.state {
            SpawnState::Ready(spawn) => Ok(spawn),
            _ => bail!("Player spawn was not ready"),
        }
    }
    /// Port of PrepareSpawnTask.Preparing#tick, with X-fast FULL dependencies
    /// and no requirement for postProcessGeneration or ticking readiness.
    pub(super) fn tick(&mut self, live: &mut LiveLevel, dimension: DimensionType) -> Result<bool> {
        if let SpawnState::Searching {
            dimension: key,
            yaw,
            pitch,
            search,
        } = &mut self.state
        {
            let Some(position) = search.poll(live, dimension)? else {
                return Ok(false);
            };
            self.state = SpawnState::Loading {
                spawn: PreparedPlayerSpawn {
                    dimension: key.clone(),
                    position,
                    yaw: *yaw,
                    pitch: *pitch,
                },
                futures: None,
            };
        }
        if let SpawnState::Loading { spawn, futures } = &mut self.state {
            if futures.is_none() {
                *futures = Some(request_ticketed_full(
                    live,
                    TicketType::PlayerSpawn,
                    chunk(spawn.position),
                    3,
                )?);
            }
            if !full_range_ready(live, futures.as_ref().expect("requested spawn range"))? {
                live.level
                    .poll_chunk_acquisition_mut()
                    .map_err(game_error)?;
                return Ok(false);
            }
            self.state = SpawnState::Ready(spawn.clone());
        }
        Ok(matches!(self.state, SpawnState::Ready(_)))
    }
    /// Port of PrepareSpawnTask.Ready#keepAlive. Refreshing the existing native
    /// ticket is not a chunk read and does not add UNKNOWN.
    pub(super) fn keep_alive(&self, live: &LiveLevel, tick: u64) -> Result<()> {
        let spawn = self.ready()?;
        live.generation()
            .with_tickets_mut(|tickets| tickets.add_player_spawn(chunk(spawn.position), tick));
        Ok(())
    }
}

struct SpawnSearch {
    suggestion: BlockPos,
    finder: PlayerSpawnFinder,
    candidate: Option<SpawnCandidate>,
}
struct SpawnCandidate {
    position: Option<[i32; 2]>,
    futures: Vec<AcquireHandle>,
}
impl SpawnSearch {
    fn schedule_candidate(&mut self, live: &mut LiveLevel) -> Result<()> {
        let position = self.finder.next_candidate();
        let [x, z] = position.unwrap_or([self.suggestion.x, self.suggestion.z]);
        let futures = request_ticketed_full(
            live,
            TicketType::SpawnSearch,
            ChunkPos::new(x >> 4, z >> 4),
            0,
        )?;
        self.candidate = Some(SpawnCandidate { position, futures });
        Ok(())
    }
    /// Port of PlayerSpawnFinder#scheduleNext/scheduleCandidate. The pending
    /// candidate remains reachable while main-thread generation tasks run.
    fn poll(&mut self, live: &mut LiveLevel, dimension: DimensionType) -> Result<Option<Vec3>> {
        if self.candidate.is_none() {
            self.schedule_candidate(live)?;
        }
        let candidate = self.candidate.as_ref().expect("scheduled candidate");
        if !full_range_ready(live, &candidate.futures)? {
            live.level
                .poll_chunk_acquisition_mut()
                .map_err(game_error)?;
            return Ok(None);
        }
        let position = candidate.position;
        let context = live.level.context().clone();
        let result = with_spawn_world(&mut live.level, dimension, |world| match position {
            Some([x, z]) => {
                world.ensure_chunk([x >> 4, z >> 4])?;
                if let Some(pos) = level_respawn_position(world, context.generator.access(), x, z)?
                    && world.no_collision_no_liquid(pos)?
                {
                    Ok(Some(Vec3::new(
                        f64::from(pos[0]) + 0.5,
                        f64::from(pos[1]),
                        f64::from(pos[2]) + 0.5,
                    )))
                } else {
                    Ok(None)
                }
            }
            None => {
                fixup_spawn_height(world, coordinates(self.suggestion)).map(|pos| Some(vec3(pos)))
            }
        })?;
        self.candidate = None;
        if result.is_none() {
            self.schedule_candidate(live)?;
        }
        Ok(result)
    }
}

/// Port of addTicketAndLoadWithRadius's ticket/range half. In particular, the
/// search's FULL future is requested without adding an early UNKNOWN ticket.
fn request_ticketed_full(
    live: &mut LiveLevel,
    kind: TicketType,
    center: ChunkPos,
    radius: i32,
) -> Result<Vec<AcquireHandle>> {
    live.generation().with_tickets_mut(|tickets| {
        tickets.add(
            center,
            Ticket {
                kind,
                level: ticket_level(ChunkStatus::Full).wrapping_sub(radius),
                expires: None,
            },
        )
    });
    live.update_chunk_tickets().map_err(game_error)?;
    let mut futures = Vec::new();
    for z in -radius..=radius {
        for x in -radius..=radius {
            futures.push(
                live.generation()
                    .request_status(
                        &live.level,
                        ChunkPos::new(center.x.wrapping_add(x), center.z.wrapping_add(z)),
                        ChunkStatus::Full,
                    )
                    .map_err(game_error)?,
            );
        }
    }
    Ok(futures)
}
fn full_range_ready(live: &LiveLevel, futures: &[AcquireHandle]) -> Result<bool> {
    let mut ready = true;
    for &future in futures {
        match live
            .generation()
            .acquisition_result(future)
            .map_err(game_error)?
        {
            AcquireResult::Pending => ready = false,
            AcquireResult::Complete(Some(_)) => (),
            AcquireResult::Complete(None) => {
                return Err(game_error(LevelError::ChunkAcquisition(
                    future.pos,
                    future.status,
                    AcquireFailure::MissingResult,
                )));
            }
        }
    }
    Ok(ready)
}

/// Concrete live SpawnWorld. Trait errors retain the original LevelError at this
/// short cold boundary rather than converting required failures into empty facts.
struct LiveSpawnWorld<'a> {
    level: &'a mut Level,
    dimension: DimensionType,
    failure: Option<LevelError>,
}
impl LiveSpawnWorld<'_> {
    fn required<T>(
        &mut self,
        result: std::result::Result<T, LevelError>,
    ) -> std::result::Result<T, SpawnError> {
        result.map_err(|error| {
            let message = format!("live spawn: {error:?}");
            self.failure = Some(error);
            SpawnError(message)
        })
    }
}
impl SpawnWorld for LiveSpawnWorld<'_> {
    fn dimension(&self) -> DimensionType {
        self.dimension.clone()
    }
    fn ensure_chunk(&mut self, position: [i32; 2]) -> std::result::Result<(), SpawnError> {
        let pos = ChunkPos::new(position[0], position[1]);
        let result = self
            .level
            .get_chunk_mut(pos, ChunkStatus::Full, true)
            .and_then(|chunk| chunk.ok_or(LevelError::MissingChunk(pos)));
        self.required(result).map(|_| ())
    }
    fn height(&mut self, map: Heightmap, x: i32, z: i32) -> std::result::Result<i32, SpawnError> {
        self.ensure_chunk([x >> 4, z >> 4])?;
        let map = match map {
            Heightmap::WorldSurface => LiveHeightmap::WorldSurface,
            Heightmap::OceanFloor => LiveHeightmap::OceanFloor,
            Heightmap::MotionBlocking => LiveHeightmap::MotionBlocking,
            Heightmap::MotionBlockingNoLeaves => LiveHeightmap::MotionBlockingNoLeaves,
            Heightmap::WorldSurfaceWg | Heightmap::OceanFloorWg => {
                return Err(SpawnError(
                    "unsupported live spawn generation heightmap".into(),
                ));
            }
        };
        // PlayerSpawnFinder reads LevelChunk#getHeight directly, including
        // outside Level#getHeight's horizontal fallback bounds.
        let pos = ChunkPos::new(x >> 4, z >> 4);
        let result = self
            .level
            .chunk(pos)
            .ok_or(LevelError::MissingChunk(pos))
            .and_then(|chunk| {
                chunk
                    .height_at(map, (x & 15) as u8, (z & 15) as u8)
                    .ok_or(LevelError::UnsupportedWeatherQuery)
            })
            .map(|y| y.wrapping_sub(1));
        self.required(result)
    }
    fn has_fluid(&mut self, position: [i32; 3]) -> std::result::Result<bool, SpawnError> {
        let pos = block(position);
        if self.level.is_in_valid_bounds(pos) {
            self.ensure_chunk([pos.x >> 4, pos.z >> 4])?;
        }
        let result = self
            .level
            .get_block_state(pos)
            .map(|state| !state.data().fluid.empty);
        self.required(result)
    }
    fn collision_up_face_full(
        &mut self,
        position: [i32; 3],
    ) -> std::result::Result<bool, SpawnError> {
        let pos = block(position);
        if self.level.is_in_valid_bounds(pos) {
            self.ensure_chunk([pos.x >> 4, pos.z >> 4])?;
        }
        let result = self
            .level
            .get_block_state(pos)
            .and_then(|state| {
                self.level.collision_shape(
                    state,
                    pos,
                    &ferraloom_game::block::BlockCollisionContext::empty(),
                )
            })
            .map(|shape| shapes::is_face_full(&shape, ferraloom_util::Direction::Up));
        self.required(result)
    }
    fn no_collision_no_liquid(
        &mut self,
        position: [i32; 3],
    ) -> std::result::Result<bool, SpawnError> {
        let result = self.level.no_collision(None, player_box(position), true);
        self.required(result)
    }
}
fn with_spawn_world<T>(
    level: &mut Level,
    dimension: DimensionType,
    operation: impl FnOnce(&mut LiveSpawnWorld<'_>) -> std::result::Result<T, SpawnError>,
) -> Result<T> {
    let mut world = LiveSpawnWorld {
        level,
        dimension,
        failure: None,
    };
    let result = operation(&mut world);
    match result {
        Ok(value) => Ok(value),
        Err(error) => match world.failure {
            Some(required) => Err(anyhow::Error::new(required)),
            None => Err(anyhow::Error::new(error)),
        },
    }
}
fn coordinates(pos: BlockPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}
fn block(pos: [i32; 3]) -> BlockPos {
    BlockPos::new(pos[0], pos[1], pos[2])
}
fn vec3(pos: [f64; 3]) -> Vec3 {
    Vec3::new(pos[0], pos[1], pos[2])
}
fn chunk(pos: Vec3) -> ChunkPos {
    ChunkPos::containing(BlockPos::new(
        ferraloom_util::mth::floor(pos.x),
        ferraloom_util::mth::floor(pos.y),
        ferraloom_util::mth::floor(pos.z),
    ))
}
fn player_box(pos: [i32; 3]) -> Aabb {
    let dimensions = EntityType::PLAYER.data();
    let radius = f64::from(dimensions.width) / 2.0;
    let x = f64::from(pos[0]) + 0.5;
    let y = f64::from(pos[1]);
    let z = f64::from(pos[2]) + 0.5;
    Aabb::new(
        x - radius,
        y,
        z - radius,
        x + radius,
        y + f64::from(dimensions.height),
        z + radius,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
    use ferraloom_game::level::{LevelContext, chunk::LevelChunk};
    use std::{io::Read, sync::OnceLock};

    fn dimension() -> DimensionType {
        DimensionType {
            min_y: -64,
            height: 384,
            logical_height: 384,
            has_ceiling: false,
            has_skylight: true,
            fast_lava: false,
            coordinate_scale: 1.0,
            infiniburn: ferraloom_worldgen::structure::processor::BlockSet::Tag(
                "minecraft:infiniburn_overworld".parse().unwrap(),
            ),
        }
    }
    fn level() -> Level {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        static TAGS: OnceLock<Arc<TagSet<Block>>> = OnceLock::new();
        let tags = TAGS.get_or_init(|| {
            let jar = std::env::var_os("FERRALOOM_VANILLA_JAR").unwrap();
            let resources =
                ResourceManager::new(vec![Pack::vanilla_jar(std::path::Path::new(&jar)).unwrap()]);
            Arc::new(TagSet::load(&resources).unwrap())
        });
        let mut level = Level::new(Region::new(&BEHAVIORS), LevelContext::fixture(0, 63));
        level.set_block_tags(tags.clone());
        level
    }
    fn column(level: &mut Level, x: i32, z: i32, floor: i32) {
        let mut chunk = LevelChunk::new(
            ChunkPos::new(x >> 4, z >> 4),
            -4..20,
            LevelChunkSection::new(
                Strategy::block_states(),
                Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
                BiomeId(0),
            ),
        );
        for y in -64..=floor {
            chunk.set_block_state(BlockPos::new(x, y, z), Block::STONE.default_state());
        }
        chunk.prime_heightmaps(level.block_tags().unwrap());
        level.add_chunk(chunk).unwrap();
    }

    #[test]
    fn live_height_fixup_matches_native_player_boxes_and_positions() {
        let mut text = String::new();
        flate2::read::GzDecoder::new(
            &include_bytes!("../../ferraloom-worldgen/tests/golden/spawn_height_fixup.json.gz")[..],
        )
        .read_to_string(&mut text)
        .unwrap();
        let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut compared = 0;
        // These nine native worlds are exactly representable as live columns.
        // The mock oracle's solid ceilings outside build height are not.
        for case in cases.as_array().unwrap().iter().filter(|case| {
            case["ceiling"] == -1
                && (-64..=317).contains(&case["floor"].as_i64().unwrap())
                && case["start"].as_i64().unwrap() < 319
        }) {
            let mut level = level();
            column(&mut level, -3, 7, case["floor"].as_i64().unwrap() as i32);
            let mut boxes = Vec::new();
            let actual = with_spawn_world(&mut level, dimension(), |world| {
                struct Observed<'a, 'b> {
                    world: &'a mut LiveSpawnWorld<'b>,
                    boxes: &'a mut Vec<[f64; 6]>,
                }
                impl SpawnWorld for Observed<'_, '_> {
                    fn dimension(&self) -> DimensionType {
                        self.world.dimension()
                    }
                    fn ensure_chunk(&mut self, p: [i32; 2]) -> Result<(), SpawnError> {
                        self.world.ensure_chunk(p)
                    }
                    fn height(
                        &mut self,
                        map: Heightmap,
                        x: i32,
                        z: i32,
                    ) -> Result<i32, SpawnError> {
                        self.world.height(map, x, z)
                    }
                    fn has_fluid(&mut self, p: [i32; 3]) -> Result<bool, SpawnError> {
                        self.world.has_fluid(p)
                    }
                    fn collision_up_face_full(&mut self, p: [i32; 3]) -> Result<bool, SpawnError> {
                        self.world.collision_up_face_full(p)
                    }
                    fn no_collision_no_liquid(&mut self, p: [i32; 3]) -> Result<bool, SpawnError> {
                        let bb = player_box(p);
                        self.boxes
                            .push([bb.min_x, bb.min_y, bb.min_z, bb.max_x, bb.max_y, bb.max_z]);
                        self.world.no_collision_no_liquid(p)
                    }
                }
                fixup_spawn_height(
                    &mut Observed {
                        world,
                        boxes: &mut boxes,
                    },
                    [-3, case["start"].as_i64().unwrap() as i32, 7],
                )
            })
            .unwrap();
            assert_eq!(serde_json::to_value(actual).unwrap(), case["position"]);
            assert_eq!(serde_json::to_value(boxes).unwrap(), case["calls"]);
            compared += 1;
        }
        assert_eq!(compared, 9);
    }

    #[test]
    fn live_spawn_height_is_chunk_height_and_outside_bounds_does_not_acquire() {
        let mut level = level();
        column(&mut level, -3, 7, 63);
        with_spawn_world(&mut level, dimension(), |world| {
            for map in [
                Heightmap::MotionBlocking,
                Heightmap::WorldSurface,
                Heightmap::OceanFloor,
            ] {
                assert_eq!(world.height(map, -3, 7)?, 63);
            }
            assert!(world.collision_up_face_full([-3, 63, 7])?);
            assert!(!world.has_fluid([-3, 63, 7])?);
            // No chunks are present here. Native Level bounds reject the read
            // before ServerChunkCache is consulted, for both block and fluid.
            for pos in [[32, -65, 32], [32, 320, 32], [i32::MAX, 63, 32]] {
                assert!(!world.has_fluid(pos)?);
                assert!(!world.collision_up_face_full(pos)?);
            }
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn live_spawn_keeps_the_original_required_error() {
        let mut level = level();
        let error = with_spawn_world(&mut level, dimension(), |world| {
            world.height(Heightmap::MotionBlocking, 32, 32)
        })
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<LevelError>(),
            Some(&LevelError::MissingChunk(ChunkPos::new(2, 2)))
        );
    }
}
