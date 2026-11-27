//! Item rendering through the resolved item model pipeline: the
//! `ItemModelResolver` of `horizon-assets` fills an [`ItemRenderState`]
//! (`ItemStackRenderState`: layers of baked quads or special renderers, each
//! with its display transform, local transform, tints and foil), and
//! [`ItemRenderState::submit`] turns it into the quads `ItemFeatureRenderer`
//! and the special model renderers submit, in pose space. A sink then draws
//! them: [`gui`] into the HUD batch with the GUI item lighting, [`WorldSink`]
//! into entity vertices with the level's lighting.
pub mod gui;
mod special;

use crate::block_entity_render::ModelPart;
use glam::{Mat4, Vec3};
use horizon_assets::{
    animation::AnimationMetadata,
    item_bake::{BakedItemModel, SpriteLookup, SpriteMask, bake_item_model},
    item_model::{
        ClientItem, Components, Context, DisplayContext, FoilType, ItemEnv, ItemModels,
        LayerContent, LevelState, OwnerState, SpecialModel, StackView,
    },
    palette::PalettedPermutations,
    resolve::{MISSING_MODEL, ModelLibrary},
};
use horizon_world::nbt::Tag;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

/// The geometry of one layer.
#[derive(Clone, Debug)]
pub enum LayerGeometry {
    Quads(Arc<BakedItemModel>),
    Special {
        model: SpecialModel,
        argument: Value,
    },
}
/// `ItemStackRenderState.LayerRenderState`.
#[derive(Clone, Debug)]
pub struct LayerRender {
    pub geometry: LayerGeometry,
    pub tints: Vec<i32>,
    pub foil: FoilType,
    /// `applyTransform`: the display transform, then the local transform.
    pub matrix: Mat4,
    /// `particleMaterial` (atlas sprite name).
    pub particle: String,
    pub uses_block_light: bool,
    /// Untransformed extents (quad corners or the special model's parts).
    pub extents: Arc<Vec<Vec3>>,
}
/// `ItemStackRenderState`.
#[derive(Clone, Debug)]
pub struct ItemRenderState {
    pub display: DisplayContext,
    pub layers: Vec<LayerRender>,
    pub oversized_in_gui: bool,
    pub animated: bool,
}

/// The atlas name of a sprite identifier (`minecraft:item/apple` -> `item/apple`).
pub fn sprite_name(sprite: &str) -> &str {
    sprite.strip_prefix("minecraft:").unwrap_or(sprite)
}

// ---------------------------------------------------------------------------
// Sprites for baking
// ---------------------------------------------------------------------------

/// The alpha masks of item and block sprites, read from the texture files
/// (every unique animation frame) and the items atlas's palette sprites.
struct FileSprites {
    root: PathBuf,
    permutations: Vec<PalettedPermutations>,
    cache: Mutex<HashMap<String, Option<SpriteMask>>>,
    /// Sprites with a pixel that is neither fully transparent nor opaque
    /// (`Transparency.hasTranslucent`), filled in as sprites load.
    translucent: Mutex<HashMap<String, bool>>,
}
impl SpriteLookup for FileSprites {
    fn mask(&self, sprite: &str) -> Option<SpriteMask> {
        if let Ok(cache) = self.cache.lock()
            && let Some(hit) = cache.get(sprite)
        {
            return hit.clone();
        }
        let mask = self.load(sprite);
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(sprite.to_owned(), mask.clone());
        }
        mask
    }
}
fn argb_pixels(rgba: &[u8]) -> Vec<u32> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]]))
        .collect()
}
impl FileSprites {
    fn png(&self, sprite: &str, directory: &str) -> PathBuf {
        crate::atlas_sources::texture_path(&self.root, sprite, directory)
    }
    fn image(&self, sprite: &str, directory: &str) -> Option<(usize, usize, Vec<u32>)> {
        let (w, h, rgba) = crate::hud::decode(&self.png(sprite, directory)).ok()?;
        Some((w as usize, h as usize, argb_pixels(&rgba)))
    }
    fn load(&self, sprite: &str) -> Option<SpriteMask> {
        let png = self.png(sprite, "textures");
        let (iw, ih, argb) = match self.image(sprite, "textures") {
            Some(image) => image,
            None => {
                let p = self.permutations.iter().find_map(|s| s.find(sprite))?;
                let (w, h, mut pixels) = self.image(p.base, "textures")?;
                let (_, _, key) = self.image(p.palette_key, "textures/palettes")?;
                let (_, _, target) = self.image(p.palette, "textures/palettes")?;
                horizon_assets::palette::apply_palette(&mut pixels, &key, &target)?;
                (w, h, pixels)
            }
        };
        let translucent = argb.iter().any(|p| matches!(p >> 24, 1..=254));
        if let Ok(mut cache) = self.translucent.lock() {
            cache.insert(sprite.to_owned(), translucent);
        }
        let alpha: Vec<u8> = argb.iter().map(|p| (p >> 24) as u8).collect();
        let animation = fs::read_to_string(png.with_extension("png.mcmeta"))
            .ok()
            .and_then(|m| serde_json::from_str::<Value>(&m).ok())
            .and_then(|m| m.get("animation").map(Value::to_string))
            .and_then(|a| AnimationMetadata::from_json(&a).ok());
        let (w, h) = match &animation {
            Some(a) => {
                let [w, h] = a.frame_size(iw as i32, ih as i32);
                (w as usize, h as usize)
            }
            None => (iw, ih),
        };
        if w == 0 || h == 0 {
            return None;
        }
        let columns = iw / w;
        let total = columns * (ih / h);
        let clip = animation.and_then(|a| a.clip(total as u32).ok().flatten());
        let frames: Vec<usize> = match clip {
            Some(clip) => {
                let mut unique = Vec::new();
                for (index, _) in clip.sequence() {
                    if !unique.contains(&(*index as usize)) {
                        unique.push(*index as usize);
                    }
                }
                unique
            }
            None => vec![0],
        };
        let frames = frames
            .into_iter()
            .map(|f| {
                let (fx, fy) = (f % columns * w, f / columns * h);
                (0..h)
                    .flat_map(|y| (0..w).map(move |x| (x, y)))
                    .map(|(x, y)| alpha[(fy + y) * iw + fx + x] == 0)
                    .collect()
            })
            .collect();
        Some(SpriteMask {
            width: w,
            height: h,
            frames,
        })
    }
}

// ---------------------------------------------------------------------------
// The renderer
// ---------------------------------------------------------------------------

/// The baked item models and everything resolution needs.
pub struct ItemRenderer {
    pub models: ItemModels,
    library: Arc<ModelLibrary>,
    sprites: FileSprites,
    baked: Mutex<HashMap<String, Arc<BakedItemModel>>>,
    extents: Mutex<HashMap<String, Arc<Vec<Vec3>>>>,
    /// Entity model layers of the special renderers.
    parts: HashMap<String, ModelPart>,
    /// `colormap/grass.png` as ARGB.
    grass: Vec<i32>,
}

/// Every `items/*.json` client item definition, by identifier.
fn load_definitions(root: &Path) -> BTreeMap<String, ClientItem> {
    let mut out = BTreeMap::new();
    let Ok(namespaces) = fs::read_dir(root.join("assets")) else {
        return out;
    };
    for ns in namespaces.flatten() {
        let namespace = ns.file_name().to_string_lossy().into_owned();
        let Ok(files) = fs::read_dir(ns.path().join("items")) else {
            continue;
        };
        for f in files.flatten() {
            let path = f.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let name = path.file_stem().unwrap_or_default().to_string_lossy();
            if let Some(item) = fs::read_to_string(&path)
                .ok()
                .and_then(|t| ClientItem::from_json(&t).ok())
            {
                out.insert(format!("{namespace}:{name}"), item);
            }
        }
    }
    out
}

impl ItemRenderer {
    /// Reuse the immutable models prepared with the block resources at `root`.
    /// Defer item-only validation failures until after item definitions are loaded.
    pub fn load_with_library(
        root: &Path,
        library: std::result::Result<Arc<ModelLibrary>, String>,
    ) -> crate::Result<Self> {
        let prototypes: BTreeMap<String, Components> =
            serde_json::from_slice(&fs::read(root.join("items.json"))?)?;
        let models = ItemModels {
            definitions: load_definitions(root),
            prototypes,
        };
        let library = library?;
        let permutations = PalettedPermutations::from_atlas(
            &fs::read_to_string(root.join("assets/minecraft/atlases/items.json"))
                .unwrap_or_default(),
        );
        let parts = special::load_parts(root);
        let grass = crate::hud::decode(&root.join("assets/minecraft/textures/colormap/grass.png"))
            .map(|(_, _, rgba)| argb_pixels(&rgba).into_iter().map(|p| p as i32).collect())
            .unwrap_or_default();
        Ok(Self {
            models,
            library,
            sprites: FileSprites {
                root: root.to_path_buf(),
                permutations,
                cache: Mutex::default(),
                translucent: Mutex::default(),
            },
            baked: Mutex::default(),
            extents: Mutex::default(),
            parts,
            grass,
        })
    }
    /// `ModelBakery` item baking, cached per model.
    pub fn bake(&self, model: &str) -> Arc<BakedItemModel> {
        if let Ok(cache) = self.baked.lock()
            && let Some(hit) = cache.get(model)
        {
            return hit.clone();
        }
        let baked = bake_item_model(&self.library, &self.sprites, model)
            .or_else(|_| bake_item_model(&self.library, &self.sprites, MISSING_MODEL))
            .map(Arc::new)
            .unwrap_or_else(|_| {
                Arc::new(BakedItemModel {
                    quads: Vec::new(),
                    uses_block_light: false,
                    particle: horizon_assets::model::Material {
                        sprite: horizon_assets::item_bake::MISSING_SPRITE.to_owned(),
                        force_translucent: false,
                    },
                    transforms: Default::default(),
                })
            });
        if let Ok(mut cache) = self.baked.lock() {
            cache.insert(model.to_owned(), baked.clone());
        }
        baked
    }
    /// `CuboidItemModelWrapper.computeExtents`: the distinct quad corners.
    fn quad_extents(&self, model: &str, baked: &BakedItemModel) -> Arc<Vec<Vec3>> {
        if let Ok(cache) = self.extents.lock()
            && let Some(hit) = cache.get(model)
        {
            return hit.clone();
        }
        let mut out: Vec<Vec3> = Vec::new();
        for q in &baked.quads {
            for p in q.positions {
                let p = Vec3::from(p);
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        let out = Arc::new(out);
        if let Ok(mut cache) = self.extents.lock() {
            cache.insert(model.to_owned(), out.clone());
        }
        out
    }
    fn special_extents(&self, model: &SpecialModel) -> Arc<Vec<Vec3>> {
        let key = model.to_json().to_string();
        if let Ok(cache) = self.extents.lock()
            && let Some(hit) = cache.get(&key)
        {
            return hit.clone();
        }
        let out = Arc::new(special::extents(self, model));
        if let Ok(mut cache) = self.extents.lock() {
            cache.insert(key, out.clone());
        }
        out
    }
    /// `ItemModelResolver.updateForTopItem`.
    pub fn resolve(
        &self,
        stack: &StackView,
        display: DisplayContext,
        owner: Option<&OwnerState>,
        level: Option<&LevelState>,
        seed: i32,
        env: &dyn ItemEnv,
    ) -> ItemRenderState {
        let ctx = Context {
            display,
            owner,
            level,
            seed,
        };
        let resolved = self.models.resolve(stack, &ctx, env);
        let mut animated = false;
        let layers = resolved
            .layers
            .into_iter()
            .map(|layer| {
                let (geometry, baked, extents) = match layer.content {
                    LayerContent::Model { model } => {
                        let baked = self.bake(&model);
                        let extents = self.quad_extents(&model, &baked);
                        (LayerGeometry::Quads(baked.clone()), baked, extents)
                    }
                    LayerContent::Missing => {
                        let baked = self.bake(MISSING_MODEL);
                        let extents = self.quad_extents(MISSING_MODEL, &baked);
                        (LayerGeometry::Quads(baked.clone()), baked, extents)
                    }
                    LayerContent::Special {
                        base,
                        model,
                        argument,
                    } => {
                        let baked = self.bake(&base);
                        let extents = self.special_extents(&model);
                        (LayerGeometry::Special { model, argument }, baked, extents)
                    }
                };
                if layer.foil != FoilType::None {
                    animated = true;
                }
                LayerRender {
                    geometry,
                    tints: layer.tints,
                    foil: layer.foil,
                    matrix: glam_matrix(&baked.layer_matrix(display, &layer.transform)),
                    uses_block_light: baked.uses_block_light,
                    particle: sprite_name(&baked.particle.sprite).to_owned(),
                    extents,
                }
            })
            .collect();
        ItemRenderState {
            display,
            layers,
            oversized_in_gui: resolved.oversized_in_gui,
            animated,
        }
    }
    /// `GrassColor.get` over the loaded colormap.
    pub fn grass_color(&self, temperature: f64, downfall: f64) -> i32 {
        let rain = downfall * temperature;
        let x = ((1.0 - temperature) * 255.0) as i64;
        let y = ((1.0 - rain) * 255.0) as i64;
        let index = (y << 8 | x) as usize;
        self.grass.get(index).copied().unwrap_or(-65281)
    }
    pub(crate) fn part(&self, key: &str) -> Option<&ModelPart> {
        self.parts.get(key)
    }
}
/// A `horizon-assets` (row-major, JOML-exact) matrix as glam's.
pub fn glam_matrix(m: &horizon_assets::math::Mat4) -> Mat4 {
    Mat4::from_cols_array_2d(&m.rows()).transpose()
}

// ---------------------------------------------------------------------------
// Stacks
// ---------------------------------------------------------------------------

/// An NBT value as the JSON its component codec reads.
pub fn tag_json(tag: &Tag) -> Value {
    match tag {
        Tag::End => Value::Null,
        Tag::Byte(v) => Value::from(*v),
        Tag::Short(v) => Value::from(*v),
        Tag::Int(v) => Value::from(*v),
        Tag::Long(v) => Value::from(*v),
        Tag::Float(bits) => serde_json::Number::from_f64(f64::from(f32::from_bits(*bits)))
            .map_or(Value::Null, Value::Number),
        Tag::Double(bits) => {
            serde_json::Number::from_f64(f64::from_bits(*bits)).map_or(Value::Null, Value::Number)
        }
        Tag::ByteArray(a) => Value::Array(a.iter().map(|b| Value::from(*b as i8)).collect()),
        Tag::String(s) => Value::String(s.to_string_lossy()),
        Tag::List(l) => Value::Array(l.iter().map(tag_json).collect()),
        Tag::Compound(c) => Value::Object(
            c.iter()
                .map(|(k, v)| (k.to_string_lossy(), tag_json(v)))
                .collect(),
        ),
        Tag::IntArray(a) => Value::Array(a.iter().map(|v| Value::from(*v)).collect()),
        Tag::LongArray(a) => Value::Array(a.iter().map(|v| Value::from(*v)).collect()),
    }
}
/// The resolver's view of a client stack: its item, count and component patch.
pub fn stack_view(stack: &crate::items::ItemStack) -> StackView {
    let mut view = StackView::new(&stack.item, stack.count as i32);
    view.bundle_selected = stack.data().map_or(-1, |d| d.bundle_selected.0);
    if let Some(Tag::Compound(c)) = crate::item_data::components_tag(stack.damage, stack.data()) {
        for (k, v) in &c {
            let key = k.to_string_lossy();
            match key.strip_prefix('!') {
                Some(removed) => view.patch.insert(removed.to_owned(), None),
                None => view.patch.insert(key, Some(tag_json(v))),
            };
        }
    }
    view
}

// ---------------------------------------------------------------------------
// World drawing
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// A bad item model surfaces only once items.json has loaded, so a broken
    /// items.json still reports itself first.
    #[test]
    fn item_model_errors_wait_for_the_item_definitions() -> crate::Result<()> {
        let root =
            std::env::temp_dir().join(format!("horizon-item-model-errors-{}", std::process::id()));
        let model = root.join("assets/horizon/models/INVALID.json");
        fs::create_dir_all(model.parent().unwrap())?;
        fs::write(model, "{}")?;
        let library = crate::model_library::ParsedModels::load(&root).into_item_library();
        assert!(library.is_err());
        fs::write(root.join("items.json"), "{")?;
        let load = || {
            ItemRenderer::load_with_library(&root, library.clone())
                .err()
                .unwrap()
                .to_string()
        };
        let broken_items = load();
        fs::write(root.join("items.json"), "{}")?;
        let bad_model = load();
        let _ = fs::remove_dir_all(&root);
        assert!(!broken_items.starts_with("item models:"));
        assert!(bad_model.starts_with("item models:"));
        Ok(())
    }

    #[test]
    fn nbt_components_read_as_json() {
        let tag = Tag::compound([
            ("a", Tag::Int(3)),
            ("b", Tag::List(vec![Tag::String("x".into())])),
            ("c", Tag::Float(1.5f32.to_bits())),
        ]);
        assert_eq!(
            tag_json(&tag),
            serde_json::json!({"a": 3, "b": ["x"], "c": 1.5})
        );
    }
}
