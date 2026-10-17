use super::*;
use crate::protocol::{put_string, put_var_int, read_packet};

fn handshake(intent: i32, version: i32) -> Vec<u8> {
    let mut payload = Vec::new();
    put_var_int(&mut payload, version);
    put_string(&mut payload, "localhost").unwrap();
    payload.extend_from_slice(&25565u16.to_be_bytes());
    put_var_int(&mut payload, intent);
    payload
}

async fn packet(stream: &mut (impl AsyncRead + Unpin)) -> Packet {
    timeout(Duration::from_secs(2), read_packet(stream))
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

#[test]
fn offline_identity_matches_java_name_uuid() {
    assert_eq!(
        offline_uuid("Notch"),
        [
            0xb5, 0x0a, 0xd3, 0x85, 0x82, 0x9d, 0x31, 0x41, 0xa2, 0x16, 0x7e, 0x7d, 0x75, 0x39,
            0xba, 0x7f
        ]
    );
    assert_ne!(offline_uuid("Notch"), offline_uuid("notch"));
}

#[tokio::test]
async fn actual_tcp_status_ping_and_shutdown_with_stalled_peer() {
    let server = Server::bind("127.0.0.1:0".parse().unwrap(), ServerConfig::default())
        .await
        .unwrap();
    let address = server.local_addr().unwrap();
    let (stop, stopped) = watch::channel(false);
    let running = tokio::spawn(server.run(stopped));
    let mut stalled = TcpStream::connect(address).await.unwrap();
    let mut client = TcpStream::connect(address).await.unwrap();
    write_packet(&mut client, 0, &handshake(1, 0))
        .await
        .unwrap();
    write_packet(&mut client, 0, &[]).await.unwrap();
    let status = packet(&mut client).await;
    assert_eq!(status.id, 0);
    let mut reader = Reader::new(&status.payload);
    let value: serde_json::Value = serde_json::from_str(reader.string(32767).unwrap()).unwrap();
    reader.finish().unwrap();
    assert_eq!(value["version"]["protocol"], PROTOCOL_VERSION);
    assert_eq!(value["players"]["online"], 0);
    let challenge = i64::MIN.to_be_bytes();
    write_packet(&mut client, 1, &challenge).await.unwrap();
    let pong = packet(&mut client).await;
    assert_eq!(
        (pong.id, pong.payload.as_slice()),
        (1, challenge.as_slice())
    );
    assert!(read_packet(&mut client).await.unwrap().is_none());
    stop.send(true).unwrap();
    timeout(Duration::from_secs(2), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(read_packet(&mut stalled).await.unwrap().is_none());
}

#[tokio::test]
async fn offline_login_has_distinct_profile_and_session_and_requires_ack() {
    let config = ServerConfig {
        offline: true,
        ..ServerConfig::default()
    };
    let (mut client, server) = tokio::io::duplex(1024);
    let running = tokio::spawn(async move { serve_connection(server, &config).await });
    write_packet(&mut client, 0, &handshake(2, PROTOCOL_VERSION))
        .await
        .unwrap();
    let mut hello = Vec::new();
    put_string(&mut hello, "Notch").unwrap();
    hello.extend_from_slice(&[0xff; 16]);
    write_packet(&mut client, 0, &hello).await.unwrap();
    let login = packet(&mut client).await;
    assert_eq!(login.id, 2);
    let mut reader = Reader::new(&login.payload);
    assert_eq!(reader.take(16).unwrap(), offline_uuid("Notch"));
    assert_eq!(reader.string(16).unwrap(), "Notch");
    assert_eq!(reader.var_int().unwrap(), 0);
    let session = reader.take(16).unwrap();
    assert_eq!(session[6] >> 4, 4);
    assert_eq!(session[8] >> 6, 2);
    reader.finish().unwrap();
    write_packet(&mut client, 3, &[]).await.unwrap();
    let refusal = packet(&mut client).await;
    assert_eq!(refusal.id, 2); // Configuration disconnect, no pretend play entry.
    assert_eq!(refusal.payload[0], 8); // NBT string component.
    running.await.unwrap().unwrap();
}

#[tokio::test]
async fn online_mode_and_wrong_version_cannot_silently_admit_offline_players() {
    for (offline, version) in [(false, PROTOCOL_VERSION), (true, PROTOCOL_VERSION - 1)] {
        let config = ServerConfig {
            offline,
            ..ServerConfig::default()
        };
        let (mut client, server) = tokio::io::duplex(1024);
        let running = tokio::spawn(async move { serve_connection(server, &config).await });
        write_packet(&mut client, 0, &handshake(2, version))
            .await
            .unwrap();
        let refusal = packet(&mut client).await;
        assert_eq!(refusal.id, 0);
        let mut reader = Reader::new(&refusal.payload);
        let component: serde_json::Value =
            serde_json::from_str(reader.string(32767).unwrap()).unwrap();
        assert!(component["text"].as_str().unwrap().contains(if offline {
            "26.3"
        } else {
            "authentication"
        }));
        reader.finish().unwrap();
        running.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn malformed_handshake_and_out_of_order_status_are_rejected() {
    for (first, following) in [
        (vec![0; 6], None),
        (handshake(1, PROTOCOL_VERSION), Some((1, vec![0; 8]))),
    ] {
        let (mut client, server) = tokio::io::duplex(1024);
        let running =
            tokio::spawn(async move { serve_connection(server, &ServerConfig::default()).await });
        write_packet(&mut client, 0, &first).await.unwrap();
        if let Some((id, payload)) = following {
            write_packet(&mut client, id, &payload).await.unwrap();
        }
        assert!(running.await.unwrap().is_err());
    }
}
