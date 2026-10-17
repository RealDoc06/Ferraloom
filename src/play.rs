//! Initial world entry and per-connection player state. Vanilla gameplay,
//! persistence, collision and interaction rules are not yet implemented.

use std::{
    collections::BTreeSet,
    io,
    sync::{Arc, OnceLock},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncWrite},
    time::{self, Instant, MissedTickBehavior},
};

use crate::{
    protocol::{self, Reader, put_string, put_var_int, write_packet},
    reference,
    server::{ServerConfig, ServerState},
    world::Chunk,
};

#[derive(Clone, Copy, Debug)]
struct Position {
    x: f64,
    y: f64,
    z: f64,
    yaw: f32,
    pitch: f32,
}

impl Position {
    fn spawn() -> Self {
        Self {
            x: 8.5,
            y: 64.0,
            z: 8.5,
            yaw: 0.0,
            pitch: 0.0,
        }
    }
    fn valid(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.z.is_finite()
            && self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.x.abs() <= 30_000_000.0
            && self.z.abs() <= 30_000_000.0
            && self.y.abs() <= 20_000_000.0
    }
    fn center(&self) -> (i32, i32) {
        (
            (self.x / 16.0).floor() as i32,
            (self.z / 16.0).floor() as i32,
        )
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub async fn enter(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    config: &ServerConfig,
    state: &ServerState,
) -> io::Result<()> {
    let Some(_session) = state.admit_player() else {
        return disconnect(stream, "The server is full").await;
    };
    let entity = state.allocate_entity()?;
    let registries = reference::registries()?;
    let mut login = Vec::new();
    login.extend_from_slice(&entity.to_be_bytes());
    login.push(0); // Hardcore.
    put_var_int(&mut login, 1);
    put_string(&mut login, "minecraft:overworld")?;
    put_var_int(&mut login, config.max_players as i32);
    put_var_int(&mut login, 2);
    put_var_int(&mut login, 2);
    login.extend_from_slice(&[0, 1, 0]); // Reduced debug, death screen, limited crafting.
    put_var_int(
        &mut login,
        registries.id("minecraft:dimension_type", "minecraft:overworld")?,
    );
    put_string(&mut login, "minecraft:overworld")?;
    login.extend_from_slice(&0i64.to_be_bytes());
    put_var_int(&mut login, 1); // Creative development world.
    put_var_int(&mut login, 0); // No previous game mode.
    login.extend_from_slice(&[0, 1, 0]); // Debug, flat, last death present.
    put_var_int(&mut login, 0);
    put_var_int(&mut login, 63);
    login.extend_from_slice(&[0, 0]); // Online mode, secure chat.
    write_packet(stream, 50, &login).await?;
    for (id, value) in [(97, 2), (113, 2)] {
        let mut payload = Vec::new();
        put_var_int(&mut payload, value);
        write_packet(stream, id, &payload).await?;
    }
    let mut spawn = Vec::new();
    put_string(&mut spawn, "minecraft:overworld")?;
    let packed_position = (8i64 << 38) | (8i64 << 12) | 64;
    spawn.extend_from_slice(&packed_position.to_be_bytes());
    spawn.extend_from_slice(&[0; 8]); // Yaw and pitch.
    write_packet(stream, 99, &spawn).await?;
    let mut loading = vec![13];
    loading.extend_from_slice(&0f32.to_be_bytes());
    write_packet(stream, 39, &loading).await?;
    let mut position = Position::spawn();
    let mut teleport = Vec::new();
    put_var_int(&mut teleport, 1);
    for value in [position.x, position.y, position.z, 0.0, 0.0, 0.0] {
        teleport.extend_from_slice(&value.to_be_bytes());
    }
    teleport.extend_from_slice(&[0; 12]); // Yaw,pitch,float32; relatives,int32.
    write_packet(stream, 73, &teleport).await?;
    let mut loaded = BTreeSet::new();
    let mut center = position.center();
    send_chunks(stream, center, &mut loaded).await?;

    let (mut input, mut output) = tokio::io::split(stream);
    let mut interval = time::interval(Duration::from_secs(10));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    interval.tick().await; // First challenge follows the first10second interval.
    let mut challenge = 0i64;
    let mut pending: Option<(i64, Instant)> = None;
    let mut teleport_pending = true;
    loop {
        // Retain the SAME read future across timer ticks. Dropping an in-flight
        // framing read at each keepalive would lose partial packet bytes.
        let incoming = protocol::read_packet(&mut input);
        tokio::pin!(incoming);
        let packet = loop {
            tokio::select! {
                result=&mut incoming => break match result? { Some(packet)=>packet,None=>return Ok(()) },
                _=interval.tick()=>{
                    if pending.is_some_and(|(_,sent)| sent.elapsed() >= Duration::from_secs(30)) {
                        disconnect(&mut output,"Timed out").await?; return Ok(());
                    }
                    if pending.is_none() {
                        challenge=challenge.wrapping_add(1);
                        write_packet(&mut output,45,&challenge.to_be_bytes()).await?;
                        pending=Some((challenge,Instant::now()));
                    }
                }
            }
        };
        let mut fields = Reader::new(&packet.payload);
        match packet.id {
            0 => {
                if fields.var_int()? != 1 || !teleport_pending {
                    return Err(invalid("unexpected teleport acknowledgement"));
                }
                let accepted = read_position(&mut fields, position, true, true)?;
                fields.finish()?;
                if !accepted.valid()
                    || (accepted.x - position.x).abs() > 0.001
                    || (accepted.y - position.y).abs() > 0.001
                    || (accepted.z - position.z).abs() > 0.001
                {
                    return Err(invalid("teleport position mismatch"));
                }
                teleport_pending = false;
            }
            11 => {
                let rate = fields.float()?;
                if !rate.is_finite() || rate <= 0.0 {
                    return Err(invalid("invalid chunk batch rate"));
                }
                fields.finish()?;
            }
            28 => {
                let value = i64::from_be_bytes(fields.take(8)?.try_into().expect("eight bytes"));
                fields.finish()?;
                if pending.map(|(value, _)| value) != Some(value) {
                    return Err(invalid("unexpected keepalive response"));
                }
                pending = None;
            }
            30..=33 => {
                let next = read_position(
                    &mut fields,
                    position,
                    packet.id == 30 || packet.id == 31,
                    packet.id == 31 || packet.id == 32,
                )?;
                let flags = fields.byte()?;
                if flags & !3 != 0 || !next.valid() {
                    return Err(invalid("invalid player movement"));
                }
                fields.finish()?;
                if teleport_pending {
                    continue;
                }
                // Physical movement validation is unfinished. Bound jumps here
                // so one packet cannot request an arbitrarily distant region.
                if (next.x - position.x).abs() > 100.0 || (next.z - position.z).abs() > 100.0 {
                    return Err(invalid("development movement bound exceeded"));
                }
                position = next;
                if position.center() != center {
                    center = position.center();
                    send_chunks(&mut output, center, &mut loaded).await?;
                }
            }
            44 => fields.finish()?,
            _ => { /* Other gameplay packets remain unimplemented. */ }
        }
    }
}

fn read_position(
    fields: &mut Reader<'_>,
    mut position: Position,
    xyz: bool,
    rotation: bool,
) -> io::Result<Position> {
    if xyz {
        position.x = fields.double()?;
        position.y = fields.double()?;
        position.z = fields.double()?;
    }
    if rotation {
        position.yaw = fields.float()?;
        position.pitch = fields.float()?;
    }
    Ok(position)
}

async fn send_chunks(
    output: &mut (impl AsyncWrite + Unpin),
    center: (i32, i32),
    loaded: &mut BTreeSet<(i32, i32)>,
) -> io::Result<()> {
    let mut cache = Vec::new();
    put_var_int(&mut cache, center.0);
    put_var_int(&mut cache, center.1);
    write_packet(output, 96, &cache).await?;
    // Retire columns outside our advertised radius. The client cache handles
    // retirement on center changes; this set only bounds server bookkeeping.
    loaded.retain(|&(x, z)| (x - center.0).abs() <= 2 && (z - center.1).abs() <= 2);
    let needed: Vec<_> = (center.1 - 2..=center.1 + 2)
        .flat_map(|z| (center.0 - 2..=center.0 + 2).map(move |x| (x, z)))
        .filter(|key| !loaded.contains(key))
        .collect();
    if needed.is_empty() {
        return Ok(());
    }
    write_packet(output, 12, &[]).await?;
    let biome = reference::registries()?.id("minecraft:worldgen/biome", "minecraft:plains")?;
    static FLAT: OnceLock<Arc<Chunk>> = OnceLock::new();
    let terrain = FLAT.get_or_init(|| Arc::new(Chunk::flat()));
    for &(x, z) in &needed {
        let terrain = Arc::clone(terrain);
        let payload = tokio::task::spawn_blocking(move || terrain.packet(x, z, biome))
            .await
            .map_err(io::Error::other)?;
        write_packet(output, 46, &payload).await?;
        loaded.insert((x, z));
    }
    let mut count = Vec::new();
    put_var_int(&mut count, needed.len() as i32);
    write_packet(output, 11, &count).await
}

async fn disconnect(output: &mut (impl AsyncWrite + Unpin), reason: &str) -> io::Result<()> {
    let mut message = vec![8];
    message.extend_from_slice(&(reason.len() as u16).to_be_bytes());
    message.extend_from_slice(reason.as_bytes());
    write_packet(output, 32, &message).await
}
