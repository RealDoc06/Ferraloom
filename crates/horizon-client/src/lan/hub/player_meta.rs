//! Ferraloom: what vanilla syncs and checks for each player that Horizon's
//! hub left out. Entity data the player's own client and its trackers read
//! (`Entity.DATA_TICKS_FROZEN`, the shoulder
//! parrots, the player's own effect swirls), the scoreboard criteria that
//! follow a player, who sees a spectator, and the connection's checks
//! (`ServerGamePacketListenerImpl`: floating, chat and command spam, the
//! advancements tab, the recipe book's highlights, the hat in the list).
//!
//! Everything here runs when something changes (a packet, a move, a new
//! value), never as a scan of every player each tick.
use super::{HostEvent, Hub, TickCtx};
use crate::lan::guest::Guest;
use horizon_protocol::{
    entity_data::{Value, serializer},
    packets::play::{self, Clientbound as C},
    text::translatable,
};

/// The server.properties rules the hub enforces for its players.
#[derive(Clone, Copy, Debug)]
pub struct PlayerRules {
    /// `allow-flight` (`MinecraftServer.allowFlight`: true but on a
    /// dedicated server, where it is the property).
    pub allow_flight: bool,
    /// `chat-spam-threshold-seconds` and `command-spam-threshold-seconds`
    /// in ticks (`TickThrottler`'s threshold; 0 or less checks nothing).
    pub chat_spam_ticks: i32,
    pub command_spam_ticks: i32,
}

impl Default for PlayerRules {
    /// An integrated server's: flight allowed, no spam checks.
    fn default() -> Self {
        Self {
            allow_flight: true,
            chat_spam_ticks: 0,
            command_spam_ticks: 0,
        }
    }
}

impl PlayerRules {
    /// A dedicated server's, from its properties (`DedicatedServer`).
    pub fn dedicated(
        allow_flight: bool,
        chat_spam_seconds: i32,
        command_spam_seconds: i32,
    ) -> Self {
        Self {
            allow_flight,
            chat_spam_ticks: chat_spam_seconds.saturating_mul(20),
            command_spam_ticks: command_spam_seconds.saturating_mul(20),
        }
    }
}

/// Port of `TickThrottler` with the step vanilla uses for chat (20).
#[derive(Clone, Copy, Debug, Default)]
pub struct Throttle {
    count: i32,
}

impl Throttle {
    pub fn increment(&mut self) {
        self.count = self.count.saturating_add(20);
    }
    pub fn tick(&mut self) {
        if self.count > 0 {
            self.count -= 1;
        }
    }
    pub fn is_under(&self, threshold: i32) -> bool {
        threshold <= 0 || self.count < threshold
    }
}

/// `ServerPlayer.lastRecorded*`: the criteria values last written to the
/// scoreboard (None: not yet, vanilla's sentinels).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RecordedCriteria {
    pub health_and_absorption: Option<f32>,
    pub food: Option<i32>,
    pub air: Option<i32>,
    pub armor: Option<i32>,
    pub xp: Option<i32>,
    pub level: Option<i32>,
}

/// The entity data values a player's client holds of it (defaults: none
/// frozen, empty shoulders, no swirls).
#[derive(Clone, Debug, Default, PartialEq)]
struct SentMeta {
    frozen: i32,
    shoulders: [Option<i32>; 2],
    particles: Vec<(u8, i32, Value)>,
}

/// Per player: what this module keeps beside `Guest`.
#[derive(Clone, Debug, Default)]
pub struct PlayerMeta {
    /// What the scoreboard criteria were last set to (game/scoreboard.rs).
    pub recorded: RecordedCriteria,
    /// `Player.DATA_SHOULDER_PARROT_LEFT/RIGHT`: the parrot variant on each
    /// shoulder (`extractParrotVariant` of `ShoulderEntityLeft/Right`).
    pub shoulders: [Option<i32>; 2],
    sent: SentMeta,
    /// `clientIsFloating` and `aboveGroundTickCount`.
    floating: bool,
    above_ground_ticks: i32,
    chat_spam: Throttle,
    command_spam: Throttle,
}

impl PlayerMeta {
    /// `PlayerList.respawn`: a new player, whose client starts from the
    /// defaults; its shoulders are what it kept.
    pub fn respawned(&mut self) {
        self.recorded = RecordedCriteria::default();
        self.sent = SentMeta::default();
        self.floating = false;
        self.above_ground_ticks = 0;
    }
}

/// `Player.extractParrotVariant`: the variant of a shoulder compound that
/// holds a parrot (`Parrot.Variant.LEGACY_CODEC`, clamped to 0..=4).
pub fn shoulder_variant(tag: Option<&horizon_world::nbt::Tag>) -> Option<i32> {
    use horizon_world::nbt::Tag;
    let tag = tag?;
    match tag.get("id") {
        Some(Tag::String(id)) if id.to_string_lossy() == "minecraft:parrot" => {}
        _ => return None,
    }
    let variant = match tag.get("Variant")? {
        Tag::Int(v) => *v,
        Tag::Byte(v) => i32::from(*v),
        Tag::Short(v) => i32::from(*v),
        Tag::Long(v) => *v as i32,
        _ => return None,
    };
    Some(variant.clamp(0, 4))
}

/// The shoulder parrots a player's saved compound holds.
pub fn shoulders_of(saved: Option<&horizon_world::nbt::Tag>) -> [Option<i32>; 2] {
    ["ShoulderEntityLeft", "ShoulderEntityRight"]
        .map(|key| shoulder_variant(saved.and_then(|s| s.get(key))))
}

/// The entity data a player has beside Horizon's own (trackers' view):
/// non-default values into `meta`, every entry's default into `defaults`.
pub fn tracked_meta(
    ticks_frozen: i32,
    shoulders: [Option<i32>; 2],
    meta: &mut Vec<(u8, i32, Value)>,
    defaults: &mut Vec<(u8, i32, Value)>,
) {
    if ticks_frozen != 0 {
        meta.push((7, serializer::INT, Value::Int(ticks_frozen)));
    }
    for (index, parrot) in [(19, shoulders[0]), (20, shoulders[1])] {
        if let Some(variant) = parrot {
            meta.push((
                index,
                serializer::OPTIONAL_UNSIGNED_INT,
                Value::Id(variant + 1),
            ));
        }
    }
    defaults.extend([
        (7, serializer::INT, Value::Int(0)),
        (19, serializer::OPTIONAL_UNSIGNED_INT, Value::Id(0)),
        (20, serializer::OPTIONAL_UNSIGNED_INT, Value::Id(0)),
    ]);
}

/// `LivingEntity.isDeadOrDying` and the other states `tickPlayer` doesn't
/// count floating in.
fn may_float(g: &Guest) -> bool {
    !g.sleeping && g.vehicle.is_none() && !g.dead
}

impl Hub {
    /// The entity ids of the spectating players, which the players who are
    /// not spectating never pair with (`ServerPlayer.broadcastToPlayer`; a
    /// spectator's camera is always its own here). Empty, and nothing
    /// allocated, while nobody spectates.
    pub(super) fn hidden_spectators(&self, ctx: &TickCtx) -> Vec<i32> {
        const SPECTATOR: i32 = 3;
        let host = ctx
            .host
            .filter(|h| h.game_mode == SPECTATOR)
            .map(|h| h.entity_id);
        let mut guests = self
            .guests
            .iter()
            .filter(|g| self.here(g) && g.spectator())
            .map(|g| g.entity_id)
            .peekable();
        if host.is_none() && guests.peek().is_none() {
            return Vec::new();
        }
        host.into_iter().chain(guests).collect()
    }

    /// The guest's own entity data that changed since its client was told
    /// (ServerEntity.sendDirtyEntityData to the player itself): ticks
    /// frozen and shoulders, in index order. A new player's client
    /// (joined, respawned) starts from the defaults; one that changed level
    /// keeps what it had (ClientboundRespawnPacket.KEEP_ENTITY_DATA).
    pub(super) fn own_meta_changes(&mut self, i: usize) -> Vec<(u8, i32, Value)> {
        let g = &mut self.guests[i];
        let frozen = g.motion.as_ref().map_or(0, |m| m.ticks_frozen);
        let meta = &mut g.meta;
        let mut values = Vec::new();
        let base = &meta.sent;
        if frozen != base.frozen {
            values.push((7, serializer::INT, Value::Int(frozen)));
        }
        for (k, index) in [19u8, 20].into_iter().enumerate() {
            if meta.shoulders[k] != base.shoulders[k] {
                let id = meta.shoulders[k].map_or(0, |v| v + 1);
                values.push((index, serializer::OPTIONAL_UNSIGNED_INT, Value::Id(id)));
            }
        }
        meta.sent.frozen = frozen;
        meta.sent.shoulders = meta.shoulders;
        values
    }

    /// The guest's own effect swirls (`LivingEntity.DATA_EFFECT_PARTICLES`
    /// and `DATA_EFFECT_AMBIENCE_ID`), which vanilla sends the player too:
    /// the entries that changed since its client was told, after its
    /// effects changed (`updateSynchronizedMobEffectParticles`).
    pub(super) fn own_particle_changes(&mut self, i: usize) -> Vec<(u8, i32, Value)> {
        let g = &self.guests[i];
        let now = self.effect_meta(
            g.effects
                .list
                .iter()
                .filter(|e| e.visible)
                .map(|e| (e.id, e.ambient)),
        );
        let g = &mut self.guests[i];
        if now == g.meta.sent.particles {
            return Vec::new();
        }
        let mut values = Vec::new();
        for (index, default) in [
            (
                10,
                (10, serializer::PARTICLES, Value::Particles(Vec::new())),
            ),
            (11, (11, serializer::BOOLEAN, Value::Bool(false))),
        ] {
            let was = g.meta.sent.particles.iter().find(|m| m.0 == index);
            match now.iter().find(|m| m.0 == index) {
                Some(entry) if was != Some(entry) => values.push(entry.clone()),
                None if was.is_some() => values.push(default),
                _ => {}
            }
        }
        g.meta.sent.particles = now;
        values
    }

    /// Send [`Self::own_particle_changes`], if any.
    pub(super) fn sync_own_particles(&mut self, i: usize) {
        let values = self.own_particle_changes(i);
        if !values.is_empty() {
            let g = &self.guests[i];
            g.send(&C::SetEntityData(play::SetEntityData {
                id: g.entity_id,
                values,
            }));
        }
    }

    /// The end of handlePlayerPositionChange's accepted move:
    /// `clientIsFloating`, from how far the client went up (`oyDist`) and
    /// whether every block around and just below the new box is air
    /// (`noBlocksAround`).
    pub(super) fn update_floating(&mut self, i: usize, risen: f64, ctx: &TickCtx) {
        let allow_flight = self.player_rules.allow_flight;
        // playerStandsOnSomething: the last move's verticalCollisionBelow,
        // the server's own gravity step since the last packet (a player
        // held a little off the ground lands there every few ticks), then
        // this move's own, in the air here.
        let stands = self.guests[i]
            .motion
            .as_mut()
            .is_some_and(|m| std::mem::take(&mut m.server_below));
        let g = &self.guests[i];
        let floating = risen >= -0.03125
            && !stands
            && !allow_flight
            && !g.spectator()
            && !g.creative()
            && !g.fall_flying
            && !super::guest_body(g).spin_attack
            && g.effects.get("levitation").is_none()
            && no_blocks_around(g, ctx);
        self.guests[i].meta.floating = floating;
    }

    /// `ServerPlayer.teleportTo`'s `resetFlyingTicks`.
    pub(super) fn reset_flying_ticks(&mut self, i: usize) {
        self.guests[i].meta.above_ground_ticks = 0;
    }

    /// The connection's part of `ServerGamePacketListenerImpl.tick`:
    /// `tickPlayer`'s floating check (true: kicked), then the spam
    /// throttlers count down.
    pub(super) fn tick_connection(&mut self, i: usize, events: &mut Vec<HostEvent>) -> bool {
        let g = &mut self.guests[i];
        g.meta.chat_spam.tick();
        g.meta.command_spam.tick();
        if !(g.meta.floating && may_float(g)) {
            g.meta.floating = false;
            g.meta.above_ground_ticks = 0;
            return false;
        }
        g.meta.above_ground_ticks += 1;
        if g.meta.above_ground_ticks <= max_flying_ticks(g) {
            return false;
        }
        let (conn, name) = (g.conn, g.name.clone());
        events.push(HostEvent::Warn(format!(
            "{name} was kicked for floating too long!"
        )));
        self.kick(conn, translatable("multiplayer.disconnect.flying", &[]));
        true
    }

    /// `detectRateSpam`: an operator or the single-player owner may spam.
    fn detect_spam(&mut self, i: usize, command: bool) {
        let rules = self.player_rules;
        let owner = self.singleplayer_owner;
        let g = &mut self.guests[i];
        let (throttle, threshold) = if command {
            (&mut g.meta.command_spam, rules.command_spam_ticks)
        } else {
            (&mut g.meta.chat_spam, rules.chat_spam_ticks)
        };
        throttle.increment();
        if !throttle.is_under(threshold) && g.permission == 0 && owner != Some(g.uuid) {
            let conn = g.conn;
            self.kick(conn, translatable("disconnect.spam", &[]));
        }
    }
    /// `detectChatRateSpam`, after a chat message was broadcast.
    pub(super) fn detect_chat_spam(&mut self, i: usize) {
        self.detect_spam(i, false);
    }
    /// `detectCommandRateSpam`, after a command.
    pub(super) fn detect_command_spam(&mut self, i: usize) {
        self.detect_spam(i, true);
    }

}
/// `getMaximumFlyingTicks`: 80 ticks at the default gravity, more for less.
fn max_flying_ticks(g: &mut Guest) -> i32 {
    let gravity = g.motion.as_mut().map_or(0.08, |m| {
        m.attribute("minecraft:gravity")
            .or_else(|_| m.attribute("gravity"))
            .unwrap_or(0.08)
    });
    if gravity < 9.999_999_747_378_752E-6 {
        return i32::MAX;
    }
    (80.0 * (0.08 / gravity).max(1.0)).ceil() as i32
}

/// `noBlocksAround`: every block in the box grown by 0.0625 and reaching
/// 0.55 lower is air. A block in a chunk the server doesn't have counts as
/// one (no kick for what the server can't see).
fn no_blocks_around(g: &Guest, ctx: &TickCtx) -> bool {
    let d = g.dimensions();
    let half = f64::from(d.width) / 2.0 + 0.0625;
    let p = g.position;
    let lo = [p[0] - half, p[1] - 0.0625 - 0.55, p[2] - half].map(|v| v.floor() as i32);
    let hi = [
        p[0] + half,
        p[1] + f64::from(d.height) + 0.0625,
        p[2] + half,
    ]
    .map(|v| v.floor() as i32);
    // From the bottom up: the ground under a walking player ends it at once.
    for y in lo[1]..=hi[1] {
        for x in lo[0]..=hi[0] {
            for z in lo[2]..=hi[2] {
                if !ctx.world.loaded(x, z) || !ctx.world.is_air(ctx.world.block(x, y, z)) {
                    return false;
                }
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_world::nbt::Tag;

    /// `extractParrotVariant`: a parrot's variant, clamped; anything else,
    /// or a parrot with no variant saved, is no parrot.
    #[test]
    fn only_a_saved_parrot_with_a_variant_sits_on_a_shoulder() {
        let entity = |id: &str, variant: Option<i32>| {
            let mut tag = Tag::compound([("id", Tag::String(id.into()))]);
            if let (Some(v), Tag::Compound(map)) = (variant, &mut tag) {
                map.insert("Variant".into(), Tag::Int(v));
            }
            tag
        };
        let saved = Tag::compound([
            ("ShoulderEntityLeft", entity("minecraft:parrot", Some(9))),
            ("ShoulderEntityRight", entity("minecraft:parrot", None)),
        ]);
        assert_eq!(shoulders_of(Some(&saved)), [Some(4), None]);
        let other = Tag::compound([("ShoulderEntityLeft", entity("minecraft:cat", Some(1)))]);
        assert_eq!(shoulders_of(Some(&other)), [None, None]);
        assert_eq!(shoulders_of(None), [None, None]);
    }

    /// `TickThrottler(20, 200)`: the tenth message within a tick reaches the
    /// threshold, and each tick takes one off.
    #[test]
    fn the_spam_throttle_counts_twenty_a_message_and_one_a_tick() {
        let mut t = Throttle::default();
        for _ in 0..9 {
            t.increment();
        }
        assert!(t.is_under(200));
        t.tick();
        t.increment();
        assert!(t.is_under(200));
        t.increment();
        assert!(!t.is_under(200));
        assert!(t.is_under(0));
    }
}
