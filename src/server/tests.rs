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
    let running = tokio::spawn(async move {
        serve_connection(server, &config, &ServerState::new(config.max_players)).await
    });
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
    assert_eq!(packet(&mut client).await.id, 13);
    assert_eq!(packet(&mut client).await.id, 15);
    write_packet(&mut client, 7, &[0]).await.unwrap();
    let refusal = packet(&mut client).await;
    assert_eq!(refusal.id, 2); // Missing core-pack capability is a typed refusal.
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
        let running = tokio::spawn(async move {
            serve_connection(server, &config, &ServerState::new(config.max_players)).await
        });
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
        let running = tokio::spawn(async move {
            serve_connection(server, &ServerConfig::default(), &ServerState::new(20)).await
        });
        write_packet(&mut client, 0, &first).await.unwrap();
        if let Some((id, payload)) = following {
            write_packet(&mut client, id, &payload).await.unwrap();
        }
        assert!(running.await.unwrap().is_err());
    }
}

#[tokio::test]
async fn actual_tcp_core_pack_configuration_flat_chunks_and_movement() {
    let config = ServerConfig {
        offline: true,
        ..ServerConfig::default()
    };
    let server = Server::bind("127.0.0.1:0".parse().unwrap(), config)
        .await
        .unwrap();
    let address = server.local_addr().unwrap();
    let (stop, stopped) = watch::channel(false);
    let running = tokio::spawn(server.run(stopped));
    let mut client = TcpStream::connect(address).await.unwrap();
    write_packet(&mut client, 0, &handshake(2, PROTOCOL_VERSION))
        .await
        .unwrap();
    let mut hello = Vec::new();
    put_string(&mut hello, "TestPlayer").unwrap();
    hello.extend_from_slice(&[0; 16]);
    write_packet(&mut client, 0, &hello).await.unwrap();
    assert_eq!(packet(&mut client).await.id, 2);
    write_packet(&mut client, 3, &[]).await.unwrap();
    assert_eq!(packet(&mut client).await.id, 13);
    let offered = packet(&mut client).await;
    assert_eq!(offered.id, 15);
    write_packet(&mut client, 7, &offered.payload)
        .await
        .unwrap();
    let mut registries = 0;
    let mut entries = 0;
    loop {
        let incoming = packet(&mut client).await;
        if incoming.id == 3 {
            assert!(incoming.payload.is_empty());
            break;
        }
        let mut fields = Reader::new(&incoming.payload);
        match incoming.id {
            7 => {
                assert!(fields.string(32767).unwrap().starts_with("minecraft:"));
                let count = fields.var_int().unwrap();
                for _ in 0..count {
                    fields.string(32767).unwrap();
                    assert!(!fields.boolean().unwrap());
                }
                fields.finish().unwrap();
                registries += 1;
                entries += count;
            }
            14 => {
                assert!(fields.var_int().unwrap() > 0);
            }
            id => panic!("unexpected configuration packet {id}"),
        }
    }
    assert_eq!((registries, entries), (32, 432));
    write_packet(&mut client, 3, &[]).await.unwrap();
    let mut position = None;
    let mut chunks = BTreeSet::new();
    let mut logged_in = false;
    loop {
        let incoming = packet(&mut client).await;
        match incoming.id {
            50 => {
                logged_in = true;
            }
            73 => {
                position = Some(incoming.payload);
            }
            46 => {
                let coordinates = check_flat_chunk(&incoming.payload);
                assert!(chunks.insert(coordinates));
            }
            11 => {
                assert_eq!(Reader::new(&incoming.payload).var_int().unwrap(), 25);
                break;
            }
            12 | 39 | 96 | 97 | 99 | 113 => {}
            id => panic!("unexpected initial play packet {id}"),
        }
    }
    assert!(logged_in);
    assert_eq!(chunks.len(), 25);
    let position = position.unwrap();
    let mut fields = Reader::new(&position);
    let teleport = fields.var_int().unwrap();
    let (x, y, z) = (
        fields.double().unwrap(),
        fields.double().unwrap(),
        fields.double().unwrap(),
    );
    assert_eq!((x, y, z), (8.5, 64.0, 8.5));
    fields.take(24).unwrap(); // Delta.
    let rotation = fields.take(8).unwrap();
    assert_eq!(fields.take(4).unwrap(), [0; 4]);
    fields.finish().unwrap();
    let mut accepted = Vec::new();
    put_var_int(&mut accepted, teleport);
    for value in [x, y, z] {
        accepted.extend_from_slice(&value.to_be_bytes());
    }
    accepted.extend_from_slice(rotation);
    write_packet(&mut client, 0, &accepted).await.unwrap();
    write_packet(&mut client, 44, &[]).await.unwrap();
    let mut movement = Vec::new();
    for value in [17.5f64, y, z] {
        movement.extend_from_slice(&value.to_be_bytes());
    }
    movement.push(1);
    write_packet(&mut client, 30, &movement).await.unwrap();
    assert_eq!(packet(&mut client).await.id, 96);
    assert_eq!(packet(&mut client).await.id, 12);
    let mut added = 0;
    loop {
        let incoming = packet(&mut client).await;
        if incoming.id == 11 {
            assert_eq!(Reader::new(&incoming.payload).var_int().unwrap(), 5);
            break;
        }
        assert_eq!(incoming.id, 46);
        assert_eq!(check_flat_chunk(&incoming.payload).0, 3);
        added += 1;
    }
    assert_eq!(added, 5);
    drop(client);
    stop.send(true).unwrap();
    timeout(Duration::from_secs(2), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

use std::collections::BTreeSet;

fn check_flat_chunk(payload: &[u8]) -> (i32, i32) {
    let mut fields = Reader::new(payload);
    let x = i32::from_be_bytes(fields.take(4).unwrap().try_into().unwrap());
    let z = i32::from_be_bytes(fields.take(4).unwrap().try_into().unwrap());
    assert_eq!(fields.var_int().unwrap(), 2);
    for kind in [1, 4] {
        assert_eq!(fields.var_int().unwrap(), kind);
        assert_eq!(fields.var_int().unwrap(), 37);
        fields.take(37 * 8).unwrap();
    }
    let length = fields.var_int().unwrap() as usize;
    let mut section = Reader::new(fields.take(length).unwrap());
    for index in 0..24 {
        assert_eq!(
            section.unsigned_short().unwrap(),
            if index < 8 { 4096 } else { 0 }
        );
        assert_eq!(section.unsigned_short().unwrap(), 0);
        let bits = section.byte().unwrap();
        if bits == 0 {
            section.var_int().unwrap();
        } else {
            assert_eq!(bits, 4);
            let count = section.var_int().unwrap();
            assert!((2..=3).contains(&count));
            for _ in 0..count {
                section.var_int().unwrap();
            }
            section.take(256 * 8).unwrap();
        }
        assert_eq!(section.byte().unwrap(), 0);
        section.var_int().unwrap();
    }
    section.finish().unwrap();
    assert_eq!(fields.var_int().unwrap(), 0); // No BEs.
    for expected in [
        &[0, 0xfe, 0xff, 3][..],
        &[][..],
        &[0xff, 1][..],
        &[0xff, 0xff, 0xff, 3][..],
    ] {
        let length = fields.var_int().unwrap() as usize;
        assert_eq!(fields.take(length).unwrap(), expected);
    }
    assert_eq!(fields.var_int().unwrap(), 17);
    for _ in 0..17 {
        assert_eq!(fields.var_int().unwrap(), 2048);
        assert!(fields.take(2048).unwrap().iter().all(|&byte| byte == 0xff));
    }
    assert_eq!(fields.var_int().unwrap(), 0);
    fields.finish().unwrap();
    (x, z)
}
