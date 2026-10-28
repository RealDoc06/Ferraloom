use anyhow::{Context, Result, ensure};
use ferraloom_data::packets as packet_types;
use ferraloom_protocol::{
    self as p, Reader, Uuid, Wire,
    codec::{BitSet, VarInt},
    common, configuration as cfg, handshake, login,
    play::{self, clientbound as cb, serverbound as sb},
};
use ferraloom_server::{network::PacketIo, registries::RegistrySet};
use std::{
    collections::{BTreeMap, HashSet},
    net::SocketAddr,
    path::Path,
    time::Duration,
};
use tokio::net::TcpStream;

pub const NAME: &str = "JoinBot";
pub fn properties(root: &Path) -> Result<()> {
    std::fs::write(root.join("eula.txt"), "eula=true\n")?;
    // Both servers use this featureless flat world for Strict protocol/join
    // coverage. Restore default noise once the block families clear the skips.
    let generator = serde_json::json!({
        "layers": [{"height":4,"block":"minecraft:bedrock"}],
        "biome":"minecraft:plains", "features":false, "lakes":false,
        "structure_overrides":[]
    });
    std::fs::write(
        root.join("server.properties"),
        format!(
            "server-ip=127.0.0.1\nserver-port=0\nonline-mode=false\nwhite-list=false\nenforce-secure-profile=false\nview-distance=2\nsimulation-distance=2\nlevel-seed=12345\ngenerate-structures=false\npause-when-empty-seconds=0\nlevel-type=flat\ngenerator-settings={generator}\n"
        ),
    )?;
    std::fs::write(
        root.join("ops.json"),
        serde_json::to_vec(
            &serde_json::json!([{"uuid":Uuid::offline_player(NAME).to_string(),"name":NAME,"level":4,"bypassesPlayerLimit":true}]),
        )?,
    )?;
    Ok(())
}
#[derive(Debug, PartialEq, Eq)]
pub struct Sequence {
    pub login: Vec<packet_types::login::clientbound::Packet>,
    pub configuration: Vec<(
        packet_types::configuration::clientbound::Packet,
        Option<String>,
    )>,
    pub registries: Vec<(String, Vec<String>)>,
    pub join: Vec<packet_types::play::clientbound::Packet>,
}
pub struct Outcome {
    pub sequence: Sequence,
    pub time_updates: Vec<cb::SetTime>,
    pub frozen_periodic_time: Option<cb::SetTime>,
    pub chunks: usize,
    pub unloaded: usize,
    pub position: [f64; 3],
}
struct Client {
    io: PacketIo<TcpStream>,
    registries: RegistrySet,
    position: [f64; 3],
    rotation: [f32; 2],
    chunks: HashSet<(i32, i32)>,
    unloads: usize,
    sequence: Sequence,
    time_updates: Vec<cb::SetTime>,
    joining: bool,
}
impl Client {
    async fn connect(address: SocketAddr) -> Result<Self> {
        let socket = TcpStream::connect(address).await?;
        socket.set_nodelay(true)?;
        let mut io = PacketIo::new(socket);
        io.send(&handshake::ServerboundPacket::Intention(
            handshake::ClientIntention {
                protocol_version: p::PROTOCOL_VERSION,
                host_name: "localhost".into(),
                port: address.port(),
                intention: handshake::ClientIntent::Login,
            },
        ))
        .await?;
        io.send(&login::ServerboundPacket::Hello(login::ServerboundHello {
            name: NAME.into(),
            profile_id: Uuid::NIL,
        }))
        .await?;
        let mut sequence = Sequence {
            login: Vec::new(),
            configuration: Vec::new(),
            registries: Vec::new(),
            join: Vec::new(),
        };
        loop {
            let packet = login::ClientboundPacket::from_bytes(&io.read().await?)?;
            sequence.login.push(
                packet_types::login::clientbound::Packet::from_protocol_id(packet.packet_id())
                    .context("unknown login packet type")?,
            );
            match packet {
                login::ClientboundPacket::Compression(p) => io.compression(p.threshold.0),
                login::ClientboundPacket::LoginFinished(p) => {
                    ensure!(
                        p.game_profile.id == Uuid::offline_player(NAME),
                        "offline UUID mismatch"
                    );
                    break;
                }
                login::ClientboundPacket::Disconnect(p) => {
                    anyhow::bail!("login disconnect: {:?}", p.reason)
                }
                _ => anyhow::bail!("unexpected login packet"),
            }
        }
        io.send(&login::ServerboundPacket::LoginAcknowledged(
            login::LoginAcknowledged {},
        ))
        .await?;
        io.send(&cfg::ServerboundPacket::ClientInformation(
            common::serverbound::ClientInformation {
                language: common::FriendlyString("en_us".into()),
                view_distance: 2,
                chat_visibility: common::serverbound::ChatVisibility::Full,
                chat_colors: true,
                model_customisation: 127,
                main_hand: common::serverbound::HumanoidArm::Right,
                text_filtering_enabled: false,
                allows_listing: true,
                particle_status: common::serverbound::ParticleStatus::All,
            },
        ))
        .await?;
        let mut registries = RegistrySet {
            resources: std::sync::Arc::new(ferraloom_data::datapack::ResourceManager::default()),
            entries: BTreeMap::new(),
            tags: common::clientbound::UpdateTags { tags: Vec::new() },
        };
        loop {
            let packet = cfg::ClientboundPacket::from_bytes(&io.read().await?)?;
            let registry = if let cfg::ClientboundPacket::RegistryData(p) = &packet {
                Some(p.registry.to_string())
            } else {
                None
            };
            if !matches!(packet, cfg::ClientboundPacket::KeepAlive(_)) {
                sequence.configuration.push((
                    packet_types::configuration::clientbound::Packet::from_protocol_id(
                        packet.packet_id(),
                    )
                    .context("unknown configuration packet type")?,
                    registry,
                ));
            }
            match packet {
                cfg::ClientboundPacket::SelectKnownPacks(p) => {
                    io.send(&cfg::ServerboundPacket::SelectKnownPacks(
                        cfg::ServerboundSelectKnownPacks {
                            known_packs: p.known_packs,
                        },
                    ))
                    .await?
                }
                cfg::ClientboundPacket::RegistryData(p) => {
                    let entries = p
                        .entries
                        .into_iter()
                        .map(|entry| {
                            assert!(
                                entry.data.is_none(),
                                "negotiated core data should be omitted"
                            );
                            entry.id.to_string()
                        })
                        .collect::<Vec<_>>();
                    sequence
                        .registries
                        .push((p.registry.to_string(), entries.clone()));
                    registries.entries.insert(p.registry.to_string(), entries);
                }
                cfg::ClientboundPacket::UpdateTags(tags) => registries.tags = tags,
                cfg::ClientboundPacket::FinishConfiguration(_) => {
                    io.send(&cfg::ServerboundPacket::FinishConfiguration(
                        cfg::FinishConfiguration {},
                    ))
                    .await?;
                    break;
                }
                cfg::ClientboundPacket::KeepAlive(p) => {
                    io.send(&cfg::ServerboundPacket::KeepAlive(
                        common::serverbound::KeepAlive { id: p.id },
                    ))
                    .await?
                }
                cfg::ClientboundPacket::Disconnect(p) => {
                    anyhow::bail!("configuration disconnect: {:?}", p.reason)
                }
                _ => {}
            }
        }
        Ok(Self {
            io,
            registries,
            position: [0.; 3],
            rotation: [0.; 2],
            chunks: HashSet::new(),
            unloads: 0,
            sequence,
            time_updates: Vec::new(),
            joining: true,
        })
    }
    async fn next(&mut self) -> Result<cb::Packet> {
        let bytes = self.io.read().await?;
        let mut reader = Reader::with_context(&bytes, &self.registries);
        let packet = cb::Packet::decode(&mut reader)?;
        reader.finish()?;
        let periodic_time = if let cb::Packet::SetTime(time) = &packet {
            let periodic = !self.time_updates.is_empty();
            self.time_updates.push(time.clone());
            periodic
        } else {
            false
        };
        // MinecraftServer.java:1078-1081/1142-1145 emits periodic SetTime independently of join packet order.
        if self.joining && !periodic_time {
            self.sequence.join.push(
                packet_types::play::clientbound::Packet::from_protocol_id(packet.packet_id())
                    .context("unknown play packet type")?,
            );
            if matches!(packet, cb::Packet::SetEntityData(_)) {
                self.joining = false;
            }
        }
        match &packet {
            cb::Packet::PlayerPosition(p) => {
                self.position = [
                    p.change.position.x,
                    p.change.position.y,
                    p.change.position.z,
                ];
                self.rotation = [p.change.y_rot, p.change.x_rot];
                self.io
                    .send(&sb::Packet::AcceptTeleportation(sb::AcceptTeleportation {
                        id: p.id,
                        x: self.position[0],
                        y: self.position[1],
                        z: self.position[2],
                        y_rot: self.rotation[0],
                        x_rot: self.rotation[1],
                    }))
                    .await?;
                self.io
                    .send(&sb::Packet::PlayerLoaded(sb::PlayerLoaded {}))
                    .await?;
            }
            cb::Packet::KeepAlive(p) => {
                self.io
                    .send(&sb::Packet::KeepAlive(common::serverbound::KeepAlive {
                        id: p.id,
                    }))
                    .await?
            }
            cb::Packet::ChunkBatchFinished(_) => {
                self.io
                    .send(&sb::Packet::ChunkBatchReceived(sb::ChunkBatchReceived {
                        desired_chunks_per_tick: 64.,
                    }))
                    .await?
            }
            cb::Packet::LevelChunkWithLight(chunk) => {
                ensure!(chunk.chunk_data.sections.len() == 24, "overworld sections");
                ensure!(
                    chunk
                        .light_data
                        .sky_y_mask
                        .0
                        .words()
                        .iter()
                        .any(|&v| v != 0),
                    "chunk lacks sky light"
                );
                self.chunks.insert((chunk.x, chunk.z));
            }
            cb::Packet::ForgetLevelChunk(_) => self.unloads += 1,
            cb::Packet::Disconnect(p) => anyhow::bail!("play disconnect: {:?}", p.reason),
            _ => {}
        }
        Ok(packet)
    }
    async fn until(&mut self, mut predicate: impl FnMut(&Self, &cb::Packet) -> bool) -> Result<()> {
        loop {
            let packet = self.next().await?;
            if predicate(self, &packet) {
                return Ok(());
            }
        }
    }
    async fn command(&mut self, text: &str) -> Result<()> {
        self.io
            .send(&sb::Packet::ChatCommand(sb::ChatCommand {
                command: text.into(),
            }))
            .await?;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
enum ScenarioPhase {
    Connect,
    InitialChunks,
    Chat,
    Spectator,
    Walk,
    DestinationChunks,
    Freeze,
    PeriodicTime,
    Save,
    Stop,
}

pub async fn scenario(address: SocketAddr, sample_periodic_time: bool) -> Result<Outcome> {
    let mut phase = ScenarioPhase::Connect;
    tokio::time::timeout(
        Duration::from_secs(240),
        run_scenario(address, sample_periodic_time, &mut phase),
    )
    .await
    .with_context(|| format!("join/walk scenario timed out at {phase:?}"))?
}
async fn run_scenario(
    address: SocketAddr,
    sample_periodic_time: bool,
    phase: &mut ScenarioPhase,
) -> Result<Outcome> {
    let mut client = Client::connect(address).await?;
    *phase = ScenarioPhase::InitialChunks;
    client
        .until(|c, p| c.chunks.len() >= 9 && matches!(p, cb::Packet::ChunkBatchFinished(_)))
        .await?;
    *phase = ScenarioPhase::Chat;
    client
        .io
        .send(&sb::Packet::Chat(Box::new(sb::Chat {
            message: common::Utf8("hello parity".into()),
            timestamp: time::OffsetDateTime::now_utc()
                .unix_timestamp_nanos()
                .div_euclid(1_000_000) as i64,
            salt: 0,
            signature: None,
            last_seen_messages: play::LastSeenMessagesUpdate {
                offset: VarInt(0),
                acknowledged: BitSet::default(),
                checksum: 0,
            },
        })))
        .await?;
    client
        .until(|_, p| matches!(p,cb::Packet::PlayerChat(p)if p.body.content.0=="hello parity"))
        .await?;
    *phase = ScenarioPhase::Spectator;
    client.command("gamemode spectator").await?;
    client
        .until(|_, p| matches!(p,cb::Packet::GameEvent(p)if p.event==3 && p.param==3.))
        .await?;
    let start = client.position;
    // Stay below ordinary flying speed while vanilla generates neighboring terrain.
    *phase = ScenarioPhase::Walk;
    for step in 1..=100 {
        let x = start[0] + f64::from(step);
        client.position[0] = x;
        client
            .io
            .send(&sb::Packet::MovePlayerPosRot(sb::MovePlayerPosRot {
                x,
                y: start[1],
                z: start[2],
                y_rot: client.rotation[0],
                x_rot: client.rotation[1],
                flags: play::MovementFlags {
                    on_ground: false,
                    horizontal_collision: false,
                },
            }))
            .await?;
        client
            .io
            .send(&sb::Packet::ClientTickEnd(sb::ClientTickEnd {}))
            .await?;
        client
            .io
            .send(&sb::Packet::PingRequest(common::serverbound::PingRequest {
                time: i64::from(step),
            }))
            .await?;
        client
            .until(|_, p| matches!(p,cb::Packet::PongResponse(p) if p.time==i64::from(step)))
            .await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let target = (
        (start[0] + 100.).floor() as i32 >> 4,
        start[2].floor() as i32 >> 4,
    );
    if !client.chunks.contains(&target) || client.unloads == 0 {
        *phase = ScenarioPhase::DestinationChunks;
        client
            .until(|c, p| {
                c.chunks.contains(&target)
                    && c.unloads > 0
                    && matches!(p, cb::Packet::ChunkBatchFinished(_))
            })
            .await?;
    }
    let frozen_periodic_time = if sample_periodic_time {
        // Freeze only this parity fixture so the sampled gameTime can match saved overworld Time exactly.
        *phase = ScenarioPhase::Freeze;
        client.command("tick freeze").await?;
        client
            .until(|_, p| matches!(p, cb::Packet::TickingState(p) if p.is_frozen))
            .await?;
        *phase = ScenarioPhase::PeriodicTime;
        loop {
            if let cb::Packet::SetTime(time) = client.next().await? {
                break Some(time);
            }
        }
    } else {
        None
    };
    *phase = ScenarioPhase::Save;
    client.command("save-all flush").await?;
    client.until(|_,p|matches!(p,cb::Packet::SystemChat(p) if p.content.to_json().is_ok_and(|s|s.contains("Saved the game")||s.contains("commands.save.success")))).await?;
    *phase = ScenarioPhase::Stop;
    client.command("stop").await?;
    loop {
        let bytes = client.io.read().await?;
        let mut reader = Reader::with_context(&bytes, &client.registries);
        if let cb::Packet::Disconnect(_) = cb::Packet::decode(&mut reader)? {
            break;
        }
    }
    ensure!(
        client.position[0] >= start[0] + 99.,
        "movement was corrected"
    );
    Ok(Outcome {
        sequence: client.sequence,
        time_updates: client.time_updates,
        frozen_periodic_time,
        chunks: client.chunks.len(),
        unloaded: client.unloads,
        position: [start[0] + 100., start[1], start[2]],
    })
}
pub fn verify_save(root: &Path, position: [f64; 3]) -> Result<()> {
    let access = ferraloom_storage::LevelStorageAccess::open(root.join("world"))?;
    let level = access.read_level_data()?;
    ensure!(
        level.data.get_int_or("DataVersion", -1) == ferraloom_storage::DATA_VERSION,
        "saved level version"
    );
    let player = access
        .player_data()
        .load(&Uuid::offline_player(NAME).to_string())?
        .context("saved player missing")?;
    let pos = player.get_list("Pos").context("saved position missing")?;
    ensure!(
        pos.0
            == position
                .into_iter()
                .map(ferraloom_nbt::Tag::Double)
                .collect::<Vec<_>>(),
        "saved position differs: {:?}",
        pos
    );
    let mut regions = ferraloom_storage::RegionFileStorage::new(
        access.dimension("minecraft:overworld")?.region,
        ferraloom_storage::RegionFileVersion::Deflate,
        false,
    );
    let pos = ferraloom_util::ChunkPos::new(
        position[0].floor() as i32 >> 4,
        position[2].floor() as i32 >> 4,
    );
    let chunk = regions
        .read(pos)?
        .context("saved destination chunk missing")?;
    ferraloom_storage::SerializableChunkData::parse(chunk)?;
    Ok(())
}
