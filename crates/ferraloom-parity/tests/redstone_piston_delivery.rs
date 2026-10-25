//! Wire discriminator for PistonPhaseVectors' ordinary/sticky, off=0/1/2/4 cases.
//! Receive-order observations never consult the asynchronously advanced BotState.
//! Clock brackets are offsets from the logged initial server query; adding that
//! origin recovers each confirmed absolute endpoint without inventing send times.
#[allow(dead_code)]
mod level_support;
#[allow(dead_code)]
mod redstone_support;

use ferraloom_parity::{
    Bot, ObservationLog, Result, RunMode, Session, Ticks, ensure,
    protocol::{Wire, play::clientbound as cb},
    run,
};
use ferraloom_util::BlockPos;
use serde::Serialize;
use serde_json::Value;

const POS: BlockPos = BlockPos::new(8, -59, 8);

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "event")]
enum Delivery {
    Packet {
        fence: usize,
        game_time_before: i64,
        game_time_after: Option<i64>,
        kind: &'static str,
        payload: Vec<u8>,
        decoded: String,
    },
    Command {
        label: String,
        command: String,
    },
    Fence {
        ordinal: usize,
        label: String,
        game_time: i64,
    },
}

struct Trace {
    baseline: i64,
    confirmed: i64,
    fence: usize,
    entries: Vec<Delivery>,
}

impl Trace {
    fn new(baseline: i64) -> Self {
        Self {
            baseline,
            confirmed: baseline,
            fence: 0,
            entries: vec![Delivery::Fence {
                ordinal: 0,
                label: "begin".into(),
                game_time: 0,
            }],
        }
    }

    fn packet(&mut self, packet: &cb::Packet) -> Result<()> {
        let kind = match packet {
            cb::Packet::BlockUpdate(_) => "BlockUpdate",
            cb::Packet::SectionBlocksUpdate(_) => "SectionBlocksUpdate",
            cb::Packet::BlockEvent(_) => "BlockEvent",
            cb::Packet::BlockEntityData(_) => "BlockEntityData",
            _ => return Ok(()),
        };
        // The decoded codecs preserve section-entry order. Keep the complete
        // packet encoding and fields; never sort or coalesce changed positions.
        self.entries.push(Delivery::Packet {
            fence: self.fence,
            game_time_before: self.confirmed - self.baseline,
            game_time_after: None,
            kind,
            payload: packet.to_bytes()?,
            decoded: format!("{packet:?}"),
        });
        Ok(())
    }

    async fn until(
        &mut self,
        bot: &mut Bot,
        label: &str,
        mut predicate: impl FnMut(&cb::Packet) -> Result<bool>,
    ) -> Result<()> {
        let mut failure = None;
        bot.wait_for(label, Ticks::DEFAULT, |_, packet| {
            match self.packet(packet).and_then(|()| predicate(packet)) {
                Ok(done) => done,
                Err(error) => {
                    failure = Some(error);
                    true
                }
            }
        })
        .await?;
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(())
    }

    async fn acknowledge(
        &mut self,
        bot: &mut Bot,
        label: &str,
        command: &str,
        expected: &[&str],
    ) -> Result<()> {
        bot.command(command).await?;
        self.until(bot, label, |packet| {
            let Some(message) = message(packet)? else {
                return Ok(false);
            };
            if expected.iter().any(|key| translated(&message, key)) {
                return Ok(true);
            }
            ensure!(
                ![
                    "commands.generic.failed",
                    "command.unknown.command",
                    "command.unknown.argument"
                ]
                .iter()
                .any(|key| translated(&message, key)),
                "{command}: {message}"
            );
            Ok(false)
        })
        .await?;
        self.entries.push(Delivery::Command {
            label: label.into(),
            command: command.into(),
        });
        Ok(())
    }

    async fn query(&mut self, bot: &mut Bot, label: &str) -> Result<()> {
        bot.command("time query gametime").await?;
        let mut clock = None;
        self.until(bot, label, |packet| {
            clock = message(packet)?.as_ref().and_then(game_time);
            Ok(clock.is_some())
        })
        .await?;
        let time = clock.expect("the matching wire response supplies the time");
        ensure!(time >= self.confirmed, "server gameTime moved backwards");
        for entry in &mut self.entries {
            if let Delivery::Packet {
                game_time_after, ..
            } = entry
                && game_time_after.is_none()
            {
                *game_time_after = Some(time - self.baseline);
            }
        }
        self.confirmed = time;
        self.fence += 1;
        self.entries.push(Delivery::Fence {
            ordinal: self.fence,
            label: label.into(),
            game_time: time - self.baseline,
        });
        Ok(())
    }

    async fn command(
        &mut self,
        bot: &mut Bot,
        label: &str,
        command: &str,
        expected: &[&str],
    ) -> Result<()> {
        self.acknowledge(bot, label, command, expected).await?;
        self.query(bot, label).await
    }

    async fn step(&mut self, bot: &mut Bot, label: &str) -> Result<()> {
        let target = self.confirmed + 1;
        self.acknowledge(bot, label, "tick step 1", &["commands.tick.step.success"])
            .await?;
        // A clock packet is a wire progress marker, not a client snapshot or a
        // guess based on elapsed wall time. Every update before it is retained.
        self.until(bot, label, |packet| {
            if let cb::Packet::SetTime(time) = packet {
                ensure!(time.game_time <= target, "tick step overshot {target}");
                return Ok(time.game_time == target);
            }
            Ok(false)
        })
        .await?;
        self.query(bot, label).await?;
        ensure!(
            self.confirmed == target,
            "tick step did not stop at {target}"
        );
        Ok(())
    }
}

fn message(packet: &cb::Packet) -> Result<Option<Value>> {
    match packet {
        cb::Packet::SystemChat(message) => {
            Ok(Some(serde_json::from_str(&message.content.to_json()?)?))
        }
        _ => Ok(None),
    }
}

fn translated(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields.get("translate").and_then(Value::as_str) == Some(key)
                || fields.values().any(|value| translated(value, key))
        }
        Value::Array(values) => values.iter().any(|value| translated(value, key)),
        _ => false,
    }
}

fn game_time(value: &Value) -> Option<i64> {
    if value.get("translate")?.as_str()? != "commands.time.query.gametime" {
        return None;
    }
    let value = value.get("with")?.as_array()?.first()?;
    value
        .as_i64()
        .or_else(|| value.as_str()?.parse().ok())
        .or_else(|| value.get("text")?.as_str()?.parse().ok())
}

async fn record(session: &mut Session) -> Result<Vec<Delivery>> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    redstone_support::command(bot, "tick freeze", "commands.tick.status.frozen").await?;
    bot.teleport([4.5, -59.0, 4.5]).await?;
    redstone_support::loaded(bot, &[POS, POS.east(2)]).await?;
    let mut trace = Trace::new(redstone_support::game_time(bot).await?);
    eprintln!("PISTON_DELIVERY_CLOCK_ORIGIN {}", trace.baseline);
    for sticky in [false, true] {
        for off in [0, 1, 2, 4] {
            let case = format!("sticky={sticky},off={off}");
            trace
                .command(
                    bot,
                    &format!("{case}.clear"),
                    "fill 7 -59 8 10 -59 8 minecraft:air",
                    &["commands.fill.success", "commands.fill.failed"],
                )
                .await?;
            let block = if sticky { "sticky_piston" } else { "piston" };
            trace
                .command(
                    bot,
                    &format!("{case}.piston"),
                    &format!("setblock 8 -59 8 minecraft:{block}[facing=east]"),
                    &["commands.setblock.success"],
                )
                .await?;
            trace
                .command(
                    bot,
                    &format!("{case}.stone"),
                    "setblock 9 -59 8 minecraft:stone",
                    &["commands.setblock.success"],
                )
                .await?;
            trace
                .command(
                    bot,
                    &format!("{case}.on"),
                    "setblock 7 -59 8 minecraft:redstone_block",
                    &["commands.setblock.success"],
                )
                .await?;
            for tick in 0..=8 {
                if tick > 0 {
                    trace.step(bot, &format!("{case}.tick{tick}")).await?;
                }
                if tick == off {
                    trace
                        .command(
                            bot,
                            &format!("{case}.off"),
                            "setblock 7 -59 8 minecraft:air",
                            &["commands.setblock.success"],
                        )
                        .await?;
                }
            }
        }
    }
    trace.query(bot, "end").await?;
    Ok(trace.entries)
}

fn compare(expected: &[Delivery], actual: &[Delivery]) -> Result<()> {
    fn packets(entries: &[Delivery]) -> Vec<&Delivery> {
        entries
            .iter()
            .filter(|entry| matches!(entry, Delivery::Packet { .. }))
            .collect::<Vec<_>>()
    }
    let expected_packets = packets(expected);
    let actual_packets = packets(actual);
    for index in 0..expected_packets.len().max(actual_packets.len()) {
        let expected = expected_packets.get(index);
        let actual = actual_packets.get(index);
        ensure!(
            expected == actual,
            "first divergent outbound block packet #{index}:\nvanilla={expected:#?}\nFerraloom={actual:#?}"
        );
    }
    for index in 0..expected.len().max(actual.len()) {
        ensure!(
            expected.get(index) == actual.get(index),
            "packet payloads agree but first divergent command fence position #{index}:\nvanilla={:#?}\nFerraloom={:#?}",
            expected.get(index),
            actual.get(index),
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires dedicated builder vanilla/JDK; exact live piston packet comparison"]
async fn piston_packets_preserve_payload_order_game_time_brackets_and_command_fences() -> Result<()>
{
    let mut expected: Option<Vec<Delivery>> = None;
    run(
        redstone_support::options(),
        RunMode::Both,
        async |session| {
            let entries = record(session).await?;
            eprintln!("PISTON_DELIVERY_TRACE {}", serde_json::to_string(&entries)?);
            let mut log = ObservationLog::default();
            log.record("wire", &entries)?;
            if let Some(expected) = &expected {
                compare(expected, &entries)?;
            } else {
                expected = Some(entries);
            }
            Ok(log)
        },
    )
    .await
}

#[test]
fn command_fences_and_packet_payload_changes_are_both_comparison_failures() {
    let fence = |ordinal| Delivery::Fence {
        ordinal,
        label: "tick".into(),
        game_time: 1,
    };
    let packet = |value| Delivery::Packet {
        fence: 0,
        game_time_before: 0,
        game_time_after: Some(1),
        kind: "BlockEvent",
        payload: vec![value],
        decoded: value.to_string(),
    };
    let error = compare(&[packet(1)], &[packet(2)]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("first divergent outbound block packet #0")
    );
    let error = compare(&[packet(1), fence(1)], &[fence(1), packet(1)]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("first divergent command fence position #0")
    );
}
