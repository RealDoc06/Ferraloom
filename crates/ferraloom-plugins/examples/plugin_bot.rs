//! A scripted player for the plugin bridge's end-to-end test
//! (tools/plugins_e2e.py): it joins a 26.3 server offline as vanilla's
//! client does (as `horizon-bots` does), then does what its standard input
//! says and prints what it is told.
//!
//!   plugin_bot --server 127.0.0.1:25565 --name Tester
//!
//! Input lines: `command <line>` (no slash), `chat <text>`, `dig <x> <y> <z>`
//! (a creative player's START_DESTROY_BLOCK on the top face), `suggest <text>`
//! (the chat box asking for completions of `/<text>`), `quit`.
//! Output lines: `joined`, `pos <x> <y> <z>` for each teleport, `chat <text>`
//! for each system message (its plain text), `block <x> <y> <z> <state>`,
//! `commands ftest=<bool> fadmin=<bool>` for each command tree (whether it
//! names those), `boss add <title> <progress>` / `boss progress <progress>` /
//! `boss remove` for boss bars, `suggestions <start> <length> <a,b,...>`,
//! `disconnected <why>`.
use horizon_protocol::{
    PROTOCOL_VERSION,
    buf::{Reader, Writer},
    connection::{ConnWriter, Connection},
    crypto::offline_uuid,
    ids,
    packets::{
        Empty, configuration,
        handshake::{self, Intention},
        login::{self, ServerHello},
        play,
    },
};
use horizon_world::nbt::Tag;
use std::{
    io::{BufRead, Write},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn say(line: String) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Handshake, offline login and configuration, as `horizon-bots` joins.
fn join(host: &str, port: u16, name: &str) -> Result<Connection, String> {
    let mut c = Connection::connect(host, port, Duration::from_secs(10)).map_err(err)?;
    let intention = handshake::Serverbound::Intention(Intention {
        protocol: PROTOCOL_VERSION,
        host: host.into(),
        port,
        intent: handshake::LOGIN,
    });
    c.send(&intention.encode()).map_err(err)?;
    let hello = login::Serverbound::Hello(ServerHello {
        name: name.into(),
        uuid: offline_uuid(name),
    });
    c.send(&hello.encode()).map_err(err)?;
    loop {
        let (id, body) = c.recv().map_err(err)?;
        match login::Clientbound::decode(id, &body).map_err(err)? {
            login::Clientbound::Compression(p) => c.set_compression(p.threshold),
            login::Clientbound::LoginFinished(_) => {
                c.send(&login::Serverbound::LoginAcknowledged(Empty).encode())
                    .map_err(err)?;
                break;
            }
            login::Clientbound::Disconnect(p) => return Err(format!("refused: {}", p.reason_json)),
            login::Clientbound::Hello(_) => return Err("the server is in online mode".into()),
            login::Clientbound::CustomQuery(p) => {
                let answer = login::CustomQueryAnswer {
                    transaction: p.transaction,
                    payload: None,
                };
                c.send(&login::Serverbound::CustomQueryAnswer(answer).encode())
                    .map_err(err)?;
            }
            login::Clientbound::CookieRequest(p) => {
                let answer = login::CookieResponse {
                    key: p.key,
                    payload: None,
                };
                c.send(&login::Serverbound::CookieResponse(answer).encode())
                    .map_err(err)?;
            }
            login::Clientbound::Unknown(..) => {}
        }
    }
    use configuration::{Clientbound as Cb, Serverbound as Sb};
    use ids::configuration::clientbound as cb;
    let info = Sb::ClientInformation(configuration::ClientInformation::new(2));
    c.send(&info.encode()).map_err(err)?;
    loop {
        let (id, body) = c.recv().map_err(err)?;
        if !matches!(
            id,
            cb::SELECT_KNOWN_PACKS
                | cb::KEEP_ALIVE
                | cb::PING
                | cb::CODE_OF_CONDUCT
                | cb::COOKIE_REQUEST
                | cb::DISCONNECT
                | cb::FINISH_CONFIGURATION
        ) {
            continue;
        }
        let reply = match Cb::decode(id, &body).map_err(err)? {
            Cb::SelectKnownPacks(p) => Sb::SelectKnownPacks(p),
            Cb::KeepAlive(p) => Sb::KeepAlive(p),
            Cb::Ping(p) => Sb::Pong(p),
            Cb::CodeOfConduct(_) => Sb::AcceptCodeOfConduct(Empty),
            Cb::CookieRequest(p) => Sb::CookieResponse(configuration::CookieResponse {
                key: p.key,
                payload: None,
            }),
            Cb::Disconnect(_) => return Err("disconnected during configuration".into()),
            Cb::FinishConfiguration(_) => {
                c.send(&Sb::FinishConfiguration(Empty).encode())
                    .map_err(err)?;
                return Ok(c);
            }
            _ => continue,
        };
        c.send(&reply.encode()).map_err(err)?;
    }
}

/// A chat component's text: `text`, then `extra`; a translation as its key
/// and arguments (`multiplayer.player.joined[Tester]`).
fn plain(tag: &Tag, out: &mut String) {
    match tag {
        Tag::String(s) => out.push_str(&s.to_string_lossy()),
        Tag::List(parts) => parts.iter().for_each(|p| plain(p, out)),
        Tag::Compound(_) => {
            if let Some(text) = tag.get("text") {
                plain(text, out);
            }
            if let Some(key) = tag.get("translate") {
                plain(key, out);
                if let Some(with) = tag.get("with") {
                    out.push('[');
                    plain(with, out);
                    out.push(']');
                }
            }
            if let Some(extra) = tag.get("extra") {
                plain(extra, out);
            }
        }
        _ => {}
    }
}

fn raw(id: i32, write: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(id);
    write(&mut w);
    w.data
}

/// The play state: answers what a client must, prints what the test checks.
fn read(mut reader: horizon_protocol::connection::ConnReader, writer: Arc<Mutex<ConnWriter>>) {
    use ids::play::clientbound as cb;
    use ids::play::serverbound as sb;
    let send = |body: Vec<u8>| writer.lock().unwrap().send(&body);
    let mut loaded = false;
    loop {
        let Ok((id, body)) = reader.recv() else {
            say("disconnected connection closed".into());
            std::process::exit(0);
        };
        let mut r = Reader::new(&body);
        let sent = match id {
            cb::KEEP_ALIVE => r
                .i64()
                .map_or(Ok(()), |v| send(raw(sb::KEEP_ALIVE, |w| w.i64(v)))),
            cb::PING => r
                .i32()
                .map_or(Ok(()), |v| send(raw(sb::PONG, |w| w.i32(v)))),
            cb::CHUNK_BATCH_FINISHED => send(
                play::Serverbound::ChunkBatchReceived(play::ChunkBatchReceived {
                    chunks_per_tick: 25.0,
                })
                .encode(),
            ),
            cb::PLAYER_POSITION => match play::Clientbound::decode(id, &body) {
                Ok(play::Clientbound::PlayerPosition(p)) => {
                    let accept =
                        play::Serverbound::AcceptTeleportation(play::AcceptTeleportation {
                            id: p.teleport,
                            x: p.position[0],
                            y: p.position[1],
                            z: p.position[2],
                            yaw: p.yaw,
                            pitch: p.pitch,
                        });
                    let mut result = send(accept.encode());
                    say(format!(
                        "pos {} {} {}",
                        p.position[0], p.position[1], p.position[2]
                    ));
                    if !loaded {
                        loaded = true;
                        result = result
                            .and_then(|_| send(play::Serverbound::PlayerLoaded(Empty).encode()));
                        say("joined".into());
                    }
                    result
                }
                _ => Ok(()),
            },
            cb::SYSTEM_CHAT => {
                if let Ok(play::Clientbound::SystemChat(p)) = play::Clientbound::decode(id, &body) {
                    let mut text = String::new();
                    plain(&p.content, &mut text);
                    say(format!("chat {text}"));
                }
                Ok(())
            }
            cb::COMMANDS => {
                let has = |name: &[u8]| body.windows(name.len()).any(|w| w == name);
                say(format!(
                    "commands ftest={} fadmin={}",
                    has(b"ftest"),
                    has(b"fadmin")
                ));
                Ok(())
            }
            cb::BOSS_EVENT => {
                if let Ok(play::Clientbound::BossEvent(e)) = play::Clientbound::decode(id, &body) {
                    match e.action {
                        play::BossAction::Add { name, progress, .. } => {
                            let mut text = String::new();
                            plain(&name, &mut text);
                            say(format!("boss add {text} {progress}"));
                        }
                        play::BossAction::UpdateProgress { progress } => {
                            say(format!("boss progress {progress}"))
                        }
                        play::BossAction::Remove => say("boss remove".into()),
                        _ => {}
                    }
                }
                Ok(())
            }
            cb::COMMAND_SUGGESTIONS => {
                let mut words = Vec::new();
                let read = (|| {
                    let (_id, start, length) = (r.varint()?, r.varint()?, r.varint()?);
                    for _ in 0..r.varint()? {
                        words.push(r.string(32767)?);
                        if r.bool()? {
                            r.nbt()?;
                        }
                    }
                    Ok::<_, horizon_protocol::Error>((start, length))
                })();
                if let Ok((start, length)) = read {
                    say(format!("suggestions {start} {length} {}", words.join(",")));
                }
                Ok(())
            }
            cb::BLOCK_UPDATE => {
                if let Ok(play::Clientbound::BlockUpdate(p)) = play::Clientbound::decode(id, &body)
                {
                    say(format!(
                        "block {} {} {} {}",
                        p.pos[0], p.pos[1], p.pos[2], p.state
                    ));
                }
                Ok(())
            }
            cb::DISCONNECT | cb::START_CONFIGURATION => {
                say(format!("disconnected packet {id}"));
                std::process::exit(0);
            }
            _ => Ok(()),
        };
        if sent.is_err() {
            say("disconnected send failed".into());
            std::process::exit(0);
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut server, mut name) = ("127.0.0.1:25565".to_owned(), "Tester".to_owned());
    while let Some(arg) = args.next() {
        let value = args.next().unwrap_or_default();
        match arg.as_str() {
            "--server" => server = value,
            "--name" => name = value,
            _ => {
                eprintln!("plugin_bot: unknown argument {arg}");
                std::process::exit(2);
            }
        }
    }
    let (host, port) = server.rsplit_once(':').expect("--server HOST:PORT");
    let connection = match join(host, port.parse().expect("a port"), &name) {
        Ok(c) => c,
        Err(e) => {
            say(format!("disconnected {e}"));
            std::process::exit(1);
        }
    };
    let (reader, writer) = connection.split();
    let writer = Arc::new(Mutex::new(writer));
    std::thread::spawn({
        let writer = writer.clone();
        move || read(reader, writer)
    });
    let mut sequence = 0;
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let (verb, rest) = line.split_once(' ').unwrap_or((&line, ""));
        let packet = match verb {
            "command" => play::Serverbound::ChatCommand(play::ChatCommand {
                command: rest.into(),
            }),
            "chat" => play::Serverbound::Chat(play::Chat {
                message: rest.into(),
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis() as i64),
                salt: 0,
                signature: None,
                last_seen: play::LastSeenUpdate {
                    offset: 0,
                    acknowledged: [0; 3],
                    checksum: 0,
                },
            }),
            "dig" => {
                let pos: Vec<i32> = rest.split(' ').filter_map(|v| v.parse().ok()).collect();
                let [x, y, z] = pos[..] else {
                    eprintln!("plugin_bot: dig <x> <y> <z>");
                    continue;
                };
                sequence += 1;
                play::Serverbound::PlayerAction(play::PlayerAction {
                    action: 0,
                    pos: [x, y, z],
                    direction: 1,
                    sequence,
                })
            }
            "suggest" => {
                let body = raw(ids::play::serverbound::COMMAND_SUGGESTION, |w| {
                    w.varint(1);
                    w.string(&format!("/{rest}"));
                });
                if writer.lock().unwrap().send(&body).is_err() {
                    break;
                }
                continue;
            }
            "quit" => break,
            _ => {
                eprintln!("plugin_bot: unknown input {line:?}");
                continue;
            }
        };
        if writer.lock().unwrap().send(&packet.encode()).is_err() {
            break;
        }
    }
}
