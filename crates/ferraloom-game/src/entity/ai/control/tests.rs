use super::{
    Operation,
    runtime::{self, ControlRuntime},
};
use crate::entity::{Mob, animal::Cow};
use ferraloom_util::Vec3;
use std::convert::Infallible;

// These are the explicit services of the Java harness, not a second live Mob.
struct Fixture {
    cow: Cow,
    walkable: bool,
    done: bool,
    liquid: bool,
    affected: bool,
    block: &'static str,
}
impl Fixture {
    fn new() -> Self {
        let mut cow = Cow::default();
        let base = &mut cow.as_mob_mut().parent.parent;
        base.position = Vec3::new(8.5, 64.0, 8.5);
        base.old_position = base.position;
        base.eye_height = 1.3;
        base.yaw = 0.0;
        base.pitch = 0.0;
        base.on_ground = false;
        cow.as_mob_mut().parent.y_head_rot = 0.0;
        cow.as_mob_mut().parent.y_body_rot = 0.0;
        Self {
            cow,
            walkable: true,
            done: true,
            liquid: false,
            affected: true,
            block: "air",
        }
    }
    fn apply(&mut self, input: &str) {
        let a: Vec<_> = input.split(',').collect();
        let f = |i: usize| a[i].parse::<f32>().unwrap();
        let d = |i: usize| a[i].parse::<f64>().unwrap();
        let position = self.cow.as_mob().parent.parent.position;
        match a[0] {
            "strafe" => self.cow.as_mob_mut().ai.move_control.strafe(f(1), f(2)),
            "want" => self
                .cow
                .as_mob_mut()
                .ai
                .move_control
                .set_wanted_position(position + Vec3::new(d(1), d(2), d(3)), d(4)),
            "move" => runtime::tick_move(self).unwrap(),
            "wait" => self.cow.as_mob_mut().ai.move_control.set_wait(),
            "jump" => runtime::tick_jump(self).unwrap(),
            "ground" => self.cow.as_mob_mut().parent.parent.on_ground = a[1].parse().unwrap(),
            "liquid" => self.liquid = a[1].parse().unwrap(),
            "affected" => self.affected = a[1].parse().unwrap(),
            "walkable" => self.walkable = a[1].parse().unwrap(),
            "done" => self.done = a[1].parse().unwrap(),
            "yaw" => self.cow.as_mob_mut().parent.parent.set_y_rot(f(1)),
            "pitch" => self.cow.as_mob_mut().parent.parent.set_x_rot(f(1)),
            "head" => self.cow.as_mob_mut().parent.y_head_rot = f(1),
            "body" => self.cow.as_mob_mut().parent.y_body_rot = f(1),
            "position" => {
                self.cow.as_mob_mut().parent.parent.position = Vec3::new(d(1), d(2), d(3))
            }
            "old" => {
                let base = &mut self.cow.as_mob_mut().parent.parent;
                base.old_position.x = d(1);
                base.old_position.z = d(2);
            }
            "look" => self.cow.as_mob_mut().ai.look_control.set_look_at(
                Vec3::new(
                    position.x + d(1),
                    position.y + f64::from(1.3_f32) + d(2),
                    position.z + d(3),
                ),
                f(4),
                f(5),
            ),
            "lookTick" => runtime::tick_look(self).unwrap(),
            "bodyTick" => runtime::tick_body(self).unwrap(),
            "block" => {
                self.block = match a[1] {
                    "air" => "air",
                    "slab" => "slab",
                    "door" => "door",
                    "fence" => "fence",
                    _ => panic!("{input}"),
                }
            }
            _ => panic!("{input}"),
        }
    }
    fn snapshot(&self) -> String {
        let m = self.cow.as_mob();
        let l = &m.parent;
        let base = &l.parent;
        let operation = match m.ai.move_control.operation {
            Operation::Wait => "WAIT",
            Operation::MoveTo => "MOVE_TO",
            Operation::Strafe => "STRAFE",
            Operation::Jumping => "JUMPING",
        };
        let optional = |v: Option<f32>| v.map(hex).unwrap_or_else(|| "-".into());
        let eye = Vec3::new(
            base.position.x,
            base.position.y + f64::from(base.eye_height),
            base.position.z,
        );
        format!(
            "{operation}:{},{},{}:{},{},{},{}:{}:{}:{},{}:{},{}",
            hex(l.speed),
            hex(l.zza),
            hex(l.xxa),
            hex(base.yaw),
            hex(base.pitch),
            hex(l.y_head_rot),
            hex(l.y_body_rot),
            l.jumping,
            m.ai.look_control.look_at_cooldown,
            optional(m.ai.look_control.y_rot_d(base.position)),
            optional(m.ai.look_control.x_rot_d(eye)),
            m.ai.body_rotation_control.head_stable_time,
            hex(m.ai.body_rotation_control.last_stable_y_head_rot)
        )
    }
}
fn hex(value: f32) -> String {
    format!("{:x}", value.to_bits())
}
impl ControlRuntime for Fixture {
    type Error = Infallible;
    fn mob(&self) -> Result<&Mob, Infallible> {
        Ok(self.cow.as_mob())
    }
    fn mob_mut(&mut self) -> Result<&mut Mob, Infallible> {
        Ok(self.cow.as_mob_mut())
    }
    fn movement_speed(&self) -> Result<f64, Infallible> {
        Ok(0.2)
    }
    fn max_up_step(&self) -> Result<f32, Infallible> {
        Ok(0.6)
    }
    fn is_walkable(&mut self, _dx: f32, _dz: f32) -> Result<bool, Infallible> {
        Ok(self.walkable)
    }
    fn collision_height(&mut self) -> Result<Option<f64>, Infallible> {
        Ok(match self.block {
            "slab" => Some(64.5),
            "door" => Some(65.0),
            "fence" => Some(65.5),
            _ => None,
        })
    }
    fn collision_is_door_or_fence(&self) -> Result<bool, Infallible> {
        // The Java Bootstrap-only harness has no bound datapack tags.
        Ok(false)
    }
    fn is_in_liquid(&self) -> Result<bool, Infallible> {
        Ok(self.liquid)
    }
    fn is_affected_by_fluids(&self) -> Result<bool, Infallible> {
        Ok(self.affected)
    }
    fn navigation_is_done(&self) -> Result<bool, Infallible> {
        Ok(self.done)
    }
    fn max_head_y_rot(&self) -> Result<i32, Infallible> {
        Ok(75)
    }
    fn carrying_mob_passenger(&self) -> Result<bool, Infallible> {
        Ok(false)
    }
    fn set_speed(&mut self, speed: f32) -> Result<(), Infallible> {
        self.cow.as_mob_mut().parent.speed = speed;
        self.set_zza(speed)
    }
    fn set_zza(&mut self, value: f32) -> Result<(), Infallible> {
        self.cow.as_mob_mut().parent.zza = value;
        Ok(())
    }
    fn set_xxa(&mut self, value: f32) -> Result<(), Infallible> {
        self.cow.as_mob_mut().parent.xxa = value;
        Ok(())
    }
    fn set_jumping(&mut self, value: bool) -> Result<(), Infallible> {
        self.cow.as_mob_mut().parent.jumping = value;
        Ok(())
    }
}

#[test]
fn native_control_state_machines_match_move_look_jump_and_body_transitions() {
    let mut fixture = Fixture::new();
    let mut sequence = "";
    let mut checked = 0;
    for row in include_str!("../../../../tests/java/ai/control/vectors.tsv")
        .lines()
        .filter(|r| r.starts_with("control\t"))
    {
        let f: Vec<_> = row.split('\t').collect();
        if sequence != f[1] {
            fixture = Fixture::new();
            sequence = f[1];
        }
        fixture.apply(f[2]);
        assert_eq!(fixture.snapshot(), f[3], "{row}");
        checked += 1;
    }
    assert_eq!(checked, 83);
}
