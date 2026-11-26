//! Shared GUI widgets drawn as HUD quads: nine-slice sprites (gui.scaling
//! metadata), buttons, sliders, tiled menu backgrounds, separators and the
//! EditBox text field with vanilla cursor, selection and scrolling.
use crate::hud::rgba;
use std::{collections::HashMap, path::Path, sync::RwLock, time::Instant};

/// `GuiSpriteScaling.NineSlice`: the sprite's size in GUI pixels, whatever
/// its texture's resolution, its borders (left, top, right, bottom) in the
/// same units, and whether the edges and centre stretch rather than tile.
#[derive(Clone, Copy, Debug, PartialEq)]
struct NineSlice {
    width: u32,
    height: u32,
    border: [u32; 4],
    stretch_inner: bool,
}
/// Nine-slice scaling from `.png.mcmeta` files.
static NINE_SLICE: RwLock<Option<HashMap<String, NineSlice>>> = RwLock::new(None);

/// A `gui.scaling` section of type `nine_slice`.
fn parse_nine_slice(v: &serde_json::Value) -> Option<NineSlice> {
    let s = &v["gui"]["scaling"];
    if s["type"] != "nine_slice" {
        return None;
    }
    let size = |k: &str| s[k].as_u64().map(|n| n as u32);
    let border = &s["border"];
    let border = if let Some(n) = border.as_u64() {
        [n as u32; 4]
    } else {
        let g = |k: &str| border[k].as_u64().unwrap_or(0) as u32;
        [g("left"), g("top"), g("right"), g("bottom")]
    };
    Some(NineSlice {
        width: size("width")?,
        height: size("height")?,
        border,
        stretch_inner: s["stretch_inner"].as_bool().unwrap_or(false),
    })
}
/// Read `gui.scaling` metadata for every GUI sprite under `root`.
pub fn load_metadata(root: &Path) {
    let mut map = HashMap::new();
    let base = root.join("assets/minecraft/textures");
    let mut stack = vec![base.join("gui/sprites")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let name = p.to_string_lossy().into_owned();
            let Some(stripped) = name.strip_suffix(".png.mcmeta") else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let Some(nine_slice) = serde_json::from_str(&text)
                .ok()
                .as_ref()
                .and_then(parse_nine_slice)
            else {
                continue;
            };
            let key = Path::new(stripped)
                .strip_prefix(&base)
                .map(|r| r.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            map.insert(key, nine_slice);
        }
    }
    *NINE_SLICE.write().unwrap_or_else(|e| e.into_inner()) = Some(map);
}

/// `AbstractScrollArea`'s scrolling of a list `top..bottom` holding `content`
/// pixels: the amount, the mouse wheel and the held scrollbar.
#[derive(Clone, Copy, Debug, Default)]
pub struct Scroll {
    pub amount: f32,
}

/// The EditBox text field.
#[derive(Clone, Debug)]
pub struct EditBox {
    pub value: String,
    /// Cursor and selection anchor, in characters.
    pub cursor: usize,
    pub highlight: usize,
    display: usize,
    pub max_length: usize,
    pub bordered: bool,
    /// `MultiLineEditBox`: a book page keeps line breaks. A single-line box strips them.
    pub multiline: bool,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub focused: bool,
    focused_at: Instant,
    /// Grey completion shown after the text (EditBox.setSuggestion).
    pub suggestion: Option<String>,
    pub text_color: u32,
}

fn byte_index(s: &str, chars: usize) -> usize {
    s.char_indices().nth(chars).map_or(s.len(), |(i, _)| i)
}
pub fn char_len(s: &str) -> usize {
    s.chars().count()
}
impl EditBox {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            highlight: 0,
            display: 0,
            max_length: 32,
            bordered: true,
            multiline: false,
            x,
            y,
            width,
            height,
            focused: true,
            focused_at: Instant::now(),
            suggestion: None,
            text_color: rgba(224, 224, 224, 255),
        }
    }
    pub fn reset_blink(&mut self) {
        self.focused_at = Instant::now();
    }
    pub fn set_value(&mut self, value: &str) {
        self.value = value.chars().take(self.max_length).collect();
        self.move_cursor_to_end(false);
    }
    fn selected(&self) -> (usize, usize) {
        (
            self.cursor.min(self.highlight),
            self.cursor.max(self.highlight),
        )
    }
    pub fn selected_text(&self) -> String {
        let (a, b) = self.selected();
        self.value.chars().skip(a).take(b - a).collect()
    }
    /// EditBox.insertText: replace the selection, respecting the maximum length.
    pub fn insert_text(&mut self, text: &str) {
        let (a, b) = self.selected();
        let filtered = filter_text(text, self.multiline);
        let room = self
            .max_length
            .saturating_sub(char_len(&self.value) - (b - a));
        let insert: String = filtered.chars().take(room).collect();
        let (ba, bb) = (byte_index(&self.value, a), byte_index(&self.value, b));
        self.value.replace_range(ba..bb, &insert);
        let pos = a + char_len(&insert);
        self.set_cursor(pos, false);
    }
    pub fn set_cursor(&mut self, pos: usize, select: bool) {
        self.cursor = pos.min(char_len(&self.value));
        if !select {
            self.highlight = self.cursor;
        }
        self.scroll_to(self.cursor);
        self.reset_blink();
    }
    pub fn move_cursor_to_end(&mut self, select: bool) {
        self.set_cursor(char_len(&self.value), select);
    }
    fn scroll_to(&mut self, pos: usize) {
        // EditBox.scrollTo without font access is completed in `layout`.
        self.display = self.display.min(char_len(&self.value));
        if pos < self.display {
            self.display = pos;
        }
    }
}

/// `StringUtil.filterText`: the allowed characters (`isAllowedChatCharacter`),
/// and line breaks where the field takes several lines.
fn filter_text(input: &str, multiline: bool) -> String {
    input
        .chars()
        .filter(|&c| (c != '§' && c >= ' ' && c != '\u{7f}') || (multiline && c == '\n'))
        .collect()
}
