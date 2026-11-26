//! Model animation: ports of each vanilla `EntityModel.setupAnim` family
//! (quadrupeds, humanoids, creepers, spiders, chickens...) applied to a map of
//! part poses before the model is drawn.
use super::{Part, keyframes::AnimationState};
use std::collections::HashMap;

/// The setupAnim implementation a model uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anim {
    Nautilus,
    HappyGhast,
    Sniffer,
    Breeze,
    Warden,
    Allay,
    Creaking,
    Camel,
    Wither,
    Vex,
    Ravager,
    Guardian,
    Shulker,
    Quadruped,
    Sheep,
    Chicken,
    Zombie,
    Drowned,
    ZombieVillager,
    Skeleton,
    Creeper,
    Spider,
    Enderman,
    Silverfish,
    Slime,
    MagmaCube,
    Witch,
    Villager,
    IronGolem,
    SnowGolem,
    CopperGolem,
    Illager,
    Piglin,
    ZombifiedPiglin,
    Hoglin,
    Wolf,
    Feline,
    Fox,
    Rabbit,
    Horse,
    Llama,
    Goat,
    Panda,
    PolarBear,
    Parrot,
    Bee,
    Turtle,
    Armadillo,
    Frog,
    Axolotl,
    Squid,
    Fish,
    Pufferfish,
    Dolphin,
    Bat,
    Phantom,
    Ghast,
    Blaze,
    Strider,
    /// The humanoid model of the earlier simple renderer.
    Humanoid {
        zombie_arms: bool,
    },
}
/// Client-side animation clocks advanced each tick.
#[derive(Clone, Debug, Default)]
pub struct AnimState {
    pub attack_anim: f32,
    pub attack_anim_o: f32,
    pub flap: f32,
    pub flap_o: f32,
    pub flap_speed: f32,
    pub flap_speed_o: f32,
    pub flapping: f32,
    /// Generic per-species keyframe animation states.
    pub states: [AnimationState; 6],
    /// Per-species animation values (tail wag, squid tentacles, squish...)
    /// with their previous-tick copies for interpolation.
    pub v: [f32; 8],
    pub vo: [f32; 8],
}
impl AnimState {
    pub fn begin_tick(&mut self) {
        self.attack_anim_o = self.attack_anim;
        self.vo = self.v;
    }
    /// Interpolated value `i` at the render partial tick.
    pub fn lerp(&self, i: usize, partial: f32) -> f32 {
        self.vo[i] + (self.v[i] - self.vo[i]) * partial
    }
    pub fn end_tick(&mut self, _swinging: bool) {}
}
/// Part poses for one draw, keyed by part name.
#[derive(Clone, Default)]
pub struct Poses {
    map: HashMap<String, [f32; 9]>,
    dummy: [f32; 9],
    /// Parts with `skipDraw` set: their own cubes are not drawn, children are.
    skipped: std::collections::HashSet<String>,
    /// Parts with `visible` cleared: neither they nor their children draw.
    hidden: std::collections::HashSet<String>,
}
impl Poses {
    pub fn new(model: &Part) -> Self {
        let mut poses = Self::default();
        poses.reset(model);
        poses
    }
    /// Refills this for another model, keeping the map and the sets' capacity:
    /// a mob draws its body and every equipment layer each frame.
    pub fn reset(&mut self, model: &Part) {
        fn walk(p: &Part, map: &mut HashMap<String, [f32; 9]>) {
            map.insert(p.name.clone(), p.pose);
            for c in &p.children {
                walk(c, map);
            }
        }
        self.map.clear();
        self.dummy = [0.0; 9];
        self.skipped.clear();
        self.hidden.clear();
        walk(model, &mut self.map);
    }
    pub fn get(&mut self, name: &str) -> &mut [f32; 9] {
        match self.map.get_mut(name) {
            Some(p) => p,
            None => &mut self.dummy,
        }
    }
    pub fn pose(&self, part: &Part) -> [f32; 9] {
        self.map.get(&part.name).copied().unwrap_or(part.pose)
    }
    pub fn skip_draw(&mut self, name: &str) {
        self.skipped.insert(name.to_owned());
    }
    pub fn skips(&self, part: &Part) -> bool {
        self.skipped.contains(&part.name)
    }
    pub fn has(&self, name: &str) -> bool {
        self.map.contains_key(name)
    }
    pub fn xrot(&mut self, n: &str, v: f32) {
        self.get(n)[3] = v;
    }
    pub fn yrot(&mut self, n: &str, v: f32) {
        self.get(n)[4] = v;
    }
    pub fn zrot(&mut self, n: &str, v: f32) {
        self.get(n)[5] = v;
    }
    /// `ModelPart.visible = v`; a hidden part and its children do not draw.
    pub fn visible(&mut self, n: &str, v: bool) {
        if v {
            self.hidden.remove(n);
        } else if self.map.contains_key(n) {
            self.hidden.insert(n.to_owned());
        }
    }
    pub fn is_hidden(&self, part: &Part) -> bool {
        self.hidden.contains(&part.name)
    }
}
