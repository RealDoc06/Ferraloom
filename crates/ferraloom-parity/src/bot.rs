//! Login and configuration are generalized from ferraloom-server/tests/support's real-jar bot.
use anyhow::{Context, Result, bail, ensure};
use ferraloom_data::packets as packet_types;
use ferraloom_protocol::{
    self as p, Reader, Uuid, Wire,
    codec::{BitSet, VarInt},
    common,
    component::{HashedStack, ItemStack},
    configuration as cfg, handshake, login,
    play::{self, clientbound as cb, serverbound as sb},
    transport::{CompressionDecoder, CompressionEncoder, FrameEncoder},
};
use ferraloom_server::{network::PacketIo, registries::RegistrySet};
use ferraloom_util::{BlockPos, Direction};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

/// Observed game-tick budget from SetTime; a separate wall-clock watchdog catches stopped clocks.
#[derive(Clone, Copy, Debug)]
pub struct Ticks(pub u32);
impl Ticks {
    pub const JOIN: Self = Self(2400);
    pub const DEFAULT: Self = Self(200);
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sequence {
    pub login: Vec<packet_types::login::clientbound::Packet>,
    pub configuration: Vec<(
        packet_types::configuration::clientbound::Packet,
        Option<String>,
    )>,
    pub registries: Vec<(String, Vec<String>)>,
    pub join: Vec<packet_types::play::clientbound::Packet>,
}
#[derive(Clone, Debug, Default)]
pub struct Container {
    pub state_id: i32,
    pub slots: Vec<ItemStack>,
    pub carried: ItemStack,
}
#[derive(Clone, Debug)]
pub struct Entity {
    pub id: i32,
    pub uuid: Uuid,
    pub entity_type: i32,
    pub position: [f64; 3],
    pub rotation: [f32; 2],
    pub data: BTreeMap<u8, p::entity_data::EntityDataValue>,
}
#[derive(Clone, Debug, Default)]
pub struct BotState {
    pub entity_id: Option<i32>,
    pub position: [f64; 3],
    pub rotation: [f32; 2],
    pub teleports: u64,
    pub game_time: Option<i64>,
    pub game_mode: Option<play::GameType>,
    pub difficulty: Option<cb::ChangeDifficulty>,
    pub chunks: BTreeSet<(i32, i32)>,
    pub unloads: usize,
    pub sections: BTreeMap<(i32, i32), Vec<play::LevelChunkSectionData>>,
    pub block_updates: BTreeMap<(i32, i32, i32), ferraloom_data::BlockState>,
    /// Overworld default; scenarios entering custom dimensions must set the minimum explicitly.
    pub min_section: i32,
    pub entities: BTreeMap<i32, Entity>,
    pub containers: BTreeMap<i32, Container>,
    /// Player Inventory indices (0..42), distinct from inventory-menu slot numbers.
    pub inventory: BTreeMap<i32, ItemStack>,
    /// Latest full advancement progress, including packets consumed during join.
    pub advancement_progress: BTreeMap<p::Identifier, play::AdvancementProgress>,
    pub scores: BTreeMap<(String, String), i32>,
    pub objectives: BTreeMap<String, play::ObjectiveOperation>,
    pub display_objectives: Vec<cb::SetDisplayObjective>,
    pub messages: Vec<cb::Packet>,
    pub error: Option<String>,
}
impl BotState {
    /// None means unknown/unloaded, never air. Includes single and section block updates.
    pub fn block(&self, pos: BlockPos) -> Option<ferraloom_data::BlockState> {
        if let Some(state) = self.block_updates.get(&(pos.x, pos.y, pos.z)) {
            return Some(*state);
        }
        let section = usize::try_from((pos.y >> 4) - self.min_section).ok()?;
        let palette = &self
            .sections
            .get(&(pos.x >> 4, pos.z >> 4))?
            .get(section)?
            .states;
        let index = ((pos.y & 15) * 256 + (pos.z & 15) * 16 + (pos.x & 15)) as usize;
        let id = if palette.bits == 0 {
            palette.palette.first()?.0
        } else {
            let per = 64 / usize::from(palette.bits);
            let mask = 1u64
                .checked_shl(u32::from(palette.bits))
                .map_or(u64::MAX, |v| v - 1);
            let value = (((*palette.data.get(index / per)? as u64)
                >> ((index % per) * usize::from(palette.bits)))
                & mask) as usize;
            if palette.palette.is_empty() {
                value as i32
            } else {
                palette.palette.get(value)?.0
            }
        };
        u16::try_from(id)
            .ok()
            .and_then(ferraloom_data::BlockState::from_id)
    }
}
pub struct Bot {
    pub name: String,
    state: Arc<Mutex<BotState>>,
    sequence: Arc<Mutex<Sequence>>,
    send: mpsc::Sender<sb::Packet>,
    events: broadcast::Receiver<Arc<cb::Packet>>,
    tasks: Vec<JoinHandle<()>>,
    sequence_id: i32,
    ping_id: i64,
    input: play::Input,
}
impl Drop for Bot {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
impl Bot {
    pub async fn connect(address: SocketAddr, name: &str) -> Result<Self> {
        tokio::time::timeout(Duration::from_secs(180), Self::connect_inner(address, name))
            .await
            .context("login/configuration watchdog")?
    }
    async fn connect_inner(address: SocketAddr, name: &str) -> Result<Self> {
        let socket = TcpStream::connect(address).await?;
        socket.set_nodelay(true)?;
        let mut io = PacketIo::new(socket);
        let mut compression = -1;
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
            name: name.into(),
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
                login::ClientboundPacket::Compression(p) => {
                    compression = p.threshold.0;
                    io.compression(compression);
                }
                login::ClientboundPacket::LoginFinished(p) => {
                    ensure!(
                        p.game_profile.id == Uuid::offline_player(name),
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
                            known_packs: p
                                .known_packs
                                .into_iter()
                                .filter(|pack| {
                                    pack.namespace == "minecraft"
                                        && pack.id == "core"
                                        && pack.version == "26.3"
                                })
                                .collect(),
                        },
                    ))
                    .await?
                }
                cfg::ClientboundPacket::RegistryData(p) => {
                    let entries = p
                        .entries
                        .into_iter()
                        .map(|entry| entry.id.to_string())
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
                cfg::ClientboundPacket::Ping(p) => {
                    io.send(&cfg::ServerboundPacket::Pong(common::serverbound::Pong {
                        id: p.id,
                    }))
                    .await?;
                }
                cfg::ClientboundPacket::Disconnect(p) => {
                    anyhow::bail!("configuration disconnect: {:?}", p.reason)
                }
                _ => {}
            }
        }
        Self::start_pump(io, registries, sequence, name.to_owned(), compression).await
    }
    async fn start_pump(
        io: PacketIo<TcpStream>,
        registries: RegistrySet,
        sequence: Sequence,
        name: String,
        compression: i32,
    ) -> Result<Self> {
        let (mut read, mut write) = io.socket.into_split();
        let (send, mut outbound) = mpsc::channel::<sb::Packet>(256);
        let (events, receiver) = broadcast::channel(4096);
        let state = Arc::new(Mutex::new(BotState {
            min_section: -4,
            ..BotState::default()
        }));
        let sequence = Arc::new(Mutex::new(sequence));
        let writer_state = state.clone();
        let writer_registries = registries.clone();
        let writer = tokio::spawn(async move {
            let result = async {
                let mut encoder = CompressionEncoder::new(compression);
                while let Some(packet) = outbound.recv().await {
                    let mut body = p::Writer::with_context(&writer_registries);
                    packet.encode(&mut body)?;
                    write
                        .write_all(&FrameEncoder::encode(&encoder.encode(body.as_slice())?)?)
                        .await?;
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if let Err(e) = result {
                writer_state.lock().unwrap().error = Some(format!("packet writer: {e:#}"));
            }
        });
        let reader_state = state.clone();
        let reader_sequence = sequence.clone();
        let reply = send.clone();
        let reader = tokio::spawn(async move {
            let result = async {
                let mut decoder = CompressionDecoder::new(compression, true);
                let mut joining = true;
                loop {
                    let mut length = 0usize;
                    let mut complete = false;
                    for i in 0..3 {
                        let b = read.read_u8().await?;
                        length |= usize::from(b & 127) << (i * 7);
                        if b & 128 == 0 {
                            complete = true;
                            break;
                        }
                    }
                    ensure!(complete && length > 0, "invalid frame length");
                    let mut bytes = vec![0; length];
                    read.read_exact(&mut bytes).await?;
                    let bytes = decoder.decode(&bytes)?;
                    let packet = {
                        let mut input = Reader::with_context(&bytes, &registries);
                        let packet = cb::Packet::decode(&mut input)?;
                        input.finish()?;
                        packet
                    };
                    if joining {
                        reader_sequence.lock().unwrap().join.push(
                            packet_types::play::clientbound::Packet::from_protocol_id(
                                packet.packet_id(),
                            )
                            .context("unknown play packet")?,
                        );
                        if matches!(packet, cb::Packet::SetEntityData(_)) {
                            joining = false;
                        }
                    }
                    let response = {
                        let mut state = reader_state.lock().unwrap();
                        apply(&mut state, &packet)?;
                        match &packet {
                            cb::Packet::KeepAlive(p) => {
                                Some(sb::Packet::KeepAlive(sb::KeepAlive { id: p.id }))
                            }
                            cb::Packet::Ping(p) => Some(sb::Packet::Pong(sb::Pong { id: p.id })),
                            cb::Packet::ChunkBatchFinished(_) => {
                                Some(sb::Packet::ChunkBatchReceived(sb::ChunkBatchReceived {
                                    desired_chunks_per_tick: 64.,
                                }))
                            }
                            cb::Packet::PlayerPosition(p) => {
                                Some(sb::Packet::AcceptTeleportation(sb::AcceptTeleportation {
                                    id: p.id,
                                    x: state.position[0],
                                    y: state.position[1],
                                    z: state.position[2],
                                    y_rot: state.rotation[0],
                                    x_rot: state.rotation[1],
                                }))
                            }
                            cb::Packet::Disconnect(p) => bail!("play disconnect: {:?}", p.reason),
                            _ => None,
                        }
                    };
                    if let Some(response) = response {
                        reply.send(response).await?;
                    }
                    if matches!(packet, cb::Packet::PlayerPosition(_)) {
                        reply
                            .send(sb::Packet::PlayerLoaded(sb::PlayerLoaded {}))
                            .await?;
                    }
                    let _ = events.send(Arc::new(packet));
                }
                #[allow(unreachable_code)]
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if let Err(e) = result {
                reader_state.lock().unwrap().error = Some(format!("packet reader: {e:#}"));
            }
        });
        let mut bot = Self {
            name,
            state,
            sequence,
            send,
            events: receiver,
            tasks: vec![reader, writer],
            sequence_id: 0,
            ping_id: 0,
            input: play::Input {
                forward: false,
                backward: false,
                left: false,
                right: false,
                jump: false,
                shift: false,
                sprint: false,
            },
        };
        bot.wait_for("join and first chunk batch", Ticks::JOIN, |s, p| {
            s.entity_id.is_some()
                && s.teleports > 0
                && !s.chunks.is_empty()
                && matches!(p, cb::Packet::ChunkBatchFinished(_))
        })
        .await?;
        Ok(bot)
    }
    /// Set this after a dimension change before querying blocks in a non-overworld world.
    pub fn set_min_section(&mut self, section: i32) {
        self.state.lock().unwrap().min_section = section;
    }
    pub fn snapshot(&self) -> BotState {
        self.state.lock().unwrap().clone()
    }
    pub fn sequence(&self) -> Sequence {
        self.sequence.lock().unwrap().clone()
    }
    /// Raw typed packet escape hatch. All normal actions use the same ordered writer.
    pub async fn send(&self, packet: sb::Packet) -> Result<()> {
        self.send.send(packet).await.context("bot writer stopped")
    }
    /// Waits consume packets in order, failing on queue overrun instead of losing observations.
    /// SetTime is emitted every 20 server ticks, so deadlines have that granularity.
    pub async fn wait_for(
        &mut self,
        label: &str,
        budget: Ticks,
        mut predicate: impl FnMut(&BotState, &cb::Packet) -> bool,
    ) -> Result<Arc<cb::Packet>> {
        let start = self.state.lock().unwrap().game_time;
        let mut baseline = start;
        let duration = Duration::from_secs(30) + Duration::from_millis(u64::from(budget.0) * 200);
        tokio::time::timeout(duration, async {
            loop {
                let packet = self.events.recv().await.with_context(|| {
                    format!(
                        "{label}: bot stream closed or observation queue overrun; {:?}",
                        self.state.lock().unwrap().error
                    )
                })?;
                let state = self.state.lock().unwrap();
                if predicate(&state, &packet) {
                    return Ok(packet);
                }
                if let Some(tick) = state.game_time {
                    let initial = *baseline.get_or_insert(tick);
                    ensure!(
                        tick - initial < i64::from(budget.0),
                        "{label}: exceeded {} observed game ticks ({} -> {tick})",
                        budget.0,
                        initial
                    );
                }
                if let Some(error) = &state.error {
                    bail!("{label}: {error}");
                }
            }
        })
        .await
        .with_context(|| {
            format!(
                "{label}: server progress watchdog expired (budget {} ticks)",
                budget.0
            )
        })?
    }
    pub async fn wait_ticks(&mut self, ticks: Ticks) -> Result<()> {
        let initialized = self.state.lock().unwrap().game_time.is_some();
        if !initialized {
            self.wait_for("initial tick clock", Ticks::DEFAULT, |_, p| {
                matches!(p, cb::Packet::SetTime(_))
            })
            .await?;
        }
        let start = self
            .state
            .lock()
            .unwrap()
            .game_time
            .context("no server clock")?;
        self.wait_for("observed game-tick wait", Ticks(ticks.0 + 40), |s, _| {
            s.game_time.is_some_and(|t| t - start >= i64::from(ticks.0))
        })
        .await?;
        Ok(())
    }
    pub async fn command(&self, text: &str) -> Result<()> {
        self.send(sb::Packet::ChatCommand(sb::ChatCommand {
            command: text.trim_start_matches('/').into(),
        }))
        .await
    }
    /// Uses the signable-command packet with unsigned arguments, as an offline client does.
    /// Plain command() intentionally keeps the no-signing-context command path available.
    pub async fn signed_command(&self, text: &str) -> Result<()> {
        self.send(sb::Packet::ChatCommandSigned(sb::ChatCommandSigned {
            command: text.trim_start_matches('/').into(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_millis()
                .try_into()?,
            salt: 0,
            argument_signatures: play::ArgumentSignatures(Vec::new()),
            last_seen_messages: play::LastSeenMessagesUpdate {
                offset: VarInt(0),
                acknowledged: BitSet::default(),
                checksum: 0,
            },
        }))
        .await
    }
    pub async fn chat(&self, text: &str) -> Result<()> {
        self.send(sb::Packet::Chat(Box::new(sb::Chat {
            message: common::Utf8(text.into()),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_millis()
                .try_into()?,
            salt: 0,
            signature: None,
            last_seen_messages: play::LastSeenMessagesUpdate {
                offset: VarInt(0),
                acknowledged: BitSet::default(),
                checksum: 0,
            },
        })))
        .await
    }
    /// An ordered packet barrier, also useful to surface pending authoritative corrections.
    pub async fn synchronize(&mut self) -> Result<()> {
        self.ping_id += 1;
        let time = self.ping_id;
        self.send(sb::Packet::PingRequest(sb::PingRequest { time }))
            .await?;
        self.wait_for(
            "ping barrier",
            Ticks::DEFAULT,
            |_, p| matches!(p,cb::Packet::PongResponse(p) if p.time==time),
        )
        .await?;
        Ok(())
    }
    pub async fn teleport(&mut self, position: [f64; 3]) -> Result<()> {
        let count = self.state.lock().unwrap().teleports;
        self.command(&format!(
            "tp {} {} {}",
            position[0], position[1], position[2]
        ))
        .await?;
        self.wait_for("teleport", Ticks::DEFAULT, |s, p| {
            s.teleports > count && matches!(p, cb::Packet::PlayerPosition(_))
        })
        .await?;
        self.synchronize().await
    }
    /// At most one block per 100 ms; tick-end follows every positional packet.
    /// For collision-free travel use spectator, as in the original real-jar join bot.
    pub async fn move_to(&mut self, target: [f64; 3], on_ground: bool) -> Result<()> {
        ensure!(
            target.into_iter().all(f64::is_finite),
            "non-finite movement"
        );
        let start = self.state.lock().unwrap().position;
        let distance = start
            .into_iter()
            .zip(target)
            .map(|(a, b)| (a - b).abs())
            .fold(0., f64::max);
        let steps = distance.ceil().max(1.) as u32;
        ensure!(
            steps <= 10000,
            "movement path is too long; split into explicit legs"
        );
        for step in 1..=steps {
            let pos = std::array::from_fn(|i| {
                start[i] + (target[i] - start[i]) * f64::from(step) / f64::from(steps)
            });
            self.move_step(pos, on_ground).await?;
        }
        Ok(())
    }
    async fn move_step(&mut self, position: [f64; 3], on_ground: bool) -> Result<()> {
        let rotation = self.state.lock().unwrap().rotation;
        {
            self.state.lock().unwrap().position = position;
        }
        self.send(sb::Packet::MovePlayerPosRot(sb::MovePlayerPosRot {
            x: position[0],
            y: position[1],
            z: position[2],
            y_rot: rotation[0],
            x_rot: rotation[1],
            flags: play::MovementFlags {
                on_ground,
                horizontal_collision: false,
            },
        }))
        .await?;
        self.send(sb::Packet::ClientTickEnd(sb::ClientTickEnd {}))
            .await?;
        self.synchronize().await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        Ok(())
    }
    pub async fn look(&mut self, yaw: f32, pitch: f32) -> Result<()> {
        ensure!(yaw.is_finite() && pitch.is_finite(), "non-finite look");
        self.state.lock().unwrap().rotation = [yaw, pitch];
        self.send(sb::Packet::MovePlayerRot(sb::MovePlayerRot {
            y_rot: yaw,
            x_rot: pitch,
            flags: play::MovementFlags {
                on_ground: false,
                horizontal_collision: false,
            },
        }))
        .await?;
        self.send(sb::Packet::ClientTickEnd(sb::ClientTickEnd {}))
            .await?;
        self.synchronize().await
    }
    pub async fn input(&mut self, input: play::Input) -> Result<()> {
        self.input = input;
        self.send(sb::Packet::PlayerInput(sb::PlayerInput { input }))
            .await
    }
    pub async fn sneak(&mut self, enabled: bool) -> Result<()> {
        self.input(play::Input {
            shift: enabled,
            ..self.input
        })
        .await
    }
    /// Sets the client's jump input. Supply positional trajectory with move_to/move_step,
    /// since the server validates client movement rather than simulating it for a bot.
    pub async fn jump(&mut self, pressed: bool) -> Result<()> {
        self.input(play::Input {
            jump: pressed,
            ..self.input
        })
        .await
    }
    fn next_sequence(&mut self) -> VarInt {
        self.sequence_id += 1;
        VarInt(self.sequence_id)
    }
    pub async fn use_item(&mut self, hand: play::InteractionHand) -> Result<()> {
        let sequence = self.next_sequence();
        let rotation = self.state.lock().unwrap().rotation;
        self.send(sb::Packet::UseItem(sb::UseItem {
            hand,
            sequence,
            y_rot: rotation[0],
            x_rot: rotation[1],
        }))
        .await
    }
    pub async fn use_item_on(
        &mut self,
        hand: play::InteractionHand,
        hit_result: play::BlockHitResult,
    ) -> Result<()> {
        let sequence = self.next_sequence();
        self.send(sb::Packet::UseItemOn(sb::UseItemOn {
            hand,
            hit_result,
            sequence,
        }))
        .await
    }
    pub async fn attack(&self, entity_id: i32) -> Result<()> {
        self.send(sb::Packet::Attack(sb::Attack {
            entity_id: VarInt(entity_id),
        }))
        .await
    }
    pub async fn swing(&self) -> Result<()> {
        self.send(sb::Packet::Punch(sb::Punch {})).await
    }
    pub async fn pick_block(&self, pos: BlockPos, include_data: bool) -> Result<()> {
        self.send(sb::Packet::PickItemFromBlock(sb::PickItemFromBlock {
            pos,
            include_data,
        }))
        .await
    }
    pub async fn drop_item(&mut self, whole_stack: bool) -> Result<()> {
        self.player_action(if whole_stack {
            play::PlayerAction::DropAllItems
        } else {
            play::PlayerAction::DropItem
        })
        .await
    }
    pub async fn swap_hands(&mut self) -> Result<()> {
        self.player_action(play::PlayerAction::SwapItemWithOffhand)
            .await
    }
    async fn player_action(&mut self, action: play::PlayerAction) -> Result<()> {
        let sequence = self.next_sequence();
        self.send(sb::Packet::PlayerAction(sb::PlayerActionPacket {
            action,
            pos: BlockPos::new(0, 0, 0),
            direction: play::DirectionByte(Direction::Down),
            sequence,
        }))
        .await
    }
    /// Caller supplies the expected result of the click, not untyped or precomputed hashes.
    /// This supports every menu without duplicating the game's slot rules in the harness.
    pub async fn click(
        &self,
        container_id: i32,
        slot: i16,
        button: i8,
        kind: play::ContainerInput,
        predicted_slots: &[(i16, ItemStack)],
        carried: &ItemStack,
    ) -> Result<()> {
        let state_id = self
            .state
            .lock()
            .unwrap()
            .containers
            .get(&container_id)
            .context("container has not been received")?
            .state_id;
        let changed_slots = play::ChangedSlots(
            predicted_slots
                .iter()
                .map(|(slot, stack)| {
                    Ok(play::ChangedSlot {
                        slot: *slot,
                        stack: HashedStack::create(stack)?,
                    })
                })
                .collect::<Result<_>>()?,
        );
        self.send(sb::Packet::ContainerClick(sb::ContainerClick {
            container_id: VarInt(container_id),
            state_id: VarInt(state_id),
            slot_num: slot,
            button_num: button,
            container_input: kind,
            changed_slots,
            carried_item: HashedStack::create(carried)?,
        }))
        .await
    }
}
fn apply(s: &mut BotState, p: &cb::Packet) -> Result<()> {
    match p {
        cb::Packet::Login(p) => {
            s.entity_id = Some(p.player_id);
            s.game_mode = Some(p.common_player_spawn_info.game_type);
        }
        cb::Packet::PlayerPosition(p) => {
            let xyz = [
                p.change.position.x,
                p.change.position.y,
                p.change.position.z,
            ];
            for (i, v) in xyz.into_iter().enumerate() {
                s.position[i] = if p.relatives & (1 << i) != 0 {
                    s.position[i] + v
                } else {
                    v
                };
            }
            s.rotation = [
                if p.relatives & 8 != 0 {
                    s.rotation[0] + p.change.y_rot
                } else {
                    p.change.y_rot
                },
                if p.relatives & 16 != 0 {
                    s.rotation[1] + p.change.x_rot
                } else {
                    p.change.x_rot
                },
            ];
            s.teleports += 1;
        }
        cb::Packet::SetTime(p) => s.game_time = Some(p.game_time),
        cb::Packet::GameEvent(p) if p.event == 3 => {
            s.game_mode = Some(match p.param as u8 {
                1 => play::GameType::Creative,
                2 => play::GameType::Adventure,
                3 => play::GameType::Spectator,
                _ => play::GameType::Survival,
            })
        }
        cb::Packet::LevelChunkWithLight(p) => {
            s.chunks.insert((p.x, p.z));
            s.sections.insert((p.x, p.z), p.chunk_data.sections.clone());
            s.block_updates
                .retain(|&(x, _, z), _| (x >> 4, z >> 4) != (p.x, p.z));
        }
        cb::Packet::ForgetLevelChunk(p) => {
            s.unloads += 1;
            s.chunks.remove(&(p.pos.x, p.pos.z));
            s.sections.remove(&(p.pos.x, p.pos.z));
            s.block_updates
                .retain(|&(x, _, z), _| (x >> 4, z >> 4) != (p.pos.x, p.pos.z));
        }
        cb::Packet::BlockUpdate(p) => {
            s.block_updates.insert(
                (p.pos.x, p.pos.y, p.pos.z),
                u16::try_from(p.block_state.0)
                    .ok()
                    .and_then(ferraloom_data::BlockState::from_id)
                    .context("invalid block update")?,
            );
        }
        cb::Packet::SectionBlocksUpdate(p) => {
            for c in &p.changes {
                s.block_updates.insert(
                    (
                        (p.section_pos.x << 4) + i32::from((c.position >> 8) & 15),
                        (p.section_pos.y << 4) + i32::from(c.position & 15),
                        (p.section_pos.z << 4) + i32::from((c.position >> 4) & 15),
                    ),
                    c.state,
                );
            }
        }
        cb::Packet::AddEntity(p) => {
            s.entities.insert(
                p.id.0,
                Entity {
                    id: p.id.0,
                    uuid: p.uuid,
                    entity_type: p.entity_type.0,
                    position: [p.x, p.y, p.z],
                    rotation: [
                        f32::from(p.y_rot.0) * 360. / 256.,
                        f32::from(p.x_rot.0) * 360. / 256.,
                    ],
                    data: BTreeMap::new(),
                },
            );
        }
        cb::Packet::SetEntityData(p) => {
            if let Some(e) = s.entities.get_mut(&p.id.0) {
                for v in &p.packed_items.0 {
                    e.data.insert(v.id, v.value.clone());
                }
            }
        }
        cb::Packet::RemoveEntities(p) => {
            for id in &p.entity_ids {
                s.entities.remove(&id.0);
            }
        }
        cb::Packet::MoveEntityPos(p) => {
            if let Some(e) = s.entities.get_mut(&p.entity_id.0) {
                apply_delta(e, &p.delta);
            }
        }
        cb::Packet::MoveEntityPosRot(p) => {
            if let Some(e) = s.entities.get_mut(&p.entity_id.0) {
                apply_delta(e, &p.delta);
                e.rotation = [
                    f32::from(p.y_rot.0) * 360. / 256.,
                    f32::from(p.x_rot.0) * 360. / 256.,
                ];
            }
        }
        cb::Packet::MoveEntityRot(p) => {
            if let Some(e) = s.entities.get_mut(&p.entity_id.0) {
                e.rotation = [
                    f32::from(p.y_rot.0) * 360. / 256.,
                    f32::from(p.x_rot.0) * 360. / 256.,
                ];
            }
        }
        cb::Packet::TeleportEntity(p) => {
            if let Some(e) = s.entities.get_mut(&p.id.0) {
                for (i, v) in [
                    p.change.position.x,
                    p.change.position.y,
                    p.change.position.z,
                ]
                .into_iter()
                .enumerate()
                {
                    e.position[i] = if p.relatives & (1 << i) != 0 {
                        e.position[i] + v
                    } else {
                        v
                    };
                }
                e.rotation = [
                    if p.relatives & 8 != 0 {
                        e.rotation[0] + p.change.y_rot
                    } else {
                        p.change.y_rot
                    },
                    if p.relatives & 16 != 0 {
                        e.rotation[1] + p.change.x_rot
                    } else {
                        p.change.x_rot
                    },
                ];
            }
        }
        cb::Packet::EntityPositionSync(p) => {
            if let Some(e) = s.entities.get_mut(&p.id.0) {
                let pos = match &p.position {
                    play::PositionPath::Linear(v) => v,
                    play::PositionPath::Stepped(v) => {
                        &v.last().context("empty entity path")?.position
                    }
                };
                e.position = [pos.x, pos.y, pos.z];
                e.rotation = [p.y_rot, p.x_rot];
            }
        }
        cb::Packet::ContainerSetContent(p) => {
            if p.container_id.0 == 0 {
                for (slot, stack) in p.items.iter().enumerate() {
                    if let Some(index) = inventory_slot(slot) {
                        s.inventory.insert(index, stack.clone());
                    }
                }
            }
            s.containers.insert(
                p.container_id.0,
                Container {
                    state_id: p.state_id.0,
                    slots: p.items.clone(),
                    carried: p.carried_item.clone(),
                },
            );
        }
        cb::Packet::ContainerSetSlot(p) => {
            let c = s.containers.entry(p.container_id.0).or_default();
            c.state_id = p.state_id.0;
            if let Ok(slot) = usize::try_from(p.slot) {
                c.slots
                    .resize(c.slots.len().max(slot + 1), ItemStack::default());
                c.slots[slot] = p.item_stack.clone();
                if p.container_id.0 == 0
                    && let Some(index) = inventory_slot(slot)
                {
                    s.inventory.insert(index, p.item_stack.clone());
                }
            }
        }
        cb::Packet::SetPlayerInventory(p) => {
            s.inventory.insert(p.slot.0, p.contents.clone());
            let menu_slot = match p.slot.0 {
                0..=8 => Some(p.slot.0 as usize + 36),
                9..=35 => Some(p.slot.0 as usize),
                36..=39 => Some(44 - p.slot.0 as usize),
                40 => Some(45),
                _ => None,
            };
            if let Some(slot) = menu_slot {
                let c = s.containers.entry(0).or_default();
                c.slots
                    .resize(c.slots.len().max(slot + 1), ItemStack::default());
                c.slots[slot] = p.contents.clone();
            }
        }
        cb::Packet::SetCursorItem(p) => {
            for c in s.containers.values_mut() {
                c.carried = p.contents.clone();
            }
        }
        cb::Packet::ContainerClose(p) => {
            s.containers.remove(&p.container_id.0);
        }
        cb::Packet::ChangeDifficulty(p) => s.difficulty = Some(p.clone()),
        cb::Packet::SetScore(p) => {
            s.scores
                .insert((p.objective_name.clone(), p.owner.clone()), p.score.0);
        }
        cb::Packet::UpdateAdvancements(p) => {
            if p.should_reset {
                s.advancement_progress.clear();
            }
            for id in &p.removed {
                s.advancement_progress.remove(id);
            }
            for progress in &p.progress {
                s.advancement_progress
                    .insert(progress.id.clone(), progress.clone());
            }
        }
        cb::Packet::ResetScore(p) => s.scores.retain(|(objective, owner), _| {
            owner != &p.owner || p.objective_name.as_ref().is_some_and(|o| o != objective)
        }),
        cb::Packet::SetObjective(p) => {
            if matches!(p.operation, play::ObjectiveOperation::Remove {}) {
                s.objectives.remove(&p.objective_name);
                s.scores
                    .retain(|(objective, _), _| objective != &p.objective_name);
                s.display_objectives
                    .retain(|display| display.objective_name != p.objective_name);
            } else {
                s.objectives
                    .insert(p.objective_name.clone(), p.operation.clone());
            }
        }
        cb::Packet::SetDisplayObjective(p) => {
            s.display_objectives
                .retain(|display| display.slot != p.slot);
            if !p.objective_name.is_empty() {
                s.display_objectives.push(p.clone());
            }
        }
        cb::Packet::PlayerChat(_) | cb::Packet::SystemChat(_) | cb::Packet::DisguisedChat(_) => {
            s.messages.push(p.clone())
        }
        _ => {}
    }
    Ok(())
}
/// Port of InventoryMenu's equipment/hotbar slot layout to Inventory indices.
fn inventory_slot(menu_slot: usize) -> Option<i32> {
    match menu_slot {
        5..=8 => Some(44 - menu_slot as i32),
        9..=35 => Some(menu_slot as i32),
        36..=44 => Some(menu_slot as i32 - 36),
        45 => Some(40),
        _ => None,
    }
}
/// Port of VecDeltaCodec#decode: zero axes preserve precision; Math.round ties toward +infinity.
fn apply_delta(entity: &mut Entity, delta: &play::EntityDelta) {
    let mut apply = |x: i16, y: i16, z: i16| {
        for (i, v) in [x, y, z].into_iter().enumerate() {
            if v != 0 {
                entity.position[i] =
                    ((entity.position[i] * 4096. + 0.5).floor() + f64::from(v)) / 4096.;
            }
        }
    };
    match &delta.delta {
        play::VecDelta::Linear { xa, ya, za } => apply(*xa, *ya, *za),
        play::VecDelta::Stepped(steps) => {
            for step in steps {
                apply(step.xa, step.ya, step.za)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn advancement_updates_reset_remove_and_replace_whole_progress() -> Result<()> {
        let first = p::Identifier::parse("test:first")?;
        let second = p::Identifier::parse("test:second")?;
        let progress = |id: &p::Identifier, names: &[&str]| play::AdvancementProgress {
            id: id.clone(),
            criteria: names
                .iter()
                .map(|name| play::CriterionProgress {
                    name: (*name).into(),
                    obtained: Some(1),
                })
                .collect(),
        };
        let packet = |reset, removed, progress| {
            cb::Packet::UpdateAdvancements(cb::UpdateAdvancements {
                should_reset: reset,
                added: Vec::new(),
                removed,
                progress,
                show_advancements: true,
            })
        };
        let mut state = BotState::default();
        apply(
            &mut state,
            &packet(
                true,
                Vec::new(),
                vec![progress(&first, &["a", "b"]), progress(&second, &["c"])],
            ),
        )?;
        let replacement = progress(&first, &["new"]);
        apply(
            &mut state,
            &packet(false, vec![second.clone()], vec![replacement.clone()]),
        )?;
        assert_eq!(state.advancement_progress.len(), 1);
        assert_eq!(state.advancement_progress[&first], replacement);
        assert!(!state.advancement_progress.contains_key(&second));
        apply(
            &mut state,
            &packet(true, Vec::new(), vec![progress(&second, &["reset"])]),
        )?;
        assert_eq!(state.advancement_progress.len(), 1);
        assert!(!state.advancement_progress.contains_key(&first));
        assert_eq!(
            state.advancement_progress[&second],
            progress(&second, &["reset"])
        );
        Ok(())
    }
    #[test]
    fn entity_delta_preserves_zero_axes_and_java_negative_ties() {
        let mut entity = Entity {
            id: 1,
            uuid: Uuid::NIL,
            entity_type: 0,
            position: [-0.5 / 4096., 12.123456789, 0.],
            rotation: [0.; 2],
            data: BTreeMap::new(),
        };
        apply_delta(
            &mut entity,
            &play::EntityDelta {
                on_ground: false,
                delta: play::VecDelta::Linear {
                    xa: 1,
                    ya: 0,
                    za: -1,
                },
            },
        );
        assert_eq!(entity.position, [1. / 4096., 12.123456789, -1. / 4096.]);
    }
    #[test]
    fn block_queries_distinguish_unknown_from_air_and_apply_section_updates() -> Result<()> {
        let mut state = BotState {
            min_section: -4,
            ..BotState::default()
        };
        let pos = BlockPos::new(-1, -1, -1);
        assert_eq!(state.block(pos), None);
        let section = play::LevelChunkSectionData {
            non_empty_block_count: 0,
            fluid_count: 0,
            states: play::PalettedContainerData {
                bits: 0,
                palette: vec![VarInt(i32::from(
                    ferraloom_data::Block::AIR.default_state().id(),
                ))],
                data: vec![],
            },
            biomes: play::PalettedContainerData {
                bits: 0,
                palette: vec![VarInt(0)],
                data: vec![],
            },
        };
        state.sections.insert((-1, -1), vec![section; 4]);
        state.chunks.insert((-1, -1));
        assert_eq!(
            state.block(pos),
            Some(ferraloom_data::Block::AIR.default_state())
        );
        apply(
            &mut state,
            &cb::Packet::SectionBlocksUpdate(cb::SectionBlocksUpdate {
                section_pos: play::SectionPos {
                    x: -1,
                    y: -1,
                    z: -1,
                },
                changes: vec![play::SectionBlockChange {
                    position: 4095,
                    state: ferraloom_data::Block::STONE.default_state(),
                }],
            }),
        )?;
        assert_eq!(
            state.block(pos),
            Some(ferraloom_data::Block::STONE.default_state())
        );
        apply(
            &mut state,
            &cb::Packet::ForgetLevelChunk(cb::ForgetLevelChunk {
                pos: play::ChunkPos { x: -1, z: -1 },
            }),
        )?;
        assert_eq!(state.block(pos), None);
        Ok(())
    }
}
