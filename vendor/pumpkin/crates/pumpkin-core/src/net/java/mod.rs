use pumpkin_protocol::java::client::play::{CChunkBatchEnd, CChunkBatchStart, CPlayDisconnect};
use pumpkin_world::level::SyncChunk;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};
use std::{io::Write, sync::Arc};

use bytes::Bytes;
use crossbeam::atomic::AtomicCell;
use pumpkin_data::packet::CURRENT_MC_VERSION;
use pumpkin_data::translation;
use pumpkin_protocol::java::server::play::{
    SAttack, SBlockEntityTagQuery, SBundleItemSelected, SChangeDifficulty, SChangeGameMode,
    SChatAck, SChatCommand, SChatCommandSigned, SChatMessage, SChunkBatch, SClickSlot,
    SClientCommand, SClientInformationPlay, SClientTickEnd, SCloseContainer, SCommandSuggestion,
    SConfigurationAcknowledged, SConfirmTeleport, SContainerButtonClick,
    SContainerSlotStateChanged, SCookieResponse as SPCookieResponse, SCustomPayload,
    SDebugSampleSubscription, SDebugSubscriptionRequest, SEditBook, SEntityTagQuery, SInteract,
    SJigsawGenerate, SKeepAlive, SLockDifficulty, SMoveVehicle, SPaddleBoat, SPickItemFromBlock,
    SPlaceRecipe, SPlayPingRequest, SPlayPong, SPlayResourcePack, SPlayerAbilities, SPlayerAction,
    SPlayerCommand, SPlayerInput, SPlayerLoaded, SPlayerPosition, SPlayerPositionRotation,
    SPlayerRotation, SPlayerSession, SRecipeBookChangeSettings, SRecipeBookSeenRecipe, SRenameItem,
    SSeenAdvancement, SSelectTrade, SSetBeacon, SSetCommandBlock, SSetCommandMinecart,
    SSetCreativeSlot, SSetGameRule, SSetHeldItem, SSetJigsawBlock, SSetPlayerGround,
    SSetStructureBlock, SSetTestBlock, SSpectateEntity, SSwingArm, STeleportToEntity,
    STestInstanceBlockAction, SUpdateSign, SUseItem, SUseItemOn,
};
use pumpkin_protocol::packet::MultiVersionJavaPacket;
use pumpkin_protocol::{
    ClientPacket, ConnectionState, PacketDecodeError, RawPacket, ServerPacket,
    codec::var_int::VarInt,
    java::{
        client::{config::CConfigDisconnect, login::CLoginDisconnect},
        packet_decoder::TCPNetworkDecoder,
        packet_encoder::TCPNetworkEncoder,
    },
    ser::{NetworkReadExt, NetworkWriteExt, WritingError},
};
use pumpkin_util::text::TextComponent;
use pumpkin_util::version::JavaMinecraftVersion;
use tokio::{
    io::{BufReader, BufWriter},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::oneshot,
    time::Instant,
};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tracing::{debug, error, warn};

pub mod chunk_data;
pub mod handshake;
pub mod login;
mod outgoing;
pub mod pending;
pub mod play;
pub mod recipe_helper;
pub mod status;

pub use chunk_data::{CChunkData, ChunkLightExt};
use outgoing::{DISCONNECT_FLUSH_TIMEOUT, OutgoingPacket, run_outgoing_packet_writer};

use arc_swap::ArcSwap;
use pending::PendingConnection;

use crate::entity::player::Player;
use crate::entity::player::packet_activity::{ActivityLease, AdmissionError};
use crate::net::{
    ClientPlatform, GameProfile, MAX_PENDING_BYTES, PacketRateLimiter, PlayerConfig,
    decrement_pending_bytes,
};
use crate::plugin::api::events::world::chunk_send::ChunkSend;
use crate::plugin::player::player_custom_payload::PlayerCustomPayloadEvent;
use crate::plugin::server::packet::PacketSentEvent;
use crate::{error::PumpkinError, server::Server};

/// An owned continuation; the FIFO caller transfers its activity lease to the client task.
pub(crate) type DeferredPacket = std::pin::Pin<Box<dyn Future<Output = ()> + Send>>;

/// A timer may run many times while a single frame is incomplete. Keep the same
/// decode future pinned: the upstream decoder stores partial framing in it.
async fn read_with_keep_alive<T, F, K, KF>(
    read: F,
    interval: &mut tokio::time::Interval,
    mut keep_alive: K,
) -> Option<T>
where
    F: Future<Output = Option<T>>,
    K: FnMut() -> KF,
    KF: Future<Output = bool>,
{
    tokio::pin!(read);
    loop {
        tokio::select! {
            packet = &mut read => return packet,
            _ = interval.tick() => {
                if !keep_alive().await {
                    return None;
                }
            }
        }
    }
}

pub struct JavaClient {
    pub id: u64,
    /// The protocol the client speaks. Play packets are always encoded/decoded as
    /// `CURRENT_MC_VERSION`. Older clients are not admitted; the packet events are the hook
    /// for a plugin that converts them.
    pub version: AtomicCell<JavaMinecraftVersion>,
    /// The client's game profile information. Direct field (lock-free).
    pub gameprofile: GameProfile,
    /// The client's configuration settings. Lock-free `ArcSwap`.
    pub config: ArcSwap<PlayerConfig>,
    /// The Address used to connect to the Server, Sent in the Handshake. Direct field.
    pub server_address: String,
    /// The current connection state of the client (e.g., Handshaking, Status, Play).
    pub connection_state: AtomicCell<ConnectionState>,
    /// The client's IP address. Direct field (lock-free).
    pub address: SocketAddr,
    /// The client's brand or modpack information. Lock-free `ArcSwap`.
    pub brand: ArcSwap<Option<String>>,
    /// Associated player reference. Lock-free `ArcSwap`.
    pub player: ArcSwap<Option<Arc<Player>>>,
    /// A collection of tasks associated with this client. The tasks await completion when removing the client.
    tasks: TaskTracker,
    rt_handle: tokio::runtime::Handle,
    /// An notifier that is triggered when this client is closed.
    close_token: CancellationToken,
    /// Per-connection FIFO of serialized packets (vanilla Netty eventLoop).
    /// Unbounded like vanilla; `MAX_PENDING_BYTES` is the limit.
    outgoing_packet_queue_send: UnboundedSender<OutgoingPacket>,
    outgoing_packet_queue_recv: Option<UnboundedReceiver<OutgoingPacket>>,
    /// Tracks total buffered payload bytes in the outgoing queue.
    pub pending_bytes: Arc<AtomicUsize>,
    /// The packet encoder for outgoing packets.
    network_writer: std::sync::Mutex<Option<TCPNetworkEncoder<BufWriter<OwnedWriteHalf>>>>,
    /// The packet decoder for incoming packets.
    network_reader: std::sync::Mutex<Option<TCPNetworkDecoder<BufReader<OwnedReadHalf>>>>,
    /// Keep Alive:
    ///
    /// Whether we are waiting for a response after sending a keep alive packet.
    pub wait_for_keep_alive: AtomicBool,
    /// Set to `true` when any movement packet is received this tick.
    /// On `SClientTickEnd` (≥1.21.4), if still `false`, the player's known
    /// movement is zeroed (they stood still). Matches vanilla's `receivedMovementThisTick`.
    pub received_movement_this_tick: AtomicBool,
    /// The keep alive packet payload we send. The client should respond with the same id.
    pub keep_alive_id: AtomicCell<i64>,
    /// The last time we sent a keep alive packet.
    pub last_keep_alive_time: AtomicCell<Instant>,
    /// The last time any packet was received from the client.
    pub last_packet_time: AtomicCell<Instant>,
    /// Recent in-flight keep alive IDs with their sent timestamps.
    pub pending_keep_alives: std::sync::Mutex<Vec<(i64, Instant)>>,

    pub packet_sequence: AtomicI32,
    /// Packet rate limiter for incoming client packets.
    pub packet_limiter: PacketRateLimiter,
    /// Vanilla `suspendFlushingOnServerThread`.
    suspend_flushing: Arc<AtomicBool>,
}

impl JavaClient {
    #[must_use]
    pub fn from_pending(
        pending: PendingConnection,
        gameprofile: GameProfile,
        config: PlayerConfig,
    ) -> Self {
        let (send, recv) = tokio::sync::mpsc::unbounded_channel();

        Self {
            id: pending.id,
            gameprofile,
            config: ArcSwap::from_pointee(config),
            server_address: pending.server_address,
            address: pending.address,
            connection_state: pending.connection_state,
            close_token: pending.close_token,
            tasks: TaskTracker::new(),
            rt_handle: tokio::runtime::Handle::current(),
            outgoing_packet_queue_send: send,
            outgoing_packet_queue_recv: Some(recv),
            pending_bytes: Arc::new(AtomicUsize::new(0)),
            version: pending.version,
            network_writer: std::sync::Mutex::new(Some(pending.network_writer)),
            network_reader: std::sync::Mutex::new(Some(pending.network_reader)),
            brand: ArcSwap::from_pointee(pending.brand),
            player: ArcSwap::from_pointee(None),
            wait_for_keep_alive: AtomicBool::new(false),
            received_movement_this_tick: AtomicBool::new(false),
            keep_alive_id: AtomicCell::new(0),
            last_keep_alive_time: AtomicCell::new(Instant::now()),
            last_packet_time: AtomicCell::new(Instant::now()),
            pending_keep_alives: std::sync::Mutex::new(Vec::new()),
            packet_sequence: AtomicI32::new(-1),
            packet_limiter: pending.packet_limiter,
            suspend_flushing: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Vanilla `ServerCommonPacketListenerImpl.suspendFlushing`.
    pub fn suspend_flushing(&self) {
        self.suspend_flushing.store(true, Ordering::Release);
    }

    /// Vanilla `resumeFlushing`: queue `flushChannel` then lift the hold.
    pub fn resume_flushing(&self) {
        self.flush_channel();
        self.suspend_flushing.store(false, Ordering::Release);
    }

    /// Flushes Channel even while suspended.
    pub fn flush_channel(&self) {
        if self
            .outgoing_packet_queue_send
            .send(OutgoingPacket::Flush)
            .is_err()
            && !self.close_token.is_cancelled()
        {
            warn!(
                "Failed to queue flush for client {}: channel closed",
                self.id
            );
            self.close();
        }
    }

    pub fn set_player(&self, player: &Arc<Player>) {
        if let Some(server) = player.world().server.upgrade() {
            player.packet_activity.bind(player, &server.packet_runner);
        }
        self.player.store(Arc::new(Some(player.clone())));
        if self.is_closed() {
            player.packet_activity.seal();
        }
    }

    pub async fn progress_player_packets(&self, player: &Arc<Player>, server: &Arc<Server>) {
        let Some(mut network_reader) = self
            .network_reader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        else {
            return;
        };

        let keep_alive_time = server.advanced_config.networking.java.keep_alive_time;
        let mut keep_alive_interval =
            tokio::time::interval(std::time::Duration::from_secs(keep_alive_time.max(1)));
        let timeout_duration =
            std::time::Duration::from_secs(keep_alive_time.saturating_mul(2).max(1));

        // Skip the immediate first tick so we don't send a keep-alive the exact millisecond they join
        keep_alive_interval.tick().await;

        while let Some(packet) = read_with_keep_alive(
            self.get_packet_with_reader(&mut network_reader),
            &mut keep_alive_interval,
            || self.send_keep_alive_or_timeout(timeout_duration),
        )
        .await
        {
            if !self.packet_limiter.check_packet() {
                warn!(
                    "Client {} ({}) exceeded packet rate limit (rate: {}/s)",
                    self.id,
                    self.gameprofile.name,
                    self.packet_limiter.max_rate()
                );
                self.kick(TextComponent::text(
                    server
                        .advanced_config
                        .networking
                        .java
                        .packet_limiter
                        .kick_message
                        .clone(),
                ))
                .await;
                break;
            }
            match self.receive_play_packet(player, packet) {
                Ok(()) => {}
                Err(AdmissionError::Closed) => break,
                Err(AdmissionError::Full) => {
                    self.kick(TextComponent::text("Too many pending packets"))
                        .await;
                    break;
                }
            }
        }
    }

    /// Admit the original wire packet before interpreting transport liveness.
    /// The event stays in the ordered queue; no plugin callback runs here. Its
    /// later cancellation/rewrite cannot undo an acknowledged transport reply.
    pub(crate) fn receive_play_packet(
        &self,
        player: &Player,
        packet: RawPacket,
    ) -> Result<(), AdmissionError> {
        let _activity = player
            .packet_activity
            .enter_non_packet()
            .ok_or(AdmissionError::Closed)?;
        if self.is_closed() {
            return Err(AdmissionError::Closed);
        }
        let keep_alive = if packet.id == SKeepAlive::to_id(CURRENT_MC_VERSION) {
            let mut payload = &packet.payload[..];
            SKeepAlive::read(&mut payload, &CURRENT_MC_VERSION)
                .ok()
                .filter(|_| payload.is_empty())
        } else {
            None
        };
        // Keep the same packet-count/byte limits even for transport events. A
        // rejected admission must not update liveness or the player's latency.
        player
            .packet_activity
            .enqueue(&player.inbound_packets, packet)?;
        player.packet_activity.schedule();
        self.last_packet_time.store(Instant::now());
        if let Some(keep_alive) = keep_alive {
            let mut pending = self
                .pending_keep_alives
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // Only a challenge still outstanding on THIS connection counts.
            // In particular there is no last-ID fallback for duplicates/replays.
            if let Some(index) = pending
                .iter()
                .position(|(id, _)| *id == keep_alive.keep_alive_id)
            {
                let (_, sent) = pending.swap_remove(index);
                let ping = sent.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
                let smoothed =
                    (u64::from(player.ping.load(Ordering::Relaxed)) * 3 + u64::from(ping)) / 4;
                player.ping.store(smoothed as u32, Ordering::Relaxed);
                self.wait_for_keep_alive
                    .store(!pending.is_empty(), Ordering::Relaxed);
            }
        }
        Ok(())
    }

    pub(crate) async fn send_keep_alive_or_timeout(&self, timeout: std::time::Duration) -> bool {
        if self.is_closed() {
            return false;
        }
        let has_timed_out = {
            let pending = self
                .pending_keep_alives
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            pending.iter().any(|(_, sent)| sent.elapsed() > timeout)
        } || self.last_packet_time.load().elapsed() > timeout;
        if has_timed_out {
            self.kick(pumpkin_macros::translate_cross!(
                translation::java::DISCONNECT_TIMEOUT,
                translation::bedrock::DISCONNECT_TIMEOUT
            ))
            .await;
            return false;
        }
        let id = {
            let mut pending = self
                .pending_keep_alives
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // Unique per connection until integer wrap, even if multiple timer
            // ticks happen in one wall-clock millisecond or the clock goes back.
            if pending.len() >= 16 {
                return true;
            }
            let id = self.keep_alive_id.load().wrapping_add(1);
            let now = Instant::now();
            self.keep_alive_id.store(id);
            self.wait_for_keep_alive.store(true, Ordering::Relaxed);
            self.last_keep_alive_time.store(now);
            pending.push((id, now));
            id
        };
        self.enqueue_client_packet(&pumpkin_protocol::java::client::play::CKeepAlive::new(id))
            .await;
        !self.is_closed()
    }

    pub async fn await_tasks(&self) {
        // Only the outer lifecycle may drain. An active callback can call close
        // without self-joining; descendants can still register before idle.
        self.close();
        if let Some(player) = self.player.load_full().as_ref() {
            player.packet_activity.drain().await;
        }
        self.tasks.close();
        self.tasks.wait().await;
    }

    /// Spawns a task associated with this client. All tasks spawned with this method are awaited
    /// when the client. This means tasks should complete in a reasonable amount of time or select
    /// on `Self::await_close_interrupt` to cancel the task when the client is closed
    ///
    /// Returns an `Option<JoinHandle<F::Output>>`. If the client is closed, this returns `None`.
    pub fn spawn_task<F>(&self, task: F) -> Option<JoinHandle<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let player = self.player.load_full();
        // Register under a drain-only admission lease so a close racing the
        // token check cannot observe an empty tracker before this spawn.
        let _registration = if let Some(player) = player.as_ref() {
            Some(player.packet_activity.enter_non_packet()?)
        } else {
            None
        };
        if self.close_token.is_cancelled() {
            None
        } else {
            Some(self.tasks.spawn_on(task, &self.rt_handle))
        }
    }

    /// A lease proves this operation was admitted before close (or by an owned
    /// ancestor). Do not reject it merely because its socket has since closed.
    pub(crate) fn spawn_activity_task<F>(
        &self,
        activity: ActivityLease,
        work: F,
    ) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        activity.submit(work).spawn_on(&self.tasks, &self.rt_handle)
    }

    /// Transfer/respawn select explicit guarded source and destination stages.
    pub(crate) fn spawn_world_activity_task<F>(
        &self,
        activity: ActivityLease,
        work: F,
    ) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        activity
            .submit_world_stages(work)
            .spawn_on(&self.tasks, &self.rt_handle)
    }

    pub async fn send_chunks(&self, chunks: &[SyncChunk]) {
        let player = self.player.load_full();
        let Some(player) = player.as_ref() else {
            return;
        };
        let Some(server) = player.world().server.upgrade() else {
            return;
        };

        let mut valid_chunks = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            let mut event = ChunkSend::new(player.world(), chunk.clone());
            server.plugin_manager.fire(&server, &mut event).await;
            if !event.cancelled {
                valid_chunks.push(chunk.clone());
            }
        }

        if valid_chunks.is_empty() {
            return;
        }

        let (tx, rx) = oneshot::channel();
        rayon::spawn(move || {
            let mut serialized = Vec::with_capacity(valid_chunks.len());
            for chunk in valid_chunks {
                let mut buf = Vec::with_capacity(32 * 1024);
                if let Err(err) = buf.write_var_int(&VarInt(CChunkData::to_id(CURRENT_MC_VERSION)))
                {
                    error!("Failed to write chunk data id: {err:?}");
                    continue;
                }
                if let Err(err) =
                    CChunkData(&chunk).write_packet_data(&mut buf, &CURRENT_MC_VERSION)
                {
                    error!("Failed to write chunk data: {err:?}");
                    continue;
                }
                serialized.push(Bytes::from(buf));
            }
            let _ = tx.send(serialized);
        });

        let Ok(serialized) = rx.await else {
            return;
        };
        let sent_count = serialized.len();
        if sent_count == 0 {
            return;
        }

        self.send_packet(&CChunkBatchStart).await;

        // One FIFO per connection: batch start/data/end stay in enqueue order.
        for chunk_data in serialized {
            self.send_packet_now_data(chunk_data).await;
        }

        self.send_packet(&CChunkBatchEnd::new(sent_count as u16))
            .await;
    }

    pub async fn enqueue_packet(&self, packet_data: Bytes) {
        self.enqueue_packet_data(packet_data).await;
    }

    #[allow(clippy::unused_async)]
    pub async fn enqueue_packet_data(&self, packet_data: Bytes) {
        self.try_enqueue_packet_data(packet_data);
    }

    /// Outbound choke point of all enqueue/send paths. `None` when the packet must be dropped.
    fn reserve_pending_bytes(&self, packet_data: Bytes) -> Option<(Bytes, usize)> {
        if self.close_token.is_cancelled() {
            return None;
        }
        let packet_data = self.translate_outgoing(packet_data)?;

        // Reserve first, release again if it does not fit.
        let packet_len = packet_data.len();
        let prev_bytes = self.pending_bytes.fetch_add(packet_len, Ordering::AcqRel);
        let new_bytes = prev_bytes.saturating_add(packet_len);

        if new_bytes > MAX_PENDING_BYTES {
            decrement_pending_bytes(&self.pending_bytes, packet_len);
            if !self.close_token.is_cancelled() {
                warn!(
                    "Client {} outbound packet buffer overflow ({} bytes > {} bytes). Closing connection.",
                    self.id, new_bytes, MAX_PENDING_BYTES
                );
                self.close();
            }
            return None;
        }

        Some((packet_data, packet_len))
    }

    /// `PacketSentEvent` for clients the multiversion plugin admitted below
    /// `CURRENT_MC_VERSION`: it gets the 26.3 id + payload and rewrites both.
    /// `None` when cancelled.
    fn translate_outgoing(&self, packet_data: Bytes) -> Option<Bytes> {
        if self.version.load() == CURRENT_MC_VERSION {
            return Some(packet_data);
        }
        // TODO: packets sent before `set_player` (e.g. an `add_player` kick) go out untranslated.
        let player = self.player.load_full();
        let Some(player) = player.as_ref() else {
            return Some(packet_data);
        };
        let Some(server) = player.world().server.upgrade() else {
            return Some(packet_data);
        };
        if !server.plugin_manager.has_handlers::<PacketSentEvent>() {
            return Some(packet_data);
        }

        let mut reader = &packet_data[..];
        let Ok(packet_id) = reader.get_var_int() else {
            return Some(packet_data);
        };
        let payload = packet_data.slice(packet_data.len() - reader.len()..);
        let mut event = PacketSentEvent::new_raw(player.clone(), packet_id.0, payload);
        server.plugin_manager.fire_blocking(&server, &mut event);
        if event.cancelled {
            return None;
        }

        let mut framed = Vec::with_capacity(5 + event.payload.len());
        framed.write_var_int(&VarInt(event.packet_id)).ok()?;
        framed.extend_from_slice(&event.payload);
        Some(framed.into())
    }

    pub fn try_enqueue_packet(&self, packet_data: Bytes) {
        self.try_enqueue_packet_data(packet_data);
    }

    pub fn try_enqueue_packet_data(&self, packet_data: Bytes) {
        let Some((packet_data, packet_len)) = self.reserve_pending_bytes(packet_data) else {
            return;
        };
        self.queue_outgoing(OutgoingPacket::normal(packet_data), packet_len);
    }

    /// `false` once the writer is gone. Then the connection is closed.
    fn queue_outgoing(&self, packet: OutgoingPacket, packet_len: usize) -> bool {
        if self.outgoing_packet_queue_send.send(packet).is_ok() {
            return true;
        }
        decrement_pending_bytes(&self.pending_bytes, packet_len);
        // It is expected that the packet will fail if closed
        if !self.close_token.is_cancelled() {
            warn!(
                "Failed to add packet to the outgoing packet queue for client {}: channel closed",
                self.id
            );
            // Connection to the client closed since the stream is in an unknown state
            self.close();
        }
        false
    }

    pub async fn await_close_interrupt(&self) {
        self.close_token.cancelled().await;
    }

    pub async fn get_packet_with_reader<R: tokio::io::AsyncRead + Unpin>(
        &self,
        network_reader: &mut TCPNetworkDecoder<R>,
    ) -> Option<RawPacket> {
        tokio::select! {
            () = self.await_close_interrupt() => {
                debug!("Canceling player packet processing");
                None
            },
            packet_result = network_reader.get_raw_packet() => {
                match packet_result {
                    Ok(packet) => Some(packet),
                    Err(err) => {
                        if !matches!(err, PacketDecodeError::ConnectionClosed) {
                            debug!("Failed to decode packet from client {}: {}", self.id, err);
                            let text = format!("Error while reading incoming packet {err}");
                            self.kick(TextComponent::text(text)).await;
                        }
                        None
                    }
                }
            }
        }
    }

    /// Disconnect packet for the current state. `None` in handshake/status.
    fn serialize_disconnect(&self, reason: &TextComponent) -> Option<Bytes> {
        match self.connection_state.load() {
            ConnectionState::Login => {
                // TextComponent implements Serialize and writes in bytes instead of String
                let packet = CLoginDisconnect::new(
                    serde_json::to_string(&reason.0).unwrap_or_else(|_| String::new()),
                );
                self.serialize_packet(&packet).ok()
            }
            ConnectionState::Config => {
                let reason_text = reason.clone().get_text();
                let packet = CConfigDisconnect::new(&reason_text);
                self.serialize_packet(&packet).ok()
            }
            ConnectionState::Play => {
                let packet = CPlayDisconnect::new(reason);
                self.serialize_packet(&packet).ok()
            }
            _ => None,
        }
    }

    pub fn try_kick(&self, reason: &TextComponent) {
        self.seal_activity();
        if let Some(data) = self
            .serialize_disconnect(reason)
            .and_then(|data| self.translate_outgoing(data))
        {
            let packet_len = data.len();
            let _ = self.pending_bytes.fetch_add(packet_len, Ordering::AcqRel);
            // The writer drains and flushes it after `close()`
            if self
                .outgoing_packet_queue_send
                .send(OutgoingPacket::normal(data))
                .is_err()
            {
                decrement_pending_bytes(&self.pending_bytes, packet_len);
                // Expected: the writer task is already gone.
                debug!(
                    "Disconnect packet for client {} dropped: outgoing packet queue closed",
                    self.id
                );
            }
        }
        let reason_text = reason.clone().get_text();
        warn!("Closing connection for {}: {reason_text}", self.id);
        self.close();
    }

    pub async fn kick(&self, reason: TextComponent) {
        self.kick_explicit(&reason, true).await;
    }

    pub async fn kick_explicit(&self, reason: &TextComponent, send_packet: bool) {
        self.seal_activity();
        if send_packet && let Some(data) = self.serialize_disconnect(reason) {
            // Stalled peer: never flushes -> Close anyway.
            let _ = tokio::time::timeout(
                DISCONNECT_FLUSH_TIMEOUT,
                self.send_and_wait(data, OutgoingPacket::flushed),
            )
            .await;
        }
        let reason_text = reason.clone().get_text();
        warn!("Closing connection for {}: {reason_text}", self.id);
        self.close();
    }

    pub async fn send_packet_now(&self, packet: Bytes) {
        self.send_packet_now_data(packet).await;
    }

    /// Enqueue on the per-connection FIFO and wait until the writer has
    /// `write_frame`d into the `BufWriter`. Never waits for a TCP flush.
    pub async fn send_packet_now_data(&self, packet: Bytes) {
        self.send_and_wait(packet, OutgoingPacket::high_priority)
            .await;
    }

    /// Enqueue and wait for the writer's completion, `Framed` or `Flushed` per `make`.
    async fn send_and_wait(
        &self,
        packet: Bytes,
        make: fn(Bytes, oneshot::Sender<()>) -> OutgoingPacket,
    ) {
        let Some((packet, packet_len)) = self.reserve_pending_bytes(packet) else {
            return;
        };

        let (completion_tx, completion_rx) = oneshot::channel();
        if !self.queue_outgoing(make(packet, completion_tx), packet_len) {
            return;
        }

        if completion_rx.await.is_err() && !self.close_token.is_cancelled() {
            // The outgoing packet task dropped before confirming the write.
            self.close();
        }
    }

    pub fn write_packet_for_version<P: ClientPacket>(
        packet: &P,
        version: JavaMinecraftVersion,
        write: impl Write,
    ) -> Result<(), WritingError> {
        pumpkin_protocol::java::packet_encoder::write_packet(packet, &version, write)
    }

    pub fn serialize_packet_for_version<P: ClientPacket>(
        packet: &P,
        version: JavaMinecraftVersion,
    ) -> Result<Bytes, WritingError> {
        pumpkin_protocol::java::packet_encoder::serialize_packet(packet, &version)
    }

    pub fn serialize_packet<P: ClientPacket>(&self, packet: &P) -> Result<Bytes, WritingError> {
        Self::serialize_packet_for_version(packet, CURRENT_MC_VERSION)
    }

    pub fn try_send_packet<P: ClientPacket>(&self, packet: &P) {
        if let Ok(data) = self.serialize_packet(packet) {
            self.try_enqueue_packet(data);
        }
    }

    pub async fn send_packet<P: ClientPacket>(&self, packet: &P) {
        if let Ok(data) = self.serialize_packet(packet) {
            self.send_packet_now(data).await;
        }
    }

    pub async fn enqueue_client_packet<P: ClientPacket>(&self, packet: &P) {
        if let Ok(data) = self.serialize_packet(packet) {
            self.enqueue_packet(data).await;
        }
    }

    pub fn write_packet<P: ClientPacket>(
        &self,
        packet: &P,
        write: impl Write,
    ) -> Result<(), WritingError> {
        Self::write_packet_for_version(packet, CURRENT_MC_VERSION, write)
    }

    /// Handles an incoming packet, routing it to the appropriate handler based on the current connection state.
    ///
    /// This function takes a `RawPacket` and routes it to the corresponding handler based on the current connection state.
    /// It supports the following connection states:
    ///
    /// - **Handshake:** Handles handshake packets.
    /// - **Status:** Handles status request and ping packets.
    /// - **Login/Transfer:** Handles login and transfer packets.
    /// - **Config:** Handles configuration packets.
    pub fn start_outgoing_packet_task(&mut self) {
        let Some(packet_receiver) = self.outgoing_packet_queue_recv.take() else {
            return;
        };
        let close_token = self.close_token.clone();
        let pending_bytes = self.pending_bytes.clone();
        let Some(writer) = self
            .network_writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        else {
            return;
        };
        let id = self.id;
        let suspend_flushing = self.suspend_flushing.clone();
        self.spawn_task(async move {
            run_outgoing_packet_writer(
                packet_receiver,
                writer,
                close_token,
                suspend_flushing,
                pending_bytes,
                id,
            )
            .await;
        });
    }

    fn seal_activity(&self) {
        if let Some(player) = self.player.load_full().as_ref() {
            player.packet_activity.seal();
        }
    }

    /// Closes the connection to the client.
    ///
    /// This function marks the connection as closed using an atomic flag. It's generally preferable
    /// to use the `kick` function if you want to send a specific message to the client explaining the reason for the closure.
    /// However, use `close` in scenarios where sending a message is not critical or might not be possible (e.g., sudden connection drop).
    ///
    /// # Notes
    ///
    /// This function does not attempt to send any disconnect packets to the client.
    /// Packets already queued are still written and flushed, bounded by `DISCONNECT_FLUSH_TIMEOUT`.
    pub fn close(&self) {
        self.seal_activity();
        self.close_token.cancel();
    }

    pub fn is_closed(&self) -> bool {
        self.close_token.is_cancelled()
    }

    #[expect(clippy::too_many_lines)]
    pub(crate) fn handle_play_packet(
        &self,
        player: &Arc<Player>,
        server: &Arc<Server>,
        packet: &RawPacket,
    ) -> Result<Option<DeferredPacket>, Box<dyn PumpkinError>> {
        // The multiversion plugin has converted older clients' packets to 26.3 by now.
        let version = CURRENT_MC_VERSION;

        let mut event = crate::plugin::server::packet::PacketReceivedEvent::new(
            player.clone(),
            packet.id,
            packet.payload.clone(),
        );
        server.plugin_manager.fire_blocking(server, &mut event);
        if event.cancelled {
            return Ok(None);
        }
        if packet.id == SKeepAlive::to_id(version) {
            // Liveness was handled at wire admission. Keep its event ordered,
            // but never replay it or promote a rewritten transport event into
            // gameplay. Preserve normal decode errors for malformed replies.
            SKeepAlive::read(&mut &packet.payload[..], &version)?;
            return Ok(None);
        }

        let mut payload = &event.payload[..];
        match event.packet_id {
            id if id == SConfirmTeleport::to_id(version) => {
                self.handle_confirm_teleport(
                    player,
                    &SConfirmTeleport::read(&mut payload, &version)?,
                );
            }
            id if id == SChangeGameMode::to_id(version) => {
                self.handle_change_game_mode(
                    player,
                    &SChangeGameMode::read(&mut payload, &version)?,
                );
            }
            id if id == SChatAck::to_id(version) => {
                let packet = SChatAck::read(&mut payload, &version)?;
                self.handle_chat_ack(player, &packet);
            }
            id if id == SChatCommand::to_id(version) => {
                let packet = SChatCommand::read(&mut payload, &version)?;
                let cmd = packet.command.to_string();
                let client_platform = player.client.clone();
                let player_c = player.clone();
                let server_c = server.clone();
                return Ok(Some(Box::pin(async move {
                    if let ClientPlatform::Java(client) = client_platform.as_ref() {
                        let packet = SChatCommand { command: &cmd };
                        client
                            .handle_chat_command(&player_c, &server_c, &packet)
                            .await;
                    }
                })));
            }
            id if id == SChatCommandSigned::to_id(version) => {
                // Existing unsigned fallback/signature handling remains a separate
                // security gap. Serial completion does not validate this envelope.
                let mut signed_payload = payload;
                let cmd =
                    if let Ok(signed) = SChatCommandSigned::read(&mut signed_payload, &version) {
                        signed.command.to_string()
                    } else {
                        SChatCommand::read(&mut payload, &version)?
                            .command
                            .to_string()
                    };
                let client_platform = player.client.clone();
                let player_c = player.clone();
                let server_c = server.clone();
                return Ok(Some(Box::pin(async move {
                    if let ClientPlatform::Java(client) = client_platform.as_ref() {
                        let packet = SChatCommand { command: &cmd };
                        client
                            .handle_chat_command(&player_c, &server_c, &packet)
                            .await;
                    }
                })));
            }
            id if id == SChatMessage::to_id(version) => {
                let packet = SChatMessage::read(&mut payload, &version)?;
                let msg = packet.message.to_string();
                let signature = packet.signature.map(<[u8]>::to_vec);
                let ack = packet.acknowledged.to_vec();
                let ts = packet.timestamp;
                let salt = packet.salt;
                let count = packet.message_count;
                let checksum = packet.checksum;
                let client_platform = player.client.clone();
                let player_c = player.clone();
                let server_c = server.clone();
                return Ok(Some(Box::pin(async move {
                    if let ClientPlatform::Java(client) = client_platform.as_ref() {
                        let packet = SChatMessage {
                            message: &msg,
                            timestamp: ts,
                            salt,
                            signature: signature.as_deref(),
                            message_count: count,
                            acknowledged: &ack,
                            checksum,
                        };
                        client
                            .handle_chat_message(&server_c, &player_c, packet)
                            .await;
                    }
                })));
            }
            id if id == SClientInformationPlay::to_id(version) => {
                self.handle_client_information(
                    server,
                    player,
                    &SClientInformationPlay::read(&mut payload, &version)?,
                );
            }
            id if id == SClientCommand::to_id(version) => {
                return Ok(self
                    .handle_client_status(player, &SClientCommand::read(&mut payload, &version)?));
            }
            id if id == SPlayerInput::to_id(version) => {
                self.handle_player_input(
                    player,
                    &SPlayerInput::read(&mut payload, &version)?,
                    server,
                );
            }
            id if id == SMoveVehicle::to_id(version) => {
                self.handle_move_vehicle(player, &SMoveVehicle::read(&mut payload, &version)?);
            }
            id if id == SPaddleBoat::to_id(version) => {
                self.handle_paddle_boat(player, &SPaddleBoat::read(&mut payload, &version)?);
            }
            id if id == SInteract::to_id(version) => {
                self.handle_interact(player, &SInteract::read(&mut payload, &version)?, server);
            }
            id if id == SBundleItemSelected::to_id(version) => {
                self.handle_bundle_item_selected(
                    player,
                    &SBundleItemSelected::read(&mut payload, &version)?,
                );
            }
            id if id == SAttack::to_id(version) => {
                self.handle_attack(player, &SAttack::read(&mut payload, &version)?, server);
            }
            id if id == STeleportToEntity::to_id(version) => {
                self.handle_teleport_to_entity(
                    player,
                    &STeleportToEntity::read(&mut payload, &version)?,
                    server,
                );
            }
            id if id == SKeepAlive::to_id(version) => {
                // A gameplay event rewritten to keepalive cannot authenticate a
                // transport reply. Only original wire packets reach that path.
            }
            id if id == SClientTickEnd::to_id(version) => {
                self.handle_client_tick_end(player);
            }
            id if id == STestInstanceBlockAction::to_id(version) => {
                self.handle_test_instance_block_action(
                    player,
                    &STestInstanceBlockAction::read(&mut payload, &version)?,
                );
            }
            id if id == SSetTestBlock::to_id(version) => {
                self.handle_set_test_block(player, &SSetTestBlock::read(&mut payload, &version)?);
            }
            id if id == SDebugSubscriptionRequest::to_id(version) => {
                self.handle_debug_subscription_request(
                    player,
                    &SDebugSubscriptionRequest::read(&mut payload, &version)?,
                );
            }
            id if id == SDebugSampleSubscription::to_id(version) => {
                self.handle_debug_sample_subscription(
                    player,
                    &SDebugSampleSubscription::read(&mut payload, &version)?,
                );
            }
            id if id == SPlayerPosition::to_id(version) => {
                self.handle_position(
                    player,
                    server,
                    &SPlayerPosition::read(&mut payload, &version)?,
                );
            }
            id if id == SPlayerPositionRotation::to_id(version) => {
                self.handle_position_rotation(
                    player,
                    server,
                    &SPlayerPositionRotation::read(&mut payload, &version)?,
                );
            }
            id if id == SPlayerRotation::to_id(version) => {
                self.handle_rotation(player, &SPlayerRotation::read(&mut payload, &version)?);
            }
            id if id == SSetPlayerGround::to_id(version) => {
                self.handle_player_ground(player, &SSetPlayerGround::read(&mut payload, &version)?);
            }
            id if id == SPickItemFromBlock::to_id(version) => {
                self.handle_pick_item_from_block(
                    player,
                    &SPickItemFromBlock::read(&mut payload, &version)?,
                );
            }
            id if id
                == pumpkin_protocol::java::server::play::SPickItemFromEntity::to_id(version) =>
            {
                self.handle_pick_item_from_entity(
                    player,
                    &pumpkin_protocol::java::server::play::SPickItemFromEntity::read(
                        &mut payload,
                        &version,
                    )?,
                );
            }
            id if id == SPlayerAbilities::to_id(version) => {
                self.handle_player_abilities(
                    player,
                    &SPlayerAbilities::read(&mut payload, &version)?,
                    server,
                );
            }
            id if id == SPlayerAction::to_id(version) => {
                self.handle_player_action(
                    player,
                    &SPlayerAction::read(&mut payload, &version)?,
                    server,
                );
            }
            id if id == SSetCommandBlock::to_id(version) => {
                self.handle_set_command_block(
                    player,
                    &SSetCommandBlock::read(&mut payload, &version)?,
                );
            }
            id if id == SSetJigsawBlock::to_id(version) => {
                self.handle_set_jigsaw_block(
                    player,
                    &SSetJigsawBlock::read(&mut payload, &version)?,
                );
            }
            id if id == SJigsawGenerate::to_id(version) => {
                self.handle_jigsaw_generate(
                    player,
                    &SJigsawGenerate::read(&mut payload, &version)?,
                );
            }
            id if id == SPlayerCommand::to_id(version) => {
                self.handle_player_command(
                    player,
                    &SPlayerCommand::read(&mut payload, &version)?,
                    server,
                );
            }
            id if id == SPlayerLoaded::to_id(version) => {
                Self::handle_player_loaded(player);
            }
            id if id == SPlayPingRequest::to_id(version) => {
                self.handle_play_ping_request(&SPlayPingRequest::read(&mut payload, &version)?);
            }
            id if id == SClickSlot::to_id(version) => {
                player.on_slot_click(SClickSlot::read(&mut payload, &version)?, server);
            }
            id if id == SContainerButtonClick::to_id(version) => {
                player.on_container_button_click(&SContainerButtonClick::read(
                    &mut payload,
                    &version,
                )?);
            }
            id if id == SSetHeldItem::to_id(version) => {
                self.handle_set_held_item(
                    server,
                    player,
                    &SSetHeldItem::read(&mut payload, &version)?,
                );
            }
            id if id == SSetCreativeSlot::to_id(version) => {
                self.handle_set_creative_slot(
                    player,
                    SSetCreativeSlot::read(&mut payload, &version)?,
                )?;
            }
            id if id == SSwingArm::to_id(version) => {
                self.handle_swing_arm(server, player, &SSwingArm::read(&mut payload, &version)?);
            }
            id if id == SUpdateSign::to_id(version) => {
                self.handle_sign_update(player, &SUpdateSign::read(&mut payload, &version)?);
            }
            id if id == SEditBook::to_id(version) => {
                self.handle_edit_book(player, &SEditBook::read(&mut payload, &version)?);
            }
            id if id == SUseItemOn::to_id(version) => {
                self.handle_use_item_on(
                    player,
                    &SUseItemOn::read(&mut payload, &version)?,
                    server,
                )?;
            }
            id if id == SUseItem::to_id(version) => {
                self.handle_use_item(player, &SUseItem::read(&mut payload, &version)?, server);
            }
            id if id == SCommandSuggestion::to_id(version) => {
                self.handle_command_suggestion(
                    player,
                    &SCommandSuggestion::read(&mut payload, &version)?,
                    server,
                );
            }
            id if id == SPCookieResponse::to_id(version) => {
                self.handle_cookie_response(&SPCookieResponse::read(&mut payload, &version)?);
            }
            id if id == SCloseContainer::to_id(version) => {
                let _ = SCloseContainer::read(&mut payload, &version)?;
                self.handle_close_container(player);
            }
            id if id == SChunkBatch::to_id(version) => {
                self.handle_chunk_batch(player, &SChunkBatch::read(&mut payload, &version)?);
            }
            id if id == SPlayerSession::to_id(version) => {
                let session = SPlayerSession::read(&mut payload, &version)?;
                let client_platform = player.client.clone();
                let player_c = player.clone();
                let server_c = server.clone();
                return Ok(Some(Box::pin(async move {
                    if let ClientPlatform::Java(client) = client_platform.as_ref() {
                        client
                            .handle_chat_session_update(&player_c, &server_c, session)
                            .await;
                    }
                })));
            }
            id if id == SCustomPayload::to_id(version) => {
                let payload = SCustomPayload::read(&mut payload, &version)?;
                let channel_str = payload.channel.to_string();
                let mut event = PlayerCustomPayloadEvent::new(
                    player.clone(),
                    channel_str.clone(),
                    Bytes::copy_from_slice(payload.data),
                );
                server.plugin_manager.fire_blocking(server, &mut event);

                if channel_str == "minecraft:register" {
                    if let Ok(channels_data) = std::str::from_utf8(payload.data) {
                        for ch in channels_data.split('\0') {
                            if !ch.is_empty() {
                                let mut reg_event = crate::plugin::api::events::player::player_register_channel::PlayerRegisterChannelEvent::new(
                                    player.clone(),
                                    ch.to_string(),
                                );
                                server.plugin_manager.fire_blocking(server, &mut reg_event);
                                let mut ch_event = crate::plugin::api::events::player::player_channel::PlayerChannelEvent {
                                    player: player.clone(),
                                    channel: ch.to_string(),
                                    cancelled: false,
                                };
                                server.plugin_manager.fire_blocking(server, &mut ch_event);
                            }
                        }
                    }
                } else if channel_str == "minecraft:unregister"
                    && let Ok(channels_data) = std::str::from_utf8(payload.data)
                {
                    for ch in channels_data.split('\0') {
                        if !ch.is_empty() {
                            let mut unreg_event = crate::plugin::api::events::player::player_unregister_channel::PlayerUnregisterChannelEvent::new(
                                player.clone(),
                                ch.to_string(),
                            );
                            server
                                .plugin_manager
                                .fire_blocking(server, &mut unreg_event);
                        }
                    }
                }
            }
            id if id == SRecipeBookChangeSettings::to_id(version) => {
                self.handle_recipe_book_change_settings(
                    server,
                    player,
                    &SRecipeBookChangeSettings::read(&mut payload, &version)?,
                );
            }
            id if id == SRecipeBookSeenRecipe::to_id(version) => {
                self.handle_recipe_book_seen_recipe(
                    server,
                    player,
                    &SRecipeBookSeenRecipe::read(&mut payload, &version)?,
                );
            }
            id if id == SRenameItem::to_id(version) => {
                player.on_rename_item(&SRenameItem::read(&mut payload, &version)?);
            }
            id if id == SPlaceRecipe::to_id(version) => {
                let packet = SPlaceRecipe::read(&mut payload, &version)?;
                self.handle_place_recipe(server, player, &packet);
            }
            id if id
                == pumpkin_protocol::java::server::play::SCustomClickAction::to_id(version) =>
            {
                let packet = pumpkin_protocol::java::server::play::SCustomClickAction::read(
                    &mut payload,
                    &version,
                )?;
                let mut event = crate::plugin::api::events::dialog::dialog_click_action::DialogClickActionEvent::new(
                    player.clone(),
                    packet.action_id.to_string(),
                    packet.payload.map(Bytes::copy_from_slice),
                );
                server.plugin_manager.fire_blocking(server, &mut event);
            }
            id if id == SSelectTrade::to_id(version) => {
                self.handle_select_trade(player, &SSelectTrade::read(&mut payload, &version)?);
            }
            id if id == SSeenAdvancement::to_id(version) => {
                self.handle_seen_advancement(
                    player,
                    &SSeenAdvancement::read(&mut payload, &version)?,
                );
            }
            id if id == SPlayResourcePack::to_id(version) => {
                self.handle_play_resource_pack_response(
                    server,
                    player,
                    &SPlayResourcePack::read(&mut payload, &version)?,
                );
            }
            id if id == SPlayPong::to_id(version) => {
                self.handle_play_pong(player, &SPlayPong::read(&mut payload, &version)?);
            }
            id if id == SLockDifficulty::to_id(version) => {
                self.handle_lock_difficulty(
                    server,
                    player,
                    &SLockDifficulty::read(&mut payload, &version)?,
                );
            }
            id if id == SChangeDifficulty::to_id(version) => {
                self.handle_change_difficulty(
                    server,
                    player,
                    &SChangeDifficulty::read(&mut payload, &version)?,
                );
            }
            id if id == SSetBeacon::to_id(version) => {
                self.handle_set_beacon(player, &SSetBeacon::read(&mut payload, &version)?);
            }
            id if id == SContainerSlotStateChanged::to_id(version) => {
                self.handle_container_slot_state_changed(
                    player,
                    &SContainerSlotStateChanged::read(&mut payload, &version)?,
                );
            }
            id if id == SSpectateEntity::to_id(version) => {
                self.handle_spectate_entity(
                    player,
                    &SSpectateEntity::read(&mut payload, &version)?,
                );
            }
            id if id == SSetCommandMinecart::to_id(version) => {
                self.handle_set_command_minecart(
                    player,
                    &SSetCommandMinecart::read(&mut payload, &version)?,
                );
            }
            id if id == SSetStructureBlock::to_id(version) => {
                self.handle_set_structure_block(
                    player,
                    &SSetStructureBlock::read(&mut payload, &version)?,
                );
            }
            id if id == SSetGameRule::to_id(version) => {
                self.handle_set_game_rule(player, &SSetGameRule::read(&mut payload, &version)?);
            }
            id if id == SBlockEntityTagQuery::to_id(version) => {
                self.handle_block_entity_tag_query(
                    player,
                    &SBlockEntityTagQuery::read(&mut payload, &version)?,
                );
            }
            id if id == SEntityTagQuery::to_id(version) => {
                self.handle_entity_tag_query(
                    player,
                    &SEntityTagQuery::read(&mut payload, &version)?,
                );
            }
            id if id == SConfigurationAcknowledged::to_id(version) => {
                self.handle_configuration_acknowledged(player);
            }
            _ => {
                warn!("Failed to handle player packet id {}", event.packet_id);
            }
        }
        Ok(None)
    }
}

// UNRUN in the isolated implementation worktree. Root owns compilation/execution.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod frame_timer_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Use the production encoder, including its persistent cipher state for
    /// both frames. No synthetic parser or encryption implementation is tested.
    async fn frames(compressed: bool, encrypted: bool, data: &[u8]) -> (Vec<u8>, usize) {
        let (writer, mut reader) = tokio::io::duplex(8192);
        let mut encoder = TCPNetworkEncoder::new(writer);
        if compressed {
            encoder.set_compression((0, 6));
        }
        if encrypted {
            encoder.set_encryption(&[7; 16]).unwrap();
        }
        encoder
            .write_packet(Bytes::copy_from_slice(data))
            .await
            .unwrap();
        encoder.flush().await.unwrap();
        // Duplex reads the bytes already buffered, without waiting for EOF.
        let mut first = vec![0; 8192];
        let length = reader.read(&mut first).await.unwrap();
        first.truncate(length);
        encoder
            .write_packet(Bytes::from_static(&[5, 11, 12]))
            .await
            .unwrap();
        encoder.flush().await.unwrap();
        drop(encoder);
        reader.read_to_end(&mut first).await.unwrap();
        (first, length)
    }

    #[tokio::test(start_paused = true)]
    async fn actual_decoder_retains_fragmented_frames_across_repeated_keep_alive_ticks() {
        // Multi-byte frame length and ID in plain mode, compression header and
        // stream boundaries in compressed mode, and persistent CFB8 state too.
        let mut data = vec![0xac, 0x02]; // packet ID 300
        data.extend((0..384).map(|value| (value % 251) as u8));
        for (compressed, encrypted) in [(false, false), (true, false), (false, true), (true, true)]
        {
            let (wire, first_len) = frames(compressed, encrypted, &data).await;
            // All plain length/ID/payload boundaries; representative compressed
            // headers/body boundaries. Zlib trailer validation is a separate
            // upstream decoder concern, not this timer-cancellation test.
            let splits: Vec<_> = if compressed {
                vec![1, 2, 3, first_len / 2]
            } else {
                (1..first_len).collect()
            };
            for split in splits {
                let (mut writer, reader) = tokio::io::duplex(8192);
                let mut decoder = TCPNetworkDecoder::new(reader);
                if compressed {
                    decoder.set_compression(0);
                }
                if encrypted {
                    decoder.set_encryption(&[7; 16]).unwrap();
                }
                let mut interval = tokio::time::interval(Duration::from_secs(1));
                interval.tick().await;
                let timer_calls = AtomicUsize::new(0);
                writer.write_all(&wire[..split]).await.unwrap();
                let packet = {
                    let read = async { Some(decoder.get_raw_packet().await.unwrap()) };
                    let mut pending =
                        Box::pin(read_with_keep_alive(read, &mut interval, || async {
                            timer_calls.fetch_add(1, Ordering::Relaxed);
                            true
                        }));
                    // Poll first so the actual decoder consumes the fragment.
                    assert!(futures::poll!(&mut pending).is_pending());
                    for tick in 1..=2 {
                        tokio::time::advance(Duration::from_secs(1)).await;
                        // advance changes time; poll the production reader until
                        // its timer branch actually runs, without scheduler races.
                        tokio::time::timeout(
                            Duration::from_secs(1),
                            std::future::poll_fn(|cx| {
                                assert!(pending.as_mut().poll(cx).is_pending());
                                if timer_calls.load(Ordering::Relaxed) == tick {
                                    std::task::Poll::Ready(())
                                } else {
                                    std::task::Poll::Pending
                                }
                            }),
                        )
                        .await
                        .unwrap();
                    }
                    writer.write_all(&wire[split..]).await.unwrap();
                    tokio::time::timeout(Duration::from_secs(1), pending)
                        .await
                        .unwrap()
                        .unwrap()
                };
                assert_eq!(
                    packet.id, 300,
                    "split={split}, compressed={compressed}, encrypted={encrypted}"
                );
                assert_eq!(packet.payload.as_ref(), &data[2..]);
                let second = tokio::time::timeout(Duration::from_secs(1), decoder.get_raw_packet())
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(second.id, 5);
                assert_eq!(second.payload.as_ref(), &[11, 12]);
                drop(writer);
                assert!(matches!(
                    decoder.get_raw_packet().await,
                    Err(PacketDecodeError::ConnectionClosed)
                ));
            }
        }
    }
}
