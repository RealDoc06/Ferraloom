//! Ferraloom: the spectator camera of a server's players (`ServerPlayer.camera`).
//! A spectator's click on an entity (`handleSpectatorAction`) and `/spectate`
//! (game/commands/extra.rs) attach their view to it; the camera's part of
//! `ServerPlayer.tick` keeps them on it. The cameras live in
//! `CommandExtras::guest_cameras`, which is empty unless someone spectates
//! through an entity, so a server with no such spectator does no work here.
use super::{Game, commands::Target, lan::LanHost};
use horizon_core::geometry::Vec3;
use horizon_protocol::packets::play::{self, Clientbound as C};

/// `Player.entityInteractionRange` (its attribute's default; a spectator has
/// no creative modifier) plus the buffer `handleSpectatorAction` gives.
const SPECTATE_RANGE: f64 = 3.0 + 3.0;

/// The network id of a camera entity (a guest's through `guest_id`).
fn camera_id(t: Target, guest_id: impl Fn(u32) -> Option<i32>) -> Option<i32> {
    match t {
        Target::Player => None,
        Target::Guest(conn) => guest_id(conn),
        Target::Mob(id)
        | Target::Item(id)
        | Target::Orb(id)
        | Target::Projectile(id)
        | Target::Stand(id)
        | Target::Frame(id)
        | Target::Painting(id)
        | Target::Boat(id)
        | Target::Cart(id)
        | Target::Other(id)
        | Target::Crystal(id)
        | Target::Dragon(id) => i32::try_from(id).ok(),
    }
}

impl Game<'_> {
    /// Port of ServerGamePacketListenerImpl#handleSpectatorAction: a loaded
    /// spectator watches through the pickable entity they clicked, if it is
    /// inside the world border and within their entity reach plus 3.
    pub(super) fn guest_spectator_action(
        &mut self,
        host: &mut LanHost,
        gi: usize,
        id: Option<i32>,
    ) {
        let g = &host.hub.guests[gi];
        let Some(id) = id.filter(|_| g.loaded && g.spectator()) else {
            return;
        };
        let Some((target, (lo, hi))) = self.pickable(host, gi, id) else {
            return;
        };
        let feet = [(lo[0] + hi[0]) / 2.0, lo[1], (lo[2] + hi[2]) / 2.0];
        if !self.border.contains(feet[0].floor(), feet[2].floor()) {
            return;
        }
        let eye = [g.position[0], g.position[1] + g.eye_height(), g.position[2]];
        let d2: f64 = (0..3)
            .map(|a| (eye[a] - eye[a].clamp(lo[a], hi[a])).powi(2))
            .sum();
        if d2 < SPECTATE_RANGE * SPECTATE_RANGE {
            self.set_guest_camera(host, gi, Some(target));
        }
    }

    /// `Level.getEntityOrPart` limited to what `Entity.isPickable` lets a
    /// spectator watch here: players not spectating, living mobs, boats,
    /// minecarts, armor stands, item frames and paintings, with their boxes.
    fn pickable(
        &self,
        host: &LanHost,
        gi: usize,
        id: i32,
    ) -> Option<(Target, ([f64; 3], [f64; 3]))> {
        let corners = |b: horizon_core::geometry::Aabb| {
            let (lo, hi) = (b.min_position(), b.max_position());
            ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
        };
        let own = host.hub.guests[gi].conn;
        if let Some(g) = host.hub.guests.iter().find(|g| g.entity_id == id) {
            let fine = g.conn != own && host.hub.here(g) && !g.dead && !g.spectator();
            return fine.then(|| (Target::Guest(g.conn), corners(g.bounds())));
        }
        let uid = u32::try_from(id).ok()?;
        if let Some(m) = self.mobs.iter().find(|m| m.id == uid) {
            return m.alive().then(|| (Target::Mob(uid), corners(m.bounds())));
        }
        let (lo, hi, _) = self.object_target(id)?;
        let target = if self.boats.iter().any(|b| b.id == uid) {
            Target::Boat(uid)
        } else if self.carts.iter().any(|c| c.id == uid) {
            Target::Cart(uid)
        } else if self.armor_stands.iter().any(|a| a.id == uid) {
            Target::Stand(uid)
        } else if self.item_frames.iter().any(|f| f.id == uid) {
            Target::Frame(uid)
        } else if self.paintings.iter().any(|p| p.id == uid) {
            Target::Painting(uid)
        } else {
            return None;
        };
        Some((target, (lo, hi)))
    }

    /// Where camera `t` is and looks while it is alive, out of a guest's turn.
    fn guest_camera_view(&self, host: &LanHost, t: Target) -> Option<([f64; 3], [f32; 2])> {
        match t {
            Target::Guest(conn) => host
                .hub
                .guest(conn)
                .filter(|g| !g.dead && host.hub.here(g))
                .map(|g| (g.position, [g.yaw, g.pitch])),
            _ => self.spectator_view(t),
        }
    }

    /// Port of ServerPlayer#setCamera for guest `gi`, out of their turn (None:
    /// their own view back): the player goes to the camera with their own
    /// look (`teleportTo`), then their client is told whose eyes it sees by.
    pub(super) fn set_guest_camera(
        &mut self,
        host: &mut LanHost,
        gi: usize,
        target: Option<Target>,
    ) {
        let conn = host.hub.guests[gi].conn;
        if self.cmds.guest_cameras.get(&conn).copied() == target {
            return;
        }
        let at = match target {
            Some(t) => match self.guest_camera_view(host, t) {
                Some((at, _)) => at,
                None => return,
            },
            None => host.hub.guests[gi].position,
        };
        let id = match target {
            Some(t) => camera_id(t, |c| host.hub.guest(c).map(|g| g.entity_id)),
            None => Some(host.hub.guests[gi].entity_id),
        };
        let Some(id) = id else {
            return;
        };
        match target {
            Some(t) => self.cmds.guest_cameras.insert(conn, t),
            None => self.cmds.guest_cameras.remove(&conn),
        };
        if let Some(m) = host.hub.guests[gi].motion.as_mut() {
            m.position = Vec3::new(at[0], at[1], at[2]);
        }
        host.hub.teleport(gi, Some(at));
        host.hub.guests[gi].send(&C::SetCamera(play::SetCamera { id }));
    }

    /// `ServerPlayer.setCamera` from a command, in the guest's turn (`conn`
    /// in place, the hub in `self.lan`): the teleport goes out at once, before
    /// the camera, and the body in place moves with it.
    pub(super) fn set_acting_guest_camera(&mut self, conn: u32, target: Option<Target>) {
        if self.cmds.guest_cameras.get(&conn).copied() == target {
            return;
        }
        let own = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        let at = match target {
            Some(t) => self.spectator_view(t).map(|(at, _)| at),
            None => own,
        };
        let guest_id = |c: u32| self.command_guest(c).map(|g| g.entity_id);
        let id = match target {
            Some(t) => camera_id(t, guest_id),
            None => guest_id(conn),
        };
        let (Some(at), Some(id)) = (at, id) else {
            return;
        };
        let Some(gi) = self.lan.as_ref().and_then(|h| h.hub.guest_index(conn)) else {
            return;
        };
        match target {
            Some(t) => self.cmds.guest_cameras.insert(conn, t),
            None => self.cmds.guest_cameras.remove(&conn),
        };
        if let Some(p) = self.player.as_mut() {
            p.position = Vec3::new(at[0], at[1], at[2]);
        }
        // The turn's own look, which the record gets back when it ends.
        let (yaw, pitch) = (self.yaw, self.pitch);
        if let Some(host) = self.lan.as_mut() {
            let g = &mut host.hub.guests[gi];
            (g.yaw, g.pitch) = (yaw, pitch);
            host.hub.teleport(gi, Some(at));
            host.hub.guests[gi].send(&C::SetCamera(play::SetCamera { id }));
        }
    }

    /// The camera part of ServerPlayer#tick for the guests in this level who
    /// watch through an entity: they follow a living camera (`absSnapTo`, no
    /// packet) and get their own view back when it dies or goes, when they
    /// sneak (`wantsToStopRiding`) or once they are no longer spectators
    /// (`ServerPlayerGameMode.setGameModeForPlayer`).
    pub(super) fn tick_guest_cameras(&mut self, host: &mut LanHost) {
        if self.cmds.guest_cameras.is_empty() {
            return;
        }
        let cameras: Vec<(u32, Target)> = self
            .cmds
            .guest_cameras
            .iter()
            .map(|(c, t)| (*c, *t))
            .collect();
        for (conn, t) in cameras {
            let Some(gi) = host.hub.guest_index(conn) else {
                self.cmds.guest_cameras.remove(&conn);
                continue;
            };
            if !host.hub.here(&host.hub.guests[gi]) {
                continue;
            }
            let view = self.guest_camera_view(host, t);
            let g = &mut host.hub.guests[gi];
            if let Some((at, [yaw, pitch])) = view.filter(|_| g.spectator()) {
                g.position = at;
                (g.yaw, g.pitch) = (yaw, pitch);
                if let Some(m) = g.motion.as_mut() {
                    m.position = Vec3::new(at[0], at[1], at[2]);
                }
                // Avatar input flag 32: shift.
                if g.input & 32 == 0 {
                    continue;
                }
            }
            self.set_guest_camera(host, gi, None);
        }
    }
}
