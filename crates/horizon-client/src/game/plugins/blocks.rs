//! The blocks area: block reads and writes in every level, the block table
//! `Material` and `BlockData` are built from, and the block events: the
//! player's break and place (fired here, from `game.rs`), and those fired by
//! block behaviour in `horizon-world` ([`GameLevel::plugin_level_event`]).
use super::{DIMS, GameHost, Pending, STOP, pend, world_index};
use crate::game::{Dim, Game, GameLevel, lan::LanHost};
use ferraloom_plugins::{
    BlockHost, BreakOutcome, Event, Location, Placing, PlayerHost, PlayerId, ServerHost, Simple,
    WorldHost, listens,
};
use horizon_client::containers::BlockEntity;
use horizon_core::geometry::Direction;
use horizon_protocol::packets::play::{self, Clientbound as C};
use horizon_world::{
    level::{Dimension as LevelDimension, states::BlockId},
    plugin_hooks::{self, Hook, PluginEvent},
    registry::BlockStateId,
};
use std::{cell::Cell, sync::atomic::Ordering};

/// The Bukkit event behind each `horizon-world` hook.
const HOOKS: [(Event, Hook); 12] = [
    (Event::BlockPhysics, Hook::Physics),
    (Event::BlockFromTo, Hook::FromTo),
    (Event::BlockBurn, Hook::Burn),
    (Event::BlockIgnite, Hook::Ignite),
    (Event::BlockGrow, Hook::Grow),
    (Event::BlockFade, Hook::Fade),
    (Event::BlockForm, Hook::Form),
    (Event::BlockSpread, Hook::Spread),
    (Event::LeavesDecay, Hook::LeavesDecay),
    (Event::BlockPistonExtend, Hook::PistonExtend),
    (Event::BlockPistonRetract, Hook::PistonRetract),
    (Event::BlockRedstone, Hook::Redstone),
];

/// Copies the latched event bits into `horizon-world`'s hooks: after each
/// latch (`tick_plugins`, `enable_plugins`, `disable_plugins`).
pub(super) fn sync_level_hooks() {
    plugin_hooks::set(
        HOOKS
            .iter()
            .filter(|(event, _)| listens(*event))
            .map(|&(_, hook)| hook),
    );
}

/// Bukkit's `BlockFace` ordinal of a direction.
fn face(direction: Direction) -> i32 {
    match direction {
        Direction::North => 0,
        Direction::East => 1,
        Direction::South => 2,
        Direction::West => 3,
        Direction::Up => 4,
        Direction::Down => 5,
    }
}

fn level_index(dimension: LevelDimension) -> usize {
    match dimension {
        LevelDimension::Overworld => 0,
        LevelDimension::Nether => 1,
        LevelDimension::End => 2,
    }
}

/// Every block for the bridge (`ferraloom_plugins::table_line`'s format),
/// from the state table and the default state's facts.
fn block_table(game: &Game) -> String {
    let registry = game.pack.assets.registry;
    let behaviours = game.res.behaviours();
    let table = &behaviours.table;
    let mut out = String::new();
    for i in 0..table.block_count() {
        let block = BlockId(i as u16);
        let info = table.info(block);
        let facts = game.res.facts().get(info.default);
        let air = matches!(
            &*info.name,
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        );
        let flags = u32::from(air)
            | u32::from(facts.legacy_solid) << 1
            | u32::from(facts.has_class("FallingBlock")) << 2
            | u32::from(facts.can_occlude) << 3
            | u32::from(facts.block_entity) << 5
            | u32::from(behaviours.data(info.default).ignite_odds > 0) << 6
            | u32::from(facts.requires_tool) << 7
            | u32::from(facts.emission.min(15)) << 8;
        let properties: Vec<(&str, Vec<&str>)> = registry
            .state(info.base)
            .map(|s| {
                s.properties
                    .keys()
                    .filter_map(|key| {
                        let values = table.values(block, table.key_of(key)?);
                        Some((key.as_str(), values.map(|v| &*v.text).collect()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        ferraloom_plugins::table_line(
            &mut out,
            &info.name,
            info.base.0,
            info.count,
            info.default.0,
            flags,
            [facts.hardness, facts.explosion_resistance, facts.friction],
            &facts.item,
            &properties,
        );
    }
    out
}

impl GameHost<'_, '_> {
    /// `f` with `dim`'s level in place, then the level that was (as a
    /// command run in another dimension does); none when it is not loaded.
    pub(super) fn in_level<R>(&mut self, dim: Dim, f: impl FnOnce(&mut Game) -> R) -> Option<R> {
        if dim == self.game.dim {
            return Some(f(self.game));
        }
        if !self.game.levels.parked_dims().contains(&dim) {
            return None;
        }
        let back = self.game.enter_command_level(dim)?;
        let r = f(self.game);
        if let Err(e) = self.game.enter_level(None, back) {
            log::error!("Ferraloom: could not put {} back: {e}", back.id());
        }
        Some(r)
    }
}

impl BlockHost for GameHost<'_, '_> {
    fn block_table(&mut self) -> String {
        block_table(self.game)
    }

    fn block_state(&mut self, world: usize, [x, y, z]: [i32; 3]) -> Option<u32> {
        let w = self.game.level_world(*DIMS.get(world)?)?;
        w.chunks.get(&(x >> 4, z >> 4))?;
        Some(w.block(x, y, z).0)
    }

    fn set_block_state(&mut self, world: usize, pos: [i32; 3], state: u32, physics: bool) -> bool {
        let Some(&dim) = DIMS.get(world) else {
            return false;
        };
        if self.block_state(world, pos).is_none()
            || self
                .game
                .pack
                .assets
                .registry
                .state(BlockStateId(state))
                .is_none()
        {
            return false;
        }
        // `/setblock`'s placement: without physics as its `strict` mode.
        let placed = self.in_level(dim, |g| g.place_state(pos, BlockStateId(state), !physics));
        matches!(placed, Some(Ok(true)))
    }

    fn light(&mut self, world: usize, [x, y, z]: [i32; 3]) -> Option<[u8; 3]> {
        let dim = *DIMS.get(world)?;
        let darken = if dim == self.game.dim {
            self.game.sky_darken()
        } else {
            0
        };
        let w = self.game.level_world(dim)?;
        w.chunks.get(&(x >> 4, z >> 4))?;
        let packed = w.light(x, y, z);
        // Only the Overworld has sky light.
        let sky = if dim == Dim::Overworld {
            packed >> 4
        } else {
            0
        };
        let block = packed & 15;
        let max = block.max((i32::from(sky) - darken).max(0) as u8);
        Some([sky, block, max])
    }

    fn biome(&mut self, world: usize, [x, y, z]: [i32; 3]) -> Option<String> {
        let biomes = self.game.res.biomes().clone();
        let w = self.game.level_world(*DIMS.get(world)?)?;
        let chunk = w.chunks.get(&(x >> 4, z >> 4))?;
        Some(biomes.get(chunk.biome(x, y, z)).name.clone())
    }

    fn break_naturally(&mut self, world: usize, pos: [i32; 3]) -> bool {
        let Some(&dim) = DIMS.get(world) else {
            return false;
        };
        let Some(state) = self.block_state(world, pos) else {
            return false;
        };
        if self.game.res.facts().is_air(BlockStateId(state)) {
            return false;
        }
        let broke = self.in_level(dim, |g| g.destroy_block_dropping(pos).is_ok());
        broke == Some(true)
    }

    fn sign_lines(
        &mut self,
        world: usize,
        [x, y, z]: [i32; 3],
        front: bool,
    ) -> Option<Vec<String>> {
        let w = self.game.level_world(*DIMS.get(world)?)?;
        let Some(BlockEntity::Sign(sign)) = w.block_entities.get(&(x, y, z)) else {
            return None;
        };
        let side = if front { &sign.front } else { &sign.back };
        Some(side.lines.to_vec())
    }

    fn set_sign_lines(
        &mut self,
        world: usize,
        pos: [i32; 3],
        front: bool,
        lines: &[String],
    ) -> bool {
        let Some(&dim) = DIMS.get(world) else {
            return false;
        };
        let key = (pos[0], pos[1], pos[2]);
        let set = self.in_level(dim, |g| {
            let Some(BlockEntity::Sign(sign)) = g.world.block_entities.get_mut(&key) else {
                return false;
            };
            let side = if front {
                &mut sign.front
            } else {
                &mut sign.back
            };
            for (line, new) in side
                .lines
                .iter_mut()
                .zip(lines.iter().chain(std::iter::repeat(&String::new())))
            {
                line.clone_from(new);
            }
            g.mark_block_entity_updated(key);
            true
        });
        set == Some(true)
    }
}

thread_local! {
    /// An explosion no entity caused is running (`World.createExplosion`):
    /// it fires BlockExplodeEvent.
    static BLOCK_EXPLOSION: Cell<bool> = const { Cell::new(false) };
    /// What the last `BlockBreakEvent` left of the break it let go on: its
    /// drops and experience, read by the break as it drops them.
    static BREAK: Cell<Option<BreakOutcome>> = const { Cell::new(None) };
}

impl Game<'_> {
    /// ServerPlayerGameMode.destroyBlock's BlockBreakEvent, for the guest
    /// whose turn it is, before anything breaks: true when cancelled. The
    /// guest's client, which predicted the break, is sent the block (and its
    /// block entity) back. Otherwise the event's drops and experience wait
    /// for [`Game::plugin_break_drops`] and [`Game::plugin_break_exp`].
    pub(in crate::game) fn plugin_block_break(&mut self, pos: [i32; 3]) -> bool {
        BREAK.set(None);
        let (Some(id), Some(conn)) = (self.lan_acting_uuid, self.lan_acting) else {
            return false;
        };
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let exp = self.vanilla_break_exp(state);
        let world = world_index(self.dim);
        // Inside the guest's turn the hub is the one the turn lent, unless it is in place.
        let in_place = self.lan.is_some();
        let outcome = ferraloom_plugins::borrow_lent(|lent: Option<&mut LanHost>| {
            let mut host = GameHost::new(self, lent.filter(|_| !in_place));
            let outcome = ferraloom_plugins::block_break(&mut host, id, world, pos, state.0, exp);
            if outcome.cancelled
                && let Some(hub) = host.hub()
            {
                let update = C::BlockUpdate(play::BlockUpdate {
                    pos,
                    state: state.0 as i32,
                });
                hub.hub.send_to(conn, &update);
            }
            outcome
        });
        if outcome.cancelled {
            // CraftBukkit resends the block entity too: the block log sends both.
            self.resend_block(pos);
            return true;
        }
        BREAK.set(Some(outcome));
        false
    }

    /// Whether a block broken now drops its items: false when its
    /// BlockBreakEvent said `setDropItems(false)`.
    #[inline]
    pub(in crate::game) fn plugin_break_drops(&self) -> bool {
        !listens(Event::BlockBreak) || BREAK.get().is_none_or(|o| o.drop_items)
    }

    /// The experience a block broken now drops when its BlockBreakEvent
    /// decided it (`setExpToDrop`); none to compute vanilla's.
    #[inline]
    pub(in crate::game) fn plugin_break_exp(&self) -> Option<i32> {
        if !listens(Event::BlockBreak) {
            return None;
        }
        BREAK.get().map(|o| o.exp)
    }

    /// `Block.getExpDrop` as CraftBukkit reads it for BlockBreakEvent: a
    /// survival player with the right tool, no silk touch (but on a spawner).
    fn vanilla_break_exp(&mut self, state: BlockStateId) -> i32 {
        let facts = self.res.facts().get(state).clone();
        let held = self.held().cloned();
        let harvest = !facts.requires_tool
            || self
                .res
                .items()
                .correct_for_drops(held.as_ref(), &facts.name);
        let silk = held
            .as_ref()
            .is_some_and(|h| h.enchantment_level("minecraft:silk_touch") > 0);
        if self.creative || !harvest || (silk && &*facts.name != "minecraft:spawner") {
            return 0;
        }
        self.block_experience(&facts.name) as i32
    }

    /// The block at `pos` and its block entity go out again to the players
    /// who see it, with the tick's other block changes.
    fn resend_block(&mut self, pos: [i32; 3]) {
        if let Some(log) = self.world.block_log.as_mut() {
            log.push((pos[0], pos[1], pos[2]));
        }
    }

    /// SignBlockEntity.updateSignText's SignChangeEvent, for the lines a
    /// guest just wrote (`old` the ones before): the event's lines stay,
    /// or the old ones come back when it is cancelled.
    pub(in crate::game) fn plugin_sign_change(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        key: (i32, i32, i32),
        front: bool,
        old: [String; 4],
    ) {
        let Some(id) = host.hub.guest(conn).map(|g| g.uuid) else {
            return;
        };
        let Some(BlockEntity::Sign(sign)) = self.world.block_entities.get(&key) else {
            return;
        };
        let lines = if front { &sign.front } else { &sign.back }.lines.to_vec();
        let world = world_index(self.dim);
        let pos = [key.0, key.1, key.2];
        let kept = ferraloom_plugins::sign_change(
            &mut GameHost::new(self, Some(host)),
            id,
            world,
            pos,
            front,
            &lines,
        );
        let Some(BlockEntity::Sign(sign)) = self.world.block_entities.get_mut(&key) else {
            return;
        };
        let side = if front {
            &mut sign.front
        } else {
            &mut sign.back
        };
        match kept {
            None => side.lines = old,
            Some(kept) => {
                for (line, new) in side.lines.iter_mut().zip(kept) {
                    *line = new;
                }
            }
        }
    }

    /// BlockItem.place's BlockPlaceEvent for the guest whose turn it is,
    /// before the block is placed (CraftBukkit captures the placement and
    /// fires before it takes effect): true when cancelled or `canBuild` was
    /// left false. The guest's client, which predicted the placement, is
    /// sent the blocks and its inventory back.
    pub(in crate::game) fn plugin_block_place(
        &mut self,
        ctx: &horizon_world::level::PlaceContext,
    ) -> bool {
        let Some(id) = self.lan_acting_uuid else {
            return false;
        };
        let placement = self.with_level(|b, level, ticks| b.placement_state(level, ticks, ctx));
        let Some((at, placed)) = placement else {
            return false;
        };
        let pos = [at.x, at.y, at.z];
        let against = [ctx.clicked_pos.x, ctx.clicked_pos.y, ctx.clicked_pos.z];
        let placing = Placing {
            player: id,
            world: world_index(self.dim),
            pos,
            placed: placed.0,
            replaced: self.world.block(pos[0], pos[1], pos[2]).0,
            against,
            hand: i32::from(self.hand == horizon_client::first_person::Hand::Off),
            can_build: true,
        };
        let in_place = self.lan.is_some();
        let cancelled = ferraloom_plugins::borrow_lent(|lent: Option<&mut LanHost>| {
            let mut host = GameHost::new(self, lent.filter(|_| !in_place));
            ferraloom_plugins::block_place(&mut host, &placing)
        });
        if cancelled {
            self.resend_block(pos);
            self.resend_block(against);
            pend(Pending::Inventory(id));
        }
        cancelled
    }
}

/// Runs `f`, an explosion no entity causes (CraftWorld.createExplosion
/// with no source), so it fires BlockExplodeEvent.
pub(super) fn as_block_explosion<R>(f: impl FnOnce() -> R) -> R {
    let outer = BLOCK_EXPLOSION.replace(true);
    let r = f();
    BLOCK_EXPLOSION.set(outer);
    r
}

impl Game<'_> {
    /// ServerExplosion.explode's BlockExplodeEvent, for an explosion no
    /// entity caused, before any block is destroyed: `targets` become the
    /// blocks the event left; true when cancelled (nothing more happens).
    /// The event's yield is not applied yet: drops follow the explosion's
    /// block interaction.
    pub(in crate::game) fn plugin_block_explode(
        &mut self,
        center: [f64; 3],
        radius: f32,
        targets: &mut Vec<[i32; 3]>,
    ) -> bool {
        if !BLOCK_EXPLOSION.get() {
            return false;
        }
        let pos = center.map(|c| c.floor() as i32);
        let state = self.world.block(pos[0], pos[1], pos[2]).0;
        let world = world_index(self.dim);
        let mut host = GameHost::new(self, None);
        let yield_ = 1.0 / radius.max(f32::MIN_POSITIVE);
        match ferraloom_plugins::block_explode(&mut host, world, pos, state, targets, yield_) {
            None => true,
            Some((kept, _)) => {
                *targets = kept;
                false
            }
        }
    }
}

impl GameLevel<'_> {
    /// [`horizon_world::level::LevelAccess::plugin_event`] for the server's
    /// levels: the event goes to plugins with this level as their host.
    /// Plugins read the level's blocks during it; what they would change
    /// elsewhere waits for the next tick (`Pending`), and block writes are
    /// refused (the level is mid-update).
    pub(in crate::game) fn plugin_level_event(&mut self, event: PluginEvent) -> i32 {
        let pass = match event {
            PluginEvent::Redstone { new, .. } => new,
            _ => 0,
        };
        if self.client_side || !ferraloom_plugins::running() {
            return pass;
        }
        let world = level_index(self.dimension);
        let p = |b: horizon_core::position::BlockPos| [b.x, b.y, b.z];
        let host = &mut LevelHost(self);
        let simple = |host: &mut LevelHost, kind, pos, args| {
            i32::from(ferraloom_plugins::cancelled(host, kind, world, pos, args))
        };
        match event {
            PluginEvent::Physics { pos, state, source } => simple(
                host,
                Simple::Physics,
                p(pos),
                [state.0 as i32, source.x, source.y, source.z, 0],
            ),
            PluginEvent::FromTo { from, to, face: f } => simple(
                host,
                Simple::FromTo,
                p(from),
                [face(f), to.x, to.y, to.z, 0],
            ),
            PluginEvent::Burn { pos, fire } => {
                simple(host, Simple::Burn, p(pos), [1, fire.x, fire.y, fire.z, 0])
            }
            PluginEvent::Ignite { pos, cause, source } => {
                let s = source.map_or([0; 3], p);
                let args = [cause as i32, s[0], s[1], s[2], i32::from(source.is_some())];
                simple(host, Simple::Ignite, p(pos), args)
            }
            PluginEvent::Grow { pos, to } => {
                simple(host, Simple::Grow, p(pos), [to.0 as i32, 0, 0, 0, 0])
            }
            PluginEvent::Fade { pos, to } => {
                simple(host, Simple::Fade, p(pos), [to.0 as i32, 0, 0, 0, 0])
            }
            PluginEvent::Form { pos, to } => {
                simple(host, Simple::Form, p(pos), [to.0 as i32, 0, 0, 0, 0])
            }
            PluginEvent::Spread { pos, source, to } => simple(
                host,
                Simple::Spread,
                p(pos),
                [to.0 as i32, source.x, source.y, source.z, 0],
            ),
            PluginEvent::LeavesDecay { pos } => simple(host, Simple::LeavesDecay, p(pos), [0; 5]),
            PluginEvent::Piston {
                pos,
                direction,
                extending,
                moved,
            } => {
                let moved: Vec<[i32; 3]> = moved.iter().map(|&b| p(b)).collect();
                let cancelled = ferraloom_plugins::piston(
                    host,
                    world,
                    p(pos),
                    face(direction),
                    extending,
                    &moved,
                );
                i32::from(cancelled)
            }
            PluginEvent::Redstone { pos, old, new } => ferraloom_plugins::simple(
                host,
                Simple::Redstone,
                world,
                p(pos),
                [old, new, 0, 0, 0],
                new,
            ),
        }
    }
}

/// The host of an event fired by block behaviour: the level being updated,
/// read in place. The hub is in `Game::lan` or lent elsewhere, so messages,
/// teleports and commands wait for the next tick, as they do for a guest's turn.
struct LevelHost<'a, 'w>(&'a mut GameLevel<'w>);

impl LevelHost<'_, '_> {
    fn here(&self, world: usize) -> bool {
        world == level_index(self.0.dimension)
    }
}

impl BlockHost for LevelHost<'_, '_> {
    fn block_state(&mut self, world: usize, [x, y, z]: [i32; 3]) -> Option<u32> {
        if !self.here(world) {
            return None;
        }
        self.0.world.chunks.get(&(x >> 4, z >> 4))?;
        Some(self.0.world.block(x, y, z).0)
    }

    fn light(&mut self, world: usize, [x, y, z]: [i32; 3]) -> Option<[u8; 3]> {
        if !self.here(world) {
            return None;
        }
        self.0.world.chunks.get(&(x >> 4, z >> 4))?;
        let packed = self.0.world.light(x, y, z);
        let sky = if self.0.dimension == LevelDimension::Overworld {
            packed >> 4
        } else {
            0
        };
        let block = packed & 15;
        let max = block.max((i32::from(sky) - self.0.darken).max(0) as u8);
        Some([sky, block, max])
    }

    fn biome(&mut self, world: usize, [x, y, z]: [i32; 3]) -> Option<String> {
        if !self.here(world) {
            return None;
        }
        let chunk = self.0.world.chunks.get(&(x >> 4, z >> 4))?;
        Some(self.0.biomes.get(chunk.biome(x, y, z)).name.clone())
    }
}

impl WorldHost for LevelHost<'_, '_> {
    /// As [`GameHost`]'s: the day time, whole or within its day.
    fn world_time(&mut self, _world: usize, full: bool) -> i64 {
        let day = self.0.world.day_time;
        if full { day } else { day.rem_euclid(24_000) }
    }
}

impl PlayerHost for LevelHost<'_, '_> {
    fn send_message(&mut self, player: PlayerId, message: &str) {
        pend(Pending::Message(player, message.to_owned()));
    }

    fn location(&mut self, player: PlayerId) -> Option<Location> {
        let g = self
            .0
            .lan?
            .hub
            .guests
            .iter()
            .find(|g| g.uuid == player && g.placed)?;
        Some(Location {
            world: world_index(Dim::from_id(&g.dimension)),
            x: g.position[0],
            y: g.position[1],
            z: g.position[2],
            yaw: g.yaw,
            pitch: g.pitch,
        })
    }

    fn teleport(&mut self, player: PlayerId, to: Location) -> bool {
        if DIMS.get(to.world).is_none() || self.location(player).is_none() {
            return false;
        }
        pend(Pending::Teleport(player, to));
        true
    }
}

// Inventory area: no inventory is reached from inside a level's tick; every
// InventoryHost method answers its default (nothing there).
impl ferraloom_plugins::InventoryHost for LevelHost<'_, '_> {}
// Entities: not reachable inside a level's block tick (every read throws, as a restricted host's).
impl ferraloom_plugins::EntityHost for LevelHost<'_, '_> {}

impl ServerHost for LevelHost<'_, '_> {
    fn console_message(&mut self, message: &str) {
        crate::game::dedicated::log("INFO", &horizon_client::text::Text::legacy(message).plain());
    }

    fn dispatch_vanilla(&mut self, line: &str) -> bool {
        pend(Pending::Vanilla(line.to_owned()));
        true
    }

    fn shutdown(&mut self) {
        STOP.store(true, Ordering::Relaxed);
    }
}
