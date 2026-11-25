//! The entities area: entity reads and changes, and the events of damage,
//! death, spawning, targeting, explosions and using an entity.
//!
//! Two hosts answer entities. [`GameHost`] holds the whole game, where a site
//! has it (a mob's death, a spawn, a packet). [`SiteHost`] is the restricted
//! one for events fired inside mob code (`mobs/plugin_sites.rs`), where only
//! the mob in hand is reachable, and in the mob pass's serial turn the pass's
//! own view (the snapshot of the other mobs and the player each mob sees):
//! it reads those, queues changes to other entities for after the pass, and
//! answers `Elsewhere` for the rest.
//!
//! Costs: none of this runs without a listener but [`Game::plugin_level_tick`]
//! and [`Game::plugin_after_mob_pass`], one `running()` check each without
//! plugins. With a damage listener, the level tick also records each mob's
//! type by id (one map entry per loaded mob per level tick), so a mob named
//! as an attacker inside mob code gets its Bukkit type.
use super::{DIMS, GameHost, Pending, STOP, pend, world_index};
use crate::game::{Dim, Game, Mob, lan::LanHost};
use ferraloom_plugins::{
    Actor, Change, EntityHost, EntityRef, EntityState, Event, Location, Named, PlayerHost,
    PlayerId, Reach, ServerHost, SpawnReason, Targeted, WorldHost, listens,
};
use horizon_client::{
    fast_hash::FxHashMap,
    mobs::{Damage, DamageKind, MobWorld, Who, plugin_sites},
};
use horizon_core::geometry::{Aabb, Vec3};
use std::cell::{Cell, RefCell};

thread_local! {
    /// The guest whose turn runs (`as_guest`), and their world: who
    /// `Who::Player` is inside mob code outside the pass.
    static ACTING: Cell<Option<(PlayerId, usize)>> = const { Cell::new(None) };
    /// The level in place, as of its tick's start.
    static IN_PLACE: Cell<usize> = const { Cell::new(0) };
    /// The game time and time of day, as of the level tick's start.
    static TIME: Cell<(i64, i64)> = const { Cell::new((0, 0)) };
    /// Each loaded mob's type by id, while a plugin listens to damage.
    static KINDS: RefCell<FxHashMap<u32, &'static str>> = RefCell::new(FxHashMap::default());
    /// The mob dying now and the guest who killed it, for `EntityDeathEvent`.
    static KILLER: Cell<Option<(u32, Option<u32>)>> = const { Cell::new(None) };
    /// The guest whose melee hit on another player lands now (`pvp_land`).
    static PVP: Cell<Option<u32>> = const { Cell::new(None) };
    /// The reason the next `spawn_mob_for` spawns with, when not its default.
    static REASON: Cell<Option<SpawnReason>> = const { Cell::new(None) };
}

/// Installs the sites inside mob code, once plugins run.
pub(super) fn install() {
    plugin_sites::install(plugin_sites::Sites {
        damage: site_damage,
        target: site_target,
        change_block: site_change_block,
    });
}

/// The guest `uuid`'s turn in `world`, until the guard drops (`as_guest`).
/// Without plugins, one check.
pub(in crate::game) fn acting(uuid: Option<PlayerId>, dim: Dim) -> impl Drop {
    struct Restore(Option<(PlayerId, usize)>, bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            if self.1 {
                ACTING.set(self.0);
            }
        }
    }
    if !ferraloom_plugins::running() {
        return Restore(None, false);
    }
    Restore(ACTING.replace(uuid.map(|u| (u, world_index(dim)))), true)
}

/// Guest `conn`'s hit on another player lands, until the guard drops
/// (`pvp_land`). Without plugins, one check.
pub(in crate::game) fn pvp_attacker(conn: Option<u32>) -> impl Drop {
    struct Restore(Option<u32>, bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            if self.1 {
                PVP.set(self.0);
            }
        }
    }
    if !ferraloom_plugins::running() {
        return Restore(None, false);
    }
    Restore(PVP.replace(conn), true)
}

/// `EntityDamageEvent.DamageCause` for a damage type's key, as CraftBukkit maps them.
fn cause_of_type(key: &str) -> &'static str {
    match key.strip_prefix("minecraft:").unwrap_or(key) {
        "player_attack"
        | "mob_attack"
        | "mob_attack_no_aggro"
        | "sting"
        | "spear"
        | "mace_smash" => "ENTITY_ATTACK",
        "arrow" | "trident" | "thrown" | "fireball" | "wither_skull" | "mob_projectile"
        | "wind_charge" => "PROJECTILE",
        "explosion" | "player_explosion" => "ENTITY_EXPLOSION",
        "bad_respawn_point" => "BLOCK_EXPLOSION",
        "fall" | "ender_pearl" => "FALL",
        "falling_block" | "falling_anvil" | "falling_stalactite" => "FALLING_BLOCK",
        "on_fire" => "FIRE_TICK",
        "in_fire" | "campfire" => "FIRE",
        "lava" => "LAVA",
        "hot_floor" => "HOT_FLOOR",
        "drown" => "DROWNING",
        "starve" => "STARVATION",
        "magic" | "indirect_magic" => "MAGIC",
        "wither" => "WITHER",
        "dragon_breath" => "DRAGON_BREATH",
        "cactus" | "sweet_berry_bush" => "CONTACT",
        "in_wall" => "SUFFOCATION",
        "cramming" => "CRAMMING",
        "fly_into_wall" => "FLY_INTO_WALL",
        "thorns" => "THORNS",
        "sonic_boom" => "SONIC_BOOM",
        "freeze" => "FREEZE",
        "lightning_bolt" => "LIGHTNING",
        "out_of_world" => "VOID",
        "dry_out" => "DRYOUT",
        "generic_kill" => "KILL",
        _ => "CUSTOM",
    }
}

/// Mob `id` dies now (`mob_died`), killed by the guest on `conn`: for its
/// death event, which reads its killer after the game forgot it.
pub(in crate::game) fn note_killer(id: u32, conn: Option<u32>) {
    if ferraloom_plugins::running() {
        KILLER.set(Some((id, conn)));
    }
}

/// `EntityDamageEvent.DamageCause` for a damage kind, as CraftBukkit maps them.
fn cause(d: &Damage) -> &'static str {
    use DamageKind as K;
    match d.kind {
        K::PlayerAttack | K::MobAttack | K::Spear | K::MaceSmash | K::Sting => "ENTITY_ATTACK",
        K::Arrow | K::Thrown | K::Fireball => "PROJECTILE",
        K::Explosion if d.attacker.is_some() => "ENTITY_EXPLOSION",
        K::Explosion => "BLOCK_EXPLOSION",
        K::Fall => "FALL",
        K::FallingBlock => "FALLING_BLOCK",
        K::OnFire => "FIRE_TICK",
        K::InFire => "FIRE",
        K::Lava => "LAVA",
        K::Drown => "DROWNING",
        K::DryOut => "DRYOUT",
        K::Magic => "MAGIC",
        K::Cactus | K::SweetBerryBush => "CONTACT",
        K::Generic => "CUSTOM",
        K::InWall => "SUFFOCATION",
        K::Thorns => "THORNS",
        K::SonicBoom => "SONIC_BOOM",
        K::Freeze => "FREEZE",
        K::OutOfWorld => "VOID",
    }
}

fn kind_of(id: u32) -> Option<&'static str> {
    KINDS.with_borrow(|k| k.get(&id).copied())
}

/// The world a pass's view is of.
fn pass_world(w: &MobWorld) -> usize {
    if w.nether {
        world_index(Dim::Nether)
    } else if w.end {
        world_index(Dim::End)
    } else {
        world_index(Dim::Overworld)
    }
}

/// `mob`'s view of `who` in the pass: the player it sees, or another mob of the snapshot.
fn pass_named(w: &MobWorld, who: Who) -> Option<Named> {
    match who {
        Who::Player => w
            .player
            .as_ref()
            .map(|p| Named::player(plugin_sites::uuid(p.uuid))),
        Who::Mob(id) => w
            .others
            .iter()
            .find(|s| s.id == id)
            .map(|s| Named::entity(id, horizon_client::mobs::species::def(s.species).name)),
    }
}

/// `who` inside mob code outside the pass: the acting guest, or a mob whose
/// type the level tick recorded.
fn site_named(who: Who) -> Option<Named> {
    match who {
        Who::Player => ACTING.get().map(|(id, _)| Named::player(id)),
        Who::Mob(id) => kind_of(id).map(|kind| Named::entity(id, kind)),
    }
}

/// `Mob::damage_with`'s site: the damage to deal, none when cancelled. The
/// armor and enchantment reductions cross as one ARMOR modifier.
fn site_damage(m: &mut Mob, source: &Damage, base: f32, amount: f32) -> Option<f32> {
    let damager = source.attacker.and_then(site_named);
    let player_source = source.attacker == Some(Who::Player);
    let base = f64::from(base);
    let dealt = f64::from(amount);
    let d = ferraloom_plugins::Damage {
        victim: plugin_sites::named(m),
        damager,
        cause: cause(source),
        damage_type: crate::game::creatures::damage_type(source.kind, player_source),
        base,
        armor: if base > 0.0 {
            (1.0 - dealt / base).clamp(0.0, 1.0)
        } else {
            0.0
        },
        magic: 0.0,
        dealt,
    };
    let world = ACTING.get().map_or_else(|| IN_PLACE.get(), |(_, w)| w);
    let mut host = SiteHost {
        mob: m,
        world,
        pass: None,
    };
    ferraloom_plugins::entity_damage(&mut host, &d).map(|d| d as f32)
}

/// `EntityTargetEvent.TargetReason` for a change of target from `was` to `now`.
fn target_reason(m: &Mob, w: &MobWorld, was: Option<Who>, now: Option<Who>) -> &'static str {
    match now {
        Some(t) if m.ai.last_hurt_by == Some(t) => "TARGET_ATTACKED_ENTITY",
        Some(Who::Player) => "CLOSEST_PLAYER",
        Some(Who::Mob(_)) => "CLOSEST_ENTITY",
        None => {
            let died = match was {
                Some(Who::Mob(id)) => w.others.iter().any(|s| s.id == id && !s.alive),
                Some(Who::Player) => w.player.as_ref().is_some_and(|p| !p.alive),
                None => false,
            };
            if died { "TARGET_DIED" } else { "FORGOT_TARGET" }
        }
    }
}

/// The mob pass's `EntityTargetEvent` site, in `m`'s serial turn.
fn site_target(m: &mut Mob, was: Option<Who>, w: &MobWorld) -> Option<Who> {
    let now = m.ai.target;
    let reason = target_reason(m, w, was, now);
    let (mob, target) = (plugin_sites::named(m), now.and_then(|t| pass_named(w, t)));
    let mut host = SiteHost {
        mob: m,
        world: pass_world(w),
        pass: Some(w),
    };
    match ferraloom_plugins::entity_target(&mut host, mob, target, reason) {
        // A handler may have set the target itself (Mob.setTarget).
        Targeted::Kept => host.mob.ai.target,
        Targeted::Cancelled => was,
        Targeted::Changed(t) => t.map(|t| match t {
            Actor::Player(_) => Who::Player,
            Actor::Entity(e) => Who::Mob(e.id),
        }),
    }
}

/// The mob pass's `EntityChangeBlockEvent` site, at `m`'s commit.
fn site_change_block(m: &mut Mob, pos: [i32; 3], to: &str, w: &MobWorld) -> bool {
    let entity = plugin_sites::named(m);
    let world = pass_world(w);
    let mut host = SiteHost {
        mob: m,
        world,
        pass: Some(w),
    };
    ferraloom_plugins::entity_change_block(&mut host, entity, world, pos, to)
}

/// The restricted host of events fired inside mob code: the mob in hand,
/// and in the mob pass its view of the others.
struct SiteHost<'a, 'w> {
    mob: &'a mut Mob,
    world: usize,
    pass: Option<&'a MobWorld<'w>>,
}

impl SiteHost<'_, '_> {
    fn named(&self, who: Who) -> Option<Named> {
        match self.pass {
            Some(w) => pass_named(w, who),
            None => site_named(who),
        }
    }
}

impl EntityHost for SiteHost<'_, '_> {
    fn entity(&mut self, e: EntityRef) -> Reach<EntityState> {
        if plugin_sites::is(e, self.mob) {
            let target = self.mob.ai.target.and_then(|t| self.named(t));
            return Reach::Here(plugin_sites::state(self.mob, self.world, |_| target));
        }
        let Some(w) = self.pass else {
            return Reach::Elsewhere;
        };
        let def = |s: &horizon_client::mobs::Seen| horizon_client::mobs::species::def(s.species);
        match w.others.iter().find(|s| e.is(s.id, def(s).name)) {
            Some(s) => Reach::Here(plugin_sites::seen_state(s, self.world, |t| {
                pass_named(w, t)
            })),
            None if w.others.iter().any(|s| s.id == e.id) => Reach::Gone,
            None => Reach::Elsewhere,
        }
    }

    /// The mob in hand changes at once; another entity after the pass, in mob order.
    fn change(&mut self, e: EntityRef, change: Change) -> Reach<bool> {
        if plugin_sites::is(e, self.mob) {
            return Reach::Here(plugin_sites::apply(self.mob, self.world, change));
        }
        plugin_sites::queue(e, change);
        Reach::Here(true)
    }

    fn entities(&mut self, _: usize, _: Option<[f64; 6]>) -> Reach<Vec<Named>> {
        Reach::Elsewhere
    }

    fn spawn(&mut self, _: &str, _: Location) -> Reach<Option<Named>> {
        Reach::Elsewhere
    }
}

impl PlayerHost for SiteHost<'_, '_> {
    fn send_message(&mut self, player: PlayerId, message: &str) {
        pend(Pending::Message(player, message.to_owned()));
    }

    /// The player the pass's mob sees; else the hub's record, when it is lent.
    fn location(&mut self, player: PlayerId) -> Option<Location> {
        if let Some(p) = self.pass.and_then(|w| w.player.as_ref())
            && plugin_sites::uuid(p.uuid) == player
        {
            return Some(Location {
                world: self.world,
                x: p.position.x,
                y: p.position.y,
                z: p.position.z,
                yaw: p.yaw,
                pitch: 0.0,
            });
        }
        ferraloom_plugins::borrow_lent(|hub: Option<&mut LanHost>| {
            let g = hub?
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
        })
    }

    fn teleport(&mut self, player: PlayerId, to: Location) -> bool {
        pend(Pending::Teleport(player, to));
        true
    }
}

impl WorldHost for SiteHost<'_, '_> {
    fn world_time(&mut self, _: usize, full: bool) -> i64 {
        let (game, day) = match self.pass {
            Some(w) => (w.game_time, w.day_time),
            None => TIME.get(),
        };
        if full { game } else { day }
    }

    /// The pass's blocks; outside the pass, none (they read as unloaded).
    fn block_at(&mut self, world: usize, [x, y, z]: [i32; 3]) -> Option<String> {
        let w = self.pass.filter(|_| world == self.world)?;
        w.world.chunks.get(&(x >> 4, z >> 4))?;
        let state = w.world.block(x, y, z);
        let s = w.registry.state(state)?;
        if s.properties.is_empty() {
            return Some(s.name.clone());
        }
        let props: Vec<String> = s
            .properties
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        Some(format!("{}[{}]", s.name, props.join(",")))
    }
}

impl ServerHost for SiteHost<'_, '_> {
    fn console_message(&mut self, message: &str) {
        crate::game::dedicated::log("INFO", &horizon_client::text::Text::legacy(message).plain());
    }

    fn dispatch_vanilla(&mut self, line: &str) -> bool {
        pend(Pending::Vanilla(line.to_owned()));
        true
    }

    fn shutdown(&mut self) {
        STOP.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// An item entity's handle type.
const ITEM: &str = "item";

impl GameHost<'_, '_> {
    /// The UUID of the guest on connection `conn`.
    fn guest_uuid(&mut self, conn: u32) -> Option<PlayerId> {
        let hub = self.hub()?;
        hub.hub
            .guests
            .iter()
            .find(|g| g.conn == conn)
            .map(|g| g.uuid)
    }

    /// `who` as mob `mob` names it: the guest it sees, or another mob.
    fn named_who(&mut self, mob: u32, who: Who) -> Option<Named> {
        match who {
            Who::Player => {
                let bound = self.hub().and_then(|h| h.mob_players.get(&mob).copied());
                match bound {
                    Some(conn) => self.guest_uuid(conn).map(Named::player),
                    None => self.game.lan_acting_uuid.map(Named::player),
                }
            }
            Who::Mob(id) => {
                let m = self.game.mobs.iter().find(|m| m.id == id && !m.removed)?;
                Some(plugin_sites::named(m))
            }
        }
    }

    fn mob_index(&self, e: EntityRef) -> Option<usize> {
        self.game.mobs.iter().position(|m| plugin_sites::is(e, m))
    }
}

impl EntityHost for GameHost<'_, '_> {
    /// The level in place only: an entity of another reads as gone.
    fn entity(&mut self, e: EntityRef) -> Reach<EntityState> {
        let world = world_index(self.game.dim);
        if let Some(i) = self.mob_index(e) {
            let (id, target) = (self.game.mobs[i].id, self.game.mobs[i].ai.target);
            let target = target.and_then(|t| self.named_who(id, t));
            let dying = KILLER.get().filter(|(dying, _)| *dying == id);
            let killer = (self.game.mobs[i].ai.last_hurt_by_player > 0)
                .then(|| match dying {
                    Some((_, conn)) => conn,
                    None => self.game.last_hurt_by_player.get(&id).copied().flatten(),
                })
                .flatten()
                .and_then(|conn| self.guest_uuid(conn));
            let mut s = plugin_sites::state(&self.game.mobs[i], world, |_| target);
            s.killer = killer;
            return Reach::Here(s);
        }
        if e.generation == EntityRef::new(0, ITEM).generation
            && let Some(item) = self.game.dropped.iter().find(|d| d.id == e.id)
        {
            let mut s = EntityState::at(Location {
                world,
                x: item.position.x,
                y: item.position.y,
                z: item.position.z,
                yaw: 0.0,
                pitch: 0.0,
            });
            s.velocity = [item.velocity.x, item.velocity.y, item.velocity.z];
            s.flags |= if item.on_ground {
                ferraloom_plugins::entity_flags::ON_GROUND
            } else {
                0
            };
            s.fire_ticks = item.fire_ticks;
            s.ticks_lived = item.age as i32;
            return Reach::Here(s);
        }
        Reach::Gone
    }

    fn change(&mut self, e: EntityRef, change: Change) -> Reach<bool> {
        let world = world_index(self.game.dim);
        if let Some(i) = self.mob_index(e) {
            if let Change::Teleport(to) = change
                && to.world != world
            {
                return Reach::Here(false);
            }
            return Reach::Here(plugin_sites::apply(&mut self.game.mobs[i], world, change));
        }
        if e.generation == EntityRef::new(0, ITEM).generation
            && let Some(i) = self.game.dropped.iter().position(|d| d.id == e.id)
        {
            if change == Change::Remove {
                self.game.dropped.remove(i);
                return Reach::Here(true);
            }
            let item = &mut self.game.dropped[i];
            match change {
                Change::Velocity([x, y, z]) => item.velocity = Vec3::new(x, y, z),
                Change::Teleport(to) if to.world == world => {
                    item.position = Vec3::new(to.x, to.y, to.z);
                }
                Change::FireTicks(t) => item.fire_ticks = t,
                _ => return Reach::Here(false),
            }
            return Reach::Here(true);
        }
        Reach::Gone
    }

    /// The level in place's mobs, items and players; nothing for another world.
    fn entities(&mut self, world: usize, within: Option<[f64; 6]>) -> Reach<Vec<Named>> {
        if world != world_index(self.game.dim) {
            return Reach::Gone;
        }
        let area =
            within.map(|b| Aabb::new(Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5])));
        let meets = |b: Aabb| area.is_none_or(|a| a.intersects(b));
        let mut found: Vec<Named> = Vec::new();
        let dim = self.game.dim;
        let acting = self.game.lan_acting_uuid;
        if let Some(acting) = acting
            && let Some(p) = self.game.player.as_ref()
            && meets(Aabb::new(p.position, p.position))
        {
            found.push(Named::player(acting));
        }
        if let Some(hub) = self.hub() {
            for g in hub
                .hub
                .guests
                .iter()
                .filter(|g| g.placed && Some(g.uuid) != acting)
            {
                let at = Vec3::new(g.position[0], g.position[1], g.position[2]);
                if Dim::from_id(&g.dimension) == dim && meets(Aabb::new(at, at)) {
                    found.push(Named::player(g.uuid));
                }
            }
        }
        for m in self
            .game
            .mobs
            .iter()
            .filter(|m| !m.removed && meets(m.bounds()))
        {
            found.push(plugin_sites::named(m));
        }
        for d in self.game.dropped.iter().filter(|d| meets(d.bounds())) {
            found.push(Named::entity(d.id, ITEM));
        }
        Reach::Here(found)
    }

    /// Mobs only yet; CreatureSpawnEvent's reason is CUSTOM.
    fn spawn(&mut self, kind: &str, at: Location) -> Reach<Option<Named>> {
        if at.world != world_index(self.game.dim) || DIMS.get(at.world).is_none() {
            return Reach::Elsewhere;
        }
        REASON.set(Some(SpawnReason::Custom));
        let id = self
            .game
            .spawn_mob_for(kind, [at.x, at.y, at.z], false, true);
        REASON.set(None);
        let Some(m) = id.and_then(|id| self.game.mobs.iter_mut().find(|m| m.id == id)) else {
            return Reach::Here(None);
        };
        m.yaw = at.yaw;
        m.head_pitch = at.pitch;
        Reach::Here(Some(plugin_sites::named(m)))
    }
}

impl Game<'_> {
    /// The start of a level's tick (`tick_world`): which level is in place,
    /// its time, and with a damage listener the mobs' types; then the
    /// changes plugins queued from mob code since.
    pub(in crate::game) fn plugin_level_tick(&mut self) {
        if !ferraloom_plugins::running() {
            return;
        }
        IN_PLACE.set(world_index(self.dim));
        TIME.set((self.world.game_time, self.world.day_time));
        KINDS.with_borrow_mut(|k| {
            k.clear();
            if listens(Event::EntityDamage) {
                k.extend(self.mobs.iter().map(|m| (m.id, plugin_sites::kind(m))));
            }
        });
        self.plugin_apply_queued();
    }

    /// After the mob pass, before its events: the changes plugins made to
    /// other entities during it, in mob order.
    pub(in crate::game) fn plugin_after_mob_pass(&mut self) {
        if ferraloom_plugins::running() {
            self.plugin_apply_queued();
        }
    }

    fn plugin_apply_queued(&mut self) {
        let queued = plugin_sites::take_queued();
        if queued.is_empty() {
            return;
        }
        let mut host = GameHost::new(self, None);
        for (e, change) in queued {
            host.change(e, change);
        }
    }

    /// A host for a site in game code: the hub the guest's turn lent, unless
    /// it is in place.
    fn with_site_host<R>(&mut self, f: impl FnOnce(&mut GameHost) -> R) -> R {
        let in_place = self.lan.is_some();
        ferraloom_plugins::borrow_lent(|lent: Option<&mut LanHost>| {
            f(&mut GameHost::new(self, lent.filter(|_| !in_place)))
        })
    }

    /// `CreatureSpawnEvent` for mob `id`, just added: true when cancelled,
    /// and the mob is taken out again (CraftBukkit asks before adding it;
    /// the plugin sees it as added, which is the same to it).
    pub(in crate::game) fn plugin_spawned(&mut self, id: u32, reason: SpawnReason) -> bool {
        let reason = REASON.get().unwrap_or(reason);
        let Some(who) = self
            .mobs
            .iter()
            .rev()
            .find(|m| m.id == id)
            .map(plugin_sites::named)
        else {
            return false;
        };
        let cancelled = self.with_site_host(|h| ferraloom_plugins::creature_spawn(h, who, reason));
        if cancelled && let Some(i) = self.mobs.iter().rposition(|m| m.id == id) {
            self.mobs.remove(i);
        }
        cancelled
    }

    /// `EntityDeathEvent` with the death's loot collected and not yet
    /// dropped: the experience orbs are what it leaves.
    pub(in crate::game) fn plugin_entity_death(
        &mut self,
        m: &Mob,
        items: Vec<horizon_client::mobs::drops::Drop>,
    ) -> Vec<horizon_client::mobs::drops::Drop> {
        use horizon_client::mobs::drops::Drop;
        let experience: i32 = items
            .iter()
            .map(|d| match d {
                Drop::Orb(v) => *v,
                Drop::Item(_) => 0,
            })
            .sum();
        let victim = plugin_sites::named(m);
        let player_source = m
            .last_damage
            .is_some_and(|(d, _)| d.attacker == Some(Who::Player));
        let damage_type = m.last_damage.map_or("minecraft:generic", |(d, _)| {
            crate::game::creatures::damage_type(d.kind, player_source)
        });
        let killer = KILLER
            .get()
            .filter(|(id, _)| *id == m.id)
            .and_then(|(_, conn)| conn);
        let kept = self.with_site_host(|h| {
            let killer = killer
                .and_then(|conn| h.guest_uuid(conn))
                .map(Named::player)
                .or_else(|| match m.last_damage.and_then(|(d, _)| d.attacker) {
                    Some(Who::Mob(id)) => h.named_who(m.id, Who::Mob(id)),
                    _ => None,
                });
            ferraloom_plugins::entity_death(h, victim, killer, damage_type, experience)
        });
        KILLER.set(None);
        if kept == experience {
            return items;
        }
        let mut out: Vec<Drop> = items
            .into_iter()
            .filter(|d| matches!(d, Drop::Item(_)))
            .collect();
        if kept > 0 {
            out.push(Drop::Orb(kept));
        }
        out
    }

    /// `EntityExplodeEvent` for the blast going off now, before blocks
    /// break: the blocks that still may (none when cancelled). Only a blast
    /// a mob set off (`combat.explosion_source`) is an entity's.
    pub(in crate::game) fn plugin_explode(
        &mut self,
        center: [f64; 3],
        radius: f32,
        blocks: Vec<[i32; 3]>,
    ) -> Vec<[i32; 3]> {
        let Some(id) = self.combat.explosion_source else {
            return blocks;
        };
        let by = self
            .mobs
            .iter()
            .find(|m| m.id == id)
            .map(plugin_sites::named)
            .or_else(|| kind_of(id).map(|kind| Named::entity(id, kind)));
        let Some(by) = by else {
            return blocks;
        };
        let at = Location {
            world: world_index(self.dim),
            x: center[0],
            y: center[1],
            z: center[2],
            yaw: 0.0,
            pitch: 0.0,
        };
        // ServerExplosion's yield: 1 / radius of the blocks drop.
        let yield_ = 1.0 / radius.max(f32::MIN_POSITIVE);
        self.with_site_host(|h| ferraloom_plugins::entity_explode(h, Some(by), at, yield_, &blocks))
            .unwrap_or_default()
    }

    /// `EntityDamageEvent` (by the attacking player or mob, when there is
    /// one) for the guest whose turn it is, before they are hurt: the damage
    /// to deal, none when cancelled. Fired before the hurt cooldown, which is
    /// the health's own; armor crosses as nothing (it is applied after).
    pub(in crate::game) fn plugin_player_damage(
        &mut self,
        amount: f32,
        damage_type: &str,
        attacker: Option<u32>,
    ) -> Option<f32> {
        let Some(victim) = self.lan_acting_uuid.map(Named::player) else {
            return Some(amount);
        };
        let pvp = PVP.get();
        let attacker_mob = attacker.and_then(|id| {
            self.mobs
                .iter()
                .find(|m| m.id == id)
                .map(plugin_sites::named)
        });
        self.with_site_host(|h| {
            let damager = pvp
                .and_then(|conn| h.guest_uuid(conn))
                .map(Named::player)
                .or(attacker_mob);
            let dealt = f64::from(amount);
            let d = ferraloom_plugins::Damage {
                victim,
                damager,
                cause: cause_of_type(damage_type),
                damage_type,
                base: dealt,
                armor: 0.0,
                magic: 0.0,
                dealt,
            };
            ferraloom_plugins::entity_damage(h, &d).map(|d| d as f32)
        })
    }

    /// `PlayerInteractEntityEvent` for guest `conn` using entity `id`,
    /// before the interaction: true when cancelled.
    pub(in crate::game) fn plugin_interact_entity(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        id: i32,
        off_hand: bool,
    ) -> bool {
        let Some(player) = host
            .hub
            .guests
            .iter()
            .find(|g| g.conn == conn)
            .map(|g| g.uuid)
        else {
            return false;
        };
        let entity = match self.mobs.iter().find(|m| m.id == id as u32) {
            Some(m) => plugin_sites::named(m),
            None => match self.dropped.iter().find(|d| d.id == id as u32) {
                Some(d) => Named::entity(d.id, ITEM),
                None => return false,
            },
        };
        let mut h = GameHost::new(self, Some(host));
        ferraloom_plugins::interact_entity(&mut h, player, entity, off_hand)
    }
}
