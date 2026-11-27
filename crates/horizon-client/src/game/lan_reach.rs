//! Ferraloom: the checks `ServerGamePacketListenerImpl.handleUseItemOn` and
//! `ServerPlayerGameMode.handleBlockBreakAction` make before a server
//! player's click on a block runs: their reach (the
//! `block_interaction_range` attribute, plus 1), where on the block the click
//! landed, and the build limits with their "Height limit" message. A few
//! comparisons per packet; nothing runs per tick.
use super::{Game, lan::LanHost};
use horizon_client::{items::ItemStack, lan::guest::Guest};
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

/// `ServerPlayer.sendBuildLimitMessage`: the red overlay line.
fn build_limit_message(host: &LanHost, conn: u32, too_high: bool, limit: i32) {
    let key = if too_high {
        "build.tooHigh"
    } else {
        "build.tooLow"
    };
    let message = horizon_client::text::Text::translate(
        key,
        &[horizon_client::text::Text::literal(limit.to_string())],
    )
    .with_color(horizon_client::text::RED);
    host.hub.send_to(
        conn,
        &C::SystemChat(play::SystemChat {
            content: horizon_client::lan::data::component_nbt(&message.component()),
            overlay: true,
        }),
    );
}

impl Game<'_> {
    /// `handleUseItemOn` before the use: within reach plus 1, the click
    /// within 1.0000001 of the block's centre on each axis, and inside the
    /// build limits (outside them the player is told why). False: nothing
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
        // Written as vanilla's `abs < limit` must hold, so NaN is refused.
        if !p
            .cursor
            .iter()
            .all(|&c| (f64::from(c) - 0.5).abs() < HIT_LIMIT)
        {
            return false;
        }
        let conn = g.conn;
        let (min_y, max_y) = (self.dim.min_y(), self.dim.max_y());
        if p.pos[1] > max_y {
            build_limit_message(host, conn, true, max_y);
            return false;
        }
        if p.pos[1] < min_y {
            build_limit_message(host, conn, false, min_y);
            return false;
        }
        true
    }

    /// `handleUseItemOn` after the use: a block placement attempt
    /// (`wasBlockPlacementAttempt`) that didn't consume the click, up from
    /// the top layer or down from the bottom one, is told of the limit; up
    /// from the top twice, as vanilla checks it twice.
    pub(super) fn guest_use_on_limit(
        &self,
        host: &LanHost,
        gi: usize,
        p: &play::UseItemOn,
        consumed: bool,
    ) {
        let g = &host.hub.guests[gi];
        let held = if p.hand == 1 {
            g.offhand.as_ref()
        } else {
            g.inventory[g.selected].as_ref()
        };
        if consumed || !held.is_some_and(|s| self.placement_attempt(host, g.conn, s)) {
            return;
        }
        let (min_y, max_y) = (self.dim.min_y(), self.dim.max_y());
        // Direction 1 up, 0 down.
        if p.face == 1 && p.pos[1] >= max_y {
            build_limit_message(host, g.conn, true, max_y);
            build_limit_message(host, g.conn, true, max_y);
        } else if p.face == 0 && p.pos[1] <= min_y {
            build_limit_message(host, g.conn, false, min_y);
        }
    }

    /// Port of ServerGamePacketListenerImpl#wasBlockPlacementAttempt: a block
    /// item or a bucket with a fluid, off cooldown.
    fn placement_attempt(&self, host: &LanHost, conn: u32, stack: &ItemStack) -> bool {
        let id = &*stack.item;
        let block = self.res.items().get(id).is_some_and(|i| i.block.is_some());
        let bucket =
            id.ends_with("_bucket") && id != "minecraft:bucket" && id != "minecraft:milk_bucket";
        let group = horizon_client::cooldowns::group(stack, None);
        let cooling = host
            .play
            .get(&conn)
            .is_some_and(|p| p.cooldowns.is_on_cooldown(&group));
        (block || bucket) && !cooling
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
