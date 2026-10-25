use ferraloom_data::Block;
use ferraloom_parity::{
    BotId, LaunchOptions, ObservationLog, Result, RunMode, Session, Ticks, WorldPreset, ensure,
    protocol::{
        chat::Component,
        play::{self, clientbound as cb, serverbound as sb},
    },
};
use ferraloom_util::BlockPos;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

#[derive(Clone, Debug, Deserialize)]
pub struct Case {
    pub name: String,
    pub path: Option<String>,
    pub steps: Vec<Step>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Step {
    pub command: String,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub keys: Vec<String>,
    #[serde(default)]
    pub chat: bool,
    #[serde(default)]
    pub silent: bool,
    #[serde(default)]
    pub stop: bool,
    #[serde(default)]
    pub signed: bool,
    #[serde(default)]
    pub literal_prefix: Option<String>,
    #[serde(default)]
    pub checks: Vec<Check>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check {
    Argument {
        key: String,
        index: usize,
        value: Value,
    },
    Mode {
        value: String,
    },
    Position {
        value: [f64; 3],
    },
    Difficulty {
        value: String,
    },
    Blocks {
        min: [i32; 3],
        max: [i32; 3],
        state: String,
        #[serde(default)]
        interior: Option<String>,
    },
    TickState {
        rate: f32,
        frozen: bool,
    },
    TickSteps {
        value: i32,
    },
    Advance {
        ticks: i64,
    },
    Weather {
        ticks: i64,
        raining: bool,
        thunder: bool,
    },
}
impl Step {
    pub fn feedback(command: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            actor: None,
            keys: vec![key.into()],
            chat: false,
            silent: false,
            stop: false,
            signed: false,
            literal_prefix: None,
            checks: Vec::new(),
        }
    }
}
impl Case {
    /// A successful descendant exercises its non-executable prefixes too. The
    /// observed leaf is explicit; no syntax-only failure grants runtime coverage.
    pub fn paths(&self) -> Vec<String> {
        let Some(mut path) = self.path.clone() else {
            return Vec::new();
        };
        let mut result = Vec::new();
        while path.starts_with("commands.path.") {
            result.push(path.clone());
            path.truncate(path.rfind('.').unwrap());
        }
        result
    }
}
#[derive(Default)]
struct Weather {
    raining: bool,
    rain_level: f32,
    thunder_level: f32,
}
struct Driver<'a> {
    session: &'a mut Session,
    bots: BTreeMap<String, BotId>,
    barrier: i64,
    weather: Weather,
}
fn component(value: &Component) -> Result<Value> {
    Ok(serde_json::from_str(&value.to_json()?)?)
}
fn translation<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(fields) if fields.get("translate").and_then(Value::as_str) == Some(key) => {
            Some(value)
        }
        Value::Object(fields) => fields.values().find_map(|v| translation(v, key)),
        Value::Array(values) => values.iter().find_map(|v| translation(v, key)),
        _ => None,
    }
}
/// Host timings are not game state. Validate their native representations and
/// replace only those timing samples, preserving rates, budgets and structure.
fn timing(value: &mut Value) -> Result<()> {
    match value {
        Value::Object(fields) => {
            let key = fields
                .get("translate")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if let Some(args) = fields.get_mut("with").and_then(Value::as_array_mut) {
                let indexes: &[usize] = match key.as_str() {
                    "commands.tick.query.rate.running" | "commands.tick.query.rate.sprinting" => {
                        &[1]
                    }
                    "commands.tick.query.percentiles" => &[0, 1, 2],
                    "commands.tick.sprint.report" => &[1],
                    _ => &[],
                };
                for &index in indexes {
                    let text = args
                        .get(index)
                        .and_then(Value::as_str)
                        .ok_or_else(|| anyhow::anyhow!("missing timing string in {key}"))?;
                    ensure!(
                        text.parse::<f64>().is_ok_and(|n| n.is_finite() && n >= 0.),
                        "invalid host timing {text}"
                    );
                    ensure!(
                        text.split_once('.').is_some_and(|(_, part)| part.len()
                            == if key == "commands.tick.sprint.report" {
                                2
                            } else {
                                1
                            }),
                        "wrong native timing precision"
                    );
                    args[index] = json!(if key == "commands.tick.sprint.report" {
                        "0.00"
                    } else {
                        "0.0"
                    });
                }
                if key == "commands.tick.sprint.report" {
                    ensure!(
                        args[0].as_i64().is_some_and(|v| v >= 0),
                        "invalid sprint rate"
                    );
                    args[0] = json!(0);
                }
            }
            for child in fields.values_mut() {
                timing(child)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                timing(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn event(
    packet: &cb::Packet,
    own_id: Option<i32>,
    names: &BTreeMap<i32, String>,
) -> Result<Option<Value>> {
    let value = match packet {
        cb::Packet::SystemChat(p) => {
            let mut content = component(&p.content)?;
            timing(&mut content)?;
            json!({"kind":"feedback", "content":content, "overlay":p.overlay})
        }
        cb::Packet::DisguisedChat(p) => {
            json!({"kind":"disguised_chat", "content":component(&p.message)?, "type":format!("{:?}",p.chat_type.chat_type), "name":component(&p.chat_type.name)?, "target":p.chat_type.target_name.as_ref().map(component).transpose()?})
        }
        cb::Packet::PlayerChat(p) => {
            json!({"kind":"player_chat", "content":p.body.content.0, "unsigned":p.unsigned_content.as_ref().map(component).transpose()?, "type":format!("{:?}",p.chat_type.chat_type), "name":component(&p.chat_type.name)?, "target":p.chat_type.target_name.as_ref().map(component).transpose()?})
        }
        cb::Packet::TickingState(p) => {
            json!({"kind":"tick_state","rate":p.tick_rate,"frozen":p.is_frozen})
        }
        cb::Packet::TickingStep(p) => json!({"kind":"tick_steps","value":p.tick_steps.0}),
        cb::Packet::EntityEvent(p)
            if Some(p.entity_id) == own_id && (22..=28).contains(&p.event_id) =>
        {
            json!({"kind":"entity_event","value":p.event_id})
        }
        cb::Packet::GameEvent(p) if matches!(p.event, 3 | 11 | 12) => {
            json!({"kind":"game_event","event":p.event,"value":p.param})
        }
        cb::Packet::GameRuleValues(p) => {
            json!({"kind":"rules","values":p.values.iter().map(|v|json!([v.key.to_string(),v.value])).collect::<Vec<_>>()})
        }
        cb::Packet::SetTime(p) if !p.clock_updates.is_empty() => {
            json!({"kind":"clocks","values":p.clock_updates.iter().map(|v|json!([format!("{:?}",v.clock),v.state.total_ticks.0,v.state.partial_tick,v.state.rate])).collect::<Vec<_>>()})
        }
        cb::Packet::ChangeDifficulty(p) => {
            json!({"kind":"difficulty","value":format!("{:?}",p.difficulty),"locked":p.locked})
        }
        cb::Packet::PlayerAbilities(p) => {
            json!({"kind":"abilities","invulnerable":p.invulnerable,"flying":p.is_flying,"may_fly":p.can_fly,"instabuild":p.instabuild,"flying_speed":p.flying_speed,"walking_speed":p.walking_speed})
        }
        cb::Packet::PlayerPosition(p) => {
            json!({"kind":"position","position":[p.change.position.x,p.change.position.y,p.change.position.z],"rotation":[p.change.y_rot,p.change.x_rot],"relatives":p.relatives})
        }
        cb::Packet::PlayerLookAt(p) => {
            json!({"kind":"look_at","anchor":format!("{:?}",p.from_anchor),"position":[p.x,p.y,p.z],"entity":p.entity.as_ref().map(|v| json!([names.get(&v.id.0),format!("{:?}",v.anchor)]))})
        }
        cb::Packet::SetDefaultSpawnPosition(p) => json!({"kind":"spawn","value":format!("{p:?}")}),
        cb::Packet::Waypoint(p) => json!({"kind":"waypoint","value":format!("{p:?}")}),
        cb::Packet::Commands(p) => {
            let mut roots: Vec<_> = p.entries[p.root_index.0 as usize]
                .children
                .iter()
                .filter_map(|id| {
                    if let play::CommandNodeKind::Literal(name) = &p.entries[id.0 as usize].kind {
                        Some(name.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            roots.sort();
            json!({"kind":"command_roots", "values":roots})
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
impl Driver<'_> {
    async fn exchange(
        &mut self,
        actor: BotId,
        command: Option<&str>,
        signed: bool,
        stop: bool,
    ) -> Result<Vec<Value>> {
        let names = self
            .bots
            .iter()
            .filter_map(|(name, id)| {
                self.session
                    .bot(*id)
                    .ok()?
                    .snapshot()
                    .entity_id
                    .map(|id| (id, name.clone()))
            })
            .collect();
        let bot = self.session.bot(actor)?;
        let own_id = bot.snapshot().entity_id;
        if let Some(command) = command {
            if signed {
                bot.signed_command(command).await?;
            } else {
                bot.command(command).await?;
            }
        }
        self.barrier += 1;
        let time = self.barrier;
        if !stop {
            bot.send(sb::Packet::PingRequest(sb::PingRequest { time }))
                .await?;
        }
        let mut events = Vec::new();
        let mut failure = None;
        let weather = &mut self.weather;
        bot.wait_for(command.unwrap_or("command barrier"), Ticks::DEFAULT, |_, packet| {
            if let cb::Packet::GameEvent(p) = packet {
                match p.event {
                    1 => weather.raining = true,
                    2 => weather.raining = false,
                    7 => weather.rain_level = p.param,
                    8 => weather.thunder_level = p.param,
                    _ => {}
                }
            }
            match event(packet, own_id, &names) { Ok(Some(value)) => events.push(value), Ok(None) => {}, Err(error) => failure = Some(error) }
            if stop { matches!(packet,cb::Packet::SystemChat(p) if component(&p.content).is_ok_and(|v|translation(&v,"commands.stop.stopping").is_some())) }
            else { matches!(packet,cb::Packet::PongResponse(p) if p.time == time) }
        }).await?;
        if let Some(failure) = failure {
            return Err(failure);
        }
        Ok(events)
    }
    async fn capture(
        &mut self,
        actor: BotId,
        command: &str,
        signed: bool,
        stop: bool,
    ) -> Result<Vec<Value>> {
        self.exchange(actor, None, false, false).await?;
        self.exchange(actor, Some(command), signed, stop).await
    }
    async fn game_time(&mut self, actor: BotId) -> Result<i64> {
        let events = self
            .capture(actor, "time query gametime", false, false)
            .await?;
        events
            .iter()
            .find_map(|value| {
                translation(value, "commands.time.query.gametime")?
                    .get("with")?
                    .get(0)?
                    .as_i64()
            })
            .ok_or_else(|| anyhow::anyhow!("missing numeric game time"))
    }
    async fn advance(&mut self, actor: BotId, ticks: i64) -> Result<i64> {
        let start = self.game_time(actor).await?;
        self.capture(actor, &format!("tick step {ticks}t"), false, false)
            .await?;
        self.wait_advance(actor, start, ticks).await
    }
    async fn wait_advance(&mut self, actor: BotId, start: i64, ticks: i64) -> Result<i64> {
        for _ in 0..(ticks + 80) {
            let delta = self.game_time(actor).await? - start;
            ensure!(delta <= ticks, "stepping exceeded requested ticks");
            if delta == ticks {
                return Ok(delta);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        ferraloom_parity::bail!("step did not reach {ticks} game ticks")
    }
    async fn step(&mut self, step: &Step) -> Result<Value> {
        let actor = self.bots[step.actor.as_deref().unwrap_or("ParityBot")];
        let advance = step.checks.iter().find_map(|check| {
            if let Check::Advance { ticks } = check {
                Some(*ticks)
            } else {
                None
            }
        });
        let baseline = if advance.is_some() {
            Some(self.game_time(actor).await?)
        } else {
            None
        };
        let mut events = self
            .capture(actor, &step.command, step.signed, step.stop)
            .await?;
        let mut checks = Vec::new();
        for check in &step.checks {
            match check {
                Check::Advance { ticks } => checks.push(
                    json!({"advance":self.wait_advance(actor,baseline.unwrap(),*ticks).await?}),
                ),
                Check::Weather {
                    ticks,
                    raining,
                    thunder,
                } => {
                    self.advance(actor, *ticks).await?;
                    ensure!(
                        self.weather.raining == *raining,
                        "rain flag mismatch: expected {raining}, observed {}",
                        self.weather.raining
                    );
                    ensure!(
                        (self.weather.thunder_level > 0.) == *thunder,
                        "thunder level mismatch"
                    );
                    ensure!(
                        (self.weather.rain_level > 0.2) == *raining,
                        "rain level disagrees with the visible rain state"
                    );
                    checks.push(json!({"raining":self.weather.raining,"rain_visible":self.weather.rain_level>0.2,"thunder_visible":self.weather.thunder_level > 0.}));
                }
                Check::Blocks {
                    min,
                    max,
                    state,
                    interior,
                } => {
                    let expected = Block::from_name(state)
                        .ok_or_else(|| anyhow::anyhow!("unknown expected block {state}"))?
                        .default_state();
                    let interior = interior
                        .as_ref()
                        .map(|state| {
                            Block::from_name(state)
                                .map(|block| block.default_state())
                                .ok_or_else(|| anyhow::anyhow!("unknown interior block {state}"))
                        })
                        .transpose()?;
                    let matches = |snapshot: &ferraloom_parity::BotState| {
                        (min[0]..=max[0]).all(|x| {
                            (min[1]..=max[1]).all(|y| {
                                (min[2]..=max[2]).all(|z| {
                                    let inside = x > min[0]
                                        && x < max[0]
                                        && y > min[1]
                                        && y < max[1]
                                        && z > min[2]
                                        && z < max[2];
                                    snapshot.block(BlockPos::new(x, y, z))
                                        == Some(if inside {
                                            interior.unwrap_or(expected)
                                        } else {
                                            expected
                                        })
                                })
                            })
                        })
                    };
                    if !matches(&self.session.bot(actor)?.snapshot()) {
                        self.session
                            .bot(actor)?
                            .wait_for("command block state", Ticks::DEFAULT, |s, _| matches(s))
                            .await?;
                    }
                    let snapshot = self.session.bot(actor)?.snapshot();
                    checks.push(json!({"blocks":(min[0]..=max[0]).flat_map(|x|(min[1]..=max[1]).flat_map(move|y|(min[2]..=max[2]).map(move|z|[x,y,z]))).map(|[x,y,z]|json!([x,y,z,snapshot.block(BlockPos::new(x,y,z)).unwrap().id()])).collect::<Vec<_>>()}));
                }
                Check::Argument { key, index, value } => {
                    let actual = events
                        .iter()
                        .find_map(|v| translation(v, key)?.get("with")?.get(*index))
                        .ok_or_else(|| anyhow::anyhow!("missing {key} argument {index}"))?;
                    ensure!(
                        actual == value,
                        "wrong {key} argument {index}: {actual} != {value}"
                    );
                    checks.push(actual.clone());
                }
                Check::Mode { value } => {
                    let actual = format!(
                        "{:?}",
                        self.session
                            .bot(actor)?
                            .snapshot()
                            .game_mode
                            .ok_or_else(|| anyhow::anyhow!("no game mode"))?
                    );
                    ensure!(actual == *value, "wrong game mode {actual}");
                    checks.push(json!({"mode":actual}));
                }
                Check::Position { value } => {
                    let actual = self.session.bot(actor)?.snapshot().position;
                    ensure!(actual == *value, "wrong teleport position {actual:?}");
                    checks.push(json!({"position":actual}));
                }
                Check::Difficulty { value } => {
                    let actual = self
                        .session
                        .bot(actor)?
                        .snapshot()
                        .difficulty
                        .ok_or_else(|| anyhow::anyhow!("no difficulty"))?;
                    ensure!(
                        format!("{:?}", actual.difficulty) == *value,
                        "wrong difficulty"
                    );
                    checks.push(json!({"difficulty":format!("{:?}",actual.difficulty),"locked":actual.locked}));
                }
                Check::TickState { rate, frozen } => ensure!(
                    events
                        .iter()
                        .any(|v| v.get("kind") == Some(&json!("tick_state"))
                            && v.get("rate").and_then(Value::as_f64) == Some(f64::from(*rate))
                            && v.get("frozen").and_then(Value::as_bool) == Some(*frozen)),
                    "missing requested tick state"
                ),
                Check::TickSteps { value } => ensure!(
                    events
                        .iter()
                        .any(|v| v.get("kind") == Some(&json!("tick_steps"))
                            && v.get("value").and_then(Value::as_i64) == Some(i64::from(*value))),
                    "missing requested step count"
                ),
            }
        }
        for key in &step.keys {
            ensure!(
                events.iter().any(|v| translation(v, key).is_some()),
                "missing expected {key}; got {events:?}"
            );
        }
        if let Some(prefix) = &step.literal_prefix {
            ensure!(
                events.iter().any(
                    |v| v.get("kind").and_then(Value::as_str) == Some("feedback")
                        && v.get("content")
                            .and_then(|v| v.as_str().or_else(|| v.get("text")?.as_str()))
                            .is_some_and(|text| text.starts_with(prefix))
                ),
                "missing literal usage beginning {prefix}"
            );
        }
        if step.chat {
            ensure!(
                events.iter().any(|v| matches!(
                    v.get("kind").and_then(Value::as_str),
                    Some("disguised_chat" | "player_chat")
                )),
                "missing broadcast chat"
            );
        }
        if step.silent {
            ensure!(
                !events.iter().any(|v| matches!(
                    v.get("kind").and_then(Value::as_str),
                    Some("feedback" | "disguised_chat" | "player_chat")
                )),
                "silent command emitted feedback"
            );
        }
        // A query's absolute game time depends on join latency; preserve its
        // numeric type and validate it against this live phase, then compare zero offset.
        if step.command == "time query gametime" {
            for value in &mut events {
                if let Some(content) = value.get_mut("content") {
                    if content.get("translate").and_then(Value::as_str)
                        == Some("commands.time.query.gametime")
                    {
                        ensure!(
                            content["with"][0].as_i64().is_some(),
                            "game time must be a long"
                        );
                        ensure!(
                            content["with"][0].as_i64()
                                == self.session.bot(actor)?.snapshot().game_time,
                            "query game time disagrees with the authoritative frozen SetTime clock"
                        );
                        content["with"][0] = json!(0);
                    }
                }
            }
        }
        if step.stop {
            self.session.stop().await?;
        }
        Ok(json!({"events":events,"checks":checks}))
    }
}

pub async fn run_family(family: &'static str) -> Result<()> {
    let options = LaunchOptions {
        preset: WorldPreset::Flat {
            layers: vec![("minecraft:bedrock".into(), 1)],
            biome: "minecraft:plains".into(),
        },
        ..Default::default()
    };
    // run always compares actual vanilla and Strict Ferraloom; single-side runs
    // cannot promote an inventory path in this sweep.
    ferraloom_parity::run(options, RunMode::Both, async |session: &mut Session| {
        let primary = session.join("ParityBot").await?;
        let guest = session.join("PathGuest").await?;
        let mut bots = BTreeMap::from([("ParityBot".into(), primary), ("PathGuest".into(), guest)]);
        if family == "admin" {
            bots.insert("KickGuest".into(), session.join("KickGuest").await?);
        }
        let mut driver = Driver {
            session,
            bots,
            barrier: 100_000,
            weather: Weather::default(),
        };
        for step in [
            Step::feedback("tick freeze", "commands.tick.status.frozen"),
            Step::feedback("gamemode spectator", "commands.gamemode.success.self"),
            Step::feedback("time set 1234", "commands.time.set.absolute"),
        ] {
            driver.step(&step).await?;
        }
        let mut log = ObservationLog::default();
        let mut failures = Vec::new();
        for case in super::cases::family(family)? {
            let mut outputs = Vec::new();
            for step in &case.steps {
                match driver.step(step).await {
                    Ok(value) => outputs.push(value),
                    Err(error) => {
                        eprintln!("PATH ERROR {} [{}]: {error:?}", case.name, step.command);
                        failures.push(format!("{}: {error:?}", case.name));
                        outputs.push(json!({"error":format!("{error:?}")}));
                        break;
                    }
                }
            }
            eprintln!("PATH {} -> {}", case.name, serde_json::to_string(&outputs)?);
            log.record(&case.name, json!({"paths":case.paths(),"steps":outputs}))?;
        }
        ensure!(
            failures.is_empty(),
            "{} command path checks failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
        Ok(log)
    })
    .await
}
