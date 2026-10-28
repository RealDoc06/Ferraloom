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
/// starvation guard. Chat/commands and captured server.execute POI work share
/// the native queue, preserving removal/addition order across task callbacks.
pub(super) struct TickTask {
    tick: u64,
    action: MainThreadAction,
}

pub(super) enum MainThreadAction {
    Packet(ListenerAndPacket),
    Poi {
        dimension: protocol::Identifier,
        update: ferraloom_game::level::poi::PoiUpdate,
    },
}

/// MinecraftServer#haveTime's two deadlines during waitUntilNextTick. A
/// successful poll retains the delayed-task budget; an idle poll drops it.
struct NextTickWait {
    next_tick: tokio::time::Instant,
    delayed_tasks: tokio::time::Instant,
    may_have_delayed_tasks: bool,
}
impl NextTickWait {
    fn deadline(&self) -> tokio::time::Instant {
        if self.may_have_delayed_tasks {
            self.delayed_tasks
        } else {
            self.next_tick
        }
    }
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
                        action: MainThreadAction::Packet(packet),
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

    pub(super) fn queue_poi_update(
        &mut self,
        dimension: protocol::Identifier,
        update: ferraloom_game::level::poi::PoiUpdate,
    ) {
        self.pending_tasks.push_back(TickTask {
            tick: self.tick,
            action: MainThreadAction::Poi { dimension, update },
        });
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

    /// Port of MinecraftServer#pollTaskInternal: main executor first, then one
    /// chunk-source task in getAllLevels order. None is shouldRunAllTasks; a
    /// deadline retains the native three-tick starvation guard for main tasks.
    async fn poll_task(&mut self, deadline: Option<tokio::time::Instant>) -> Result<bool> {
        self.receive_available_ingress()?;
        // Login/control replies also flush outside tickChildren, even when no
        // chat task is ready. No holder changes are drained here.
        self.flush_outbound();
        if let Some(task) = self.pending_tasks.pop_front_if(|task| {
            task.tick.wrapping_add(3) < self.tick
                || deadline.is_none_or(|deadline| tokio::time::Instant::now() < deadline)
        }) {
            let previous = self
                .live_levels_mut()
                .map(|live| live.level.set_poi_executor_running(true))
                .collect::<Vec<_>>();
            let result = match task.action {
                MainThreadAction::Packet(packet) => self.handle_queued_packet(packet).await,
                MainThreadAction::Poi { dimension, update } => self
                    .live_level_in_mut(&dimension)
                    .and_then(|live| live.level.apply_poi_update(update).map_err(game_error)),
            };
            for (live, previous) in self.live_levels_mut().zip(previous) {
                live.level.set_poi_executor_running(previous);
            }
            result?;
            self.flush_outbound();
            return Ok(true);
        }
        if self.tick_rate_manager.is_sprinting()
            || deadline.is_none_or(|deadline| tokio::time::Instant::now() < deadline)
        {
            for dimension in self.level_keys() {
                let lent = self.lend_server_shared_to(&dimension)?;
                let result = (|| -> Result<_> {
                    let progress = self
                        .live_level_in_mut(&dimension)?
                        .level
                        .poll_chunk_acquisition_mut()
                        .map_err(game_error)?;
                    if progress != ferraloom_game::level::acquisition::AcquireProgress::Idle {
                        // Publication callbacks emit in this task's Shared loan.
                        // tickChildren's flushing suspension has already ended.
                        self.apply_level_effects_in(&dimension)?;
                    }
                    Ok(progress)
                })();
                self.return_server_shared(lent);
                if result? != ferraloom_game::level::acquisition::AcquireProgress::Idle {
                    self.flush_outbound();
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Port of BlockableEventLoop#runAllTasks before the next-tick managed wait.
    async fn run_all_tasks(&mut self, wait: &mut NextTickWait) -> Result<()> {
        loop {
            wait.may_have_delayed_tasks = self.poll_task(Some(wait.deadline())).await?;
            if !wait.may_have_delayed_tasks {
                break;
            }
        }
        Ok(())
    }

    /// Port of MinecraftServer#waitUntilNextTick. Ingress wakes the wait, but
    /// PacketProcessor work never executes in this idle slot.
    /// Tick phases, gameTime, holder broadcasts and packet ACKs are not advanced.
    pub(super) async fn wait_until_next_tick(
        &mut self,
        deadline: tokio::time::Instant,
        tick_length: Duration,
    ) -> Result<()> {
        let mut wait = NextTickWait {
            next_tick: deadline,
            delayed_tasks: (tokio::time::Instant::now() + tick_length).max(deadline),
            may_have_delayed_tasks: true,
        };
        self.run_all_tasks(&mut wait).await?;
        while !self.stop && tokio::time::Instant::now() < wait.deadline() {
            wait.may_have_delayed_tasks = self.poll_task(None).await?;
            if wait.may_have_delayed_tasks {
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
