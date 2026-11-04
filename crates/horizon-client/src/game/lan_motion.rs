//! A guest's body on the server each tick (`guest_server_tick`), its motion
//! part first: the fluids, the moves its client sent and the falls they end
//! in, the blocks it is in and the hazards, the spin and the pose. That part
//! reads only the body, the level and the guest's record, so before the
//! guests take their turns it is run for each of them on the pool's threads,
//! on a copy of the body and the level read only (`speculate_guest_motion`).
//! A guest's turn keeps the copy when nothing it was made from has changed
//! by then (`Speculation::take_if_still_true`), and otherwise runs the part
//! itself, as it always did.
use super::{Game, lan::LanHost, lan_actions::BodySync};
use horizon_client::{
    fast_hash::FxHashMap,
    lan::guest::{Guest, SPECTATOR},
    motion::{MotionEvent, MotionWorld, PlayerMotion, ReadRandom, Scene},
};
use horizon_core::geometry::Vec3;
use horizon_world::entity_move::EntityMoveWorld;

/// What the motion part takes from the record and the game besides the body.
#[derive(Clone, PartialEq)]
pub(super) struct MotionInputs {
    /// The moves the client sent since the last tick, and whether it said it
    /// was on the ground after each.
    pub(super) moves: Vec<([f64; 3], bool)>,
    /// On the ground before the first of them.
    pub(super) from_ground: bool,
    pub(super) riding: bool,
    pub(super) sneaking: bool,
    pub(super) sleeping: bool,
    pub(super) freeze_immune: bool,
    pub(super) burn_immune: bool,
    /// `Player.isInWall`.
    pub(super) in_wall: bool,
}

/// What the motion part gives the rest of the turn.
pub(super) struct MotionResult {
    pub(super) events: Vec<MotionEvent>,
    /// Where the tick's first move started.
    pub(super) start: Vec3,
    pub(super) travelled: Vec<[f64; 3]>,
}

/// ServerPlayer.doTick's damage clock and Entity.baseTick's fluid phase
/// (burning), then for each move the guest made
/// ServerGamePacketListenerImpl.handleMovePlayer's jump
/// (`ServerPlayer.jumpFromGround`'s exhaustion), ServerPlayer.doCheckFallDamage
/// and checkMovementStatistics, then LivingEntity.baseTick's hazards, the
/// spin's countdown and Player.updatePlayerPose. The body's events are taken.
pub(super) fn guest_motion(w: &mut MotionWorld, inputs: &MotionInputs) -> MotionResult {
    w.player.health.advance_clock();
    let _ = w.fluid_base_tick();
    w.player.ground.on_ground = inputs.from_ground;
    // Player.aiStep: a flying player that isn't riding starts every tick
    // with no fall distance, so flying down in creative and switching to
    // survival lands without the flight's distance.
    if w.player.flying && !inputs.riding {
        w.player.fall_distance = 0.0;
    }
    let travelled: Vec<[f64; 3]> = inputs.moves.iter().map(|m| m.0).collect();
    // Entity.move's movementThisTick: each move, from where the
    // tick's first one started.
    let end = w.player.position;
    let start = travelled
        .iter()
        .fold(end, |p, d| p.subtract(Vec3::new(d[0], d[1], d[2])));
    w.player.history.clear();
    let mut from = start;
    for d in &travelled {
        let delta = Vec3::new(d[0], d[1], d[2]);
        let to = from.add_vector(delta);
        w.player
            .history
            .add(horizon_world::entity_move::MovementRecord {
                from,
                to,
                original: Some(delta),
            });
        from = to;
    }
    for &(moved, on_ground) in &inputs.moves {
        let movement = Vec3::new(moved[0], moved[1], moved[2]);
        if w.player.ground.on_ground && !on_ground && moved[1] > 0.0 {
            let sprinting = w.player.sprinting;
            w.player
                .cause_food_exhaustion(if sprinting { 0.2 } else { 0.05 });
        }
        let landed = (|| -> horizon_client::Result<()> {
            w.set_on_ground_with_movement(on_ground, false, movement)?;
            let pos = w.legacy_on_position()?;
            let state = w.block_state(pos)?;
            w.check_fall_damage(moved[1], on_ground, state, pos)?;
            w.check_movement_statistics(movement)
        })();
        if landed.is_err() {
            break;
        }
    }
    // A ServerPlayer travels on the server too
    // (MoveSimulationType.AUTHORITATIVE_SIDE_AND_SERVER), so every tick
    // on a ladder or vine LivingEntity.handleOnClimbable resets its fall
    // distance: climbing down doesn't hurt where it lands (#1255).
    if w.climbable().unwrap_or(false) {
        w.player.fall_distance = 0.0;
    }
    // Entity.applyEffectsFromBlocks along those moves: magma, cactus,
    // lit campfires and grown sweet berry bushes hurt (#1305).
    w.player.shift_key_down = inputs.sneaking;
    let _ = w.player.set_sleeping(inputs.sleeping);
    w.player.freeze_immune = inputs.freeze_immune;
    w.player.burn_from_stepping_immune = inputs.burn_immune;
    let _ = w.server_block_damage(start);
    let _ = w.update_soul_speed_block();
    let _ = w.tick_environment_damage();
    let _ = w.apply_fire_contact();
    let _ = w.server_hazards_tick(inputs.in_wall);
    // LivingEntity.aiStep: a Riptide spin counts down on the server's
    // copy too, or it would stay in the spin pose (eye 0.4) for good.
    w.player.count_down_server_spin();
    // Player.tick ends with updatePlayerPose: a swimmer, or a player
    // crawling under a block, takes the short box its moves are checked with.
    let _ = w.update_player_pose();
    MotionResult {
        events: std::mem::take(&mut w.player.events),
        start,
        travelled,
    }
}

/// `Game::update_totem_ready` of the record's hands.
fn holds_totem(g: &Guest) -> bool {
    let totem = |s: Option<&horizon_client::items::ItemStack>| {
        s.is_some_and(|s| &*s.item == super::totem::TOTEM)
    };
    totem(g.inventory.get(g.selected).and_then(Option::as_ref)) || totem(g.offhand.as_ref())
}

/// A guest's motion part run ahead on a copy of its body.
pub(super) struct Speculation {
    /// The guest's count of turns (`GuestPlay::turns`) when it was made.
    turns: u32,
    /// The level's block and chunk revisions it read.
    revision: (u64, u64),
    /// What the turn's start puts on the body, and the totem in hand.
    sync: BodySync,
    totem: bool,
    inputs: MotionInputs,
    /// The copy after the motion part, and what the part gave.
    body: Box<PlayerMotion>,
    result: Option<MotionResult>,
}

impl Speculation {
    /// The copy and its result, if the turn now starting would have made the
    /// same: the guest had no other turn since (so the body is as it was),
    /// the record gives the body the same fields, the inputs are the same
    /// and the level has not changed. A copy that drew from the level's
    /// random source or asked to change the level has no result.
    pub(super) fn take_if_still_true(
        self,
        turns: u32,
        g: &Guest,
        inputs: &MotionInputs,
        world: &horizon_client::world::World,
    ) -> Option<(Box<PlayerMotion>, MotionResult)> {
        let same = turns == self.turns.wrapping_add(1)
            && self.revision == (world.revision, world.chunks.membership_revision())
            && self.sync.same(&BodySync::of(g))
            && self.totem == holds_totem(g)
            && self.inputs == *inputs;
        same.then_some((self.body, self.result?))
    }
}

impl Game<'_> {
    /// The motion part of the guests in `conns` that will run it in a turn
    /// of their own, ahead on the pool's threads: copies of their bodies,
    /// synced with the record as a turn's start syncs them, moved in the
    /// level read only. Guests whose tick has a turn before this one
    /// (digging, using, riding, gliding, in bed) are left out.
    pub(super) fn speculate_guest_motion(
        &self,
        host: &mut LanHost,
        conns: &[u32],
    ) -> FxHashMap<u32, Speculation> {
        let revision = (self.world.revision, self.world.chunks.membership_revision());
        let mut planned: FxHashMap<u32, (u32, BodySync, bool, MotionInputs)> = FxHashMap::default();
        for &conn in conns {
            let (Some(g), Some(play)) = (host.hub.guest(conn), host.play.get(&conn)) else {
                continue;
            };
            let Some(m) = g.motion.as_deref() else {
                continue;
            };
            let held = g.inventory.get(g.selected).and_then(Option::as_ref);
            let alone = !g.dead
                && g.health > 0.0
                && g.game_mode != SPECTATOR
                && g.vehicle.is_none()
                && !g.fall_flying
                && g.bed.is_none()
                && !g.sleeping
                && play.on_its_own()
                && play.wears(&g.armor, held)
                && !self.on_server_trip(conn);
            if !alone {
                continue;
            }
            let inputs = MotionInputs {
                moves: g.moves.clone(),
                from_ground: if g.moves.is_empty() {
                    g.on_ground
                } else {
                    g.moves_from_ground
                },
                riding: false,
                sneaking: g.sneaking,
                sleeping: false,
                // As the body had them last: a change of armour since makes
                // the turn run the part itself.
                freeze_immune: m.freeze_immune,
                burn_immune: m.burn_from_stepping_immune,
                in_wall: self.in_wall(g.position, g.eye_height()),
            };
            planned.insert(
                conn,
                (play.turns(), BodySync::of(g), holds_totem(g), inputs),
            );
        }
        if planned.is_empty() {
            return FxHashMap::default();
        }
        // Each worker copies its guest's body itself.
        let mut work: Vec<(u32, &mut PlayerMotion, Option<Speculation>)> = (host.hub.guests)
            .iter_mut()
            .filter(|g| planned.contains_key(&g.conn))
            .filter_map(|g| Some((g.conn, g.motion.as_deref_mut()?, None)))
            .collect();
        let (world, assets, binding, resources) =
            (&self.world, &self.pack.assets, self.collision, self.motion);
        horizon_threads::pool::for_each_mut(&mut work, SPECULATION_GRAIN, |(conn, m, out)| {
            let Some((turns, sync, totem, inputs)) = planned.get(conn) else {
                return;
            };
            let mut body = Box::new((**m).clone());
            sync.apply(&mut body);
            body.health.totem_ready = *totem;
            // guest_glide_tick: a guest not gliding has no glide ticks.
            body.fall_fly_ticks = 0;
            let mut random = ReadRandom {
                random: world.random.clone(),
                touched: false,
            };
            let result = guest_motion(
                &mut MotionWorld {
                    entities: &[],
                    player: &mut body,
                    scene: Scene::Read {
                        world,
                        random: &mut random,
                    },
                    assets,
                    collision_binding: binding,
                    resources,
                },
                inputs,
            );
            *out = Some(Speculation {
                turns: *turns,
                revision,
                sync: *sync,
                totem: *totem,
                inputs: inputs.clone(),
                body,
                result: (!random.touched).then_some(result),
            });
        });
        work.into_iter()
            .filter_map(|(conn, _, spec)| Some((conn, spec?)))
            .collect()
    }
}

/// Guests per piece of `speculate_guest_motion`'s pass on the pool.
const SPECULATION_GRAIN: usize = 8;
