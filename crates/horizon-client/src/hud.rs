//! 2D overlay: one atlas holding GUI sprites, the bitmap font, item and block
//! textures, and a batch of textured quads in GUI pixels (vanilla's automatic
//! GUI scale is applied when drawing).
use crate::Result;
use horizon_assets::animation::AnimationMetadata;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

/// A sprite's atlas region, and its size in GUI pixels: the size of the
/// default resources' texture of that name, whatever the resolution of the
/// pack that replaced it (vanilla's GUI code draws in those units: `blit`
/// divides by the texture size it is given, sprites by their `gui.scaling`
/// size). Textures the default resources lack keep their own size.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub w: u32,
    pub h: u32,
}
impl Rect {
    /// The region's size in texels of an atlas this large.
    pub fn texels(&self, (width, height): (u32, u32)) -> (u32, u32) {
        (
            ((self.u1 - self.u0) * width as f32).round() as u32,
            ((self.v1 - self.v0) * height as f32).round() as u32,
        )
    }
}
/// `TextureMetadataSection`'s `blur` and `clamp`: how vanilla samples a
/// standalone texture (`ReloadableTexture.setSampler`: linear rather than
/// nearest filtering, clamp-to-edge rather than repeat addressing).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureFlags {
    pub blur: bool,
    pub clamp: bool,
}
impl TextureFlags {
    /// The `texture` section of a `.png.mcmeta` file, if it sets either flag.
    pub fn from_mcmeta(json: &str) -> Option<Self> {
        let meta: serde_json::Value = serde_json::from_str(json).ok()?;
        let section = meta.get("texture")?;
        let flag = |key| section.get(key).and_then(serde_json::Value::as_bool) == Some(true);
        let flags = Self {
            blur: flag("blur"),
            clamp: flag("clamp"),
        };
        (flags != Self::default()).then_some(flags)
    }
}
pub struct HudAtlas {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub sprites: HashMap<String, Rect>,
    /// Sprites with a 0 < alpha < 255 texel (`SpriteContents.transparency().hasTranslucent`),
    /// which puts their particles on vanilla's translucent layer.
    pub translucent: HashSet<String>,
    /// Sprites whose `.png.mcmeta` sets `blur` or `clamp`; the others
    /// sample as vanilla's defaults (nearest, repeat).
    pub flags: HashMap<String, TextureFlags>,
    /// Every animated block or item sprite the atlas packs: its atlas rect
    /// and the mcmeta-driven frame list, re-uploaded into the rect as the
    /// frames turn (vanilla's stitched atlas animates all of these).
    pub sprite_animations: Vec<(Rect, crate::sprites::SpriteAnimation)>,
    /// The default font (bitmap sheets in this atlas, unifont rasterized on demand).
    pub font: crate::font::Font,
    /// `misc/enchanted_glint_item` as its own image: vanilla's `GlintSampler`
    /// repeats it per pixel, which one sprite inside this atlas cannot.
    pub glint: (Vec<u8>, u32, u32),
    /// `misc/enchanted_glint_armor`, the armour layers' `GlintSampler`.
    pub glint_armor: (Vec<u8>, u32, u32),
}
/// Atlas rows reserved for glyphs rasterized at run time.
pub const GLYPH_ROWS: u32 = 512;
/// The largest atlas side: the 2D texture limit of the GPUs Horizon runs on
/// (Apple silicon, RADV and Intel allow 16384; NVIDIA 32768).
pub const MAX_ATLAS_SIZE: u32 = 16384;
/// 128x128 atlas slots for map textures. A lobby paints its signs as walls of
/// framed maps, so the pool holds a wall; too few slots re-upload every map
/// every frame. 26.3's `MapTextureManager` keeps a texture per map instead.
pub const MAP_SLOTS: usize = 256;
/// The atlas slot the minimap extra draws into.
pub const MINIMAP_SPRITE: &str = "horizon/minimap";
/// Atlas slots for downloaded player skins (`skins/<sha1>`), 64x64 each.
/// Vanilla registers a texture per skin. Here a skin evicted from its slot
/// is not stored again, and its player falls back to the default skin, so
/// the slots must cover a busy server's player list.
pub const SKIN_SLOTS: usize = 128;
/// Reused by the visible rows of the classic world selector.
pub const WORLD_ICON_SLOTS: usize = 32;
/// Atlas slots for the pack icons on the pack screen, one per icon drawn
/// (`PackSelectionScreen.loadPackIcon` registers a texture per pack): two lists
/// of 36-pixel rows at GUI scale 2 on a 4K screen still fit.
pub const PACK_ICON_SLOTS: usize = 64;
/// A pack icon slot's side: vanilla's own icons are 128x128.
pub const PACK_ICON_SIZE: u32 = 128;
/// Atlas slots for armour trim textures made at run time (`trims`), each the
/// size of the largest trim texture.
pub const TRIM_SLOTS: usize = 32;
pub fn decode(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    decode_bytes(&fs::read(path)?)
}
/// A PNG in memory as RGBA pixels.
pub fn decode_bytes(data: &[u8]) -> Result<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut bytes = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut bytes)?;
    let channels = info.color_type.samples();
    let mut rgba = Vec::with_capacity((info.width * info.height * 4) as usize);
    for p in bytes[..info.buffer_size()].chunks_exact(channels) {
        rgba.extend(match channels {
            4 => [p[0], p[1], p[2], p[3]],
            3 => [p[0], p[1], p[2], 255],
            2 => [p[0], p[0], p[0], p[1]],
            _ => [p[0], p[0], p[0], 255],
        });
    }
    Ok((info.width, info.height, rgba))
}
/// A PNG's width and height, from its header alone.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut header = [0u8; 24];
    fs::File::open(path).ok()?.read_exact(&mut header).ok()?;
    let field = |at: usize| u32::from_be_bytes(header[at..at + 4].try_into().unwrap_or_default());
    (header[..8] == *b"\x89PNG\r\n\x1a\n" && header[12..16] == *b"IHDR")
        .then(|| (field(16), field(20)))
}
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().is_some_and(|e| e == "png") {
            out.push(path);
        }
    }
}
/// A packed atlas image: sprite name, size and RGBA pixels.
type PackedImage = (String, u32, u32, Vec<u8>);
/// A frame of an animated strip at `x, y`, nearest-resampled to the longer
/// side of the frame: array layers are square, as `Sprites::load` keeps them
/// for the world's sprite arrays.
fn strip_frame(pixels: &[u8], stride: u32, x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
    let side = w.max(h);
    let mut out = Vec::with_capacity((side * side * 4) as usize);
    for row in 0..side {
        let source_row = y + row * h / side;
        for column in 0..side {
            let at = ((source_row * stride + x + column * w / side) * 4) as usize;
            out.extend_from_slice(&pixels[at..at + 4]);
        }
    }
    out
}
/// `image` inside a one-texel border copied from its opposite edges, the
/// texels repeat addressing reads past each edge.
fn wrap_border((name, w, h, data): &PackedImage) -> PackedImage {
    let (bw, bh) = (w + 2, h + 2);
    let mut out = Vec::with_capacity((bw * bh * 4) as usize);
    for y in 0..bh {
        let row = (y + h - 1) % h;
        for x in 0..bw {
            let at = ((row * w + (x + w - 1) % w) * 4) as usize;
            out.extend_from_slice(&data[at..at + 4]);
        }
    }
    (name.clone(), bw, bh, out)
}
/// Sprites the atlas definitions (`atlases/*.json`) add beyond the texture
/// files already packed: generated ones (`paletted_permutations` such as the
/// armour trim sprites, `unstitch` regions) and files listed under another id.
fn atlas_sprites(root: &Path, packed: &[PackedImage]) -> Vec<PackedImage> {
    use crate::atlas_sources::{self, SpriteOrigin};
    let have: std::collections::HashSet<&str> = packed.iter().map(|i| i.0.as_str()).collect();
    let textures = atlas_sources::list_textures(root);
    // Each new name with its origins in the atlases' order: the first that
    // loads is the sprite. They load at once (a content pack's atlases list
    // tens of thousands of files), and come out in the order a load of one
    // after another would give.
    let mut names: Vec<(String, Vec<(usize, SpriteOrigin)>)> = Vec::new();
    let mut slots = std::collections::HashMap::new();
    let mut order = 0;
    for (atlas, sources) in atlas_sources::atlases(root) {
        // The other atlases' renamed files (GUI sprites, particles...) are
        // looked up by their texture paths here.
        let item_models = atlas == "minecraft:blocks" || atlas == "minecraft:items";
        for (id, origin) in atlas_sources::list(&sources, &textures) {
            if !item_models && matches!(origin, SpriteOrigin::File(_)) {
                continue;
            }
            let name = id.strip_prefix("minecraft:").unwrap_or(&id).to_owned();
            if have.contains(name.as_str()) {
                continue;
            }
            // A vanilla file is packed under its own name already: only a
            // renamed one is new. Other namespaces' files (content mods) are
            // never in the walked directories.
            if let SpriteOrigin::File(texture) = &origin
                && texture.strip_prefix("minecraft:") == Some(name.as_str())
            {
                continue;
            }
            let slot = *slots.entry(name.clone()).or_insert_with(|| {
                names.push((name, Vec::new()));
                names.len() - 1
            });
            names[slot].1.push((order, origin));
            order += 1;
        }
    }
    let load = |texture: &str, directory: &str| {
        decode(&atlas_sources::texture_path(root, texture, directory)).ok()
    };
    let first = |(name, origins): &(String, Vec<(usize, SpriteOrigin)>)| {
        origins.iter().find_map(|(order, origin)| {
            let image = match origin {
                SpriteOrigin::File(texture) => {
                    let path = texture.strip_prefix("minecraft:").unwrap_or(texture);
                    load(texture, "textures").map(|(w, h, p)| {
                        // Animated strips keep their first frame, like the packed files.
                        if h > w && (path.starts_with("block/") || path.starts_with("item/")) {
                            (w, w, p[..(w * w * 4) as usize].to_vec())
                        } else {
                            (w, h, p)
                        }
                    })
                }
                generated => atlas_sources::generate(generated, &load),
            };
            image.map(|(w, h, pixels)| (*order, (name.clone(), w, h, pixels)))
        })
    };
    let mut out: Vec<_> = crate::par::par_map(&names, first)
        .into_iter()
        .flatten()
        .collect();
    out.sort_unstable_by_key(|(order, _)| *order);
    out.into_iter().map(|(_, image)| image).collect()
}
impl HudAtlas {
    /// A font glyph's atlas cell (8x8 GUI pixels) and advance.
    pub fn glyph(&self, ch: char) -> Option<(Rect, f32)> {
        let g = self.font.glyph(ch);
        g.rect.map(|rect| (rect, g.advance))
    }
    /// A font glyph's atlas cell (none for blank glyphs such as the space) and advance.
    pub fn glyph_cell(&self, ch: char) -> (Option<Rect>, f32) {
        let g = self.font.glyph(ch);
        (g.rect, g.advance)
    }
    pub fn build(root: &Path) -> Result<Self> {
        Self::build_over(root, root)
    }
    /// The atlas of a resource root that layers packs over `defaults` (the
    /// game's own resources), whose textures give the sprites' GUI sizes.
    pub fn build_over(root: &Path, defaults: &Path) -> Result<Self> {
        Self::pack(
            root,
            defaults,
            &[
                "gui",
                "item",
                "block",
                "font",
                "environment",
                "misc",
                "painting",
                "entity",
                "particle",
                "mob_effect",
                "map",
            ],
            true,
        )
    }
    /// Empty slots written at run time: map textures (game/maps.rs), the
    /// minimap, downloaded skins, world and pack icons and the on-screen keyboard's text.
    fn slots(images: &mut Vec<PackedImage>, trim: (u32, u32)) {
        for i in 0..MAP_SLOTS {
            images.push((format!("horizon/map{i}"), 128, 128, vec![0; 128 * 128 * 4]));
        }
        // The minimap extra's image (game/extras.rs).
        images.push((
            MINIMAP_SPRITE.into(),
            crate::minimap::SIZE as u32,
            crate::minimap::SIZE as u32,
            vec![0; crate::minimap::SIZE * crate::minimap::SIZE * 4],
        ));
        // Downloaded skins (game/scoreboard.rs).
        for i in 0..SKIN_SLOTS {
            images.push((format!("horizon/skin{i}"), 64, 64, vec![0; 64 * 64 * 4]));
        }
        for i in 0..TRIM_SLOTS {
            images.push((
                format!("horizon/trim{i}"),
                trim.0,
                trim.1,
                vec![0; (trim.0 * trim.1 * 4) as usize],
            ));
        }
        for i in 0..WORLD_ICON_SLOTS {
            images.push((
                format!("horizon/world-icon{i}"),
                64,
                64,
                vec![0; 64 * 64 * 4],
            ));
        }
        let side = PACK_ICON_SIZE as usize;
        for i in 0..PACK_ICON_SLOTS {
            images.push((
                format!("horizon/pack-icon{i}"),
                PACK_ICON_SIZE,
                PACK_ICON_SIZE,
                vec![0; side * side * 4],
            ));
        }
        // The on-screen keyboard's text and corner masks (ui_theme), rasterized at run time.
        images.push((
            "horizon/ui-text".into(),
            2048,
            1024,
            vec![0; 2048 * 1024 * 4],
        ));
    }
    /// Only the font's metrics, for a game that draws nothing (the
    /// integrated server): text measures as with the full atlas, without
    /// holding its textures. The pixels are dropped once the glyph widths are
    /// read, since nothing uploads them.
    pub fn fonts(root: &Path) -> Result<Self> {
        let mut atlas = Self::pack(root, root, &["font"], false)?;
        atlas.pixels = Vec::new();
        Ok(atlas)
    }
    /// Shelf placements of `images` (tallest first) in the narrowest atlas
    /// width from 4096 up that keeps the atlas, glyph rows included, within
    /// `max` texels a side; and the first row free for glyphs. High-resolution
    /// packs fill a 4096-wide atlas past what GPUs allow (Faithful 64x needs
    /// 32768 rows); vanilla's stitcher fails the reload when its atlases don't
    /// fit, and the packs are then turned off (`Minecraft.clearResourcePacksOnError`).
    fn layout(images: &[PackedImage], max: u32) -> Result<(u32, Vec<(u32, u32)>, u32)> {
        let mut width = 4096.min(max);
        loop {
            let mut placements = Vec::with_capacity(images.len());
            let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
            for (_, w, h, _) in images {
                if x + w + 1 > width {
                    x = 0;
                    y += row + 1;
                    row = 0;
                }
                placements.push((x, y));
                x += w + 1;
                row = row.max(*h);
            }
            let glyph_region = y + row + 1;
            let wide_enough = images.iter().all(|i| i.1 < width);
            if wide_enough && glyph_region + GLYPH_ROWS <= max {
                return Ok((width, placements, glyph_region));
            }
            if width >= max {
                let texels: u64 = images.iter().map(|i| u64::from(i.1) * u64::from(i.2)).sum();
                return Err(format!(
                    "the textures ({} images, {texels} texels) do not fit a {max}x{max} atlas",
                    images.len()
                )
                .into());
            }
            width = (width * 2).min(max);
        }
    }
    /// Pack the textures under these folders of `textures/`, and for the
    /// `full` atlas the generated sprites and the run-time slots too.
    fn pack(root: &Path, defaults: &Path, folders: &[&str], full: bool) -> Result<Self> {
        let textures = root.join("assets/minecraft/textures");
        let default_textures =
            (defaults != root).then(|| defaults.join("assets/minecraft/textures"));
        let mut files = Vec::new();
        for folder in folders {
            walk(&textures.join(folder), &mut files);
        }
        files.retain(|p| {
            let s = p.to_string_lossy();
            !s.contains("title/background") && !s.contains("realms") && !s.contains("presets")
        });
        type Decoded = (PackedImage, Option<TextureFlags>, Option<(u32, u32)>, bool);
        let decode_one = |path: &PathBuf| -> Option<Decoded> {
            let name = path
                .strip_prefix(&textures)
                .ok()?
                .with_extension("")
                .to_string_lossy()
                .replace('\\', "/");
            let (mut w, mut h, pixels) = decode(path).ok()?;
            let mcmeta = fs::read_to_string(path.with_extension("png.mcmeta")).ok();
            let animation = mcmeta
                .as_deref()
                .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
                .and_then(|meta| {
                    meta.get("animation")
                        .and_then(|section| AnimationMetadata::from_json(&section.to_string()).ok())
                });
            // An animated strip keeps its first frame: the region the mcmeta's
            // frame size picks (`AnimationMetadataSection.frameSize`), which is
            // the top-left square unless it names other dimensions or the
            // frames run along a row.
            let frames = name.starts_with("block/") || name.starts_with("item/");
            let pixels = if frames {
                let stride = w;
                let [fw, fh] = animation.as_ref().map_or([w.min(h) as i32; 2], |meta| {
                    meta.frame_size(w as i32, h as i32)
                });
                match (u32::try_from(fw), u32::try_from(fh)) {
                    (Ok(fw), Ok(fh))
                        if fw > 0 && fh > 0 && w % fw == 0 && h % fh == 0 && (fw, fh) != (w, h) =>
                    {
                        let side = fw.max(fh);
                        (w, h) = (side, side);
                        strip_frame(&pixels, stride, 0, 0, fw, fh)
                    }
                    _ => pixels,
                }
            } else {
                pixels
            };
            // The default texture's size, when a pack's differs from it.
            let gui_size = default_textures
                .as_ref()
                .and_then(|d| png_size(&d.join(format!("{name}.png"))))
                .map(|(dw, dh)| {
                    let side = dw.min(dh);
                    if frames { (side, side) } else { (dw, dh) }
                })
                .filter(|&size| size != (w, h));
            let flags = mcmeta.as_deref().and_then(TextureFlags::from_mcmeta);
            let animated = frames && animation.is_some();
            let (gw, gh) = gui_size.unwrap_or((w, h));
            (gw <= 1024 && (gh <= 512 || name.starts_with("font/"))).then_some((
                (name, w, h, pixels),
                flags,
                gui_size,
                animated,
            ))
        };
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get());
        let chunk = files.len().div_ceil(threads).max(1);
        let decoded: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = files
                .chunks(chunk)
                .map(|part| {
                    scope.spawn(move || part.iter().filter_map(decode_one).collect::<Vec<_>>())
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        let mut flags = HashMap::new();
        let mut gui_sizes = HashMap::new();
        let mut animated: Vec<String> = Vec::new();
        let mut images: Vec<PackedImage> = Vec::with_capacity(decoded.len());
        for (image, image_flags, gui_size, is_animated) in decoded {
            if let Some(f) = image_flags {
                flags.insert(image.0.clone(), f);
            }
            if let Some(size) = gui_size {
                gui_sizes.insert(image.0.clone(), size);
            }
            if is_animated {
                animated.push(image.0.clone());
            }
            images.push(image);
        }
        if full {
            images.extend(atlas_sprites(root, &images));
        }
        // The title screen's "JAVA EDITION" banner reads "RUST EDITION".
        if let Some(edition) = images
            .iter_mut()
            .find(|i| i.0 == crate::title_edition::SPRITE)
            && let Some(pixels) =
                crate::title_edition::rust_edition(edition.1, edition.2, &edition.3)
        {
            edition.3 = pixels;
        }
        images.push(("horizon/white".into(), 4, 4, vec![255; 64]));
        if full {
            Self::slots(&mut images, crate::trims::slot_size(root));
        }
        // Blurred sprites are sampled linearly in the atlas: a
        // border wrapped from their own edges keeps the filter off their
        // neighbours, as repeat addressing of their own texture would.
        let mut bordered = std::collections::HashSet::new();
        for image in &mut images {
            if flags.get(&image.0).is_some_and(|f: &TextureFlags| f.blur) {
                *image = wrap_border(image);
                bordered.insert(image.0.clone());
            }
        }
        images.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)));
        let (width, placements, glyph_region) = Self::layout(&images, MAX_ATLAS_SIZE)?;
        // A multiple of 1024 rather than a power of two: the default resources
        // need 4096x5120 (8192 rows before), Faithful 32x 4096x12288 (16384).
        let height = (glyph_region + GLYPH_ROWS).next_multiple_of(1024);
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let mut sprites = HashMap::new();
        let mut translucent = HashSet::new();
        for ((name, w, h, data), (px, py)) in images.iter().zip(placements) {
            if data
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] != 0 && p[3] != 255)
            {
                translucent.insert(name.clone());
            }
            for row in 0..*h {
                let src = &data[(row * w * 4) as usize..((row + 1) * w * 4) as usize];
                let dst = (((py + row) * width + px) * 4) as usize;
                pixels[dst..dst + (w * 4) as usize].copy_from_slice(src);
            }
            let b = u32::from(bordered.contains(name));
            let (px, py, w, h) = (px + b, py + b, w - 2 * b, h - 2 * b);
            let (gw, gh) = gui_sizes.get(name).copied().unwrap_or((w, h));
            sprites.insert(
                name.clone(),
                Rect {
                    u0: px as f32 / width as f32,
                    v0: py as f32 / height as f32,
                    u1: (px + w) as f32 / width as f32,
                    v1: (py + h) as f32 / height as f32,
                    w: gw,
                    h: gh,
                },
            );
        }
        // Every animated sprite animates: the packed strip is the raw first
        // frame, while the mcmeta's frame list may start anywhere, so each
        // animation's frame(0) overwrites its atlas rect before drawing.
        let mut loaded = crate::sprites::Sprites::default();
        let mut sprite_animations = Vec::new();
        animated.sort();
        for name in animated {
            let Some(rect) = sprites.get(&name).copied() else {
                continue;
            };
            let layer = loaded.insert(root, &format!("minecraft:{name}"))?;
            let Some(animation) = loaded.animations.iter().find(|a| a.layer == layer).cloned()
            else {
                continue;
            };
            let (tw, th) = rect.texels((width, height));
            let image = animation.resized_pixels(animation.frame(0), tw, th);
            let x = (rect.u0 * width as f32).round() as usize;
            let y = (rect.v0 * height as f32).round() as usize;
            let row_bytes = tw as usize * 4;
            for row in 0..th as usize {
                let dst = ((y + row) * width as usize + x) * 4;
                let src = row * row_bytes;
                pixels[dst..dst + row_bytes].copy_from_slice(&image[src..src + row_bytes]);
            }
            sprite_animations.push((rect, animation));
        }
        let white = sprites["horizon/white"];
        let glint = Self::glint_texture(root, defaults, "enchanted_glint_item");
        let glint_armor = Self::glint_texture(root, defaults, "enchanted_glint_armor");
        let font = crate::font::Font::load(
            root,
            &sprites,
            &pixels,
            (width, height),
            (glyph_region, height - glyph_region),
            white,
        );
        Ok(Self {
            pixels,
            width,
            height,
            sprites,
            translucent,
            flags,
            sprite_animations,
            glint,
            glint_armor,
            font,
        })
    }
    /// `misc/<name>.png` on its own, or a transparent texel if the resource
    /// pack lacks it.
    fn glint_texture(root: &Path, defaults: &Path, name: &str) -> (Vec<u8>, u32, u32) {
        let relative = format!("assets/minecraft/textures/misc/{name}.png");
        let mut path = root.join(&relative);
        if !path.exists() && defaults != root {
            path = defaults.join(&relative);
        }
        decode(&path).map_or((vec![0; 4], 1, 1), |(w, h, pixels)| (pixels, w, h))
    }
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct HudVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: u32,
    pub depth: f32,
    /// The `GLINT_TEXTURING` UV into the repeating glint texture, unused
    /// unless `uv.x >= 2`.
    pub glint_uv: [f32; 2],
}
/// The GUI Scale option (0 = auto).
pub static GUI_SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    u32::from(a) << 24 | u32::from(b) << 16 | u32::from(g) << 8 | u32::from(r)
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct HudUniform {
    size: [f32; 4],
    /// `EndPortalFill`: the two sprites, then the game time in x.
    end_portal: [[f32; 4]; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a PNG `width x height` whose `frames` stacked rows are solid
    /// colours, then its mcmeta beside it.
    fn write_strip(path: &Path, width: u32, height: u32, frames: &[[u8; 4]], mcmeta: Option<&str>) {
        let file = std::io::BufWriter::new(fs::File::create(path).unwrap());
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for frame in frames {
            let rows = height / frames.len() as u32;
            pixels.extend((0..width * rows).flat_map(|_| *frame));
        }
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
        if let Some(mcmeta) = mcmeta {
            let meta = path.with_extension("png.mcmeta");
            fs::write(meta, mcmeta).unwrap();
        }
    }
    /// A resource root holding one animated block texture: a strip of two
    /// 16x16 frames, red then blue, whose mcmeta starts on the blue one.
    fn animated_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("horizon-atlas-anim-{}", std::process::id()));
        let block = root.join("assets/minecraft/textures/block");
        fs::create_dir_all(&block).unwrap();
        write_strip(
            &block.join("test_anim.png"),
            16,
            32,
            &[[255, 0, 0, 255], [0, 0, 255, 255]],
            Some(r#"{"animation": {"frames": [1, 0]}}"#),
        );
        root
    }
    fn texel_at(atlas: &HudAtlas, rect: Rect, x: u32, y: u32) -> [u8; 4] {
        let px = (rect.u0 * atlas.width as f32).round() as u32 + x;
        let py = (rect.v0 * atlas.height as f32).round() as u32 + y;
        let at = ((py * atlas.width + px) * 4) as usize;
        atlas.pixels[at..at + 4].try_into().unwrap()
    }
    #[test]
    fn animated_block_sprites_register_and_pack_their_first_mcmeta_frame() {
        let root = animated_root();
        let atlas = HudAtlas::build(&root).unwrap();
        let (rect, animation) = atlas
            .sprite_animations
            .iter()
            .find(|(_, animation)| animation.frame_count() == 2)
            .expect("the animated block sprite");
        // The mcmeta's list starts on the strip's second (blue) frame.
        assert_eq!(animation.frame(0), 1);
        assert_eq!(rect.texels((atlas.width, atlas.height)), (16, 16));
        assert_eq!(texel_at(&atlas, *rect, 0, 0), [0, 0, 255, 255]);
        assert_eq!(texel_at(&atlas, *rect, 15, 15), [0, 0, 255, 255]);
    }
    #[test]
    fn a_strip_frames_size_comes_from_its_mcmeta() {
        let root = animated_root();
        let block = root.join("assets/minecraft/textures/block");
        // A row of two frames with the animation's own frame size.
        write_strip(
            &block.join("test_row.png"),
            32,
            16,
            &[[0, 255, 0, 255], [255, 255, 0, 255]],
            Some(r#"{"animation": {"width": 16, "height": 16}}"#),
        );
        let atlas = HudAtlas::build(&root).unwrap();
        let rect = atlas.sprites["block/test_row"];
        // The first frame is the left 16x16, not the whole row.
        assert_eq!(rect.texels((atlas.width, atlas.height)), (16, 16));
        assert_eq!(texel_at(&atlas, rect, 0, 0), [0, 255, 0, 255]);
    }
    /// `count` images of `w x h`; layout reads only their sizes.
    fn sized(count: usize, w: u32, h: u32) -> Vec<PackedImage> {
        (0..count)
            .map(|i| (format!("i{i}"), w, h, Vec::new()))
            .collect()
    }
    fn assert_inside(images: &[PackedImage], width: u32, placements: &[(u32, u32)], free: u32) {
        for ((_, w, h, _), (x, y)) in images.iter().zip(placements) {
            assert!(
                x + w <= width && y + h < free,
                "{x},{y} {w}x{h} outside {width}x{free}"
            );
        }
    }
    fn write_png(root: &Path, name: &str, w: u32, h: u32) {
        let path = root.join(format!("assets/minecraft/textures/{name}.png"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut encoder = png::Encoder::new(fs::File::create(path).unwrap(), w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels = vec![255; (w * h * 4) as usize];
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
    }
    #[test]
    fn a_pack_texture_keeps_the_default_textures_gui_size() {
        let temp = std::env::temp_dir().join(format!("horizon-gui-size-{}", std::process::id()));
        let (defaults, pack) = (temp.join("defaults"), temp.join("pack"));
        write_png(&defaults, "gui/container/inventory", 256, 256);
        write_png(&defaults, "gui/sprites/widget/button", 200, 20);
        write_png(&defaults, "block/stone", 16, 16);
        write_png(&defaults, "item/compass", 16, 512);
        // Faithful 64x's inventory (past the 512 rows a texture may have),
        // 32x's button and stone, an animated strip, and a texture of its own.
        write_png(&pack, "gui/container/inventory", 1024, 1024);
        write_png(&pack, "gui/sprites/widget/button", 400, 40);
        write_png(&pack, "block/stone", 32, 32);
        write_png(&pack, "item/compass", 32, 1024);
        write_png(&pack, "gui/sprites/pack_only", 30, 10);
        let atlas = HudAtlas::build_over(&pack, &defaults).unwrap();
        let _ = fs::remove_dir_all(&temp);
        let size = (atlas.width, atlas.height);
        for (name, gui, texels) in [
            ("gui/container/inventory", (256, 256), (1024, 1024)),
            ("gui/sprites/widget/button", (200, 20), (400, 40)),
            ("block/stone", (16, 16), (32, 32)),
            ("item/compass", (16, 16), (32, 32)),
            ("gui/sprites/pack_only", (30, 10), (30, 10)),
        ] {
            let r = atlas.sprites[name];
            assert_eq!(((r.w, r.h), r.texels(size)), (gui, texels), "{name}");
        }
    }
    #[test]
    fn the_default_textures_keep_a_4096_wide_atlas() {
        let images = sized(400, 64, 64);
        let (width, placements, free) = HudAtlas::layout(&images, MAX_ATLAS_SIZE).unwrap();
        assert_eq!(width, 4096);
        assert_inside(&images, width, &placements, free);
    }
    #[test]
    fn a_high_resolution_pack_widens_the_atlas_to_fit_the_gpu() {
        // 34 rows of three 1024x512 GUI textures: 17442 rows at 4096 wide,
        // past the 16384 GPUs allow (Faithful 64x needed 32768).
        let images = sized(100, 1024, 512);
        let (width, placements, free) = HudAtlas::layout(&images, MAX_ATLAS_SIZE).unwrap();
        assert_eq!(width, 8192);
        assert!(free + GLYPH_ROWS <= MAX_ATLAS_SIZE);
        assert_inside(&images, width, &placements, free);
    }
    #[test]
    fn textures_that_cannot_fit_are_an_error_not_a_gpu_panic() {
        let error = HudAtlas::layout(&sized(300, 1024, 1024), MAX_ATLAS_SIZE).unwrap_err();
        assert!(error.to_string().contains("do not fit a 16384x16384 atlas"));
        // One image wider than the largest atlas.
        assert!(HudAtlas::layout(&sized(1, 2048, 16), 2048).is_err());
    }

    #[test]
    fn a_wrapped_border_repeats_the_opposite_edges() {
        // 2x2 texels a b / c d, one byte each in red.
        let image = (
            "x".to_owned(),
            2,
            2,
            [1u8, 2, 3, 4]
                .iter()
                .flat_map(|&v| [v, 0, 0, 255])
                .collect(),
        );
        let (_, w, h, data) = wrap_border(&image);
        assert_eq!((w, h), (4, 4));
        let red: Vec<u8> = data.chunks(4).map(|t| t[0]).collect();
        #[rustfmt::skip]
        assert_eq!(red, [
            4, 3, 4, 3,
            2, 1, 2, 1,
            4, 3, 4, 3,
            2, 1, 2, 1,
        ]);
    }

    /// A synthetic default font: two 8x8 glyphs, 3 and 5 pixels wide.
    fn two_glyph_font(name: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("horizon-fonts-{name}-{}", std::process::id()));
        let font = root.join("assets/minecraft/font");
        let textures = root.join("assets/minecraft/textures/font");
        fs::create_dir_all(&font).unwrap();
        fs::create_dir_all(&textures).unwrap();
        fs::write(
            font.join("default.json"),
            r#"{"providers":[{"type":"bitmap","file":"minecraft:font/test.png","ascent":7,"chars":["ab"]}]}"#,
        )
        .unwrap();
        let mut pixels = vec![0u8; 16 * 8 * 4];
        for y in 0..8 {
            for x in (0..3).chain(8..13) {
                pixels[(y * 16 + x) * 4 + 3] = 255;
            }
        }
        let mut encoder =
            png::Encoder::new(fs::File::create(textures.join("test.png")).unwrap(), 16, 8);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
        root
    }
    #[test]
    fn the_fonts_only_atlas_measures_text_like_the_full_one() {
        let root = two_glyph_font("atlas");
        let full = HudAtlas::build(&root).unwrap();
        let fonts = HudAtlas::fonts(&root).unwrap();
        let _ = fs::remove_dir_all(&root);
        assert!(fonts.pixels.is_empty());
        let width = full.font.width("abba");
        assert!(width > 0.0);
        assert_eq!(fonts.font.width("abba"), width);
    }
    #[test]
    fn obfuscated_glyphs_keep_the_width() {
        let root = two_glyph_font("obfuscated");
        let fonts = HudAtlas::fonts(&root).unwrap();
        let _ = fs::remove_dir_all(&root);
        let (a, b) = (fonts.font.glyph('a').advance, fonts.font.glyph('b').advance);
        assert_ne!(a, b);
        for _ in 0..20 {
            assert_eq!(fonts.font.random_glyph(&Default::default(), a).advance, a);
            assert_eq!(fonts.font.random_glyph(&Default::default(), b).advance, b);
        }
        // No glyph that wide: FontSet's missing glyph.
        let missing = fonts.font.random_glyph(&Default::default(), 30.0);
        assert!(missing.rect.is_none() && missing.right == 5.0);
    }
}
