//! Mobs and the guests of an open world. The mob code sees one player
//! (`MobWorld.player`, `Who::Player`); with guests on, each mob sees the
//! player vanilla's `Level.getNearestPlayer` would give it, and keeps the
//! guest it targets until it lets go. What it then does to `Who::Player`
//! (hits, pushes, fire, its projectiles) runs with that guest swapped in.
use super::{Game, lan::LanHost};
use horizon_client::{
    fast_hash::FxHashMap,
    lan::guest::Guest,
    mobs::{Mob, MobEvent, PlayerView, Species, Who, player_index::PlayerIndex},
    motion::PlayerMotion,
    projectiles::{Hit, Projectile},
};
use horizon_core::geometry::{Aabb, Vec3};
use horizon_protocol::packets::play::{self, Clientbound as C};

/// The guests mobs can see this tick, by connection.
pub(super) type GuestViews = Vec<(u32, PlayerView)>;

/// The views of one tick's guests, indexed for the per-mob queries of
/// [`player_for`]: a level's mobs run into the thousands, its guests into the
/// hundreds. The answers are those of a scan of `views` in order.
pub(super) struct GuestLookup<'a> {
    views: &'a GuestViews,
    by_conn: FxHashMap<u32, u32>,
    targetable: GuestSubset,
    in_play: GuestSubset,
}
/// The views a predicate keeps, as an index and each index slot's view.
struct GuestSubset {
    index: PlayerIndex,
    views: Vec<u32>,
}
impl GuestSubset {
    fn new(all: &GuestViews, keep: impl Fn(&PlayerView) -> bool) -> Self {
        let views: Vec<u32> = (0..all.len() as u32)
            .filter(|&i| keep(&all[i as usize].1))
            .collect();
        Self {
            index: PlayerIndex::new(views.iter().map(|&i| {
                let p = all[i as usize].1.position;
                [p.x, p.y, p.z]
            })),
            views,
        }
    }
}
impl<'a> GuestLookup<'a> {
    pub(super) fn new(views: &'a GuestViews) -> Self {
        Self {
            views,
            by_conn: views
                .iter()
                .enumerate()
                .rev()
                .map(|(i, (conn, _))| (*conn, i as u32))
                .collect(),
            targetable: GuestSubset::new(views, targetable),
            in_play: GuestSubset::new(views, in_play),
        }
    }
    /// The guest `conn`'s view.
    pub(super) fn view(&self, conn: u32) -> Option<&'a PlayerView> {
        let slot = *self.by_conn.get(&conn)?;
        Some(&self.views[slot as usize].1)
    }
}

/// `TargetingConditions`: alive, neither creative nor spectator.
fn targetable(v: &PlayerView) -> bool {
    v.alive && !v.creative && !v.spectator
}
fn in_play(v: &PlayerView) -> bool {
    v.alive && !v.spectator
}

/// The player a mob sees this tick: `None` for the host, else a guest.
/// A mob that targets a guest, or that a guest hurt in the last 100 ticks
/// (`LivingEntity.getLastHurtByMob`), keeps them while they are still a
/// valid target; otherwise it sees the nearest player
/// (`TargetingConditions`: alive, neither creative nor spectator), or the
/// nearest one that is not a spectator if none qualifies.
pub(super) fn player_for(
    m: &Mob,
    host: Option<&PlayerView>,
    guests: &GuestLookup<'_>,
    bound: Option<u32>,
) -> Option<u32> {
    // A tame cat reads its owner wherever they are while they sleep, or just
    // woke up (`CatRelaxOnOwnerGoal` uses `getOwner`, not the nearest player).
    if m.species() == Species::Cat
        && m.extra.tame
        && let Some(owner) = m.extra.owner
    {
        let asleep = |v: &PlayerView| v.uuid == owner && (v.sleeping || v.sleep_timer > 0);
        if host.is_some_and(asleep) {
            return None;
        }
        if let Some((conn, _)) = guests.views.iter().find(|(_, v)| asleep(v)) {
            return Some(*conn);
        }
    }
    let targeting = m.ai.target == Some(Who::Player)
        || m.ai.last_hurt_by == Some(Who::Player) && m.age as i32 - m.ai.last_hurt_by_time <= 100;
    // A tame wolf, cat or parrot sees its owner (`TamableAnimal.getOwner`), while they
    // are in play, whoever is nearer. Not while it targets or was just hurt by
    // a player: then `Who::Player` is that attacker, whoever it is, and the
    // owner goals ignore it until the wolf lets go.
    if !targeting
        && matches!(m.species(), Species::Wolf | Species::Cat | Species::Parrot)
        && m.extra.tame
        && let Some(owner) = m.extra.owner
    {
        if host.is_some_and(|v| v.alive && v.uuid == owner) {
            return None;
        }
        if let Some((conn, _)) = guests
            .views
            .iter()
            .find(|(_, v)| v.alive && v.uuid == owner)
        {
            return Some(*conn);
        }
    }
    choose(m.position, targeting, host, guests, bound)
}

fn choose(
    at: Vec3,
    targeting: bool,
    host: Option<&PlayerView>,
    guests: &GuestLookup<'_>,
    bound: Option<u32>,
) -> Option<u32> {
    if targeting
        && let Some(conn) = bound
        && guests.view(conn).is_some_and(targetable)
    {
        return Some(conn);
    }
    let query = [at.x, at.y, at.z];
    // The nearest of a subset, the first on a tie, the host before any guest.
    let nearest = |ok: fn(&PlayerView) -> bool, subset: &GuestSubset| {
        let host = host
            .filter(|v| ok(v))
            .map(|v| (None, at.distance_squared(v.position)));
        let guest = subset.index.nearest(query).map(|n| {
            let conn = guests.views[subset.views[n.slot as usize] as usize].0;
            (Some(conn), n.distance_sqr)
        });
        match (host, guest) {
            (Some(h), Some(g)) => Some(if g.1 < h.1 { g } else { h }),
            (h, g) => h.or(g),
        }
    };
    nearest(targetable, &guests.targetable)
        .or_else(|| nearest(in_play, &guests.in_play))
        .and_then(|(conn, _)| conn)
}

/// The events of a mob's tick that act on the player it sees.
pub(super) fn acts_on_player(e: &MobEvent) -> bool {
    matches!(
        e,
        MobEvent::AttackPlayer(..)
            | MobEvent::AttackPlayerWithEffect { .. }
            | MobEvent::DamagePlayer { .. }
            | MobEvent::SonicBoom {
                target: Who::Player,
                ..
            }
            | MobEvent::Push {
                target: Who::Player,
                ..
            }
            | MobEvent::Knockback {
                target: Who::Player,
                ..
            }
            | MobEvent::Ignite {
                target: Who::Player,
                ..
            }
            | MobEvent::Launch {
                target: Who::Player,
                ..
            }
            | MobEvent::EffectPlayer { .. }
            | MobEvent::AxolotlSupportPlayer
            | MobEvent::ParrotToShoulder { .. }
    )
}

impl Game<'_> {
    /// How mobs see each guest in play (`player_view` with the guest swapped
    /// in; what the host alone tracks, such as its last attacker, stays out).
    pub(super) fn lan_guest_views(&mut self) -> GuestViews {
        let Some(mut host) = self.lan.take() else {
            return Vec::new();
        };
        let conns: Vec<(u32, u128, [f64; 3])> = host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead)
            .map(|g| {
                let moved = [0, 1, 2].map(|k| g.position[k] - g.previous_position[k]);
                (g.conn, g.uuid, moved)
            })
            .collect();
        let mut views = Vec::with_capacity(conns.len());
        for (conn, uuid, moved) in conns {
            // Swapped in only when that would itself change something.
            let read = host.hub.guest(conn).and_then(|g| {
                let m = g.motion.as_deref()?;
                self.guest_swap_is_idle(&host, g, m)
                    .then(|| self.guest_view(g, m))
            });
            let view = match read {
                Some(view) => Some(view),
                None => self
                    .as_guest(&mut host, conn, |g| g.player_view())
                    .flatten(),
            };
            if let Some(mut v) = view {
                v.hurt_by = None;
                v.last_hurt = None;
                v.hero = false;
                v.velocity = Vec3::new(moved[0], moved[1], moved[2]);
                v.uuid = horizon_client::lan::playerdata::uuid_ints(uuid);
                v.allies = Default::default();
                v.remote = true;
                views.push((conn, v));
            }
        }
        self.lan = Some(host);
        views
    }

    /// `player_view` as it reads with `g` swapped in, up to what
    /// `lan_guest_views` overwrites (`hurt_by`, `last_hurt`, `hero`, `velocity`,
    /// `uuid`, `allies`, `remote`): `enter_guest` puts these same fields of the
    /// record in the game's place, or on the body `m`.
    fn guest_view(&self, g: &Guest, m: &PlayerMotion) -> PlayerView {
        let (at, dimensions) = (
            Vec3::new(g.position[0], g.position[1], g.position[2]),
            m.dimensions,
        );
        let w = f64::from(dimensions.width) / 2.0;
        let item = |stack: Option<&horizon_client::items::ItemStack>| stack.map(|s| s.item.clone());
        PlayerView {
            position: at,
            bounds: Aabb::new(
                Vec3::new(at.x - w, at.y, at.z - w),
                Vec3::new(at.x + w, at.y + f64::from(dimensions.height), at.z + w),
            ),
            eye: [at.x, at.y + f64::from(dimensions.eye_height), at.z],
            alive: !m.health.dead_or_dying(),
            creative: g.creative() || g.spectator(),
            sneaking: g.sneaking,
            swimming: m.swimming,
            look: super::view_vector(g.yaw, g.pitch),
            main_hand: item(g.inventory.get(g.selected).and_then(Option::as_ref)),
            off_hand: None,
            head: item(g.armor[0].as_ref()),
            armor_cover: g.armor.iter().filter(|a| a.is_some()).count() as f32 / 4.0,
            invisible: g.effects.get("invisibility").is_some(),
            spectator: g.spectator(),
            gold_armor: self.safe_from_piglins(&g.armor),
            hurt_by: None,
            last_hurt: None,
            velocity: m.velocity,
            yaw: g.yaw,
            sprinting: g.sprinting,
            riding: g.vehicle.is_some(),
            hero: false,
            effects: g.effects.list.iter().map(|e| e.id).collect(),
            uuid: [0; 4],
            allies: Default::default(),
            remote: true,
            sleeping: g.bed.is_some(),
            sleep_timer: g.sleep_timer,
            passenger: g.vehicle.is_some(),
        }
    }

    /// Runs `f` as each guest in play here, for what strikes every player in
    /// an area (`Level.getEntities`), not only the window's own.
    pub(super) fn each_guest_hit(&mut self, f: impl FnMut(&mut Self)) {
        self.each_guest_hit_except(None, f);
    }

    /// [`Self::each_guest_hit`] leaving out the guest `except` (the one a
    /// firework rocket is attached to, which it hurts on its own).
    pub(super) fn each_guest_hit_except(
        &mut self,
        except: Option<u32>,
        mut f: impl FnMut(&mut Self),
    ) {
        let Some(mut host) = self.lan.take() else {
            return;
        };
        let conns: Vec<u32> = host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && Some(g.conn) != except)
            .map(|g| g.conn)
            .collect();
        for conn in conns {
            self.guest_hit_by(&mut host, conn, &mut f);
        }
        self.lan = Some(host);
    }

    /// Mob events aimed at a guest, run as that guest (`guest_hit_by`).
    pub(super) fn lan_guest_mob_events(&mut self, events: Vec<(u32, MobEvent)>) {
        if events.is_empty() {
            return;
        }
        let Some(mut host) = self.lan.take() else {
            return;
        };
        for (conn, e) in events {
            self.guest_hit_by(&mut host, conn, |g| {
                let _ = g.handle_mob_events(vec![e]);
            });
        }
        self.lan = Some(host);
    }

    /// The guests' boxes for the blocks entities trigger (pressure plates,
    /// tripwires, detector rails): `Entity.checkInsideBlocks` and
    /// `Level.getEntities` see every player in the level, but not spectators
    /// (`Entity.isIgnoringBlockTriggers`).
    pub(super) fn lan_block_trigger_boxes(&self) -> Vec<([f64; 3], [f64; 3])> {
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        host.hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .map(|g| {
                let b = g.bounds();
                let (lo, hi) = (b.min_position(), b.max_position());
                ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
            })
            .collect()
    }
    /// The same guests' feet and whether they stand on the ground, for the
    /// blocks that also ask that (`BigDripleafBlock.canEntityTilt`).
    pub(super) fn lan_guest_feet(&self) -> Vec<([f64; 3], bool)> {
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        host.hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .map(|g| (g.position, g.on_ground))
            .collect()
    }

    /// The guests a projectile can hit, under their entity ids.
    pub(super) fn lan_projectile_targets(&self) -> Vec<(Who, Aabb)> {
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        host.hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .map(|g| (Who::Mob(g.entity_id as u32), guest_bounds(g)))
            .collect()
    }

    /// A projectile that hit a guest: its hit runs as theirs. False when
    /// `id` is not a guest.
    pub(super) fn lan_projectile_hit(&mut self, p: &Projectile, id: u32, at: Vec3) -> bool {
        let Some(mut host) = self.lan.take() else {
            return false;
        };
        let conn = host
            .hub
            .guests
            .iter()
            .find(|g| g.entity_id as u32 == id)
            .map(|g| g.conn);
        if let Some(conn) = conn {
            self.guest_hit_by(&mut host, conn, |g| {
                let hit = Hit::Entity {
                    who: Who::Player,
                    at,
                };
                let _ = g.projectile_hit(p, hit);
            });
        }
        self.lan = Some(host);
        conn.is_some()
    }
}

impl Game<'_> {
    /// Hurt a guest: `f` runs as them, and the knockback it gave reaches
    /// their client. A hurt marks the velocity (Entity.markHurt), which goes
    /// out once the guest's server copy has travelled (`guest_server_tick`);
    /// a push with no hurt goes out at once.
    pub(super) fn guest_hit_by<R>(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        let (result, pushed) = self.as_guest(host, conn, |g| {
            let before = g.player.as_ref().map(|p| p.velocity);
            let result = f(g);
            let pushed = g
                .player
                .as_ref()
                .filter(|p| !p.health.sync_velocity && Some(p.velocity) != before)
                .map(|p| p.velocity);
            (result, pushed)
        })?;
        self.send_guest_motion(host, conn, pushed);
        Some(result)
    }

    /// A player's melee hit on a guest (Player.causeExtraKnockback on a
    /// ServerPlayer): a hurt that marked the velocity sends it at once, and
    /// the server's copy goes back to the velocity it had before the hit.
    pub(super) fn guest_struck_by<R>(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        let (result, sent) = self.as_guest(host, conn, |g| {
            let before = g.player.as_ref().map(|p| p.velocity);
            let result = f(g);
            let sent = g.player.as_mut().zip(before).and_then(|(p, before)| {
                std::mem::take(&mut p.health.sync_velocity)
                    .then(|| std::mem::replace(&mut p.velocity, before))
            });
            (result, sent)
        })?;
        self.send_guest_motion(host, conn, sent);
        Some(result)
    }

    /// ClientboundSetEntityMotionPacket to a guest about themselves.
    pub(super) fn send_guest_motion(&self, host: &LanHost, conn: u32, velocity: Option<Vec3>) {
        if let (Some(v), Some(g)) = (velocity, host.hub.guest(conn)) {
            g.send(&C::SetEntityMotion(play::SetEntityMotion {
                id: g.entity_id,
                velocity: [v.x, v.y, v.z],
            }));
        }
    }
}

/// A guest as the blocks that look for players see them
/// (`PlayerDetector`, `level.getPlayers`: the dead ones too).
pub(super) struct Present {
    pub uuid: [i32; 4],
    pub position: [f64; 3],
    pub eye: [f64; 3],
    pub height: f32,
    pub creative: bool,
    pub spectator: bool,
    pub alive: bool,
}

impl Game<'_> {
    /// Every guest in the world (`ServerLevel.getPlayers` past the host).
    pub(super) fn lan_present(&self) -> Vec<Present> {
        let guests = self
            .lan
            .iter()
            .flat_map(|h| h.hub.guests.iter().filter(|g| h.hub.here(g)));
        guests
            .filter(|g| g.loaded)
            .map(|g| {
                let [x, y, z] = g.position;
                Present {
                    uuid: horizon_client::lan::playerdata::uuid_ints(g.uuid),
                    position: g.position,
                    eye: [x, y + g.eye_height(), z],
                    height: g.dimensions().height,
                    creative: g.creative(),
                    spectator: g.spectator(),
                    alive: !g.dead,
                }
            })
            .collect()
    }
    /// The UUID of the player acting: the guest `as_guest` runs, else the host.
    pub(super) fn acting_uuid(&self) -> [i32; 4] {
        self.lan_acting_uuid
            .map(horizon_client::lan::playerdata::uuid_ints)
            .or(self.level.player_uuid)
            .unwrap_or_default()
    }
}

/// A guest's bounding box, by its pose (`Avatar.POSES`).
pub(super) fn guest_bounds(g: &horizon_client::lan::guest::Guest) -> Aabb {
    g.bounds()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(x: f64, creative: bool) -> PlayerView {
        PlayerView {
            position: Vec3::new(x, 64.0, 0.0),
            bounds: Aabb::new(Vec3::new(x, 64.0, 0.0), Vec3::new(x, 65.8, 0.0)),
            eye: [x, 65.62, 0.0],
            alive: true,
            creative,
            sneaking: false,
            swimming: false,
            look: [0.0; 3],
            main_hand: None,
            off_hand: None,
            head: None,
            armor_cover: 0.0,
            invisible: false,
            spectator: false,
            gold_armor: false,
            hurt_by: None,
            last_hurt: None,
            velocity: Vec3::ZERO,
            yaw: 0.0,
            sprinting: false,
            riding: false,
            hero: false,
            effects: std::sync::Arc::from([]),
            uuid: [0; 4],
            allies: Default::default(),
            remote: false,
            sleeping: false,
            sleep_timer: 0,
            passenger: false,
        }
    }

    const MOB: Vec3 = Vec3::new(0.0, 64.0, 0.0);

    fn choose_in(
        targeting: bool,
        host: Option<&PlayerView>,
        guests: &GuestViews,
        bound: Option<u32>,
    ) -> Option<u32> {
        choose(MOB, targeting, host, &GuestLookup::new(guests), bound)
    }

    /// The scan the index stands in for: `getNearestPlayer`'s `d < distance`.
    fn scanned(
        at: Vec3,
        targeting: bool,
        host: Option<&PlayerView>,
        guests: &GuestViews,
        bound: Option<u32>,
    ) -> Option<u32> {
        if targeting
            && let Some(conn) = bound
            && guests.iter().any(|(c, v)| *c == conn && targetable(v))
        {
            return Some(conn);
        }
        let d = |v: &PlayerView| at.distance_squared(v.position);
        let nearest = |ok: fn(&PlayerView) -> bool| {
            let mut best = host.filter(|v| ok(v)).map(|v| (None, d(v)));
            for (conn, v) in guests.iter().filter(|(_, v)| ok(v)) {
                if best.is_none_or(|(_, b)| d(v) < b) {
                    best = Some((Some(*conn), d(v)));
                }
            }
            best
        };
        nearest(targetable)
            .or_else(|| nearest(in_play))
            .and_then(|(conn, _)| conn)
    }

    #[test]
    fn the_index_answers_as_the_scan_does() {
        let mut seed = 0x2545_f491_u64;
        let mut draw = |range: i32| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            ((seed >> 33) as i32).rem_euclid(range)
        };
        for round in 0..200 {
            // Few distinct coordinates, so ties are common.
            let guests: GuestViews = (0..draw(40) as u32)
                .map(|conn| {
                    let mut v = view(f64::from(draw(9) * 8), draw(3) == 0);
                    v.position.z = f64::from(draw(5) * 8);
                    v.alive = draw(8) != 0;
                    v.spectator = draw(8) == 0;
                    (conn, v)
                })
                .collect();
            let lookup = GuestLookup::new(&guests);
            let host = (round % 3 == 0).then(|| view(f64::from(draw(9) * 8), draw(2) == 0));
            for _ in 0..30 {
                let at = Vec3::new(f64::from(draw(9) * 4), 64.0, f64::from(draw(5) * 4));
                let targeting = draw(2) == 0;
                let bound = (draw(2) == 0).then(|| draw(40) as u32);
                assert_eq!(
                    choose(at, targeting, host.as_ref(), &lookup, bound),
                    scanned(at, targeting, host.as_ref(), &guests, bound),
                );
            }
        }
    }

    #[test]
    fn the_nearest_survival_player_is_seen() {
        let host = view(10.0, false);
        let guests = vec![(7, view(3.0, false)), (8, view(-5.0, false))];
        assert_eq!(choose_in(false, Some(&host), &guests, None), Some(7));
        // A creative guest is passed over for the host further away.
        let guests = vec![(7, view(3.0, true))];
        assert_eq!(choose_in(false, Some(&host), &guests, None), None);
        // Nobody targetable: the nearest one is still seen.
        let host = view(10.0, true);
        assert_eq!(choose_in(false, Some(&host), &guests, None), Some(7));
        assert_eq!(choose_in(false, None, &Vec::new(), None), None);
    }

    #[test]
    fn a_targeted_guest_stays_the_target() {
        let host = view(1.0, false);
        let guests = vec![(7, view(12.0, false))];
        assert_eq!(choose_in(true, Some(&host), &guests, Some(7)), Some(7));
        // Gone creative: the mob looks again.
        let guests = vec![(7, view(12.0, true))];
        assert_eq!(choose_in(true, Some(&host), &guests, Some(7)), None);
        // Without a target the nearest player wins.
        let guests = vec![(7, view(12.0, false))];
        assert_eq!(choose_in(false, Some(&host), &guests, Some(7)), None);
    }
}
