//! The worlds and blocks area: world time, block reads, block events.
use super::{GameHost, world_index};
use crate::game::{Game, lan::LanHost};
use ferraloom_plugins::WorldHost;
use horizon_protocol::packets::play::{self, Clientbound as C};
use horizon_world::registry::BlockStateId;

/// `minecraft:oak_log[axis=y]`.
fn state_name(game: &Game, state: BlockStateId) -> String {
    let Some(s) = game.pack.assets.registry.state(state) else {
        return "minecraft:air".into();
    };
    if s.properties.is_empty() {
        return s.name.clone();
    }
    let props: Vec<String> = s
        .properties
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    format!("{}[{}]", s.name, props.join(","))
}

impl WorldHost for GameHost<'_, '_> {
    fn world_time(&mut self, _world: usize, full: bool) -> i64 {
        // ServerLevel.getGameTime and getDayTime: every level reads the server's.
        if full {
            self.game.world.game_time
        } else {
            self.game.world.day_time
        }
    }

    /// The level in place only; another dimension's reads as unloaded.
    fn block_at(&mut self, world: usize, [x, y, z]: [i32; 3]) -> Option<String> {
        if world != world_index(self.game.dim) {
            return None;
        }
        self.game.world.chunks.get(&(x >> 4, z >> 4))?;
        let state = self.game.world.block(x, y, z);
        Some(state_name(self.game, state))
    }
}

impl Game<'_> {
    /// ServerPlayerGameMode.destroyBlock's BlockBreakEvent, for the guest
    /// whose turn it is, before anything breaks: true when cancelled. The
    /// guest's client, which predicted the break, is sent the block back.
    pub(in crate::game) fn plugin_block_break(&mut self, pos: [i32; 3]) -> bool {
        let (Some(id), Some(conn)) = (self.lan_acting_uuid, self.lan_acting) else {
            return false;
        };
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let name = state_name(self, state);
        let world = world_index(self.dim);
        // Inside the guest's turn the hub is the one the turn lent, unless it is in place.
        let in_place = self.lan.is_some();
        ferraloom_plugins::borrow_lent(|lent: Option<&mut LanHost>| {
            let mut host = GameHost::new(self, lent.filter(|_| !in_place));
            let cancelled = ferraloom_plugins::block_break(&mut host, id, world, pos, &name);
            if cancelled && let Some(hub) = host.hub() {
                let update = C::BlockUpdate(play::BlockUpdate {
                    pos,
                    state: state.0 as i32,
                });
                hub.hub.send_to(conn, &update);
            }
            cancelled
        })
    }
}
