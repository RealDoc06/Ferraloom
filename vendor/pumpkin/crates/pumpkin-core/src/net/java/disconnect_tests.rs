//! Actual TCP close diagnostics. Execution belongs to ROOT; authored tests are UNRUN.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::data::player_server::test::{PlayerFixture, close_fixture, player_fixture};
use pumpkin_protocol::java::client::play::CSystemChatMessage;
use pumpkin_protocol::ser::NetworkReadSliceExt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const PREFIX_COUNT: usize = 10;

async fn bounded<F: Future>(future: F) -> F::Output {
    tokio::time::timeout(TEST_TIMEOUT, future)
        .await
        .expect("TCP diagnostic must complete within its outer bound")
}

/// Queue 32 real, legal chat frames before allowing any server decoding. Long
/// messages make the burst larger than the production 8192-byte `BufReader`: even
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
    let result = Box::pin(bounded(async {
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
    }))
    .await;
    bounded(close_fixture(&fixture)).await;
    assert_clean(&result);
}

#[tokio::test]
async fn actual_tcp_quiet_peer_gets_prefixes_typed_disconnect_and_fin() {
    Box::pin(actual_client_close(false)).await;
}

#[tokio::test]
async fn actual_tcp_unread_burst_gets_prefixes_typed_disconnect_and_fin() {
    Box::pin(actual_client_close(true)).await;
}

/// A transparent observer below the real `BufWriter`. Successful write/flush
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
    if matches!(route, StreamRoute::Command)
        && packet.id == CSystemChatMessage::to_id(CURRENT_MC_VERSION)
    {
        // Literal translation arguments are valid NBT strings. The upstream
        // TextComponentBase deserializer only accepts maps in `with`, so inspect
        // the complete wire value with the production strict NBT reader instead.
        let component = pumpkin_protocol::ser::NetworkReadExt::get_nbt_strict(&mut payload)?;
        let value = pumpkin_protocol::ser::nbt_tag_to_json(&component);
        let expected = if prefixes == 0 {
            "commands.trigger.set.success"
        } else {
            "commands.trigger.failed.unprimed"
        };
        if !contains_translation(&value, expected) || payload.get_bool()? || !payload.is_empty() {
            return Err(PacketDecodeError::Message(format!(
                "unexpected command feedback: {component:?}"
            )));
        }
        return Ok(StreamFeedback::Processed);
    }
    let component = payload.get_component(&CURRENT_MC_VERSION)?;
    let feedback = if packet.id == CPlayDisconnect::to_id(CURRENT_MC_VERSION) {
        if serde_json::to_value(&component).unwrap()["translate"] != "disconnect.spam" {
            return Err(PacketDecodeError::Message(format!(
                "unexpected disconnect: {component:?}"
            )));
        }
        StreamFeedback::Spam
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

#[test]
fn stream_command_feedback_reads_literal_translation_arguments() {
    let encoded = JavaClient::serialize_packet_for_version(
        &CSystemChatMessage::new(
            &TextComponent::translate(
                "commands.trigger.set.success",
                [TextComponent::text("spamprobe"), TextComponent::text("13")],
            ),
            false,
        ),
        CURRENT_MC_VERSION,
    )
    .unwrap();
    let mut payload = &encoded[..];
    let id = payload.get_var_int().unwrap().0;
    let packet = RawPacket {
        id,
        payload: Bytes::copy_from_slice(payload),
    };
    assert!(matches!(
        decode_stream_feedback(&packet, StreamRoute::Command, 0),
        Ok(StreamFeedback::Processed)
    ));
    assert!(decode_stream_feedback(&packet, StreamRoute::Command, 1).is_err());

    let mut trailing = packet.payload.to_vec();
    trailing.push(0);
    assert!(
        decode_stream_feedback(
            &RawPacket {
                id,
                payload: trailing.into(),
            },
            StreamRoute::Command,
            0,
        )
        .is_err()
    );
    assert!(
        decode_stream_feedback(
            &RawPacket {
                id,
                payload: packet.payload.slice(..packet.payload.len() - 1),
            },
            StreamRoute::Command,
            0,
        )
        .is_err()
    );
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
    let (pending_bytes, producer, observation) =
        Box::pin(drive_packet_future(&fixture.server, async {
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
        }))
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
    Box::pin(actual_tcp_streamed_spam(StreamRoute::Command)).await;
}

#[tokio::test]
async fn actual_tcp_streamed_chat_delivers_processed_prefix_and_typed_spam() {
    Box::pin(actual_tcp_streamed_spam(StreamRoute::Chat)).await;
}

fn connected_player(fixture: &PlayerFixture, socket: TcpStream) -> Arc<Player> {
    let address = socket.peer_addr().unwrap();
    let pending = PendingConnection::new(
        socket,
        address,
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
    client.java().unwrap().set_player(&player);
    player
}

mod preparation_tests {
    use super::*;
    use crate::plugin::{EventHandler, EventPriority};
    use futures::future::BoxFuture;
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::sync::{Mutex, Weak};
    use std::task::{Wake, Waker};
    use tokio::sync::Notify;

    thread_local! {
        // A reactor wake on another thread must not masquerade as the
        // synchronous notification of the production preparation waiter.
        static PROBING_RESOLUTION: Cell<bool> = const { Cell::new(false) };
    }

    struct PreparationPort {
        entered: oneshot::Sender<()>,
        release: oneshot::Receiver<bool>,
    }

    struct HoldTranslation {
        player: Weak<Player>,
        ports: Mutex<VecDeque<PreparationPort>>,
    }

    impl EventHandler<PacketSentEvent> for HoldTranslation {
        fn handle_blocking<'a>(
            &'a self,
            _server: &'a Arc<Server>,
            event: &'a mut PacketSentEvent,
        ) -> BoxFuture<'a, ()> {
            Box::pin(async move {
                if !self
                    .player
                    .upgrade()
                    .is_some_and(|player| Arc::ptr_eq(&player, &event.player))
                {
                    return;
                }
                assert_eq!(event.packet_id, CPlayDisconnect::to_id(CURRENT_MC_VERSION));
                let java = event.player.client.java().unwrap();
                assert!(
                    java.disconnect_intent.try_lock().is_ok(),
                    "callback under transport lock"
                );
                assert!(event.player.packet_activity.enter_non_packet().is_none());
                let port = self.ports.lock().unwrap().pop_front().unwrap();
                let _ = port.entered.send(());
                // Dropping the test's sender also releases a failed test's callback.
                event.cancelled = port.release.await.unwrap_or(true);
            })
        }
    }

    fn hold_translations(
        fixture: &PlayerFixture,
        player: &Arc<Player>,
        count: usize,
    ) -> VecDeque<(oneshot::Receiver<()>, oneshot::Sender<bool>)> {
        let mut ports = VecDeque::new();
        let mut controls = VecDeque::new();
        for _ in 0..count {
            let (entered, observed) = oneshot::channel();
            let (release, released) = oneshot::channel();
            ports.push_back(PreparationPort {
                entered,
                release: released,
            });
            controls.push_back((observed, release));
        }
        fixture.server.plugin_manager.register(
            Arc::new(HoldTranslation {
                player: Arc::downgrade(player),
                ports: Mutex::new(ports),
            }),
            EventPriority::Normal,
            true,
        );
        // Only selects the real translation callback. It passes the current
        // protocol bytes through unchanged; this is not a version-parity test.
        player
            .client
            .java()
            .unwrap()
            .version
            .store(JavaMinecraftVersion::V_1_21_11);
        controls
    }

    fn start_kick(player: Arc<Player>, explicit: bool) -> JoinHandle<()> {
        tokio::task::spawn_blocking(move || {
            let java = player.client.java().unwrap();
            if explicit {
                tokio::runtime::Handle::current()
                    .block_on(java.kick_explicit(&spam_reason(), true));
            } else {
                java.try_kick(&spam_reason());
            }
        })
    }

    fn queue_prefixes(java: &JavaClient) {
        java.suspend_flushing();
        for index in 0..PREFIX_COUNT {
            java.try_send_packet(&CSystemChatMessage::new(
                &TextComponent::text(format!("prefix_{index}")),
                false,
            ));
        }
    }

    async fn lifecycle(player: &Arc<Player>, server: &Arc<Server>, returned: &AtomicBool) {
        let java = player.client.java().unwrap();
        java.progress_player_packets(player, server).await;
        returned.store(true, Ordering::Release);
        java.close();
        java.await_tasks().await;
    }

    #[derive(Default)]
    struct ReadWake {
        resolution_waiter: AtomicBool,
        ready: Notify,
    }

    impl Wake for ReadWake {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            if PROBING_RESOLUTION.get() {
                self.resolution_waiter.store(true, Ordering::Relaxed);
            }
            self.ready.notify_one();
        }
    }

    /// Observe the production resolution waiter through its registered waker.
    /// This proves the complete TCP packet passed decode and was refused at
    /// sealed admission: a future still waiting for input cannot pass this gate.
    async fn reach_preparation_wait<F: Future<Output = ()>>(
        mut transport: Pin<&mut F>,
        java: &JavaClient,
        returned: &AtomicBool,
    ) {
        let signal = Arc::new(ReadWake::default());
        let waker = Waker::from(signal.clone());
        loop {
            assert!(
                transport
                    .as_mut()
                    .poll(&mut Context::from_waker(&waker))
                    .is_pending()
            );
            assert!(
                !returned.load(Ordering::Acquire),
                "reader escaped before preparation resolved"
            );
            PROBING_RESOLUTION.set(true);
            java.disconnect_changed.notify_waiters();
            PROBING_RESOLUTION.set(false);
            if signal.resolution_waiter.load(Ordering::Relaxed) {
                assert!(java.network_reader.lock().unwrap().is_none());
                assert!(!java.is_closed());
                assert!(!java.writer_finished.is_cancelled());
                return;
            }
            signal.ready.notified().await;
        }
    }

    async fn completed_tick(peer: &mut TcpStream) {
        let mut encoder = TCPNetworkEncoder::new(peer);
        encoder
            .write_packet(
                JavaClient::serialize_packet_for_version(&SClientTickEnd, CURRENT_MC_VERSION)
                    .unwrap(),
            )
            .await
            .unwrap();
        encoder.flush().await.unwrap();
    }

    async fn paused_case(fixture: &PlayerFixture, explicit: bool, abort: bool) {
        let (socket, mut peer) = tcp_pair(false).await;
        let player = connected_player(fixture, socket);
        let java = player.client.java().unwrap();
        queue_prefixes(java);
        let (entered, release) = hold_translations(fixture, &player, 1).pop_front().unwrap();
        let kick = start_kick(player.clone(), explicit);
        entered.await.unwrap();
        let deadline = java.disconnect_intent.lock().unwrap().deadline.unwrap();
        let returned = AtomicBool::new(false);
        let mut transport = Box::pin(lifecycle(&player, &fixture.server, &returned));
        completed_tick(&mut peer).await;
        reach_preparation_wait(transport.as_mut(), java, &returned).await;
        assert!(player.inbound_packets.is_empty());
        release.send(abort).unwrap();
        let observation = if abort {
            // The peer stays open: an aborted translation must not wait for its EOF.
            let ((), result) = tokio::time::timeout(Duration::from_secs(1), async {
                tokio::join!(&mut transport, kick)
            })
            .await
            .expect("resolved abort must release reader and writer promptly");
            result.unwrap();
            observe_peer(peer).await
        } else {
            let ((), result, observation) = tokio::join!(transport, kick, observe_peer(peer));
            result.unwrap();
            observation
        };
        assert_eq!(observation.prefixes, PREFIX_COUNT, "{observation:?}");
        assert!(observation.clean_eof, "{observation:?}");
        assert!(observation.failure.is_none(), "{observation:?}");
        assert_eq!(observation.typed_spam, !abort, "{observation:?}");
        let intent = java.disconnect_intent.lock().unwrap();
        assert_eq!(intent.deadline, Some(deadline));
        assert_eq!(intent.preparing, 0);
        assert_eq!(intent.queued, !abort);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn actual_tcp_completed_read_retains_decoder_during_kick_preparation() {
        let fixture = bounded(player_fixture()).await;
        for explicit in [false, true] {
            for abort in [false, true] {
                bounded(paused_case(&fixture, explicit, abort)).await;
            }
        }
        bounded(close_fixture(&fixture)).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn actual_tcp_one_aborted_preparation_does_not_close_another() {
        let fixture = bounded(player_fixture()).await;
        bounded(async {
            let (socket, mut peer) = tcp_pair(false).await;
            let player = connected_player(&fixture, socket);
            let java = player.client.java().unwrap();
            queue_prefixes(java);
            let mut controls = hold_translations(&fixture, &player, 2);
            let (entered_first, release_first) = controls.pop_front().unwrap();
            let (entered_second, release_second) = controls.pop_front().unwrap();
            let first = start_kick(player.clone(), false);
            entered_first.await.unwrap();
            let deadline = java.disconnect_intent.lock().unwrap().deadline.unwrap();
            let second = start_kick(player.clone(), true);
            entered_second.await.unwrap();
            assert_eq!(java.disconnect_intent.lock().unwrap().preparing, 2);
            let returned = AtomicBool::new(false);
            let mut transport = Box::pin(lifecycle(&player, &fixture.server, &returned));
            completed_tick(&mut peer).await;
            reach_preparation_wait(transport.as_mut(), java, &returned).await;
            release_first.send(true).unwrap();
            first.await.unwrap();
            assert!(!java.is_closed());
            assert_eq!(java.disconnect_intent.lock().unwrap().preparing, 1);
            reach_preparation_wait(transport.as_mut(), java, &returned).await;
            release_second.send(false).unwrap();
            let ((), result, observation) = tokio::join!(transport, second, observe_peer(peer));
            result.unwrap();
            assert_clean(&observation);
            assert_eq!(
                java.disconnect_intent.lock().unwrap().deadline,
                Some(deadline)
            );
            assert_eq!(java.disconnect_intent.lock().unwrap().preparing, 0);
        })
        .await;
        bounded(close_fixture(&fixture)).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn actual_tcp_independent_close_can_abort_a_preparing_reason() {
        let fixture = bounded(player_fixture()).await;
        for explicit in [false, true] {
            bounded(async {
                let (socket, mut peer) = tcp_pair(false).await;
                let player = connected_player(&fixture, socket);
                let java = player.client.java().unwrap();
                queue_prefixes(java);
                let (entered, release) =
                    hold_translations(&fixture, &player, 1).pop_front().unwrap();
                let kick = start_kick(player.clone(), explicit);
                entered.await.unwrap();
                let deadline = java.disconnect_intent.lock().unwrap().deadline.unwrap();
                let returned = AtomicBool::new(false);
                let mut transport = Box::pin(lifecycle(&player, &fixture.server, &returned));
                completed_tick(&mut peer).await;
                reach_preparation_wait(transport.as_mut(), java, &returned).await;

                // An independent close is authoritative. In contrast to the
                // ordinary preparation path, it may end the writer before enqueue.
                java.close();
                java.writer_finished.cancelled().await;
                assert!(java.outgoing_packet_queue_send.is_closed());
                assert_eq!(java.disconnect_intent.lock().unwrap().preparing, 1);
                release.send(false).unwrap();
                let ((), result) = tokio::time::timeout(Duration::from_secs(1), async {
                    tokio::join!(transport, kick)
                })
                .await
                .expect("failed enqueue must resolve the still-open peer's reader promptly");
                result.unwrap();
                let observation = observe_peer(peer).await;
                assert_eq!(observation.prefixes, PREFIX_COUNT, "{observation:?}");
                assert!(!observation.typed_spam, "{observation:?}");
                assert!(observation.clean_eof, "{observation:?}");
                assert!(observation.failure.is_none(), "{observation:?}");
                let intent = java.disconnect_intent.lock().unwrap();
                assert_eq!(intent.deadline, Some(deadline));
                assert_eq!(intent.preparing, 0);
                assert!(!intent.queued);
                assert_eq!(java.pending_bytes.load(Ordering::Acquire), 0);
            })
            .await;
        }
        bounded(close_fixture(&fixture)).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn actual_tcp_unresolved_preparation_expires_at_first_intent_deadline() {
        let fixture = bounded(player_fixture()).await;
        bounded(async {
            let (socket, mut peer) = tcp_pair(false).await;
            let player = connected_player(&fixture, socket);
            let java = player.client.java().unwrap();
            let (entered, release) = hold_translations(&fixture, &player, 1).pop_front().unwrap();
            let kick = start_kick(player.clone(), false);
            entered.await.unwrap();
            let deadline = java.disconnect_intent.lock().unwrap().deadline.unwrap();
            let returned = AtomicBool::new(false);
            let mut transport = Box::pin(lifecycle(&player, &fixture.server, &returned));
            completed_tick(&mut peer).await;
            reach_preparation_wait(transport.as_mut(), java, &returned).await;
            tokio::time::timeout_at(deadline + Duration::from_secs(1), transport)
                .await
                .unwrap();
            assert!(Instant::now() >= deadline);
            assert!(java.is_closed());
            release.send(true).unwrap();
            kick.await.unwrap();
            assert_eq!(java.disconnect_intent.lock().unwrap().preparing, 0);
            assert_eq!(
                java.disconnect_intent.lock().unwrap().deadline,
                Some(deadline)
            );
        })
        .await;
        bounded(close_fixture(&fixture)).await;
    }

    #[tokio::test]
    async fn actual_tcp_canceled_explicit_kick_resolves_intent_and_keeps_queued_reason() {
        let fixture = bounded(player_fixture()).await;
        for already_eof in [false, true] {
            Box::pin(bounded(async {
                let (socket, mut peer) = tcp_pair(false).await;
                let player = connected_player(&fixture, socket);
                let java = player.client.java().unwrap();
                if already_eof {
                    peer.shutdown().await.unwrap();
                }
                queue_prefixes(java);
                let reason = spam_reason();
                let mut kick = Box::pin(java.kick_explicit(&reason, true));
                // Current-thread runtime: the real writer cannot consume this FIFO
                // between enqueue and the first poll of the flush receipt.
                assert!(futures::poll!(&mut kick).is_pending());
                assert_eq!(java.disconnect_intent.lock().unwrap().preparing, 1);
                assert!(java.disconnect_intent.lock().unwrap().queued);
                drop(kick);
                assert!(java.is_closed());
                assert_eq!(java.disconnect_intent.lock().unwrap().preparing, 0);
                let returned = AtomicBool::new(false);
                let ((), observation) = tokio::time::timeout(Duration::from_secs(1), async {
                    tokio::join!(
                        lifecycle(&player, &fixture.server, &returned),
                        observe_peer(peer),
                    )
                })
                .await
                .expect("healthy or already-EOF peer must not incur the full kick deadline");
                assert_clean(&observation);
            }))
            .await;
        }
        bounded(close_fixture(&fixture)).await;
    }

    #[tokio::test]
    async fn actual_tcp_failed_serialization_closed_queue_and_suppressed_packet_skip_wait() {
        let fixture = bounded(player_fixture()).await;
        for case in 0..3 {
            Box::pin(bounded(async {
                let (socket, peer) = tcp_pair(false).await;
                let player = connected_player(&fixture, socket);
                let java = player.client.java().unwrap();
                match case {
                    0 => {
                        // Real serialization failure: login JSON exceeds the
                        // packet codec's string bound, without a fake serializer.
                        java.connection_state.store(ConnectionState::Login);
                        let reason = TextComponent::text("x".repeat(i16::MAX as usize + 1));
                        assert!(java.serialize_disconnect(&reason).is_none());
                        java.try_kick(&reason);
                    }
                    1 => {
                        java.close();
                        java.await_tasks().await;
                        assert!(java.outgoing_packet_queue_send.is_closed());
                        java.try_kick(&spam_reason());
                    }
                    _ => java.kick_explicit(&spam_reason(), false).await,
                }
                let returned = AtomicBool::new(false);
                tokio::time::timeout(
                    Duration::from_secs(1),
                    lifecycle(&player, &fixture.server, &returned),
                )
                .await
                .expect("no-packet resolution must not wait for the still-open peer");
                let intent = java.disconnect_intent.lock().unwrap();
                assert_eq!(intent.preparing, 0);
                assert!(!intent.queued);
                assert_eq!(intent.deadline.is_none(), case == 2);
                assert_eq!(java.pending_bytes.load(Ordering::Acquire), 0);
                drop(peer);
            }))
            .await;
        }
        bounded(close_fixture(&fixture)).await;
    }
}

#[tokio::test]
async fn actual_tcp_stalled_kicked_peer_uses_one_shared_deadline() {
    let fixture = bounded(player_fixture()).await;
    bounded(async {
        let (socket, peer) = tcp_pair(false).await;
        let player = connected_player(&fixture, socket);
        let java = player.client.java().unwrap();
        java.try_kick(&spam_reason());
        let deadline = java.disconnect_intent.lock().unwrap().deadline.unwrap();
        let mut reader = Box::pin(java.progress_player_packets(&player, &fixture.server));
        assert!(futures::poll!(&mut reader).is_pending());
        java.writer_finished.cancelled().await;
        // Actual TCP writer completed, but this peer neither reads nor sends EOF.
        assert!(Instant::now() < deadline);
        assert!(futures::poll!(&mut reader).is_pending());
        tokio::time::timeout_at(deadline + Duration::from_secs(1), reader)
            .await
            .expect("reader must not start a second close budget after writer completion");
        assert!(Instant::now() >= deadline);
        java.await_tasks().await;
        drop(peer);
    })
    .await;
    bounded(close_fixture(&fixture)).await;
}

#[tokio::test]
async fn actual_tcp_eof_and_non_kick_close_skip_terminal_wait() {
    let fixture = bounded(player_fixture()).await;
    Box::pin(bounded(async {
        for peer_eof in [true, false] {
            let (socket, mut peer) = tcp_pair(false).await;
            let player = connected_player(&fixture, socket);
            let java = player.client.java().unwrap();
            if peer_eof {
                peer.shutdown().await.unwrap();
            } else {
                java.close();
            }
            tokio::time::timeout(Duration::from_secs(1), async {
                java.progress_player_packets(&player, &fixture.server).await;
                java.await_tasks().await;
            })
            .await
            .expect("ordinary EOF/close must not incur the five-second kick wait");
            assert!(java.disconnect_intent.lock().unwrap().deadline.is_none());
            assert!(java.writer_finished.is_cancelled());
            assert!(player.inbound_packets.is_empty());
        }
    }))
    .await;
    bounded(close_fixture(&fixture)).await;
}

#[tokio::test]
async fn actual_tcp_terminal_flood_stops_at_byte_budget_before_deadline() {
    let fixture = bounded(player_fixture()).await;
    Box::pin(bounded(async {
        let (socket, mut peer) = tcp_pair(false).await;
        let player = connected_player(&fixture, socket);
        let java = player.client.java().unwrap();
        java.try_kick(&spam_reason());
        let deadline = java.disconnect_intent.lock().unwrap().deadline.unwrap();
        let lifecycle = async {
            java.progress_player_packets(&player, &fixture.server).await;
            java.await_tasks().await;
            assert!(
                Instant::now() < deadline,
                "flood must hit byte cap, not timeout"
            );
            assert!(player.inbound_packets.is_empty());
        };
        let flooding = async {
            let mut sent = 0;
            // Valid tick frames prevent malformed input from being the reason
            // this test ends. They must never reach gameplay after the seal.
            let tick =
                JavaClient::serialize_packet_for_version(&SClientTickEnd, CURRENT_MC_VERSION)
                    .unwrap();
            let mut encoder = TCPNetworkEncoder::new(tokio::io::sink());
            let mut frame = Vec::new();
            encoder.frame_packet(&tick, &mut frame).unwrap();
            let data = frame.repeat(8192 / frame.len());
            while sent < DISCONNECT_DISCARD_BYTES * 2 {
                match peer.write(&data[sent % data.len()..]).await {
                    Ok(0) => break,
                    Ok(count) => sent += count,
                    Err(error) => {
                        assert!(matches!(
                            error.kind(),
                            io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                        ));
                        break;
                    }
                }
            }
            assert!(sent >= DISCONNECT_DISCARD_BYTES);
        };
        tokio::join!(lifecycle, flooding);
    }))
    .await;
    bounded(close_fixture(&fixture)).await;
}

#[tokio::test]
async fn actual_tcp_terminal_discard_bypasses_partial_encrypted_compressed_frame() {
    Box::pin(bounded(async {
        let (mut socket, mut peer) = tcp_pair(false).await;
        let (encoded, mut encoded_reader) = tokio::io::duplex(4096);
        let mut encoder = TCPNetworkEncoder::new(encoded);
        encoder.set_compression((0, 6));
        encoder.set_encryption(&[7; 16]).unwrap();
        encoder
            .write_packet(Bytes::from(vec![5; 512]))
            .await
            .unwrap();
        encoder.flush().await.unwrap();
        drop(encoder);
        let mut wire = Vec::new();
        encoded_reader.read_to_end(&mut wire).await.unwrap();
        assert!(wire.len() > 1);
        peer.write_all(&wire[..1]).await.unwrap();
        // Prove the encrypted fragment arrived before polling the real decoder.
        assert_eq!(socket.peek(&mut [0; 1]).await.unwrap(), 1);
        let mut decoder = TCPNetworkDecoder::new(BufReader::with_capacity(1, &mut socket));
        decoder.set_compression(0);
        decoder.set_encryption(&[7; 16]).unwrap();
        let mut partial = Box::pin(decoder.get_raw_packet());
        assert!(futures::poll!(&mut partial).is_pending());
        drop(partial);
        peer.write_all(&wire[1..]).await.unwrap();
        // Deliberately not a valid encrypted/compressed frame. Terminal cleanup
        // must accept it as opaque bytes, not resume the canceled parser.
        peer.write_all(&[0xff; 29]).await.unwrap();
        peer.shutdown().await.unwrap();
        let remainder = wire.len() - 1 + 29;
        assert_eq!(
            decoder.discard_after_close(remainder - 1).await.unwrap(),
            remainder - 1
        );
        assert_eq!(decoder.discard_after_close(2).await.unwrap(), 1);
        assert_eq!(decoder.discard_after_close(1).await.unwrap(), 0);
    }))
    .await;
}

/// Full campaign-sized command burst, independent of the window-four controls.
/// Local writes are receipts, not server acknowledgments or Node callback parity.
mod full_burst_tests {
    use super::*;

    #[derive(Debug, Default)]
    struct BurstProducer {
        attempted: usize,
        completed: usize,
        tick_ends: usize,
        write_failure: Option<String>,
        clean_shutdown: bool,
    }

    /// Send all 32 commands without waiting for feedback. Keep the real 50ms
    /// `TickEnd` sender active until the concurrent reader observes the typed
    /// reason or a transport end. No delays or server-side transport gates.
    async fn produce_burst(
        mut writer: OwnedWriteHalf,
        peer_done: &CancellationToken,
        typed_spam: &AtomicBool,
        trace: &StreamTrace,
    ) -> BurstProducer {
        let mut result = BurstProducer::default();
        let mut encoder = TCPNetworkEncoder::new(&mut writer);
        encoder.set_compression(STREAM_COMPRESSION);
        let command = StreamRoute::Command.action(0);
        // Exact payload captured in B113's closed buffered campaign, including
        // packet id 7 and the command string's VarInt length (24 bytes).
        assert_eq!(&command[..], b"\x07\x18trigger spamprobe set 13");
        assert_eq!(command.len(), 26);
        let tick =
            JavaClient::serialize_packet_for_version(&SClientTickEnd, CURRENT_MC_VERSION).unwrap();
        let tick_period = Duration::from_millis(50);
        let mut ticks = tokio::time::interval_at(Instant::now() + tick_period, tick_period);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let (data, action) = tokio::select! {
                biased;
                // An already queued burst is not truncated by feedback. After
                // all commands, only the reader owns the peer's terminal decision.
                () = peer_done.cancelled(), if result.attempted == STREAM_ACTIONS => break,
                _ = ticks.tick(), if result.tick_ends < STREAM_ACTIONS => {
                    result.tick_ends += 1;
                    (&tick, None)
                }
                () = std::future::ready(()), if result.attempted < STREAM_ACTIONS => {
                    let index = result.attempted;
                    result.attempted += 1;
                    (&command, Some(index))
                }
            };
            let packet_id = (&data[..]).get_var_int().unwrap().0;
            let mut frame = Vec::new();
            encoder.frame_packet(data, &mut frame).unwrap();
            if action.is_some() {
                assert_eq!(frame.len(), 28);
                assert_eq!(&frame[..2], &[27, 0]);
            }
            trace.record(format_args!(
                "burst write start action={action:?} packet={packet_id} wire_bytes={} typed_spam={}",
                frame.len(), typed_spam.load(Ordering::Acquire)
            ));
            let write = encoder.write_frame(&frame).await;
            trace.record(format_args!(
                "burst write end action={action:?} packet={packet_id} result={write:?} typed_spam={}",
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
                "burst flush action={action:?} packet={packet_id} result={flush:?}"
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
        // Preserve a failed write independently while the reader keeps trying
        // to decode the reason. Half-close only after an actual typed reason.
        peer_done.cancelled().await;
        if typed_spam.load(Ordering::Acquire) {
            match writer.shutdown().await {
                Ok(()) => result.clean_shutdown = true,
                Err(error) => {
                    result.write_failure = Some(format!(
                        "{:?}; peer shutdown: {error:?}",
                        result.write_failure
                    ));
                }
            }
        }
        trace.record(format_args!("burst producer end: {result:?}"));
        result
    }

    /// The existing observer sends feedback receipts, but this receiver is
    /// deliberately dropped: no decoded reply can grant an action credit.
    #[tokio::test]
    #[allow(clippy::print_stderr, clippy::too_many_lines)]
    async fn actual_tcp_full_32_command_burst_delivers_prefix_typed_spam_and_clean_end() {
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
            74,
            PacketRateLimiter::new(false, 0.0, 0.0),
            Arc::downgrade(&fixture.server),
        );
        pending.connection_state.store(ConnectionState::Play);
        pending.network_reader.set_compression(STREAM_COMPRESSION.0);
        pending.network_writer.set_compression(STREAM_COMPRESSION);
        let mut profile = fixture.player.gameprofile.clone();
        profile.id = uuid::Uuid::from_u128(104);
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
        // Same production handler/admission path as the window-four control,
        // with budget for the entire burst and at most 32 periodic TickEnds.
        player.packet_activity.begin_epoch();
        {
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
            let mut score =
                ScoreboardScore::new("StreamedPlayer", "spamprobe", VarInt(0), None, None);
            score.locked = false;
            scoreboard.update_score(&NoTarget, score)
        };
        let trace = StreamTrace {
            started: Instant::now(),
            events: std::sync::Mutex::new(Vec::new()),
        };
        let peer_done = CancellationToken::new();
        let typed_spam = AtomicBool::new(false);
        let (feedback, replies) = tokio::sync::mpsc::unbounded_channel();
        drop(replies);
        let (reader, writer) = peer.into_split();
        let (pending_bytes, producer, observation) =
            Box::pin(drive_packet_future(&fixture.server, async {
                tokio::join!(
                    async {
                        java.progress_player_packets(&player, &fixture.server).await;
                        trace.record("burst server reader returned");
                        java.await_tasks().await;
                        trace.record("burst server client tasks joined");
                        java.pending_bytes.load(Ordering::Acquire)
                    },
                    produce_burst(writer, &peer_done, &typed_spam, &trace),
                    observe_stream(
                        reader,
                        StreamRoute::Command,
                        feedback,
                        &peer_done,
                        &typed_spam,
                        &trace,
                    ),
                )
            }))
            .await;
        eprintln!(
            "full burst producer={producer:?}; peer={observation:?}; pending_bytes={pending_bytes}; events={:#?}",
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
        let writer_finished = java.writer_finished.is_cancelled();
        bounded(world.remove_player(&player, false)).await;
        java.player.store(Arc::new(None));
        bounded(close_fixture(&fixture)).await;

        assert_eq!(producer.attempted, STREAM_ACTIONS, "{producer:?}");
        assert_eq!(producer.completed, STREAM_ACTIONS, "{producer:?}");
        assert!(producer.write_failure.is_none(), "{producer:?}");
        assert!(producer.clean_shutdown, "{producer:?}");
        assert_eq!(observation.prefixes, PREFIX_COUNT, "{observation:?}");
        assert!(observation.typed_spam, "{observation:?}");
        assert!(observation.protocol_failure.is_none(), "{observation:?}");
        assert!(
            matches!(observation.transport_end, Some(Ok(()))),
            "{observation:?}"
        );
        assert_eq!(pending_bytes, 0);
        assert!(queue_empty && reader_released && writer_finished);
        let score = score.unwrap();
        assert_eq!(score.value, VarInt(13));
        assert!(score.locked);
    }
}
