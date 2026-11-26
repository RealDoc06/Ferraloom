//! The camera type (`CameraType`: first person, third person behind and
//! in front, cycled by F5), the detached camera (`Camera.setup` with
//! `getMaxZoom`), and the local player drawn as a model: in third person
//! and in the inventory (`InventoryScreen.extractEntityInInventoryFollowsMouse`).
use super::Game;
use horizon_client::{
    mobs::{Anim, Brain, Mob, MobType},
    pose::Pose,
};
use horizon_core::geometry::Vec3;
use std::{path::Path, sync::Arc};

/// The player's model type (`PlayerModel` with the default skin).
pub(super) fn player_type(root: &Path) -> Option<Arc<MobType>> {
    horizon_client::mobs::load_model(root, "minecraft:player#main").map(|model| {
        Arc::new(MobType {
            id: "minecraft:player".into(),
            // Players render as a humanoid; the species only drives local AI.
            species: horizon_client::mobs::Species::Zombie,
            width: 0.6,
            height: 1.8,
            eye_height: 1.62,
            category: "misc".into(),
            max_health: 20.0,
            speed: 0.1,
            flying_speed: 0.02,
            attack_damage: 1.0,
            attack_knockback: 0.0,
            follow_range: 0.0,
            step_height: 0.6,
            armor: 0.0,
            armor_toughness: 0.0,
            knockback_resistance: 0.0,
            jump_strength: 0.42,
            attributes: Default::default(),
            gravity: 0.08,
            safe_fall_distance: 3.0,
            fall_damage_multiplier: 1.0,
            water_efficiency: 0.0,
            water_slowdown: 0.8,
            air_speed: 0.02,
            baby_scale: 0.5,
            head_rot_speed: 10.0,
            max_head_x_rot: 40.0,
            max_head_y_rot: 75.0,
            ambient_interval: 80,
            flags: Default::default(),
            model: Arc::new(model),
            // PlayerModel's slim arms, for slim skins.
            layers: horizon_client::mobs::load_model(root, "minecraft:player_slim#main")
                .map(|slim| [("player_slim#main".to_owned(), Arc::new(slim))].into())
                .unwrap_or_default(),
            texture: "entity/player/wide/steve".into(),
            brain: Brain::Passive,
            anim: Anim::Humanoid { zombie_arms: false },
            sounds: [
                "entity.player.hurt",
                "entity.player.hurt",
                "entity.player.death",
            ],
            step_sound: None,
            food: Arc::from([]),
            tempt: Arc::from([]),
        })
    })
}
fn wrap(d: f32) -> f32 {
    let d = d % 360.0;
    if d >= 180.0 {
        d - 360.0
    } else if d < -180.0 {
        d + 360.0
    } else {
        d
    }
}
impl Game<'_> {
    /// Keep the player's model in step with the player (`LivingEntity.tick`
    /// body turning, `WalkAnimationState`, the swing and the pose).
    pub(super) fn tick_player_model(&mut self) {
        let Some(p) = self.player.as_ref() else {
            self.player_model = None;
            return;
        };
        if self.player_model.is_none() {
            let Some(kind) = self.res.player_type().clone() else {
                return;
            };
            self.player_model = Some(Mob::new(0, kind, p.position, self.yaw));
        }
        let crouching = p.pose == Pose::Crouching;
        let (position, flying) = (p.position, p.flying || p.fall_flying);
        let (fall_flying, fly_ticks, velocity) = (p.fall_flying, p.fall_fly_ticks, p.velocity);
        let spinning = p.auto_spin_attack_ticks > 0;
        let using = self.using.map(|(_, ticks)| ticks as i32 + 1).unwrap_or(0);
        let using_off_hand = self
            .using
            .is_some_and(|(slot, _)| slot == super::shield::OFFHAND_SLOT);
        let blocking = self
            .using
            .and_then(|(slot, _)| self.stack_in(slot))
            .is_some_and(|stack| self.blocks_attacks(stack).is_some());
        let charged = self.inventory[self.selected]
            .as_ref()
            .is_some_and(|s| s.data().is_some_and(|d| !d.charged.is_empty()));
        let charge_duration = self.inventory[self.selected]
            .as_ref()
            .map_or(25, |s| Self::charge_ticks(s).max(1)) as i32;
        let attack = (self.swing.old_animation, self.swing.animation);
        let swinging = self.swing.swinging();
        // currentSwing.hand().asArm(mainArm).
        let swing_right_arm = swinging.then(|| {
            (self.swing.attack_hand() == horizon_client::first_person::Hand::Main)
                != self.settings.main_hand_left
        });
        // LivingEntity.isVisuallySwimming and Entity.isInWater.
        let visually_swimming =
            p.pose == Pose::Swimming || (!p.fall_flying && p.pose == Pose::FallFlying);
        let in_water = p.fluids.was_touching_water;
        let (yaw, pitch) = (self.yaw, self.pitch);
        let main = self.inventory[self.selected]
            .as_ref()
            .map(|s| s.item.clone());
        let off = self.offhand.as_ref().map(|s| s.item.clone());
        let riding = self.riding.is_some();
        let hurt = p.health.hurt_animation_time;
        let Some(m) = self.player_model.as_mut() else {
            return;
        };
        let at = Vec3::new(
            position.x,
            position.y - if crouching { 0.125 } else { 0.0 },
            position.z,
        );
        let (dx, dy, dz) = (
            at.x - m.position.x,
            at.y - m.position.y,
            at.z - m.position.z,
        );
        m.previous = m.position;
        m.position = at;
        if dx * dx + dy * dy + dz * dz > 100.0 {
            m.previous = at;
        }
        m.previous_yaw = m.yaw;
        m.previous_body_yaw = m.body_yaw;
        m.previous_head_yaw = m.head_yaw;
        m.previous_head_pitch = m.head_pitch;
        m.yaw = yaw;
        m.head_yaw = yaw;
        m.head_pitch = pitch;
        let mut target = m.body_yaw;
        if dx * dx + dz * dz > 0.002_500_000_2 {
            // LivingEntity.tick: walking backwards the body faces away from
            // the walk direction, not along it.
            let walk = (dz.atan2(dx) as f32).to_degrees() - 90.0;
            let facing = (wrap(yaw) - walk).abs();
            target = if 95.0 < facing && facing < 265.0 {
                walk - 180.0
            } else {
                walk
            };
        }
        if swinging {
            target = yaw;
        }
        // tickHeadTurn: ease toward the target, within 50 degrees of the head.
        m.body_yaw += wrap(target - m.body_yaw) * 0.3;
        let diff = wrap(yaw - m.body_yaw);
        if diff.abs() > 50.0 {
            m.body_yaw += diff - diff.signum() * 50.0;
        }
        // WalkAnimationState.update(min(distance * 4, 1), 0.4).
        let dist = (dx * dx + dz * dz + if flying { dy * dy } else { 0.0 }).sqrt() as f32;
        m.previous_limb_speed = m.limb_speed;
        m.limb_speed += ((dist * 4.0).min(1.0) - m.limb_speed) * 0.4;
        m.limb_position += m.limb_speed;
        m.age = m.age.wrapping_add(1);
        m.anim.attack_anim_o = attack.0;
        m.anim.attack_anim = attack.1;
        m.extra.main_hand = main;
        m.extra.off_hand = off;
        m.extra.crouching = crouching;
        m.extra.passenger = riding;
        m.velocity = velocity;
        m.extra.fall_flying = fall_flying;
        m.extra.fall_fly_ticks = fly_ticks;
        m.extra.auto_spin_attack = spinning;
        m.extra.main_hand_left = self.settings.main_hand_left;
        m.extra.using_item = using;
        m.extra.using_off_hand = using_off_hand;
        m.extra.blocking = blocking;
        m.extra.charged = charged;
        m.extra.ints[5] = charge_duration;
        m.extra.elytra.tick(fall_flying, crouching, velocity);
        m.extra.swim.tick(visually_swimming, in_water);
        m.extra.swing_right_arm = swing_right_arm;
        m.hurt_time = hurt;
    }
}
