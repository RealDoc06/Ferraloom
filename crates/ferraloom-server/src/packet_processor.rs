//! Native PacketProcessor and MinecraftServer's main-executor task queue.
//! Receipt order is validated once before a packet enters either native queue.
use super::*;

/// Port of PacketProcessor.ListenerAndPacket. The connection owns its listener;
/// queued payloads move in receipt order without cloning.
pub(super) struct ListenerAndPacket {
    id: ConnectionId,
    packet: Incoming,
}

pub(super) enum PacketEvent {
    Packet(ListenerAndPacket),
    Closed(ConnectionId),
    Console(String),
}

/// Port of TickTask: the birth tick controls MinecraftServer#shouldRun's
/// starvation guard. Only tryHandleChat's server.execute calls enter this queue.
pub(super) struct TickTask {
    tick: u64,
    packet: ListenerAndPacket,
}

impl MinecraftServer {
    /// Accept FIFO ingress outside tickChildren. Native login control work can
    /// reply now; ordinary packets wait for processQueuedPackets before the tick.
    fn receive_ingress(&mut self, event: Ingress) -> Result<()> {
        match event {
            Ingress::Admission {
                id,
                profile,
                address,
                reply,
            } => {
                let full = self.players.len() >= self.properties.int("max-players").max(0) as usize;
                let reason = self.access.can_join(
                    &profile,
                    address.ip(),
                    self.properties.boolean("white-list"),
                    full,
                    &self.shared().ops,
                );
                if reason.is_none() {
                    let duplicates = self
                        .connections
                        .iter()
                        .filter(|(other, c)| **other != id && c.profile.id == profile.id)
                        .map(|(id, _)| *id)
                        .collect::<Vec<_>>();
                    for id in duplicates {
                        self.connections
                            .get_mut(&id)
                            .unwrap()
                            .disconnect("You logged in from another location");
                    }
                }
                let _ = reply.send(reason);
            }
            Ingress::Connected {
                id,
                profile,
                address,
                outbound,
            } => {
                let mut c = Connection {
                    profile,
                    address,
                    outbound,
                    phase: Phase::Configuration(ConfigTask::KnownPacks),
                    incoming_sequence: 0,
                    outgoing_sequence: 0,
                    pending: Vec::new(),
                    close: false,
                    information: client_information(),
                    spawn: None,
                    spawn_input: None,
                    player_load: None,
                    player_save: None,
                    last_keepalive: Instant::now(),
                    keepalive: None,
                    last_action: Instant::now(),
                    chat_spam: 0,
                    command_spam: 0,
                    session: None,
                    last_seen: LastSeenMessagesValidator::default(),
                    signature_cache: MessageSignatureCache::default(),
                    command_message: None,
                    command_suggestion: None,
                    next_chat_index: 0,
                    network_ticks: 0,
                    rate_packets: 0,
                    average_packets: 0.,
                    received_position_this_tick: false,
                    received_movement_this_tick: false,
                    ack_block_changes_up_to: -1,
                };
                c.push(cfg::ClientboundPacket::CustomPayload(
                    common_cb::CustomPayload {
                        payload: common::CustomPayload::Brand("Ferraloom".into()),
                    },
                ));
                if !self.properties.get("bug-report-link").is_empty() {
                    c.push(cfg::ClientboundPacket::ServerLinks(
                        common_cb::ServerLinks {
                            links: vec![common_cb::ServerLink {
                                link_type: protocol::codec::Either::Left(
                                    common_cb::KnownLinkType::BugReport,
                                ),
                                link: self.properties.get("bug-report-link").into(),
                            }],
                        },
                    ));
                }
                c.push(cfg::ClientboundPacket::UpdateEnabledFeatures(
                    cfg::UpdateEnabledFeatures {
                        features: vec![crate::registries::id("vanilla")],
                    },
                ));
                c.push(cfg::ClientboundPacket::SelectKnownPacks(
                    cfg::ClientboundSelectKnownPacks {
                        known_packs: vec![core_pack()],
                    },
                ));
                self.connections.insert(id, c);
            }
            Ingress::Packet {
                id,
                sequence,
                packet,
            } => {
                let Some(connection) = self.connections.get_mut(&id) else {
                    return Ok(());
                };
                if connection.close {
                    return Ok(());
                }
                ensure!(
                    sequence == connection.incoming_sequence,
                    "connection {id} ingress sequence mismatch"
                );
                connection.incoming_sequence += 1;
                let main_executor = packet.uses_main_executor();
                let packet = ListenerAndPacket { id, packet };
                if main_executor {
                    self.pending_tasks.push_back(TickTask {
                        tick: self.tick,
                        packet,
                    });
                } else {
                    self.packet_processor.push_back(PacketEvent::Packet(packet));
                }
            }
            Ingress::Closed { id } => self.packet_processor.push_back(PacketEvent::Closed(id)),
            Ingress::Console(command) => self
                .packet_processor
                .push_back(PacketEvent::Console(command)),
        }
        Ok(())
    }

    fn receive_available_ingress(&mut self) -> Result<()> {
        while let Ok(event) = self.ingress.try_recv() {
            self.receive_ingress(event)?;
        }
        Ok(())
    }

    /// Port of PacketProcessor#processQueuedPackets at processPacketsAndTick.
    /// Unlike the old ingress-length snapshot, callbacks can enqueue more work
    /// in this pass. Main-executor tasks remain in their separate native queue.
    pub(super) async fn process_packets(&mut self) -> Result<()> {
        self.receive_available_ingress()?;
        while let Some(event) = self.packet_processor.pop_front() {
            match event {
                PacketEvent::Packet(packet) => self.handle_queued_packet(packet).await?,
                PacketEvent::Closed(id) => {
                    let dimension = match self.connections.get(&id).map(|c| c.phase) {
                        Some(Phase::Play(entity)) => self
                            .level_for_entity(entity)
                            .map_err(game_error)?
                            .dimension_key()
                            .map_err(game_error)?
                            .to_string()
                            .parse::<protocol::Identifier>()?,
                        _ => crate::registries::id("overworld"),
                    };
                    let lent = self.lend_server_shared_to(&dimension)?;
                    let result = self.remove_player(id);
                    self.return_server_shared(lent);
                    result?;
                }
                PacketEvent::Console(command) => {
                    if let Err(e) = self.execute_command(None, &command).await {
                        eprintln!("{e:#}");
                    }
                }
            }
            // This slot precedes tickChildren#suspendFlushing. Feedback is
            // visible now, while holder block changes remain until broadcast.
            self.flush_outbound();
            self.receive_available_ingress()?;
        }
        Ok(())
    }

    async fn handle_queued_packet(&mut self, packet: ListenerAndPacket) -> Result<()> {
        let ListenerAndPacket { id, packet } = packet;
        let Some(c) = self.connections.get(&id) else {
            return Ok(());
        };
        if c.close {
            return Ok(());
        }
        let phase = c.phase;
        let preparing_dimension = c.spawn.as_ref().map(|spawn| spawn.dimension().clone());
        let dimension = match phase {
            Phase::Play(entity) => Some(
                self.level_for_entity(entity)
                    .map_err(game_error)?
                    .dimension_key()
                    .map_err(game_error)?
                    .to_string()
                    .parse::<protocol::Identifier>()?,
            ),
            Phase::Configuration(_) => preparing_dimension,
        };
        let lent = match &dimension {
            Some(dimension) => self.lend_server_shared_to(dimension)?,
            None => false,
        };
        let result = match packet {
            Incoming::Configuration(p) => self.configuration(id, p),
            Incoming::Play(p) => self.play_packet(id, *p).await,
        };
        self.return_server_shared(lent);
        if let Err(e) = result {
            eprintln!("Packet rejected for {id}: {e:#}");
            if let Some(c) = self.connections.get_mut(&id) {
                c.disconnect(e.to_string());
            }
        }
        Ok(())
    }

    /// Main-executor half of MinecraftServer#pollTask. None is managedBlock's
    /// shouldRunAllTasks; a deadline uses the native three-tick starvation guard.
    async fn poll_task(&mut self, deadline: Option<tokio::time::Instant>) -> Result<bool> {
        self.receive_available_ingress()?;
        // Login/control replies also flush outside tickChildren, even when no
        // chat task is ready. No holder changes are drained here.
        self.flush_outbound();
        let Some(task) = self.pending_tasks.front() else {
            return Ok(false);
        };
        if let Some(deadline) = deadline
            && task.tick.wrapping_add(3) >= self.tick
            && tokio::time::Instant::now() >= deadline
        {
            return Ok(false);
        }
        let task = self
            .pending_tasks
            .pop_front()
            .expect("ready main-executor task");
        self.handle_queued_packet(task.packet).await?;
        self.flush_outbound();
        Ok(true)
    }

    /// Port of BlockableEventLoop#runAllTasks before the next-tick managed wait.
    async fn run_all_tasks(&mut self, deadline: tokio::time::Instant) -> Result<()> {
        while self.poll_task(Some(deadline)).await? {}
        Ok(())
    }

    /// Port of MinecraftServer#waitUntilNextTick's main-executor polling. Ingress
    /// wakes the wait, but PacketProcessor work never executes in this idle slot.
    /// Tick phases, gameTime, holder broadcasts and packet ACKs are not advanced.
    pub(super) async fn wait_until_next_tick(
        &mut self,
        deadline: tokio::time::Instant,
        tick_length: Duration,
    ) -> Result<()> {
        let delayed_tasks_deadline = (tokio::time::Instant::now() + tick_length).max(deadline);
        self.run_all_tasks(delayed_tasks_deadline).await?;
        while !self.stop && tokio::time::Instant::now() < deadline {
            if self.poll_task(None).await? {
                continue;
            }
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(deadline) => break,
                event = self.ingress.recv() => {
                    match event {
                        Some(event) => self.receive_ingress(event)?,
                        None => {
                            tokio::time::sleep_until(deadline).await;
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
