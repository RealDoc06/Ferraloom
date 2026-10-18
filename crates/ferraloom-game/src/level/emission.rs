//! Server sound and particle audiences are selected at the originating call.
use super::{Level, LevelError, effects::LevelEffect};
use crate::entity::{EntityData, EntityRef, EntityState};
#[cfg(test)]
use crate::level::game_event::test_support::{self as game_events, Observation};
use ferraloom_data::{
    BlockState,
    registries::{game_event::Id as GameEvent, sound_event::Id as SoundEvent},
};
use ferraloom_protocol::{
    codec::{Holder, VarInt},
    entity_data::Particle,
    play::{ParticleRandomization, SoundEventHolder, SoundSource, clientbound as cb},
};
use ferraloom_util::{
    Vec3,
    random::{LegacyRandomSource, Random, RandomSource},
};

/// Full ServerLevel#sendParticles overload; double inputs are narrowed only at
/// packet construction, as vanilla does. The scalar overload uses DEFAULT.
pub struct ParticleEmission {
    pub override_limiter: bool,
    pub always_show: bool,
    pub position: Vec3,
    pub count: i32,
    pub distance: Vec3,
    pub speed: Vec3,
    pub randomization: ParticleRandomization,
}
impl Level {
    /// Port of ServerLevel#levelEvent's dimension-local, non-global broadcast.
    /// Its 64-block audience uses the integer position, excluding only source.
    pub fn level_event(
        &mut self,
        source: Option<EntityRef>,
        id: i32,
        pos: ferraloom_util::BlockPos,
        data: i32,
    ) -> Result<(), LevelError> {
        let position = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        let recipients = self.broadcast_audience(source, position, 64.0)?;
        self.emit(LevelEffect::PlayerPacket {
            recipients,
            packet: Box::new(cb::Packet::LevelEvent(cb::LevelEvent {
                event_type: id,
                pos,
                data,
                global_event: false,
            })),
        });
        Ok(())
    }
    /// Controlled tests may seed the native sound stream independently.
    pub fn set_sound_seed(&mut self, seed: i64) {
        self.region_mut().sound_seed_generator = Random::Legacy(LegacyRandomSource::new(seed));
    }
    /// Port of Level#playSound and ServerLevel#playSeededSound. This consumes
    /// soundSeedGenerator synchronously, even when there is no audience.
    pub fn play_sound(
        &mut self,
        except: Option<EntityRef>,
        position: Vec3,
        sound: SoundEvent,
        source: SoundSource,
        volume: f32,
        pitch: f32,
    ) -> Result<(), LevelError> {
        let seed = self.region_mut().sound_seed_generator.next_long();
        self.play_seeded_sound(except, position, sound, source, volume, pitch, seed)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn play_seeded_sound(
        &mut self,
        except: Option<EntityRef>,
        position: Vec3,
        sound: SoundEvent,
        source: SoundSource,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> Result<(), LevelError> {
        let recipients =
            self.broadcast_audience(except, position, f64::from(sound.range(volume)))?;
        self.emit(LevelEffect::PlayerPacket {
            recipients,
            packet: Box::new(cb::Packet::Sound(cb::Sound {
                sound: SoundEventHolder(Holder::Reference(
                    sound.name().parse().expect("registered sound identifier"),
                )),
                source,
                x: (position.x * 8.0) as i32,
                y: (position.y * 8.0) as i32,
                z: (position.z * 8.0) as i32,
                volume,
                pitch,
                seed,
            })),
        });
        Ok(())
    }
    pub fn play_entity_sound(
        &mut self,
        except: Option<EntityRef>,
        entity: EntityRef,
        sound: SoundEvent,
        source: SoundSource,
        volume: f32,
        pitch: f32,
    ) -> Result<(), LevelError> {
        let seed = self.region_mut().sound_seed_generator.next_long();
        self.play_seeded_entity_sound(except, entity, sound, source, volume, pitch, seed)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn play_seeded_entity_sound(
        &mut self,
        except: Option<EntityRef>,
        entity: EntityRef,
        sound: SoundEvent,
        source: SoundSource,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> Result<(), LevelError> {
        let data = self.entity::<EntityData>(entity)?;
        let (position, id) = (data.position, data.network_id);
        let recipients =
            self.broadcast_audience(except, position, f64::from(sound.range(volume)))?;
        self.emit(LevelEffect::PlayerPacket {
            recipients,
            packet: Box::new(cb::Packet::SoundEntity(cb::SoundEntity {
                sound: SoundEventHolder(Holder::Reference(
                    sound.name().parse().expect("registered sound identifier"),
                )),
                source,
                id: VarInt(id),
                volume,
                pitch,
                seed,
            })),
        });
        Ok(())
    }
    /// Port of PlayerList#broadcast's dimension-local distance/exclusion filter.
    /// Capture before subsequent callbacks can move any recipient.
    pub(super) fn broadcast_audience(
        &self,
        except: Option<EntityRef>,
        position: Vec3,
        range: f64,
    ) -> Result<Vec<EntityRef>, LevelError> {
        let mut recipients = Vec::new();
        for id in self.all_entities() {
            if Some(id) != except
                && let EntityState::Player(player) = self.entity_state(id)?
                && player.position.distance_to_sqr(position) < range * range
            {
                recipients.push(id);
            }
        }
        Ok(recipients)
    }
    /// Level#addParticle is a server no-op, distinct from sendParticles. Entity
    /// sampling occurs before this call and must still consume its native RNG.
    pub fn add_particle(&mut self, _particle: Particle, _position: Vec3, _velocity: Vec3) {}
    pub fn send_particles(
        &mut self,
        particle: Particle,
        position: Vec3,
        count: i32,
        distance: Vec3,
        speed: f64,
    ) -> Result<i32, LevelError> {
        self.send_particles_with_options(
            particle,
            ParticleEmission {
                override_limiter: false,
                always_show: false,
                position,
                count,
                distance,
                speed: Vec3::new(speed, speed, speed),
                randomization: ParticleRandomization::Default,
            },
        )
    }
    /// Port of ServerLevel#sendParticles. Eligibility uses the player's block
    /// center and a strict 32/512 radius; alwaysShow affects only the packet.
    pub fn send_particles_with_options(
        &mut self,
        particle: Particle,
        options: ParticleEmission,
    ) -> Result<i32, LevelError> {
        let mut recipients = Vec::new();
        let range = if options.override_limiter {
            512.0
        } else {
            32.0
        };
        for id in self.all_entities() {
            if let EntityState::Player(player) = self.entity_state(id)? {
                let p = player.block_position;
                let center = Vec3::new(
                    f64::from(p.x) + 0.5,
                    f64::from(p.y) + 0.5,
                    f64::from(p.z) + 0.5,
                );
                if center.distance_to_sqr(options.position) < range * range {
                    recipients.push(id);
                }
            }
        }
        let count = recipients.len() as i32;
        let p = options.position;
        let d = options.distance;
        let s = options.speed;
        self.emit(LevelEffect::PlayerPacket {
            recipients,
            packet: Box::new(cb::Packet::LevelParticles(cb::LevelParticles {
                particle,
                override_limiter: options.override_limiter,
                always_show: options.always_show,
                x: p.x,
                y: p.y,
                z: p.z,
                x_dist: d.x as f32,
                y_dist: d.y as f32,
                z_dist: d.z as f32,
                x_max_speed: s.x as f32,
                y_max_speed: s.y as f32,
                z_max_speed: s.z as f32,
                count: VarInt(options.count),
                randomization_type: options.randomization,
            })),
        });
        Ok(count)
    }
    /// Port of ServerLevel#gameEvent. Listener callbacks and failures are
    /// synchronous at the emission call, before the producer resumes.
    pub fn game_event(
        &mut self,
        event: GameEvent,
        position: Vec3,
        source: Option<EntityRef>,
        affected_state: Option<BlockState>,
    ) -> Result<(), LevelError> {
        super::game_event::GameEventDispatcher::post(
            self,
            event,
            position,
            super::game_event::GameEventContext {
                source_entity: source,
                affected_state,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::player::{GameType, ServerPlayer},
        level::Region,
    };
    use ferraloom_util::Uuid;
    static EMPTY: Behaviors = Behaviors::EMPTY;
    fn level() -> Level {
        Level::new(
            Region::new(&EMPTY),
            crate::level::LevelContext::fixture(0, 63),
        )
    }
    fn player(level: &mut Level, id: u64, position: Vec3) -> EntityRef {
        let handle = EntityRef(id);
        let network = level.next_entity_id();
        let player = ServerPlayer::new(
            Uuid::offline_player(&format!("Sound{id}")),
            format!("Sound{id}"),
            network,
            position,
            GameType::Survival,
        );
        level
            .add_entity(handle, EntityState::Player(Box::new(player)))
            .unwrap();
        level.set_entity_pos(handle, position).unwrap();
        handle
    }
    fn only_packet(level: &mut Level) -> (Vec<EntityRef>, cb::Packet) {
        let mut effects = level.take_effects().into_iter();
        let Some(LevelEffect::PlayerPacket { recipients, packet }) = effects.next() else {
            panic!("missing packet effect")
        };
        assert!(effects.next().is_none());
        (recipients, *packet)
    }
    #[test]
    fn native_sound_stream_quantization_and_particle_center_vectors() {
        let sound = SoundEvent::ENTITY_GENERIC_EXTINGUISH_FIRE;
        for row in include_str!("../../tests/java/level/sound.tsv").lines() {
            let (case, expected) = row.split_once('\t').unwrap();
            let args: Vec<_> = case.split(':').collect();
            let mut level = level();
            match args[0] {
                "sound-stream" => {
                    level.set_sound_seed(args[1].parse().unwrap());
                    level.set_random_streams(Random::Legacy(LegacyRandomSource::new(987)), 0);
                    let mut seeds = Vec::new();
                    for _ in 0..4 {
                        level
                            .play_sound(None, Vec3::ZERO, sound, SoundSource::Neutral, 1.0, 1.0)
                            .unwrap();
                        let (_, cb::Packet::Sound(packet)) = only_packet(&mut level) else {
                            panic!("not sound")
                        };
                        seeds.push(packet.seed.to_string());
                    }
                    assert_eq!(
                        format!("{}|{}", seeds.join(","), level.random().next_int()),
                        expected
                    );
                }
                "sound-position" => {
                    level
                        .play_seeded_sound(
                            None,
                            Vec3::new(args[1].parse().unwrap(), 0.0, 0.0),
                            sound,
                            SoundSource::Neutral,
                            1.0,
                            1.0,
                            5,
                        )
                        .unwrap();
                    let (_, cb::Packet::Sound(packet)) = only_packet(&mut level) else {
                        panic!("not sound")
                    };
                    assert_eq!(packet.x.to_string(), expected);
                }
                "particle-range" => {
                    let id = player(&mut level, 1, Vec3::new(args[1].parse().unwrap(), 0.0, 0.0));
                    let p = Vec3::new(args[2].parse().unwrap(), 0.5, 0.5);
                    let mut actual = Vec::new();
                    for override_limiter in [false, true] {
                        let count = level
                            .send_particles_with_options(
                                Particle::Bubble,
                                ParticleEmission {
                                    override_limiter,
                                    always_show: true,
                                    position: p,
                                    count: 0,
                                    distance: Vec3::ZERO,
                                    speed: Vec3::ZERO,
                                    randomization: ParticleRandomization::Default,
                                },
                            )
                            .unwrap();
                        let (recipients, cb::Packet::LevelParticles(packet)) =
                            only_packet(&mut level)
                        else {
                            panic!("not particles")
                        };
                        assert_eq!(recipients, if count == 1 { vec![id] } else { Vec::new() });
                        assert_eq!(packet.count, VarInt(0));
                        actual.push((count == 1).to_string());
                    }
                    assert_eq!(actual.join(","), expected);
                }
                _ => panic!("unexpected oracle row"),
            }
        }
    }
    #[test]
    fn emission_captures_audience_before_movement_and_keeps_server_noops_distinct() {
        use crate::level::chunk::LevelChunk;
        use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
        use ferraloom_util::ChunkPos;
        use std::sync::Arc;

        let mut level = level();
        let template = LevelChunkSection::new(
            Strategy::block_states(),
            Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap())),
            BiomeId(0),
        );
        level
            .add_chunk(LevelChunk::new(ChunkPos::ZERO, -1..1, template))
            .unwrap();
        game_events::observe(&mut level);
        let a = player(&mut level, 1, Vec3::new(15.999, 0.0, 0.0));
        let b = player(&mut level, 2, Vec3::new(16.0, 0.0, 0.0));
        let except = player(&mut level, 3, Vec3::new(2.0, 0.0, 0.0));
        let sound = SoundEvent::ENTITY_GENERIC_EXTINGUISH_FIRE;
        level
            .play_sound(
                Some(except),
                Vec3::ZERO,
                sound,
                SoundSource::Players,
                1.0,
                0.5,
            )
            .unwrap();
        level.set_entity_pos(a, Vec3::new(40.0, 0.0, 0.0)).unwrap();
        assert_eq!(
            level.take_player_section_changes(),
            vec![(a, ferraloom_util::SectionPos::new(2, 0, 0))],
        );
        assert_eq!(only_packet(&mut level).0, vec![a]);
        level
            .play_seeded_entity_sound(None, except, sound, SoundSource::Players, 1.0, 1.0, 7)
            .unwrap();
        let (audience, cb::Packet::SoundEntity(packet)) = only_packet(&mut level) else {
            panic!("not attached sound")
        };
        assert_eq!(audience, vec![b, except]);
        assert_eq!(packet.seed, 7);
        level.add_particle(Particle::Bubble, Vec3::ZERO, Vec3::ZERO);
        assert!(game_events::take(&mut level).is_empty());
        let state = Some(ferraloom_data::Block::STONE.default_state());
        level
            .game_event(GameEvent::STEP, Vec3::ZERO, Some(except), state)
            .unwrap();
        assert!(
            matches!(game_events::take(&mut level).as_slice(),[Observation::GameEvent {event:GameEvent::STEP,source:Some(id),affected_state,..}] if *id==except && *affected_state==state)
        );
    }
}
