//! Actual TCP close diagnostics. Execution belongs to ROOT; authored tests are UNRUN.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::data::player_server::test::{close_fixture, player_fixture};
use pumpkin_protocol::java::client::play::CSystemChatMessage;
use pumpkin_protocol::ser::NetworkReadSliceExt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const PREFIX_COUNT: usize = 10;

async fn bounded<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(TEST_TIMEOUT, future)
        .await
        .expect("TCP diagnostic must complete within its outer bound")
}

/// Queue 32 real, legal chat frames before allowing any server decoding. Long
/// messages make the burst larger than the production 8192-byte BufReader: even
/// one canceled read cannot consume all kernel input as a side effect. This is
/// a controlled transport boundary, not a reproduction of the runtime campaign.
async fn tcp_pair(unread_burst: bool) -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (peer, accepted) = tokio::join!(
        TcpStream::connect(listener.local_addr().unwrap()),
        listener.accept()
    );
    let mut peer = peer.unwrap();
    let (socket, _) = accepted.unwrap();
    peer.set_nodelay(true).unwrap();
    socket.set_nodelay(true).unwrap();
    if unread_burst {
        let mut encoder = TCPNetworkEncoder::new(&mut peer);
        let mut wire_length = 0;
        for index in 0..32 {
            let message = format!("{index:02}{}", "x".repeat(254));
            let data = JavaClient::serialize_packet_for_version(
                &SChatMessage {
                    message: &message,
                    timestamp: 0,
                    salt: 0,
                    signature: None,
                    message_count: VarInt(0),
                    acknowledged: &[0; 3],
                    checksum: 0,
                },
                CURRENT_MC_VERSION,
            )
            .unwrap();
            wire_length += VarInt(data.len() as i32).written_size() + data.len();
            encoder.write_packet(data).await.unwrap();
        }
        encoder.flush().await.unwrap();
        assert!(wire_length > 8192);
        // Positive OS-level receipt, not merely a peer socket-write completion.
        // peek does not consume input. The outer bound also bounds this gate.
        let mut peek = vec![0; wire_length];
        loop {
            if socket.peek(&mut peek).await.unwrap() == wire_length {
                break;
            }
            tokio::task::yield_now().await;
        }
    }
    (socket, peer)
}

fn spam_reason() -> TextComponent {
    TextComponent::translate(translation::java::DISCONNECT_SPAM, [])
}

#[derive(Debug, Default)]
struct PeerObservation {
    prefixes: usize,
    typed_spam: bool,
    clean_eof: bool,
    failure: Option<String>,
}

/// Decode the actual wire using the production decoder and component codec.
/// Report errors without abandoning the other half of the bounded diagnostic.
async fn observe_peer(peer: TcpStream) -> PeerObservation {
    let (reader, mut writer) = peer.into_split();
    let mut decoder = TCPNetworkDecoder::new(BufReader::new(reader));
    let mut observation = PeerObservation::default();
    loop {
        let packet = match decoder.get_raw_packet().await {
            Ok(packet) => packet,
            Err(PacketDecodeError::ConnectionClosed) => {
                observation.clean_eof = true;
                return observation;
            }
            Err(error) => {
                observation.failure = Some(format!("peer decode/end: {error:?}"));
                return observation;
            }
        };
        let mut payload = &packet.payload[..];
        if packet.id == CSystemChatMessage::to_id(CURRENT_MC_VERSION) {
            assert!(!observation.typed_spam, "prefix after terminal frame");
            let component = payload.get_component(&CURRENT_MC_VERSION).unwrap();
            assert_eq!(
                component.get_text(),
                format!("prefix_{}", observation.prefixes)
            );
            assert!(!payload.get_bool().unwrap());
            assert!(payload.is_empty());
            observation.prefixes += 1;
        } else {
            assert_eq!(packet.id, CPlayDisconnect::to_id(CURRENT_MC_VERSION));
            assert_eq!(observation.prefixes, PREFIX_COUNT);
            assert!(!observation.typed_spam, "duplicate terminal frame");
            let component = payload.get_component(&CURRENT_MC_VERSION).unwrap();
            assert_eq!(
                serde_json::to_value(component).unwrap()["translate"],
                "disconnect.spam"
            );
            assert!(payload.is_empty());
            observation.typed_spam = true;
            // A healthy peer responds to the typed reason by half-closing.
            if let Err(error) = writer.shutdown().await {
                observation.failure = Some(format!("peer shutdown: {error:?}"));
                return observation;
            }
        }
    }
}

fn assert_clean(observation: &PeerObservation) {
    assert_eq!(observation.prefixes, PREFIX_COUNT, "{observation:?}");
    assert!(observation.typed_spam, "{observation:?}");
    assert!(observation.clean_eof, "{observation:?}");
    assert!(observation.failure.is_none(), "{observation:?}");
}

async fn actual_client_close(unread_burst: bool) {
    let fixture = bounded(player_fixture()).await;
    let result = bounded(async {
        let (socket, peer) = tcp_pair(unread_burst).await;
        let pending = PendingConnection::new(
            socket,
            peer.local_addr().unwrap(),
            71,
            PacketRateLimiter::new(false, 0.0, 0.0),
            Arc::downgrade(&fixture.server),
        );
        pending.connection_state.store(ConnectionState::Play);
        let profile = fixture.player.gameprofile.clone();
        let mut java = JavaClient::from_pending(pending, profile.clone(), PlayerConfig::default());
        java.start_outgoing_packet_task();
        let client = Arc::new(ClientPlatform::Java(java));
        let player = Arc::new(Player::new(
            client.clone(),
            profile,
            PlayerConfig::default(),
            &fixture.player.world(),
            pumpkin_util::GameMode::Survival,
        ));
        let java = client.java().unwrap();
        java.set_player(&player);
        java.suspend_flushing();
        for index in 0..PREFIX_COUNT {
            java.try_send_packet(&CSystemChatMessage::new(
                &TextComponent::text(format!("prefix_{index}")),
                false,
            ));
        }
        java.try_kick(&spam_reason());
        assert!(java.is_closed());
        // No server read has run yet. Use the real reader-owner exit and tracked
        // writer lifecycle, including the close already canceled on entry.
        let lifecycle = async {
            java.progress_player_packets(&player, &fixture.server).await;
            java.close();
            java.await_tasks().await;
            assert!(java.network_reader.lock().unwrap().is_none());
            assert!(player.inbound_packets.is_empty());
            java.pending_bytes.load(Ordering::Acquire)
        };
        let (pending_bytes, observation) = tokio::join!(lifecycle, observe_peer(peer));
        assert_eq!(
            pending_bytes, 0,
            "writer did not frame all bytes: {observation:?}"
        );
        observation
    })
    .await;
    bounded(close_fixture(&fixture)).await;
    assert_clean(&result);
}

#[tokio::test]
async fn actual_tcp_quiet_peer_gets_prefixes_typed_disconnect_and_fin() {
    actual_client_close(false).await;
}

#[tokio::test]
async fn actual_tcp_unread_burst_gets_prefixes_typed_disconnect_and_fin() {
    actual_client_close(true).await;
}

/// A transparent observer below the real BufWriter. Successful write/flush
/// polls are local transport receipts, not assertions about peer delivery.
struct ObservedTcpWriter {
    socket: Option<OwnedWriteHalf>,
    events: Arc<std::sync::Mutex<Vec<String>>>,
}

impl AsyncWrite for ObservedTcpWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<io::Result<usize>> {
        let result = Pin::new(self.socket.as_mut().unwrap()).poll_write(cx, data);
        if let Poll::Ready(result) = &result {
            self.events
                .lock()
                .unwrap()
                .push(format!("tcp write: {result:?}"));
        }
        result
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let result = Pin::new(self.socket.as_mut().unwrap()).poll_flush(cx);
        if let Poll::Ready(result) = &result {
            self.events
                .lock()
                .unwrap()
                .push(format!("tcp flush: {result:?}"));
        }
        result
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(self.socket.as_mut().unwrap()).poll_shutdown(cx)
    }
}

impl Drop for ObservedTcpWriter {
    fn drop(&mut self) {
        drop(self.socket.take());
        self.events
            .lock()
            .unwrap()
            .push("write half dropped".into());
    }
}

/// Isolate writer receipts from the final peer observation without replacing
/// the framing, buffering or outgoing-writer implementation. The unread case
/// is diagnostic: it may reset even when every write and flush succeeded. The
/// actual-client regression above, independently, requires a clean peer end.
#[tokio::test]
#[allow(clippy::print_stderr)]
async fn actual_tcp_writer_receipts_and_reader_drop_diagnostic() {
    bounded(async {
        for unread_burst in [false, true] {
            let (socket, peer) = tcp_pair(unread_burst).await;
            let (reader, writer) = socket.into_split();
            let decoder = TCPNetworkDecoder::new(BufReader::new(reader));
            let events = Arc::new(std::sync::Mutex::new(Vec::new()));
            let writer = TCPNetworkEncoder::new(BufWriter::new(ObservedTcpWriter {
                socket: Some(writer),
                events: events.clone(),
            }));
            let (send, receive) = tokio::sync::mpsc::unbounded_channel();
            let pending_bytes = Arc::new(AtomicUsize::new(0));
            for index in 0..PREFIX_COUNT {
                let data = JavaClient::serialize_packet_for_version(
                    &CSystemChatMessage::new(
                        &TextComponent::text(format!("prefix_{index}")),
                        false,
                    ),
                    CURRENT_MC_VERSION,
                )
                .unwrap();
                pending_bytes.fetch_add(data.len(), Ordering::Relaxed);
                send.send(OutgoingPacket::normal(data)).unwrap();
            }
            let data = JavaClient::serialize_packet_for_version(
                &CPlayDisconnect::new(&spam_reason()),
                CURRENT_MC_VERSION,
            )
            .unwrap();
            pending_bytes.fetch_add(data.len(), Ordering::Relaxed);
            let (flushed, receipt) = oneshot::channel();
            send.send(OutgoingPacket::flushed(data, flushed)).unwrap();
            let close = CancellationToken::new();
            close.cancel();
            drop(decoder);
            events.lock().unwrap().push("read half dropped".into());
            run_outgoing_packet_writer(
                receive,
                writer,
                close,
                Arc::new(AtomicBool::new(true)),
                pending_bytes.clone(),
                72,
            )
            .await;
            let receipt = receipt.await;
            let observation = observe_peer(peer).await;
            let events = events.lock().unwrap();
            eprintln!("unread_burst={unread_burst}, flush_receipt={receipt:?}, events={events:?}, peer={observation:?}");
            assert!(receipt.is_ok(), "{events:?}; {observation:?}");
            assert_eq!(pending_bytes.load(Ordering::Acquire), 0);
            assert!(events.iter().any(|event| event == "tcp flush: Ok(())"));
            assert_eq!(events.last().unwrap(), "write half dropped");
            if !unread_burst {
                assert_clean(&observation);
            }
        }
    })
    .await;
}

const STREAM_ACTIONS: usize = 32;
const STREAM_WINDOW: usize = 4;
const STREAM_COMPRESSION: (usize, u32) = (256, 4);

#[derive(Clone, Copy, Debug)]
enum StreamRoute {
    Command,
    Chat,
}

impl StreamRoute {
    fn action(self, index: usize) -> Bytes {
        match self {
            Self::Command => JavaClient::serialize_packet_for_version(
                &SChatCommand {
                    command: "trigger spamprobe set 13",
                },
                CURRENT_MC_VERSION,
            ),
            Self::Chat => JavaClient::serialize_packet_for_version(
                &SChatMessage {
                    message: &format!("stream_{index:02}"),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                        .try_into()
                        .unwrap(),
                    salt: 0,
                    signature: None,
                    message_count: VarInt(0),
                    acknowledged: &[0; 3],
                    checksum: 0,
                },
                CURRENT_MC_VERSION,
            ),
        }
        .unwrap()
    }
}

struct StreamTrace {
    started: Instant,
    events: std::sync::Mutex<Vec<String>>,
}

impl StreamTrace {
    fn record(&self, event: impl std::fmt::Display) {
        self.events
            .lock()
            .unwrap()
            .push(format!("{:?}: {event}", self.started.elapsed()));
    }
}

#[derive(Debug, Default)]
struct StreamProducer {
    attempted: usize,
    completed: usize,
    feedback_gates: usize,
    tick_ends: usize,
    write_failure: Option<String>,
}

/// The first four actions need no feedback. Every subsequent action needs a
/// decoded real handler reply, so the workload cannot all precede server reads.
/// This bounded window is an integration schedule, not Node IPC/callback parity.
async fn produce_stream(
    mut writer: OwnedWriteHalf,
    route: StreamRoute,
    mut feedback: UnboundedReceiver<()>,
    peer_done: &CancellationToken,
    typed_spam: &AtomicBool,
    trace: &StreamTrace,
) -> StreamProducer {
    let mut result = StreamProducer::default();
    let mut credits = STREAM_WINDOW;
    let mut encoder = TCPNetworkEncoder::new(&mut writer);
    encoder.set_compression(STREAM_COMPRESSION);
    let tick_period = Duration::from_millis(50);
    let mut ticks = tokio::time::interval_at(Instant::now() + tick_period, tick_period);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    while result.attempted < STREAM_ACTIONS {
        let (data, action) = tokio::select! {
            biased;
            () = peer_done.cancelled() => break,
            _ = ticks.tick(), if result.tick_ends < STREAM_ACTIONS => {
                result.tick_ends += 1;
                (JavaClient::serialize_packet_for_version(&SClientTickEnd, CURRENT_MC_VERSION).unwrap(), None)
            }
            reply = feedback.recv(), if credits == 0 => {
                if reply.is_none() {
                    break;
                }
                credits += 1;
                result.feedback_gates += 1;
                continue;
            }
            () = std::future::ready(()), if credits > 0 => {
                let index = result.attempted;
                result.attempted += 1;
                credits -= 1;
                (route.action(index), Some(index))
            }
        };
        // Small packets still carry the negotiated compression envelope. Frame
        // with the real encoder so the receipt includes the actual wire size.
        assert!(data.len() < STREAM_COMPRESSION.0);
        let packet_id = (&data[..]).get_var_int().unwrap().0;
        let mut frame = Vec::new();
        encoder.frame_packet(&data, &mut frame).unwrap();
        trace.record(format_args!(
            "write start action={action:?} packet={packet_id} wire_bytes={} typed_spam={}",
            frame.len(),
            typed_spam.load(Ordering::Acquire)
        ));
        let write = encoder.write_frame(&frame).await;
        trace.record(format_args!(
            "write end action={action:?} packet={packet_id} result={write:?} typed_spam={}",
            typed_spam.load(Ordering::Acquire)
        ));
        if let Err(error) = write {
            result.write_failure = Some(format!(
                "action={action:?} packet={packet_id} write: {error:?}"
            ));
            break;
        }
        let flush = encoder.flush().await;
        trace.record(format_args!(
            "flush action={action:?} packet={packet_id} result={flush:?}"
        ));
        if let Err(error) = flush {
            result.write_failure = Some(format!(
                "action={action:?} packet={packet_id} flush: {error:?}"
            ));
            break;
        }
        result.completed += usize::from(action.is_some());
    }
    drop(encoder);
    trace.record(format_args!(
        "producer stopped remaining={} peer_done={} typed_spam={} write_failure={:?}",
        STREAM_ACTIONS - result.attempted,
        peer_done.is_cancelled(),
        typed_spam.load(Ordering::Acquire),
        result.write_failure,
    ));
    // Keep concurrent reads alive after a producer error. Do not turn an error
    // into a fabricated clean end by dropping/half-closing the peer early.
    peer_done.cancelled().await;
    let shutdown = writer.shutdown().await;
    trace.record(format_args!(
        "peer write shutdown: {shutdown:?}; producer={result:?}"
    ));
    result
}

enum StreamFeedback {
    Processed,
    Spam,
    Score,
}

fn contains_translation(value: &serde_json::Value, expected: &str) -> bool {
    match value {
        serde_json::Value::Object(fields) => {
            fields.get("translate").and_then(serde_json::Value::as_str) == Some(expected)
                || fields
                    .values()
                    .any(|child| contains_translation(child, expected))
        }
        serde_json::Value::Array(items) => items
            .iter()
            .any(|child| contains_translation(child, expected)),
        _ => false,
    }
}

/// Validate replies from the real handlers, including their complete payload.
fn decode_stream_feedback(
    packet: &RawPacket,
    route: StreamRoute,
    prefixes: usize,
) -> Result<StreamFeedback, PacketDecodeError> {
    use pumpkin_protocol::java::client::play::{CDisguisedChatMessage, CUpdateScore};

    let mut payload = &packet.payload[..];
    if matches!(route, StreamRoute::Command) && packet.id == CUpdateScore::to_id(CURRENT_MC_VERSION)
    {
        let expected = JavaClient::serialize_packet_for_version(
            &CUpdateScore::new(
                "StreamedPlayer".into(),
                "spamprobe".into(),
                VarInt(13),
                None,
                None,
            ),
            CURRENT_MC_VERSION,
        )
        .unwrap();
        let mut expected_payload = &expected[..];
        expected_payload.get_var_int()?;
        if payload == expected_payload {
            return Ok(StreamFeedback::Score);
        }
        return Err(PacketDecodeError::Message(
            "unexpected trigger score payload".into(),
        ));
    }
    let component = payload.get_component(&CURRENT_MC_VERSION)?;
    let feedback = if packet.id == CPlayDisconnect::to_id(CURRENT_MC_VERSION) {
        if serde_json::to_value(&component).unwrap()["translate"] != "disconnect.spam" {
            return Err(PacketDecodeError::Message(format!(
                "unexpected disconnect: {component:?}"
            )));
        }
        StreamFeedback::Spam
    } else if matches!(route, StreamRoute::Command)
        && packet.id == CSystemChatMessage::to_id(CURRENT_MC_VERSION)
    {
        let expected = if prefixes == 0 {
            "commands.trigger.set.success"
        } else {
            "commands.trigger.failed.unprimed"
        };
        // CommandSource::send_error wraps the translated error in an empty
        // red component; success and failure need not share the same root.
        if !contains_translation(&serde_json::to_value(&component).unwrap(), expected)
            || payload.get_bool()?
        {
            return Err(PacketDecodeError::Message(format!(
                "unexpected command feedback: {component:?}"
            )));
        }
        StreamFeedback::Processed
    } else if matches!(route, StreamRoute::Chat)
        && packet.id == CDisguisedChatMessage::to_id(CURRENT_MC_VERSION)
    {
        let text = component.get_text();
        if !text.ends_with(&format!("stream_{prefixes:02}"))
            || payload.get_var_int()?.0 != i32::from(pumpkin_data::world::RAW) + 1
            || !payload
                .get_component(&CURRENT_MC_VERSION)?
                .get_text()
                .is_empty()
            || payload.get_bool()?
        {
            return Err(PacketDecodeError::Message(format!(
                "unexpected chat feedback: {text:?}"
            )));
        }
        StreamFeedback::Processed
    } else {
        return Err(PacketDecodeError::Message(format!(
            "unexpected packet {}",
            packet.id
        )));
    };
    if !payload.is_empty() {
        return Err(PacketDecodeError::Message(
            "trailing feedback payload".into(),
        ));
    }
    Ok(feedback)
}

#[derive(Debug, Default)]
struct StreamObservation {
    prefixes: usize,
    typed_spam: bool,
    // Some(Ok(())) is a decoded clean EOF, Some(Err(_)) retains the real
    // transport/decoder error, and None means no transport end was observed.
    transport_end: Option<Result<(), PacketDecodeError>>,
    protocol_failure: Option<String>,
}

/// Typed protocol delivery and the later transport end are separate receipts.
/// An EOF/reset/timeout is never inferred from successful local writes.
async fn observe_stream(
    reader: OwnedReadHalf,
    route: StreamRoute,
    feedback: UnboundedSender<()>,
    peer_done: &CancellationToken,
    typed_spam: &AtomicBool,
    trace: &StreamTrace,
) -> StreamObservation {
    let mut decoder = TCPNetworkDecoder::new(BufReader::new(reader));
    decoder.set_compression(STREAM_COMPRESSION.0);
    let mut observation = StreamObservation::default();
    let deadline = trace.started + Duration::from_secs(8);
    loop {
        let packet = match tokio::time::timeout_at(deadline, decoder.get_raw_packet()).await {
            Ok(Ok(packet)) => packet,
            Ok(Err(PacketDecodeError::ConnectionClosed)) => {
                observation.transport_end = Some(Ok(()));
                trace.record("peer transport: clean EOF");
                break;
            }
            Ok(Err(error)) => {
                observation.transport_end = Some(Err(error));
                break;
            }
            Err(_) => {
                trace.record("peer transport: deadline expired without EOF/error");
                break;
            }
        };
        trace.record(format_args!(
            "peer packet={} typed_spam={}",
            packet.id, observation.typed_spam
        ));
        if observation.typed_spam {
            observation.protocol_failure = Some("packet after typed disconnect".into());
            break;
        }
        match decode_stream_feedback(&packet, route, observation.prefixes) {
            Ok(StreamFeedback::Processed) => {
                observation.prefixes += 1;
                trace.record(format_args!("processed prefix={}", observation.prefixes));
                // The producer may already have stopped after a write failure.
                // Keep reading and retain that failure in its independent receipt.
                let receipt = feedback.send(());
                trace.record(format_args!("feedback gate: {receipt:?}"));
            }
            Ok(StreamFeedback::Spam) => {
                observation.typed_spam = true;
                typed_spam.store(true, Ordering::Release);
                trace.record("typed disconnect.spam");
                peer_done.cancel();
            }
            Ok(StreamFeedback::Score) => trace.record("trigger score=13"),
            Err(error) => {
                observation.protocol_failure = Some(format!("peer protocol: {error:?}"));
                break;
            }
        }
    }
    trace.record(format_args!("peer end: {observation:?}"));
    peer_done.cancel();
    observation
}

/// UNRUN: root must first execute this against unchanged production. A natural
/// spam kick competes with bounded deferred peer writes and concurrent reads.
/// The receive window cannot guarantee Node's callback/EPIPE ordering. No
/// server transport hooks, explicit kick, flushing hold, or terminal write gate.
#[allow(clippy::print_stderr, clippy::too_many_lines)]
async fn actual_tcp_streamed_spam(route: StreamRoute) {
    use crate::data::player_server::test::{drive_packet_future, stop_chunk_workers};
    use crate::entity::EntityBase;
    use crate::world::scoreboard::{NoTarget, ScoreboardObjective, ScoreboardScore};
    use pumpkin_protocol::java::client::play::RenderType;

    let fixture = bounded(player_fixture()).await;
    bounded(stop_chunk_workers(&fixture)).await;
    let world = fixture.player.world();
    for chunk in world.level.loaded_chunks.iter() {
        chunk.post_processing_ready.store(true, Ordering::Release);
    }
    let (socket, peer) = bounded(tcp_pair(false)).await;
    let mut pending = PendingConnection::new(
        socket,
        peer.local_addr().unwrap(),
        73,
        PacketRateLimiter::new(false, 0.0, 0.0),
        Arc::downgrade(&fixture.server),
    );
    pending.connection_state.store(ConnectionState::Play);
    pending.network_reader.set_compression(STREAM_COMPRESSION.0);
    pending.network_writer.set_compression(STREAM_COMPRESSION);
    let mut profile = fixture.player.gameprofile.clone();
    profile.id = uuid::Uuid::from_u128(103);
    profile.name = "StreamedPlayer".into();
    let mut java = JavaClient::from_pending(pending, profile.clone(), PlayerConfig::default());
    java.start_outgoing_packet_task();
    let client = Arc::new(ClientPlatform::Java(java));
    let player = Arc::new(Player::new(
        client.clone(),
        profile,
        PlayerConfig::default(),
        &world,
        pumpkin_util::GameMode::Survival,
    ));
    player
        .get_entity()
        .set_pos(pumpkin_util::math::vector3::Vector3::new(0.5, 65.0, 0.5));
    let java = client.java().unwrap();
    java.set_player(&player);
    world.add_player(&player).unwrap();
    // One real packet epoch has enough admission budget for 32 actions plus at
    // most 32 TickEnd frames. No gameplay ticks or synthetic spam-counter writes.
    player.packet_activity.begin_epoch();
    if matches!(route, StreamRoute::Command) {
        let mut scoreboard = world.scoreboard.lock().unwrap();
        scoreboard.add_objective(
            &NoTarget,
            ScoreboardObjective::new(
                "spamprobe",
                TextComponent::text("spamprobe"),
                RenderType::Integer,
                None,
                "trigger",
            ),
        );
        let mut score = ScoreboardScore::new("StreamedPlayer", "spamprobe", VarInt(0), None, None);
        score.locked = false;
        scoreboard.update_score(&NoTarget, score);
    }
    let trace = StreamTrace {
        started: Instant::now(),
        events: std::sync::Mutex::new(Vec::new()),
    };
    let peer_done = CancellationToken::new();
    let typed_spam = AtomicBool::new(false);
    let (feedback, replies) = tokio::sync::mpsc::unbounded_channel();
    let (reader, writer) = peer.into_split();
    let (pending_bytes, producer, observation) = drive_packet_future(&fixture.server, async {
        tokio::join!(
            async {
                java.progress_player_packets(&player, &fixture.server).await;
                trace.record("server reader returned");
                java.await_tasks().await;
                trace.record("server client tasks joined");
                java.pending_bytes.load(Ordering::Acquire)
            },
            produce_stream(writer, route, replies, &peer_done, &typed_spam, &trace),
            observe_stream(reader, route, feedback, &peer_done, &typed_spam, &trace),
        )
    })
    .await;
    eprintln!(
        "stream route={route:?}; producer={producer:?}; producer_error={:?}; peer={observation:?}; pending_bytes={pending_bytes}; events={:#?}",
        producer.write_failure,
        trace.events.lock().unwrap()
    );
    let score = world
        .scoreboard
        .lock()
        .unwrap()
        .get_score("StreamedPlayer", "spamprobe")
        .cloned();
    let queue_empty = player.inbound_packets.is_empty();
    let reader_released = java.network_reader.lock().unwrap().is_none();
    bounded(world.remove_player(&player, false)).await;
    java.player.store(Arc::new(None));
    bounded(close_fixture(&fixture)).await;

    assert!(producer.feedback_gates > 0, "{producer:?}");
    assert!(producer.completed > STREAM_WINDOW, "{producer:?}");
    assert!(producer.attempted <= STREAM_ACTIONS, "{producer:?}");
    assert!(observation.prefixes > STREAM_WINDOW, "{observation:?}");
    assert!(
        observation.prefixes <= producer.attempted,
        "{producer:?}; {observation:?}"
    );
    assert!(observation.typed_spam, "{producer:?}; {observation:?}");
    assert!(observation.protocol_failure.is_none(), "{observation:?}");
    assert!(observation.transport_end.is_some(), "{observation:?}");
    assert_eq!(pending_bytes, 0, "{producer:?}; {observation:?}");
    assert!(queue_empty && reader_released);
    if matches!(route, StreamRoute::Command) {
        let score = score.unwrap();
        assert_eq!(score.value, VarInt(13));
        assert!(score.locked);
    }
    // Deliberately no clean-transport assertion: write errors and EOF/reset are
    // printed independently even when the mandatory typed protocol delivery passes.
}

#[tokio::test]
async fn actual_tcp_streamed_commands_deliver_processed_prefix_and_typed_spam() {
    actual_tcp_streamed_spam(StreamRoute::Command).await;
}

#[tokio::test]
async fn actual_tcp_streamed_chat_delivers_processed_prefix_and_typed_spam() {
    actual_tcp_streamed_spam(StreamRoute::Chat).await;
}
