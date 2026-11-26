//! `SpecialModelRenderer`s: items drawn with entity models (chests, shulker
//! boxes, banners, heads, shields, tridents, decorated pots, conduits, bells,
//! books and copper golem statues). Each submits its model at the layer pose
//! as vanilla's `submit` does (no block-entity placement transform) and
//! reports `getExtents` over the same posed parts.
use super::ItemRenderer;
use crate::block_entity_render::ModelPart;
use glam::{Mat4, Quat, Vec3};
use horizon_assets::item_model::{SpecialKind, SpecialModel};
use serde_json::Value;
use std::{collections::HashMap, f32::consts::PI, path::Path};

/// Entity model layers the special renderers bake (`ModelLayers`).
const LAYERS: [(&str, &str); 22] = [
    ("chest", "minecraft:chest#main"),
    ("double_chest_left", "minecraft:double_chest_left#main"),
    ("double_chest_right", "minecraft:double_chest_right#main"),
    ("shulker_box", "minecraft:shulker_box#main"),
    ("standing_banner", "minecraft:standing_banner#main"),
    ("standing_banner_flag", "minecraft:standing_banner#flag"),
    ("wall_banner", "minecraft:wall_banner#main"),
    ("wall_banner_flag", "minecraft:wall_banner#flag"),
    ("skeleton", "minecraft:skeleton_skull#main"),
    ("wither_skeleton", "minecraft:wither_skeleton_skull#main"),
    ("zombie", "minecraft:zombie_head#main"),
    ("creeper", "minecraft:creeper_head#main"),
    ("dragon", "minecraft:dragon_skull#main"),
    ("piglin", "minecraft:piglin_head#main"),
    ("player", "minecraft:player_head#main"),
    ("bell", "minecraft:bell#main"),
    ("book", "minecraft:book#main"),
    ("conduit_shell", "minecraft:conduit#shell"),
    ("decorated_pot_base", "minecraft:decorated_pot_base#main"),
    ("decorated_pot_sides", "minecraft:decorated_pot_sides#main"),
    ("shield", "minecraft:shield#main"),
    ("trident", "minecraft:trident#main"),
];
const COPPER_GOLEM_LAYERS: [(&str, &str); 4] = [
    ("copper_golem", "minecraft:copper_golem#main"),
    (
        "copper_golem_running",
        "minecraft:copper_golem_running#main",
    ),
    (
        "copper_golem_sitting",
        "minecraft:copper_golem_sitting#main",
    ),
    ("copper_golem_star", "minecraft:copper_golem_star#main"),
];

pub(super) fn load_parts(root: &Path) -> HashMap<String, ModelPart> {
    let Ok(all) = std::fs::read(root.join("entity-models.json"))
        .map_err(|_| ())
        .and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|_| ()))
    else {
        return HashMap::new();
    };
    LAYERS
        .iter()
        .chain(COPPER_GOLEM_LAYERS.iter())
        .filter_map(|(key, layer)| {
            all.get(*layer)
                .map(|v| ((*key).to_owned(), ModelPart::parse(v)))
        })
        .collect()
}

/// A pose override for a named part (`setupAnim`).
type Adjust<'a> = &'a dyn Fn(&str, &mut [f32; 9]);
fn none(_: &str, _: &mut [f32; 9]) {}

/// `ModelPart.translateAndRotate`.
fn part_matrix(parent: Mat4, p: &[f32; 9]) -> Mat4 {
    parent
        * Mat4::from_translation(Vec3::new(p[0], p[1], p[2]) / 16.0)
        * Mat4::from_quat(Quat::from_euler(glam::EulerRot::ZYX, p[5], p[4], p[3]))
        * Mat4::from_scale(Vec3::new(p[6], p[7], p[8]))
}
/// `ModelPart.getExtentsForGui` from an identity pose.
fn extents_of(part: &ModelPart, name: &str, parent: Mat4, adjust: Adjust, out: &mut Vec<Vec3>) {
    if part.polygons.is_empty() && part.children.is_empty() {
        return;
    }
    let mut p = part.pose;
    adjust(name, &mut p);
    let m = part_matrix(parent, &p);
    for poly in &part.polygons {
        for v in poly {
            let q = m.transform_point3(Vec3::new(v[0], v[1], v[2]) / 16.0);
            if !out.contains(&q) {
                out.push(q);
            }
        }
    }
    for (child_name, child) in &part.children {
        extents_of(child, child_name, m, adjust, out);
    }
}
fn child<'a>(part: &'a ModelPart, name: &str) -> Option<&'a ModelPart> {
    part.child(name)
}

/// The chest model of a chest type, with `ChestModel.setupAnim(openness)`.
fn chest_model(chest_type: &str) -> &'static str {
    match chest_type {
        "left" => "double_chest_left",
        "right" => "double_chest_right",
        _ => "chest",
    }
}
fn skull_layer(kind: &str) -> &'static str {
    match kind {
        "skeleton" => "skeleton",
        "wither_skeleton" => "wither_skeleton",
        "zombie" => "zombie",
        "creeper" => "creeper",
        "dragon" => "dragon",
        "piglin" => "piglin",
        _ => "player",
    }
}
/// `SkullModelBase.setupAnim` for a head at `animation` (yRot = xRot = 0).
fn skull_pose(kind: &str, animation: f32) -> impl Fn(&str, &mut [f32; 9]) + '_ {
    move |name: &str, p: &mut [f32; 9]| match (kind, name) {
        (_, "head") => {
            p[3] = 0.0;
            p[4] = 0.0;
        }
        ("dragon", "jaw") => p[3] = (((animation * PI * 0.2) as f64).sin() as f32 + 1.0) * 0.2,
        ("piglin", "left_ear") => {
            p[5] = -((((animation * PI * 0.2 * 1.2) as f64).cos() + 2.5) as f32) * 0.2;
        }
        ("piglin", "right_ear") => {
            p[5] = ((((animation * PI * 0.2) as f64).cos() + 2.5) as f32) * 0.2;
        }
        _ => {}
    }
}
fn shulker_pose(progress: f32) -> impl Fn(&str, &mut [f32; 9]) {
    move |name: &str, p: &mut [f32; 9]| {
        if name == "lid" {
            p[0] = 0.0;
            p[1] = 24.0 - progress * 0.5 * 16.0;
            p[2] = 0.0;
            p[4] = 270.0 * progress * (PI / 180.0);
        }
    }
}
fn chest_pose(open: f32) -> impl Fn(&str, &mut [f32; 9]) {
    move |name: &str, p: &mut [f32; 9]| {
        if name == "lid" || name == "lock" {
            p[3] = -(open * (PI / 2.0));
        }
    }
}
/// `BannerFlagModel.setupAnim(0)`.
fn flag_pose(name: &str, p: &mut [f32; 9]) {
    if name == "flag" {
        p[3] = (-0.0125 + 0.01 * (2.0 * PI * 0.0f32).cos()) * PI;
    }
}
/// `BookModel.setupAnim`.
fn book_pose(openness: f32, flip1: f32, flip2: f32) -> impl Fn(&str, &mut [f32; 9]) {
    move |name: &str, p: &mut [f32; 9]| match name {
        "left_lid" => p[4] = PI + openness,
        "right_lid" => p[4] = -openness,
        "left_pages" => {
            p[4] = openness;
            p[0] = openness.sin();
        }
        "right_pages" => {
            p[4] = -openness;
            p[0] = openness.sin();
        }
        "flip_page1" => {
            p[4] = openness - openness * 2.0 * flip1;
            p[0] = openness.sin();
        }
        "flip_page2" => {
            p[4] = openness - openness * 2.0 * flip2;
            p[0] = openness.sin();
        }
        _ => {}
    }
}
/// `CopperGolemStatueModel.setupAnim`.
fn statue_pose(name: &str, p: &mut [f32; 9]) {
    if name == "root" {
        p[1] = 0.0;
        p[5] = PI;
    }
}
fn statue_layer(pose: &str) -> &'static str {
    match pose {
        "running" => "copper_golem_running",
        "sitting" => "copper_golem_sitting",
        "star" => "copper_golem_star",
        _ => "copper_golem",
    }
}

/// `SpecialModelRenderer.getExtents`.
pub(super) fn extents(renderer: &ItemRenderer, model: &SpecialModel) -> Vec<Vec3> {
    let mut out = Vec::new();
    let id = Mat4::IDENTITY;
    let mut add = |key: &str, adjust: Adjust| {
        if let Some(part) = renderer.part(key) {
            extents_of(part, "root", id, adjust, &mut out);
        }
    };
    match &model.kind {
        SpecialKind::Bell => add("bell", &none),
        SpecialKind::Banner { .. } => {
            // `BannerRenderer.getExtents`: the standing models only.
            add("standing_banner", &none);
            add("standing_banner_flag", &flag_pose);
        }
        SpecialKind::Book {
            open_angle,
            page1,
            page2,
        } => add(
            "book",
            &book_pose(open_angle * (PI / 180.0), *page1, *page2),
        ),
        SpecialKind::Conduit => add("conduit_shell", &none),
        SpecialKind::Chest {
            openness,
            chest_type,
            ..
        } => add(chest_model(chest_type), &chest_pose(*openness)),
        SpecialKind::CopperGolemStatue { pose, .. } => add(statue_layer(pose), &statue_pose),
        SpecialKind::DecoratedPot => {
            if let Some(base) = renderer.part("decorated_pot_base") {
                // `neck`, `top` and `bottom` are posed from an identity stack.
                for name in ["neck", "top", "bottom"] {
                    if let Some(part) = child(base, name) {
                        extents_of(part, name, id, &none, &mut out);
                    }
                }
            }
        }
        SpecialKind::EndCube { .. } => {}
        SpecialKind::Head {
            kind, animation, ..
        } => add(skull_layer(kind), &skull_pose(kind, *animation)),
        SpecialKind::PlayerHead => add("player", &none),
        SpecialKind::ShulkerBox { openness, .. } => add("shulker_box", &shulker_pose(*openness)),
        SpecialKind::Shield => add("shield", &none),
        SpecialKind::Trident => add("trident", &none),
    }
    out
}
