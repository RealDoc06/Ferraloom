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
