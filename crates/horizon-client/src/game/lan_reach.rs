//! Ferraloom: the checks `ServerGamePacketListenerImpl.handleUseItemOn` and
//! `ServerPlayerGameMode.handleBlockBreakAction` make before a server
//! player's click on a block runs: their reach (the
//! `block_interaction_range` attribute, plus 1), where on the block the click
//! landed, and a dig above the build limit. A few comparisons per packet;
//! nothing runs per tick.
use super::{Game, lan::LanHost};
use horizon_client::lan::guest::Guest;
use horizon_protocol::packets::play::{self, Clientbound as C};

/// `handleUseItemOn`: how far the click may be from the block's centre.
const HIT_LIMIT: f64 = 1.000_000_1;

/// `Player.blockInteractionRange`: the attribute, with the creative
/// modifier its tick keeps (4.5 and 5 when the body isn't there yet).
fn block_range(g: &mut Guest) -> f64 {
    let fallback = if g.creative() { 5.0 } else { 4.5 };
    g.motion
        .as_deref_mut()
        .and_then(|m| m.attribute("minecraft:block_interaction_range").ok())
        .unwrap_or(fallback)
}

/// Port of Player#isWithinBlockInteractionRange.
fn within_reach(g: &mut Guest, pos: [i32; 3], buffer: f64) -> bool {
    let eye = [g.position[0], g.position[1] + g.eye_height(), g.position[2]];
    let range = block_range(g) + buffer;
    horizon_client::lan::guest::within_block_range(eye, range, pos)
}

impl Game<'_> {
    /// `handleUseItemOn` before the use: within reach plus 1, the click
    /// within 1.0000001 of the block's centre on each axis. False: nothing
    /// is used and no block is sent back.
    pub(super) fn guest_use_on_allowed(
        &self,
        host: &mut LanHost,
        gi: usize,
        p: &play::UseItemOn,
    ) -> bool {
        let g = &mut host.hub.guests[gi];
        if !within_reach(g, p.pos, 1.0) {
            return false;
        }
        // The packet's location less Vec3.atCenterOf(pos), as floats from the block corner.
        if p.cursor
            .iter()
            .any(|&c| (f64::from(c) - 0.5).abs() >= HIT_LIMIT)
        {
            return false;
        }
        true
    }

    /// `handleBlockBreakAction`'s checks for START (0), ABORT (2) and STOP (3):
    /// out of reach plus 1 nothing happens but an abort of the block being
    /// dug (`digging`), and above the build limit the block is sent back.
    /// True when the action stops here (its sequence is still acked).
    pub(super) fn reject_guest_break(
        &self,
        host: &mut LanHost,
        gi: usize,
        a: &play::PlayerAction,
        digging: bool,
    ) -> bool {
        let g = &mut host.hub.guests[gi];
        let conn = g.conn;
        if !within_reach(g, a.pos, 1.0) {
            return !(a.action == 2 && digging);
        }
        if a.pos[1] > self.dim.max_y() {
            let state = self.world.block(a.pos[0], a.pos[1], a.pos[2]).0 as i32;
            host.hub.send_to(
                conn,
                &C::BlockUpdate(play::BlockUpdate { pos: a.pos, state }),
            );
            return true;
        }
        false
    }
}
