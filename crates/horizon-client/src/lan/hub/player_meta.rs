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
/// Per player: what this module keeps beside `Guest`.
#[derive(Clone, Debug, Default)]
pub struct PlayerMeta {
    /// What the scoreboard criteria were last set to (game/scoreboard.rs).
    pub recorded: RecordedCriteria,
}

impl PlayerMeta {
    /// `PlayerList.respawn`: a new player, whose client starts from the
    /// defaults; its shoulders are what it kept.
    pub fn respawned(&mut self) {
        self.recorded = RecordedCriteria::default();
    }
}
