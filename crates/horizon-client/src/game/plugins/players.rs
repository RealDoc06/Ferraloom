//! The players area: a player's state and what they are sent, logins,
//! joins, quits, chat, commands and the player events.
//!
//! A player's state lives in two places: the guest whose turn it is stands
//! in the game's own player (`Game::player`, `experience`, `mode`...), every
//! other guest in their hub record (`Guest`, its body in `Guest::motion`).
//! Reads take the record and lay the turn's state over it; writes go where
//! the state is, and the turn's end or the next turn's start carries them
//! over, as `enter_guest` and `leave_guest` always do.
use super::{DIMS, GameHost, Pending, is_plugin_command, pend, world_index};
use crate::game::keys::KeyCode;
use crate::game::{Dim, Game, commands::Target, lan::LanHost, lan_actions::GuestCommand};
use ferraloom_plugins::{
    Action, Cause, Client, Death, Effect, Event, Field, Interact, Interaction, Joined, Location,
    LoginResult, Moved, PlayerHost, PlayerId, PlayerState, Show, Toggle, listens,
};
use horizon_client::{
    command::args::GameMode,
    effects::EffectInstance,
    entities::Experience,
    lan::{data::component_nbt, guest::Guest},
    motion::PlayerMotion,
    text::{self, Text},
};
use horizon_core::geometry::Vec3;
use horizon_protocol::{
    entity_data::Particle,
    packets::{
        configuration::CustomPayload,
        play::{self, Clientbound as C},
    },
};
use horizon_world::{nbt::Tag, registry::BlockStateId};
use std::sync::atomic::Ordering;

fn system_chat(line: &Text) -> C {
    C::SystemChat(play::SystemChat {
        content: component_nbt(&line.component()),
        overlay: false,
    })
}

/// Legacy text (`§` codes) as a component.
fn legacy(text: &str) -> Tag {
    component_nbt(&Text::legacy(text).component())
}

/// A broadcast join or quit message (legacy text): vanilla's own translation
/// when the plugins left it as it was.
fn announcement(message: &str, default: &str, key: &str, name: &str) -> Text {
    if message == default {
        Text::translate(key, &[Text::literal(name.to_owned())]).with_color(text::YELLOW)
    } else {
        Text::legacy(message)
    }
}

/// `Player.giveExperienceLevels` (without its level-up sound).
fn give_levels(xp: &mut Experience, levels: i32) {
    xp.level = xp.level.saturating_add(levels);
    if xp.level < 0 {
        *xp = Experience::default();
    }
}

/// The state a guest's record and body hold.
fn record_state(g: &mut Guest) -> PlayerState {
    let d = g.dimensions();
    let respawn = match (&g.respawn_elsewhere, g.spawn) {
        (Some((dim, pos, _, _)), _) => Some(Location {
            world: world_index(Dim::from_id(dim)),
            x: f64::from(pos[0]),
            y: f64::from(pos[1]),
            z: f64::from(pos[2]),
            yaw: 0.0,
            pitch: 0.0,
        }),
        (None, Some([x, y, z])) => Some(Location {
            world: world_index(Dim::Overworld),
            x,
            y,
            z,
            yaw: 0.0,
            pitch: 0.0,
        }),
        (None, None) => None,
    };
    let mut s = PlayerState {
        health: f64::from(g.health),
        max_health: 20.0,
        absorption: f64::from(g.saved_absorption.unwrap_or(0.0)),
        food: g.food,
        saturation: g.saturation,
        exhaustion: 0.0,
        level: g.xp_level,
        exp: g.xp_progress,
        total_exp: g.xp_total,
        game_mode: g.game_mode,
        flying: g.flying,
        allow_flight: g.creative() || g.spectator() || g.allow_flight,
        // The abilities the hub sends (0.1 and 0.05), as Bukkit counts them.
        walk_speed: 0.2,
        fly_speed: 0.1,
        sneaking: g.sneaking,
        sprinting: g.sprinting,
        on_ground: g.on_ground,
        swimming: false,
        gliding: g.fall_flying,
        sleeping: g.sleeping,
        dead: g.dead,
        in_vehicle: g.vehicle.is_some(),
        velocity: [0.0; 3],
        fire_ticks: 0,
        air: 300,
        max_air: 300,
        fall_distance: 0.0,
        frozen_ticks: 0,
        ping: g.latency.load(Ordering::Relaxed),
        ticks_lived: i32::try_from(g.ticks).unwrap_or(i32::MAX),
        held_slot: g.selected as i32,
        op_level: i32::from(g.permission),
        eye_height: g.eye_height(),
        width: f64::from(d.width),
        height: f64::from(d.height),
        respawn,
    };
    if let Some(m) = g.motion.as_deref_mut() {
        body_state(m, &mut s);
    }
    s
}

/// What a body holds of the state.
fn body_state(m: &mut PlayerMotion, s: &mut PlayerState) {
    if let Ok(max) = m.attribute("minecraft:max_health") {
        s.max_health = max;
    }
    s.absorption = f64::from(m.health.absorption());
    s.exhaustion = m.health.food.exhaustion;
    s.swimming = m.swimming;
    s.velocity = [m.velocity.x, m.velocity.y, m.velocity.z];
    s.fire_ticks = m.fire_ticks;
    s.air = m.air;
    s.fall_distance = m.fall_distance as f32;
    s.frozen_ticks = m.ticks_frozen;
    s.fly_speed = m.flying_speed * 2.0;
}

/// Sets a field a body holds; false for the others.
fn set_body(m: &mut PlayerMotion, field: Field, v: f64) -> bool {
    match field {
        Field::Health => return m.set_health(v as f32, false).is_ok(),
        Field::Absorption => return m.set_health(v as f32, true).is_ok(),
        Field::Food => m.health.food.food = v as i32,
        Field::Saturation => m.health.food.saturation = v as f32,
        Field::Exhaustion => m.health.food.exhaustion = v as f32,
        Field::FireTicks => m.fire_ticks = v as i32,
        Field::Air => m.air = v as i32,
        Field::FallDistance => m.fall_distance = v,
        _ => return false,
    }
    true
}

/// `minecraft:oak_log[axis=x]` as a state of the registry.
fn parse_state(game: &Game, s: &str) -> Option<BlockStateId> {
    let (name, properties) = match s.split_once('[') {
        Some((name, rest)) => (name, rest.strip_suffix(']')?),
        None => (s, ""),
    };
    let mut state = horizon_world::chunk::BlockState {
        name: name.to_owned(),
        properties: Default::default(),
    };
    for pair in properties.split(',').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=')?;
        state.properties.insert(k.to_owned(), v.to_owned());
    }
    game.pack.assets.registry.resolve(&state).ok()
}

impl GameHost<'_, '_> {
    fn guest(&mut self, id: PlayerId) -> Option<&Guest> {
        let hub = self.hub()?;
        hub.hub.guests.iter().find(|g| g.uuid == id && g.placed)
    }

    fn guest_mut(&mut self, id: PlayerId) -> Option<&mut Guest> {
        let hub = self.hub()?;
        hub.hub.guests.iter_mut().find(|g| g.uuid == id && g.placed)
    }

    /// The guest whose turn it is: their state is the game's own.
    fn acting(&self, id: PlayerId) -> bool {
        self.game.lan_acting_uuid == Some(id)
    }

    /// A packet to one player; false when they are not on.
    fn send(&mut self, id: PlayerId, packet: &C) -> bool {
        self.guest(id).map(|g| g.send(packet)).is_some()
    }

    /// Sneaking, sprinting, flying and allow-flight, which the hub's record
    /// keeps (and the body of a turn in progress too).
    fn set_flag(&mut self, id: PlayerId, field: Field, on: bool) -> bool {
        if self.acting(id) {
            if field == Field::Sneaking {
                if on {
                    self.game.keys.insert(KeyCode::ShiftLeft);
                } else {
                    self.game.keys.remove(&KeyCode::ShiftLeft);
                }
            }
            let mode_flies = self.game.creative || self.game.spectator();
            if let Some(p) = self.game.player.as_deref_mut() {
                match field {
                    Field::Flying if on && !p.may_fly => return false,
                    Field::Flying => p.flying = on,
                    Field::AllowFlight => {
                        p.may_fly = on || mode_flies;
                        p.flying &= p.may_fly;
                    }
                    Field::Sprinting => {
                        let _ = p.set_sprinting(on);
                    }
                    _ => {}
                }
            }
        }
        let Some(g) = self.guest_mut(id) else {
            return false;
        };
        match field {
            Field::Flying | Field::AllowFlight => {
                if field == Field::AllowFlight {
                    // CraftPlayer.setAllowFlight: no flight, no flying.
                    g.allow_flight = on;
                    g.flying &= on || g.creative() || g.spectator();
                } else if on && !(g.creative() || g.spectator() || g.allow_flight) {
                    return false;
                } else {
                    g.flying = on;
                }
                g.send(&C::PlayerAbilities(play::PlayerAbilities {
                    flags: g.ability_flags(),
                    flying_speed: 0.05,
                    walking_speed: 0.1,
                }));
            }
            Field::Sneaking => g.sneaking = on,
            Field::Sprinting => g.sprinting = on,
            _ => return false,
        }
        true
    }

    /// `ServerPlayer.setGameMode` for a guest out of their turn: the event,
    /// then the hub's change (the next turn takes it).
    fn set_game_mode_of(&mut self, id: PlayerId, mode: i32) -> bool {
        if !(0..4).contains(&mode) {
            return false;
        }
        let Some(conn) = self
            .guest(id)
            .filter(|g| g.game_mode != mode)
            .map(|g| g.conn)
        else {
            return false;
        };
        if listens(Event::PlayerGameModeChange)
            && ferraloom_plugins::player_game_mode_change(self, id, mode)
        {
            return false;
        }
        let Some(hub) = self.hub() else {
            return false;
        };
        hub.hub.set_game_mode(conn, mode);
        true
    }

    /// A field of the guest whose turn it is, in the game's own state.
    fn set_in_turn(&mut self, field: Field, v: f64) -> bool {
        let game = &mut *self.game;
        match field {
            Field::Level => game.experience.level = v as i32,
            Field::Exp => game.experience.progress = v as f32,
            Field::TotalExp => game.experience.total = v as i32,
            Field::GiveExp => {
                game.experience.give(v as i32);
            }
            Field::GiveLevels => give_levels(&mut game.experience, v as i32),
            Field::HeldSlot => match usize::try_from(v as i64) {
                Ok(slot) if slot < 9 => game.selected = slot,
                _ => return false,
            },
            Field::GameMode => {
                let Some(&mode) = usize::try_from(v as i64)
                    .ok()
                    .and_then(|i| GameMode::ALL.get(i))
                else {
                    return false;
                };
                return game.set_game_mode(mode);
            }
            _ => {
                let Some(p) = game.player.as_deref_mut() else {
                    return false;
                };
                return set_body(p, field, v);
            }
        }
        true
    }

    /// A field of a guest out of their turn, in their record (or body).
    fn set_in_record(&mut self, id: PlayerId, field: Field, v: f64) -> bool {
        let Some(g) = self.guest_mut(id) else {
            return false;
        };
        match field {
            // enter_guest gives the body the record's health and food.
            Field::Health => g.health = v as f32,
            Field::Food => g.food = v as i32,
            Field::Saturation => g.saturation = v as f32,
            Field::Level => g.xp_level = v as i32,
            Field::Exp => g.xp_progress = v as f32,
            Field::TotalExp => g.xp_total = v as i32,
            Field::GiveExp | Field::GiveLevels => {
                let mut xp = Experience {
                    level: g.xp_level,
                    progress: g.xp_progress,
                    total: g.xp_total,
                    score: g.meta.score,
                };
                if field == Field::GiveExp {
                    xp.give(v as i32);
                } else {
                    give_levels(&mut xp, v as i32);
                }
                (g.xp_level, g.xp_progress, g.xp_total) = (xp.level, xp.progress, xp.total);
                // Player.giveExperiencePoints also raises the death-screen score.
                g.meta.score = xp.score;
            }
            Field::HeldSlot => match usize::try_from(v as i64) {
                Ok(slot) if slot < 9 => {
                    g.selected = slot;
                    g.send(&C::SetHeldSlot(play::SetHeldSlot { slot: slot as i32 }));
                }
                _ => return false,
            },
            _ => {
                let Some(m) = g.motion.as_deref_mut() else {
                    return false;
                };
                return set_body(m, field, v);
            }
        }
        true
    }
}

impl PlayerHost for GameHost<'_, '_> {
    fn send_message(&mut self, player: PlayerId, message: &str) {
        let Some(hub) = self.hub() else {
            pend(Pending::Message(player, message.to_owned()));
            return;
        };
        let to = hub.hub.guests.iter().find(|g| g.uuid == player && g.placed);
        if let Some(conn) = to.map(|g| g.conn) {
            hub.hub.send_to(conn, &system_chat(&Text::legacy(message)));
        }
    }

    fn location(&mut self, player: PlayerId) -> Option<Location> {
        // The guest whose turn it is stands in the game's own player.
        if self.acting(player) {
            let p = self.game.player.as_ref()?;
            return Some(Location {
                world: world_index(self.game.dim),
                x: p.position.x,
                y: p.position.y,
                z: p.position.z,
                yaw: self.game.yaw,
                pitch: self.game.pitch,
            });
        }
        let g = self.guest(player)?;
        Some(Location {
            world: world_index(Dim::from_id(&g.dimension)),
            x: g.position[0],
            y: g.position[1],
            z: g.position[2],
            yaw: g.yaw,
            pitch: g.pitch,
        })
    }

    /// As `/tp` moves a player (across levels too). Another player than the
    /// one acting, while the hub is out of `Game::lan`, moves at the start of
    /// the next tick.
    fn teleport(&mut self, player: PlayerId, to: Location) -> bool {
        let Some(&level) = DIMS.get(to.world) else {
            return false;
        };
        let target = if self.acting(player) {
            Target::Player
        } else if !self.hub_in_game() {
            if self.guest(player).is_none() {
                return false;
            }
            pend(Pending::Teleport(player, to));
            return true;
        } else {
            match self.guest(player) {
                Some(g) => Target::Guest(g.conn),
                None => return false,
            }
        };
        let (pos, rotation) = ([to.x, to.y, to.z], Some([to.yaw, to.pitch]));
        if !self.game.teleport_across(target, level, pos, rotation) {
            self.game.teleport_to(target, pos, rotation);
        }
        true
    }

    fn state(&mut self, player: PlayerId) -> Option<PlayerState> {
        let mut s = record_state(self.guest_mut(player)?);
        if self.acting(player) {
            let game = &mut *self.game;
            s.game_mode = game.mode_id();
            s.level = game.experience.level;
            s.exp = game.experience.progress;
            s.total_exp = game.experience.total;
            s.held_slot = game.selected as i32;
            s.in_vehicle = game.riding.is_some();
            if let Some(m) = game.player.as_deref_mut() {
                s.health = f64::from(m.health.health());
                s.food = m.health.food.food;
                s.saturation = m.health.food.saturation;
                s.flying = m.flying;
                s.allow_flight = m.may_fly;
                s.sprinting = m.sprinting;
                body_state(m, &mut s);
            }
        }
        Some(s)
    }

    fn set(&mut self, player: PlayerId, field: Field, value: f64) -> bool {
        match field {
            Field::Flying | Field::AllowFlight | Field::Sneaking | Field::Sprinting => {
                self.set_flag(player, field, value != 0.0)
            }
            _ if self.acting(player) => self.set_in_turn(field, value),
            Field::GameMode => self.set_game_mode_of(player, value as i32),
            _ => self.set_in_record(player, field, value),
        }
    }

    /// `Entity.setDeltaMovement` and `hurtMarked`: the body moves so, and the
    /// player's client is told.
    fn set_velocity(&mut self, player: PlayerId, velocity: [f64; 3]) -> bool {
        let v = Vec3::new(velocity[0], velocity[1], velocity[2]);
        let acting = self.acting(player);
        if acting && let Some(p) = self.game.player.as_deref_mut() {
            p.velocity = v;
        }
        let Some(g) = self.guest_mut(player) else {
            return false;
        };
        if !acting && let Some(m) = g.motion.as_deref_mut() {
            m.velocity = v;
        }
        g.send(&C::SetEntityMotion(play::SetEntityMotion {
            id: g.entity_id,
            velocity,
        }));
        true
    }

    fn show(&mut self, player: PlayerId, what: Show) -> bool {
        let packet = match what {
            Show::ActionBar(text) => C::SystemChat(play::SystemChat {
                content: legacy(text),
                overlay: true,
            }),
            Show::Title {
                title,
                subtitle,
                fade_in,
                stay,
                fade_out,
            } => {
                let times = C::SetTitlesAnimation(play::TitlesAnimation {
                    fade_in,
                    stay,
                    fade_out,
                });
                if !self.send(player, &times) {
                    return false;
                }
                if let Some(title) = title {
                    self.send(
                        player,
                        &C::SetTitleText(play::Text {
                            text: legacy(title),
                        }),
                    );
                }
                if let Some(subtitle) = subtitle {
                    let text = legacy(subtitle);
                    self.send(player, &C::SetSubtitleText(play::Text { text }));
                }
                return true;
            }
            Show::ResetTitle => C::ClearTitles(play::Bool1 { value: true }),
            Show::TabList { header, footer } => C::TabList(play::TabList {
                header: legacy(header),
                footer: legacy(footer),
            }),
            Show::Sound {
                sound,
                category,
                at,
                volume,
                pitch,
                seed,
            } => {
                let Some(hub) = self.hub() else {
                    return false;
                };
                // A registered sound by id, else by name (Holder.direct).
                let holder = match hub.hub.registry_id("minecraft:sound_event", sound) {
                    Some(id) => (id, None),
                    None => (-1, Some((sound.to_owned(), None))),
                };
                C::Sound(play::Sound {
                    sound: holder,
                    source: category,
                    x: (at[0] * 8.0) as i32,
                    y: (at[1] * 8.0) as i32,
                    z: (at[2] * 8.0) as i32,
                    volume,
                    pitch,
                    seed,
                })
            }
            Show::StopSound { sound, category } => C::StopSound(play::StopSound {
                source: category,
                sound: sound.map(str::to_owned),
            }),
            Show::Particle {
                particle,
                at,
                count,
                offset,
                speed,
                force,
            } => {
                let Some(hub) = self.hub() else {
                    return false;
                };
                let Some(id) = hub.hub.registry_id("minecraft:particle_type", particle) else {
                    return false;
                };
                C::LevelParticles(play::LevelParticles {
                    particle: Particle { id, data: None },
                    override_limiter: force,
                    always_show: false,
                    pos: at,
                    dist: offset,
                    max_speed: [speed; 3],
                    count,
                    randomization: 0,
                })
            }
            Show::BlockChange { pos, state } => {
                let Some(state) = parse_state(self.game, state) else {
                    return false;
                };
                C::BlockUpdate(play::BlockUpdate {
                    pos,
                    state: state.0 as i32,
                })
            }
            Show::PluginMessage { channel, data } => C::CustomPayload(CustomPayload {
                channel: channel.to_owned(),
                data: data.to_vec(),
            }),
            Show::ListName(name) => {
                let Some(hub) = self.hub() else {
                    return false;
                };
                // ClientboundPlayerInfoUpdatePacket's UPDATE_DISPLAY_NAME, to everyone.
                hub.hub
                    .broadcast(&C::PlayerInfoUpdate(play::PlayerInfoUpdate {
                        actions: 0x20,
                        entries: vec![play::PlayerInfo {
                            uuid: player,
                            display_name: Some(name.map(legacy)),
                            ..Default::default()
                        }],
                    }));
                return true;
            }
            Show::Commands => {
                let Some(hub) = self.hub() else {
                    return false;
                };
                // CraftPlayer.updateCommands: this player's tree only.
                return hub.hub.resend_plugin_commands(player);
            }
        };
        self.send(player, &packet)
    }

    fn effects(&mut self, player: PlayerId) -> Vec<Effect> {
        let effect = |e: &EffectInstance| Effect {
            id: format!("minecraft:{}", e.id),
            duration: e.duration,
            amplifier: e.amplifier,
            ambient: e.ambient,
            particles: e.visible,
            icon: true,
        };
        if self.acting(player) {
            return self.game.potions.effects.list.iter().map(effect).collect();
        }
        self.guest(player)
            .map(|g| g.effects.list.iter().map(effect).collect())
            .unwrap_or_default()
    }

    /// `LivingEntity.addEffect`: in the player's turn, as `/effect` gives it.
    fn add_effect(&mut self, player: PlayerId, e: &Effect) -> bool {
        let Some(def) = horizon_client::brewing::effect(&e.id) else {
            return false;
        };
        let mut instance = EffectInstance::new(def.id, e.duration, e.amplifier);
        instance.ambient = e.ambient;
        instance.visible = e.particles;
        if self.acting(player) {
            return self.game.add_player_effect(instance);
        }
        if self.hub_in_game() {
            let Some(conn) = self.guest(player).map(|g| g.conn) else {
                return false;
            };
            let added = self
                .game
                .for_player(Target::Guest(conn), |g| g.add_player_effect(instance));
            return added.unwrap_or(false);
        }
        self.guest_mut(player)
            .is_some_and(|g| g.effects.add(instance))
    }

    fn remove_effect(&mut self, player: PlayerId, id: &str) -> bool {
        let Some(def) = horizon_client::brewing::effect(id) else {
            return false;
        };
        let remove = |g: &mut Game| {
            let before = g.potions.effects.list.len();
            g.potions.effects.list.retain(|x| x.id != def.id);
            g.apply_effect_status();
            g.potions.effects.list.len() != before
        };
        if self.acting(player) {
            return remove(self.game);
        }
        if self.hub_in_game() {
            let Some(conn) = self.guest(player).map(|g| g.conn) else {
                return false;
            };
            return self
                .game
                .for_player(Target::Guest(conn), remove)
                .unwrap_or(false);
        }
        self.guest_mut(player).is_some_and(|g| {
            let before = g.effects.list.len();
            g.effects.list.retain(|x| x.id != def.id);
            g.effects.list.len() != before
        })
    }

    fn kick(&mut self, player: PlayerId, reason: &str) -> bool {
        let Some(conn) = self.guest(player).map(|g| g.conn) else {
            return false;
        };
        let Some(hub) = self.hub() else {
            return false;
        };
        hub.hub.kick(conn, legacy(reason));
        true
    }

    /// Commands.performCommand with the player's source, now when the hub is
    /// in place and it is not their turn, else at the next tick's start.
    fn command_as(&mut self, player: PlayerId, line: &str) -> bool {
        let Some((conn, permission)) = self.guest(player).map(|g| (g.conn, g.permission)) else {
            return false;
        };
        let command = GuestCommand {
            conn,
            command: line.to_owned(),
            permission,
            integrated: false,
            quiet: false,
        };
        if self.hub_in_game() && !self.acting(player) {
            return self.game.run_guest_command(command);
        }
        pend(Pending::Command(command));
        true
    }

    fn client(&mut self, player: PlayerId) -> Option<Client> {
        let g = self.guest(player)?;
        Some(Client {
            address: g.address,
            locale: g.info.language.clone(),
            view_distance: i32::from(g.info.view_distance),
        })
    }
}

/// The game as the plugins see it from inside a guest's turn: the hub is
/// the one the turn lent, unless it is in place.
fn in_turn<R>(game: &mut Game, f: impl FnOnce(&mut GameHost) -> R) -> R {
    let in_place = game.lan.is_some();
    ferraloom_plugins::borrow_lent(|lent: Option<&mut LanHost>| {
        f(&mut GameHost::new(game, lent.filter(|_| !in_place)))
    })
}

/// PlayerList.canPlayerLogin's verdict as `PlayerLoginEvent.Result`, and the
/// message Spigot gives plugins for it.
fn login_result(refused: Option<&Tag>) -> (LoginResult, &'static str) {
    let Some(tag) = refused else {
        return (LoginResult::Allowed, "");
    };
    let key = match tag.get("translate") {
        Some(Tag::String(key)) => key.to_string_lossy().to_string(),
        _ => String::new(),
    };
    match key.as_str() {
        "multiplayer.disconnect.not_whitelisted" => (
            LoginResult::KickWhitelist,
            "You are not whitelisted on this server!",
        ),
        "multiplayer.disconnect.banned.reason" | "multiplayer.disconnect.banned_ip.reason" => {
            (LoginResult::KickBanned, "You are banned from this server!")
        }
        "multiplayer.disconnect.server_full" => (LoginResult::KickFull, "The server is full!"),
        _ => (
            LoginResult::KickOther,
            "You are not allowed on this server!",
        ),
    }
}

/// A login on its connection's thread, after vanilla's checks (`refused`):
/// `AsyncPlayerPreLoginEvent` and `PlayerLoginEvent`, which may let in whom
/// vanilla refused or refuse whom it let in. Without plugins, `refused`.
pub(in crate::game) fn login(
    address: std::net::SocketAddr,
    id: PlayerId,
    name: &str,
    op: bool,
    refused: Option<Tag>,
) -> Option<Tag> {
    if !ferraloom_plugins::running() {
        return refused;
    }
    let (result, message) = login_result(refused.as_ref());
    let hostname = address.ip().to_string();
    let kick = ferraloom_plugins::login(id, name, address, &hostname, op, result, message)?;
    // Vanilla's verdict as it was keeps vanilla's own message.
    match refused {
        Some(tag) if kick == message => Some(tag),
        _ => Some(legacy(&kick)),
    }
}

impl Game<'_> {
    /// The logins waiting for `PlayerLoginEvent`, served each loop of the
    /// server, paused or not (vanilla handles logins while paused too).
    pub(in crate::game) fn plugin_logins(&mut self) {
        if ferraloom_plugins::running() {
            ferraloom_plugins::serve_logins(&mut GameHost::new(self, None));
        }
    }

    /// The players' state for reads off the server thread, once a plugin
    /// made one (`ferraloom_plugins::snapshots_wanted`): at each tick's start.
    pub(in crate::game) fn plugin_snapshots(&mut self) {
        if !ferraloom_plugins::snapshots_wanted() {
            return;
        }
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let snapshots = host
            .hub
            .guests
            .iter_mut()
            .filter(|g| g.placed)
            .map(|g| {
                let at = Location {
                    world: world_index(Dim::from_id(&g.dimension)),
                    x: g.position[0],
                    y: g.position[1],
                    z: g.position[2],
                    yaw: g.yaw,
                    pitch: g.pitch,
                };
                (g.uuid, record_state(g), at)
            })
            .collect();
        ferraloom_plugins::store_snapshots(snapshots);
    }

    /// PlayerList.placeNewPlayer's PlayerJoinEvent, once the guest is in the
    /// world. True when a listener chose the join message (the hub sent none;
    /// this sends the event's to every player, as CraftBukkit does).
    pub(in crate::game) fn plugin_join(&mut self, host: &mut LanHost, conn: u32) -> bool {
        if !ferraloom_plugins::running() {
            return false;
        }
        let Some(g) = host.hub.guest(conn).filter(|g| g.placed) else {
            return false;
        };
        let (id, name, entity_id, op) = (g.uuid, g.name.clone(), g.entity_id, g.permission > 0);
        let attempt = g.plugin_attempt;
        // OfflinePlayer.hasPlayedBefore and its times: the player file as it
        // was before this join (its time is the last save).
        let file = horizon_client::lan::playerdata::path(&self.world_root, id);
        let millis = |t: std::io::Result<std::time::SystemTime>| {
            t.ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as i64)
        };
        let (played_before, first_played, last_played) = match std::fs::metadata(&file) {
            Ok(m) => {
                let last = millis(m.modified());
                let first = millis(m.created());
                (true, if first == 0 { last } else { first }, last)
            }
            Err(_) => {
                let now = millis(Ok(std::time::SystemTime::now()));
                (false, now, now)
            }
        };
        let default = format!("\u{a7}e{name} joined the game");
        let joined = Joined {
            id,
            name: &name,
            entity_id,
            op,
            message: &default,
            played_before,
            first_played,
            last_played,
            attempt,
        };
        let message = ferraloom_plugins::player_join(&mut GameHost::new(self, Some(host)), &joined);
        if !listens(Event::PlayerJoin) {
            return false;
        }
        if let Some(message) = message {
            let line = announcement(&message, &default, "multiplayer.player.joined", &name);
            host.hub.broadcast(&system_chat(&line));
            crate::game::dedicated::log("INFO", &format!("System chat: {}", line.plain()));
        }
        true
    }

    /// PlayerList.remove's PlayerQuitEvent for a guest who left. True when a
    /// listener chose the quit message (sent here to everyone still on).
    pub(in crate::game) fn plugin_quit(&mut self, host: &mut LanHost, guest: &Guest) -> bool {
        if !ferraloom_plugins::running() || !guest.placed {
            return false;
        }
        LAST_MOVE.with_borrow_mut(|m| m.remove(&guest.uuid));
        let default = format!("\u{a7}e{} left the game", guest.name);
        let at = Location {
            world: world_index(Dim::from_id(&guest.dimension)),
            x: guest.position[0],
            y: guest.position[1],
            z: guest.position[2],
            yaw: guest.yaw,
            pitch: guest.pitch,
        };
        let mut gh = GameHost::new(self, Some(host));
        let message = ferraloom_plugins::player_quit(&mut gh, guest.uuid, &default, at);
        if !listens(Event::PlayerQuit) {
            return false;
        }
        if let Some(message) = message {
            let line = announcement(&message, &default, "multiplayer.player.left", &guest.name);
            host.hub.broadcast(&system_chat(&line));
            crate::game::dedicated::log("INFO", &format!("System chat: {}", line.plain()));
        }
        true
    }

    /// A player's chat line, which the hub left to AsyncPlayerChatEvent.
    pub(in crate::game) fn plugin_chat(&mut self, sender: u128, message: &str) {
        if listens(Event::AsyncPlayerChat) {
            ferraloom_plugins::chat(sender, message);
        }
    }

    /// A command a guest sent (no slash), before vanilla's: its
    /// PlayerCommandPreprocessEvent, then a plugin's command. `None` when
    /// that was all (cancelled, or a plugin took it); else the line vanilla
    /// runs, `Some(None)` for the one sent.
    pub(in crate::game) fn plugin_guest_command(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        line: &str,
    ) -> Option<Option<String>> {
        if !ferraloom_plugins::running() {
            return Some(None);
        }
        let id = host.hub.guest(conn).map(|g| g.uuid)?;
        let mut changed = None;
        if listens(Event::PlayerCommandPreprocess) {
            let gh = &mut GameHost::new(self, Some(&mut *host));
            let run = ferraloom_plugins::command_preprocess(gh, id, line)?;
            changed = (run != line).then_some(run);
        }
        let run = changed.as_deref().unwrap_or(line);
        if is_plugin_command(run)
            && ferraloom_plugins::player_command(&mut GameHost::new(self, Some(host)), id, run)
        {
            return None;
        }
        Some(changed)
    }

    /// ServerGamePacketListenerImpl.disconnect's PlayerKickEvent for a kick
    /// the server decided (`reason` in English for the plugins): the reason
    /// to kick with, `None` when a listener cancelled it. The hub is in place.
    pub(in crate::game) fn plugin_kick(
        &mut self,
        conn: u32,
        reason: &str,
        tag: Tag,
    ) -> Option<Tag> {
        if !listens(Event::PlayerKick) {
            return Some(tag);
        }
        let id = self.lan.as_ref()?.hub.guest(conn)?.uuid;
        let kick = ferraloom_plugins::player_kick(&mut GameHost::new(self, None), id, reason)?;
        Some(if kick == reason { tag } else { legacy(&kick) })
    }

    /// ServerPlayer.setGameMode's PlayerGameModeChangeEvent for the player in
    /// place (a guest's turn): true when cancelled.
    pub(in crate::game) fn plugin_game_mode(&mut self, mode: GameMode) -> bool {
        let Some(id) = self.lan_acting_uuid else {
            return false;
        };
        let id_of = GameMode::ALL.iter().position(|m| *m == mode).unwrap_or(0) as i32;
        in_turn(self, |h| {
            ferraloom_plugins::player_game_mode_change(h, id, id_of)
        })
    }

    /// TeleportCommand's PlayerTeleportEvent (COMMAND) for a player it moves
    /// to `pos` in `level`: where to, as its listeners leave it; `None` when
    /// cancelled. Anything else than a player goes as it was.
    pub(in crate::game) fn plugin_command_teleport(
        &mut self,
        t: Target,
        level: Dim,
        pos: [f64; 3],
        rotation: Option<[f32; 2]>,
    ) -> Option<(Dim, [f64; 3], Option<[f32; 2]>)> {
        let unchanged = Some((level, pos, rotation));
        if !listens(Event::PlayerTeleport) {
            return unchanged;
        }
        let id = match t {
            Target::Player => self.lan_acting_uuid,
            Target::Guest(conn) => self.command_guest(conn).map(|g| g.uuid),
            _ => None,
        };
        let Some(id) = id else {
            return unchanged;
        };
        let mut host = GameHost::new(self, None);
        let Some(from) = host.location(id) else {
            return unchanged;
        };
        let [yaw, pitch] = rotation.unwrap_or([from.yaw, from.pitch]);
        let to = Location {
            world: world_index(level),
            x: pos[0],
            y: pos[1],
            z: pos[2],
            yaw,
            pitch,
        };
        match ferraloom_plugins::player_teleport(&mut host, id, from, to, Cause::Command) {
            Moved::Unchanged => unchanged,
            Moved::Cancelled => None,
            Moved::To(l) => Some((
                DIMS.get(l.world).copied().unwrap_or(level),
                [l.x, l.y, l.z],
                Some([l.yaw, l.pitch]),
            )),
        }
    }

    /// ThrownEnderpearl.onHit's PlayerTeleportEvent (ENDER_PEARL) for the
    /// player in place, before the pearl takes them to `to`: false when
    /// cancelled (CraftBukkit keeps the endermite and the damage out too).
    /// A destination a listener changes is not followed yet.
    pub(in crate::game) fn plugin_pearl_allowed(&mut self, to: [f64; 3]) -> bool {
        if !listens(Event::PlayerTeleport) {
            return true;
        }
        let Some(id) = self.lan_acting_uuid else {
            return true;
        };
        in_turn(self, |h| {
            let Some(from) = h.location(id) else {
                return true;
            };
            let to = Location {
                x: to[0],
                y: to[1],
                z: to[2],
                ..from
            };
            ferraloom_plugins::player_teleport(h, id, from, to, Cause::EnderPearl)
                != Moved::Cancelled
        })
    }

    /// ServerPlayer.triggerDimensionChangeTriggers' PlayerChangedWorldEvent
    /// for the player in place, who arrived from `from`.
    pub(in crate::game) fn plugin_changed_world(&mut self, from: Dim) {
        let Some(id) = self.lan_acting_uuid else {
            return;
        };
        in_turn(self, |h| {
            ferraloom_plugins::player_changed_world(h, id, world_index(from))
        });
    }

    /// PlayerInteractEvent for a guest's click (outside their turn): what
    /// its listeners left to happen.
    fn plugin_interact(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        action: Action,
        block: Option<([i32; 3], i32, [f32; 3])>,
        off_hand: bool,
    ) -> Interaction {
        let Some(g) = host.hub.guest(conn) else {
            return Interaction::default();
        };
        let id = g.uuid;
        let held = if off_hand {
            g.offhand.as_ref()
        } else {
            g.inventory.get(g.selected).and_then(Option::as_ref)
        };
        let item = held.cloned();
        let world = world_index(Dim::from_id(&g.dimension));
        let interact = Interact {
            player: id,
            action,
            item: item.as_ref(),
            block: block.map(|(pos, face, _)| (world, pos, face)),
            off_hand,
            point: block.map(|(_, _, p)| p.map(f64::from)),
        };
        ferraloom_plugins::player_interact(&mut GameHost::new(self, Some(host)), &interact)
    }

    /// handleBlockBreakAction's LEFT_CLICK_BLOCK: true when denied (either
    /// result), and the client, which predicted the click, gets the block back.
    pub(in crate::game) fn plugin_left_click_block(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        pos: [i32; 3],
        face: impl Into<i32>,
    ) -> bool {
        let block = Some((pos, face.into(), [0.0; 3]));
        let i = self.plugin_interact(host, conn, Action::LeftClickBlock, block, false);
        if !(i.deny_block || i.deny_item) {
            return false;
        }
        let state = self.world.block(pos[0], pos[1], pos[2]).0 as i32;
        host.hub
            .send_to(conn, &C::BlockUpdate(play::BlockUpdate { pos, state }));
        true
    }

    /// handleUseItemOn's RIGHT_CLICK_BLOCK: what it denies stays denied for
    /// `use_hand` until [`Game::plugin_interacted`].
    pub(in crate::game) fn plugin_right_click_block(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        pos: [i32; 3],
        face: i32,
        cursor: [f32; 3],
        off_hand: bool,
    ) {
        let block = Some((pos, face, cursor));
        let i = self.plugin_interact(host, conn, Action::RightClickBlock, block, off_hand);
        DENIED.set(i);
    }

    /// The click's PlayerInteractEvent is over: nothing is denied.
    pub(in crate::game) fn plugin_interacted(&mut self) {
        DENIED.set(Interaction::default());
    }

    /// `useInteractedBlock() == DENY` for the click being handled.
    pub(in crate::game) fn plugin_denies_block(&self) -> bool {
        listens(Event::PlayerInteract) && DENIED.get().deny_block
    }

    /// `useItemInHand() == DENY` for the click being handled.
    pub(in crate::game) fn plugin_denies_item(&self) -> bool {
        listens(Event::PlayerInteract) && DENIED.get().deny_item
    }

    /// handleUseItem's RIGHT_CLICK_AIR: true when the item is denied.
    pub(in crate::game) fn plugin_right_click_air(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        off_hand: bool,
    ) -> bool {
        self.plugin_interact(host, conn, Action::RightClickAir, None, off_hand)
            .deny_item
    }

    /// handleAnimate's LEFT_CLICK_AIR for the player in place, when their
    /// swing aims at no block within reach.
    pub(in crate::game) fn plugin_left_click_air(&mut self) {
        let Some(id) = self.lan_acting_uuid else {
            return;
        };
        if self.pick().is_some() {
            return;
        }
        let item = self.stack_in(self.selected).cloned();
        let interact = Interact {
            player: id,
            action: Action::LeftClickAir,
            item: item.as_ref(),
            block: None,
            off_hand: false,
            point: None,
        };
        in_turn(self, |h| ferraloom_plugins::player_interact(h, &interact));
    }

    /// handleSetCarriedItem's PlayerItemHeldEvent: true when cancelled (the
    /// client is told its old slot again, as CraftBukkit does).
    pub(in crate::game) fn plugin_item_held(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        slot: i32,
    ) -> bool {
        let Some((id, previous)) = host.hub.guest(conn).map(|g| (g.uuid, g.selected as i32)) else {
            return false;
        };
        if previous == slot {
            return false;
        }
        let gh = &mut GameHost::new(self, Some(&mut *host));
        if !ferraloom_plugins::player_item_held(gh, id, previous, slot) {
            return false;
        }
        host.hub
            .send_to(conn, &C::SetHeldSlot(play::SetHeldSlot { slot: previous }));
        true
    }

    /// PlayerList.respawn's PlayerRespawnEvent once vanilla placed the guest:
    /// a location a plugin chose replaces vanilla's (its arrival included).
    pub(in crate::game) fn plugin_respawn(&mut self, host: &mut LanHost, conn: u32) {
        let Some(g) = host.hub.guest(conn) else {
            return;
        };
        let (id, bed, anchor) = (g.uuid, g.spawn.is_some(), g.respawn_elsewhere.is_some());
        let at = Location {
            world: world_index(Dim::from_id(&g.dimension)),
            x: g.position[0],
            y: g.position[1],
            z: g.position[2],
            yaw: g.yaw,
            pitch: g.pitch,
        };
        let gh = &mut GameHost::new(self, Some(&mut *host));
        let to = ferraloom_plugins::player_respawn(gh, id, at, bed, anchor);
        if to == at {
            return;
        }
        // The plugin's place wins over the arrival vanilla was waiting for.
        self.server_trips.retain(|t| !t.is(conn));
        pend(Pending::Teleport(id, to));
    }

    /// ServerPlayer.die's PlayerDeathEvent for a guest, before anything is
    /// said or dropped (`guest_die`): vanilla's message (`cause`, the key of
    /// what killed them outright, else their combat tracker's), experience
    /// and keepInventory, as its listeners leave them.
    pub(in crate::game) fn plugin_death(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        cause: Option<&str>,
        at: [f64; 3],
        reward: u32,
    ) -> Death {
        let keep = self.ui.rules.bool("keep_inventory");
        let Some((id, name, spectator)) = host
            .hub
            .guest(conn)
            .map(|g| (g.uuid, g.name.clone(), g.spectator()))
        else {
            return Death {
                message: None,
                dropped_exp: reward as i32,
                keep_inventory: keep,
                keep_level: keep,
            };
        };
        let me = self.player_text(&name, id);
        let text = match cause {
            Some(key) => Text::translate(key, &[me]),
            None if at[1] < f64::from(horizon_client::chunk::MIN_Y - 64) => {
                Text::translate("death.attack.outOfWorld", &[me])
            }
            None => {
                // guest_death_message: the guest's own CombatTracker.
                let mut tracker = self.guest_combat.remove(&conn).unwrap_or_default();
                std::mem::swap(&mut self.combat, &mut tracker);
                let text = self.combat_death_message_for(me);
                std::mem::swap(&mut self.combat, &mut tracker);
                self.guest_combat.insert(conn, tracker);
                text
            }
        };
        let death = Death {
            message: Some(text.plain()),
            dropped_exp: reward as i32,
            keep_inventory: keep || spectator,
            keep_level: keep || spectator,
        };
        ferraloom_plugins::player_death(&mut GameHost::new(self, Some(host)), id, death)
    }

    /// The death message as PlayerDeathEvent left it: vanilla's own when
    /// unchanged, none as empty.
    pub(in crate::game) fn plugin_death_text(death: Option<&Death>, text: Text) -> Text {
        match death.map(|d| &d.message) {
            None => text,
            Some(None) => Text::literal(String::new()),
            Some(Some(m)) if *m == text.plain() => text,
            Some(Some(m)) => Text::legacy(m),
        }
    }

    /// A custom payload from a guest's client, for the plugins' messenger.
    pub(in crate::game) fn plugin_message(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        p: &CustomPayload,
    ) {
        if !ferraloom_plugins::running() {
            return;
        }
        let Some(id) = host.hub.guest(conn).map(|g| g.uuid) else {
            return;
        };
        let gh = &mut GameHost::new(self, Some(host));
        ferraloom_plugins::plugin_message(gh, id, &p.channel, &p.data);
    }

    /// handleMovePlayer, handlePlayerInput, handlePlayerCommand and
    /// handlePlayerAbilities' events for the packets each guest of the level
    /// in place sent this tick, before the hub handles them: PlayerMoveEvent
    /// (CraftBukkit's thresholds: moved more than 1/256 of a block squared
    /// or turned more than 10 degrees since the last one), the sneak, sprint
    /// and flight toggles. A cancelled move is dropped with the moves after
    /// it until the correction, which the hub sends once it has handled the
    /// packets before it: the player goes back to where they left them (CraftBukkit's
    /// teleport to `from`); a move a listener changed teleports at once. A
    /// cancelled toggle is dropped (a flight's client is told its abilities
    /// again). Only while a plugin listens to one, or a cancelled move's
    /// correction is still to come.
    pub(in crate::game) fn plugin_inbox(&mut self, host: &mut LanHost) {
        for i in 0..host.hub.guests.len() {
            if host.hub.here(&host.hub.guests[i]) && !host.hub.guests[i].inbox.is_empty() {
                self.plugin_guest_inbox(host, i);
            }
        }
    }

    fn plugin_guest_inbox(&mut self, host: &mut LanHost, i: usize) {
        use horizon_protocol::packets::play::Serverbound as S;
        let g = &mut host.hub.guests[i];
        if g.dead || g.sleeping || g.vehicle.is_some() {
            return;
        }
        // A cancelled move's correction is still to come: what the client moved
        // since, behind the packets that pass kept, is dropped unseen.
        let correcting = g.correct_move.is_some();
        if let Some(kept) = g.correct_move {
            let mut n = 0;
            g.inbox.retain(|p| {
                n += 1;
                n <= kept || !is_move(p)
            });
        }
        let g = &host.hub.guests[i];
        let (id, world) = (g.uuid, world_index(Dim::from_id(&g.dimension)));
        let mut awaiting = g.awaiting_teleport;
        let mut at = Location {
            world,
            x: g.position[0],
            y: g.position[1],
            z: g.position[2],
            yaw: g.yaw,
            pitch: g.pitch,
        };
        let (mut sneaking, mut sprinting, mut flying) = (g.sneaking, g.sprinting, g.flying);
        // As the hub's handlePlayerAbilities: a plugin's allow-flight counts.
        let may_fly = g.creative() || g.spectator() || g.allow_flight;
        let mut k = 0;
        while k < host.hub.guests[i].inbox.len() {
            let (pos, rot) = match &host.hub.guests[i].inbox[k] {
                S::AcceptTeleportation(p) => {
                    // An invalid one stays for the hub, which kicks for it.
                    let finite = [p.x, p.y, p.z].iter().all(|v| v.is_finite())
                        && p.yaw.is_finite()
                        && p.pitch.is_finite();
                    if let Some((_, to)) = awaiting.filter(|(t, _)| *t == p.id && finite) {
                        awaiting = None;
                        (at.x, at.y, at.z) = (to[0], to[1], to[2]);
                        (at.yaw, at.pitch) = (p.yaw, p.pitch);
                        LAST_MOVE.with_borrow_mut(|m| m.insert(id, at));
                    }
                    k += 1;
                    continue;
                }
                S::MovePlayerPos(p) => (Some([p.x, p.y, p.z]), None),
                S::MovePlayerPosRot(p) => (Some([p.x, p.y, p.z]), Some((p.yaw, p.pitch))),
                S::MovePlayerRot(p) => (None, Some((p.yaw, p.pitch))),
                S::PlayerInput(p) if listens(Event::PlayerToggleSneak) => {
                    let shift = p.flags & 32 != 0;
                    if shift != sneaking {
                        // Since 1.21.3 a cancelled sneak changes nothing: the client decides.
                        let gh = &mut GameHost::new(self, Some(&mut *host));
                        ferraloom_plugins::player_toggle(gh, id, Toggle::Sneak, shift);
                        sneaking = shift;
                    }
                    k += 1;
                    continue;
                }
                S::PlayerCommand(p)
                    if matches!(p.action, 1 | 2) && listens(Event::PlayerToggleSprint) =>
                {
                    let on = p.action == 1;
                    if on != sprinting {
                        let gh = &mut GameHost::new(self, Some(&mut *host));
                        if ferraloom_plugins::player_toggle(gh, id, Toggle::Sprint, on) {
                            host.hub.guests[i].inbox.remove(k);
                            continue;
                        }
                        sprinting = on;
                    }
                    k += 1;
                    continue;
                }
                S::PlayerAbilities(p) if listens(Event::PlayerToggleFlight) => {
                    let on = p.flags & 2 != 0;
                    if may_fly && on != flying {
                        let gh = &mut GameHost::new(self, Some(&mut *host));
                        if ferraloom_plugins::player_toggle(gh, id, Toggle::Flight, on) {
                            // ServerPlayer.onUpdateAbilities: the client flies as before.
                            let g = &mut host.hub.guests[i];
                            g.inbox.remove(k);
                            g.send(&C::PlayerAbilities(play::PlayerAbilities {
                                flags: g.ability_flags(),
                                flying_speed: 0.05,
                                walking_speed: 0.1,
                            }));
                            continue;
                        }
                        flying = on;
                    }
                    k += 1;
                    continue;
                }
                _ => {
                    k += 1;
                    continue;
                }
            };
            k += 1;
            // A move with an invalid value stays for the hub, which kicks for it.
            let finite = pos.is_none_or(|p| p.iter().all(|v| v.is_finite()))
                && rot.is_none_or(|(yaw, pitch)| yaw.is_finite() && pitch.is_finite());
            if !finite || awaiting.is_some() || correcting || !listens(Event::PlayerMove) {
                continue;
            }
            let mut to = at;
            if let Some([x, y, z]) = pos {
                (to.x, to.y, to.z) = (x, y, z);
            }
            if let Some((yaw, pitch)) = rot {
                (to.yaw, to.pitch) = (yaw, pitch);
            }
            at = to;
            // The first move after joining only starts the count, as
            // CraftBukkit's lastPosX starts at Double.MAX_VALUE.
            let Some(from) = LAST_MOVE.with_borrow(|m| m.get(&id).copied()) else {
                LAST_MOVE.with_borrow_mut(|m| m.insert(id, to));
                continue;
            };
            let moved = (from.x - to.x).powi(2) + (from.y - to.y).powi(2) + (from.z - to.z).powi(2);
            let turned = (from.yaw - to.yaw).abs() + (from.pitch - to.pitch).abs();
            if moved <= 1.0 / 256.0 && turned <= 10.0 {
                continue;
            }
            LAST_MOVE.with_borrow_mut(|m| m.insert(id, to));
            let gh = &mut GameHost::new(self, Some(&mut *host));
            let back = match ferraloom_plugins::player_move(gh, id, from, to) {
                Moved::Unchanged => continue,
                Moved::Cancelled => {
                    // The hub handles the moves before this one, then corrects.
                    let g = &mut host.hub.guests[i];
                    k -= 1;
                    g.inbox.remove(k);
                    let mut later = g.inbox.split_off(k);
                    later.retain(|p| !is_move(p));
                    g.inbox.append(&mut later);
                    // The hub counts what is left once it has handled this tick's.
                    g.correct_move = Some(usize::MAX);
                    LAST_MOVE.with_borrow_mut(|m| m.insert(id, from));
                    continue;
                }
                Moved::To(other) => other,
            };
            // The move is dropped and the client sent where the event left it.
            k -= 1;
            host.hub.guests[i].inbox.remove(k);
            LAST_MOVE.with_borrow_mut(|m| m.insert(id, back));
            if back.world == world {
                let g = &mut host.hub.guests[i];
                (g.yaw, g.pitch) = (back.yaw, back.pitch);
                host.hub.teleport(i, Some([back.x, back.y, back.z]));
                awaiting = host.hub.guests[i].awaiting_teleport;
                at = back;
            } else {
                pend(Pending::Teleport(id, back));
                return;
            }
        }
    }
}

/// A movement packet (handleMovePlayer's).
fn is_move(p: &play::Serverbound) -> bool {
    use play::Serverbound as S;
    matches!(
        p,
        S::MovePlayerPos(_) | S::MovePlayerPosRot(_) | S::MovePlayerRot(_) | S::MovePlayerStatus(_)
    )
}

/// The events [`Game::plugin_inbox`] fires.
pub(in crate::game) const INBOX_EVENTS: &[Event] = &[
    Event::PlayerMove,
    Event::PlayerToggleSneak,
    Event::PlayerToggleSprint,
    Event::PlayerToggleFlight,
];

thread_local! {
    /// What the PlayerInteractEvent of the click being handled denies.
    static DENIED: std::cell::Cell<Interaction> = const {
        std::cell::Cell::new(Interaction {
            deny_block: false,
            deny_item: false,
        })
    };
    /// Where each player's last PlayerMoveEvent left them (CraftBukkit's
    /// `lastPosX`...): the next event's `from`.
    static LAST_MOVE: std::cell::RefCell<std::collections::HashMap<PlayerId, Location>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}
