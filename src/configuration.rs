//! Configuration uses the client's explicitly acknowledged vanilla core pack.
//! Clients without that exact pack are refused until full NBT sync is supported.

use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    protocol::{Reader, put_string, put_var_int, write_packet},
    reference,
    server::{ServerConfig, next_packet},
};

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub async fn disconnect(output: &mut (impl AsyncWrite + Unpin), reason: &str) -> io::Result<()> {
    let length = u16::try_from(reason.len()).map_err(|_| invalid("disconnect reason too long"))?;
    let mut nbt = vec![8];
    nbt.extend_from_slice(&length.to_be_bytes());
    nbt.extend_from_slice(reason.as_bytes());
    write_packet(output, 2, &nbt).await
}

/// Returns false after a deliberate capability refusal, true after config ACK.
pub async fn configure(
    stream: &mut (impl AsyncRead + AsyncWrite + Unpin),
    config: &ServerConfig,
) -> io::Result<bool> {
    let mut features = Vec::new();
    put_var_int(&mut features, 1);
    put_string(&mut features, "minecraft:vanilla")?;
    write_packet(stream, 13, &features).await?;
    let mut packs = Vec::new();
    put_var_int(&mut packs, 1);
    for part in ["minecraft", "core", "26.3"] {
        put_string(&mut packs, part)?;
    }
    write_packet(stream, 15, &packs).await?;
    loop {
        let packet = next_packet(stream, config).await?;
        if packet.id == 7 {
            let mut fields = Reader::new(&packet.payload);
            let count = fields.var_int()?;
            if !(0..=64).contains(&count) {
                return Err(invalid("invalid known-pack count"));
            }
            let mut accepted = false;
            for _ in 0..count {
                let namespace = fields.string(32767)?;
                let id = fields.string(32767)?;
                let version = fields.string(32767)?;
                if namespace != "minecraft" || id != "core" || version != "26.3" || accepted {
                    return Err(invalid("client selected a pack not offered"));
                }
                accepted = true;
            }
            fields.finish()?;
            if !accepted {
                disconnect(stream, "This development server requires the Minecraft 26.3 core pack; full registry NBT sync is pending").await?;
                return Ok(false);
            }
            break;
        }
        informational(packet.id, &packet.payload)?;
    }
    let data = reference::registries()?;
    for registry in &data.packets {
        write_packet(stream, 7, registry).await?;
    }
    write_packet(stream, 14, &data.tags).await?;
    write_packet(stream, 3, &[]).await?;
    loop {
        let packet = next_packet(stream, config).await?;
        if packet.id == 3 {
            Reader::new(&packet.payload).finish()?;
            return Ok(true);
        }
        informational(packet.id, &packet.payload)?;
    }
}

fn informational(id: i32, payload: &[u8]) -> io::Result<()> {
    let mut fields = Reader::new(payload);
    match id {
        0 => {
            let _locale = fields.string(16)?;
            let _view_distance = fields.byte()?;
            if !(0..=2).contains(&fields.var_int()?) {
                return Err(invalid("invalid chat visibility"));
            }
            fields.boolean()?;
            fields.byte()?;
            if !(0..=1).contains(&fields.var_int()?) {
                return Err(invalid("invalid main hand"));
            }
            fields.boolean()?;
            fields.boolean()?;
            if !(0..=2).contains(&fields.var_int()?) {
                return Err(invalid("invalid particle status"));
            }
            fields.finish()
        }
        2 => {
            fields.string(32767)?; // Brand/other opaque custom payload.
            if fields.remaining().len() > 32767 {
                return Err(invalid("custom payload too large"));
            }
            Ok(())
        }
        _ => Err(invalid("unexpected configuration packet")),
    }
}
