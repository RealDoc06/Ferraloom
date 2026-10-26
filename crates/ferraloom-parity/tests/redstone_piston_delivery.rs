//! Wire discriminator for PistonPhaseVectors' ordinary/sticky, off=0/1/2/4 cases.
//! Receive-order observations never consult the asynchronously advanced BotState.
//! Atomic datapack phases contain edits, one step and a returned clock marker.
//! Two native JVMs must agree before Ferraloom is compared; no packet or event
//! order is normalized. Clock brackets retain offsets from the logged baseline.
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
use std::path::Path;

const POS: BlockPos = BlockPos::new(8, -59, 8);
const CLOCK_FUNCTION: &str = "ferraloom_delivery:clock";

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

    fn confirm_clock(&mut self, time: i64, label: &str) -> Result<()> {
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

    async fn function(&mut self, bot: &mut Bot, label: &str, id: &str) -> Result<i64> {
        let command = format!("function {id}");
        bot.command(&command).await?;
        let mut clock = None;
        self.until(bot, label, |packet| {
            if let Some(value) = message(packet)? {
                clock = function_clock(&value, id);
                ensure!(
                    ![
                        "commands.generic.failed",
                        "command.unknown.command",
                        "command.unknown.argument",
                        "commands.function.scheduled.no_functions",
                        "commands.function.instantiationFailure"
                    ]
                    .iter()
                    .any(|key| translated(&value, key)),
                    "{command}: {value}"
                );
            }
            Ok(clock.is_some())
        })
        .await?;
        self.entries.push(Delivery::Command {
            label: label.into(),
            command,
        });
        let clock = clock.expect("the function's result supplies its actual clock");
        self.confirm_clock(clock, label)?;
        Ok(clock)
    }

    async fn phase(&mut self, bot: &mut Bot, label: &str, id: &str) -> Result<()> {
        let initial = self.confirmed;
        let clock = self.function(bot, &format!("{label}.begin"), id).await?;
        ensure!(
            clock == initial,
            "phase must return its pre-step clock {initial}, got {clock}"
        );
        let target = initial + 1;
        self.until(bot, label, |packet| {
            if let cb::Packet::SetTime(time) = packet {
                ensure!(time.game_time <= target, "tick step overshot {target}");
                return Ok(time.game_time == target);
            }
            Ok(false)
        })
        .await?;
        // SetTime is sent before that frame's chunk broadcast. This read-only
        // function runs in the following task slot, fencing the complete flush,
        // including changes produced after the stepped frame's first broadcast.
        let clock = self
            .function(bot, &format!("{label}.end"), CLOCK_FUNCTION)
            .await?;
        ensure!(clock == target, "tick step did not stop at {target}");
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

fn function_clock(value: &Value, id: &str) -> Option<i64> {
    if value.get("translate")?.as_str()? != "commands.function.result" {
        return None;
    }
    let arguments = value.get("with")?.as_array()?;
    let name = arguments.first()?;
    let name = name.as_str().or_else(|| name.get("text")?.as_str())?;
    if name != id {
        return None;
    }
    let time = arguments.get(1)?;
    time.as_i64()
        .or_else(|| time.as_str()?.parse().ok())
        .or_else(|| time.get("text")?.as_str()?.parse().ok())
}

fn phase_function(sticky: bool, off: i32, tick: i32) -> String {
    format!(
        "ferraloom_delivery:{}_off_{off}/tick_{tick}",
        if sticky { "sticky" } else { "ordinary" }
    )
}

fn write_function(pack: &Path, id: &str, commands: &[String]) -> Result<()> {
    let path = pack
        .join("data/ferraloom_delivery/function")
        .join(format!("{id}.mcfunction"));
    std::fs::create_dir_all(path.parent().expect("function parent"))?;
    std::fs::write(path, format!("{}\n", commands.join("\n")))?;
    Ok(())
}

fn phase_fixture() -> Result<tempfile::TempDir> {
    let fixture = tempfile::tempdir()?;
    let pack = fixture.path().join("datapacks/ferraloom_delivery");
    std::fs::create_dir_all(&pack)?;
    std::fs::write(
        pack.join("pack.mcmeta"),
        serde_json::to_vec(&serde_json::json!({
            "pack": {"description": "Atomic piston delivery phases", "min_format": [121, 0], "max_format": [121, 0]}
        }))?,
    )?;
    write_function(&pack, "clock", &["return run time query gametime".into()])?;
    for sticky in [false, true] {
        for off in [0, 1, 2, 4] {
            for tick in 1..=8 {
                let mut commands = Vec::new();
                if tick == 1 {
                    commands.extend([
                        "fill 7 -59 8 10 -59 8 minecraft:air".into(),
                        format!(
                            "setblock 8 -59 8 minecraft:{}[facing=east]",
                            if sticky { "sticky_piston" } else { "piston" }
                        ),
                        "setblock 9 -59 8 minecraft:stone".into(),
                        "setblock 7 -59 8 minecraft:redstone_block".into(),
                    ]);
                }
                // Original off=N is after completed tick N, before tick N+1.
                if tick == off + 1 {
                    commands.push("setblock 7 -59 8 minecraft:air".into());
                }
                commands.extend([
                    "tick step 1".into(),
                    "return run time query gametime".into(),
                ]);
                let function = phase_function(sticky, off, tick);
                write_function(
                    &pack,
                    function.strip_prefix("ferraloom_delivery:").unwrap(),
                    &commands,
                )?;
            }
        }
    }
    Ok(fixture)
}

async fn record(session: &mut Session) -> Result<Vec<Delivery>> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    redstone_support::command(bot, "tick freeze", "commands.tick.status.frozen").await?;
    bot.teleport([4.5, -59.0, 4.5]).await?;
    redstone_support::loaded(bot, &[POS.west(1), POS.east(2)]).await?;
    let mut trace = Trace::new(redstone_support::game_time(bot).await?);
    eprintln!("PISTON_DELIVERY_CLOCK_ORIGIN {}", trace.baseline);
    let initial = trace.function(bot, "initial", CLOCK_FUNCTION).await?;
    ensure!(initial == trace.baseline, "frozen setup advanced time");
    for sticky in [false, true] {
        for off in [0, 1, 2, 4] {
            for tick in 1..=8 {
                trace
                    .phase(
                        bot,
                        &format!("sticky={sticky},off={off}.tick{tick}"),
                        &phase_function(sticky, off, tick),
                    )
                    .await?;
            }
        }
    }
    ensure!(
        trace.confirmed - trace.baseline == 64,
        "all eight native cases must complete eight steps"
    );
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
    let fixture = phase_fixture()?;
    let mut options = redstone_support::options();
    options.fixture = Some(fixture.path().to_path_buf());
    let mut expected: Option<Vec<Delivery>> = None;
    let mut run_index = 0;
    // The first session and the Vanilla session inside Both are separate JVMs.
    // Ferraloom is attempted only after their complete raw streams agree.
    for mode in [RunMode::Vanilla, RunMode::Both] {
        run(options.clone(), mode, async |session| {
            let entries = record(session).await?;
            run_index += 1;
            eprintln!("PISTON_DELIVERY_RUN {run_index}");
            eprintln!("PISTON_DELIVERY_TRACE {}", serde_json::to_string(&entries)?);
            let mut log = ObservationLog::default();
            log.record("wire", &entries)?;
            if let Some(expected) = &expected {
                compare(expected, &entries)?;
            } else {
                expected = Some(entries);
            }
            Ok(log)
        })
        .await?;
    }
    ensure!(
        run_index == 3,
        "two native JVMs and one Ferraloom server must run"
    );
    Ok(())
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
