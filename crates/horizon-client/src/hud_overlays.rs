//! The `Hud` overlays laid out as [`GuiDraw`] calls like
//! [`crate::hud_layout`]: status effect icons (`extractEffects`), titles and
//! the action bar (`extractTitle`, `extractOverlayMessage` and the title
//! timers of `Hud.tick`), the selected item name (`extractSelectedItemName`
//! and its `toolHighlightTimer`), boss bars (`BossHealthOverlay`), the jump
//! bar (`JumpableVehicleBar`), the scoreboard sidebar, item decorations
//! (`GuiGraphicsExtractor.itemDecorations`), the camera overlays, the
//! sleep overlay and the subtitles (`SubtitleOverlay`).
use crate::text::Text;

/// The saving indicator: `Hud.tickAutosaveIndicator` fades a value towards 1
/// while the server is saving and `extractSavingIndicator` draws the text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SavingIndicator {
    last: f32,
    value: f32,
}
impl SavingIndicator {
    /// `Hud.tickAutosaveIndicator`: `Mth.lerp(0.2F, value, saving ? 1 : 0)`.
    pub fn tick(&mut self, saving: bool) {
        self.last = self.value;
        self.value += (if saving { 1.0 } else { 0.0 } - self.value) * 0.2;
    }
}

/// Titles and the action bar: `Hud.title`, `subtitle`, `titleTime`, the
/// fade times and `overlayMessageString` / `overlayMessageTime`.
#[derive(Clone, Debug, PartialEq)]
pub struct Titles {
    pub title: Option<Text>,
    pub subtitle: Option<Text>,
    pub time: i32,
    pub fade_in: i32,
    pub stay: i32,
    pub fade_out: i32,
    pub overlay: Option<Text>,
    pub overlay_time: i32,
    /// `animateOverlayMessageColor`: the "Now Playing" rainbow.
    pub overlay_animated: bool,
}
impl Default for Titles {
    fn default() -> Self {
        Self {
            title: None,
            subtitle: None,
            time: 0,
            fade_in: 10,
            stay: 70,
            fade_out: 20,
            overlay: None,
            overlay_time: 0,
            overlay_animated: false,
        }
    }
}
impl Titles {
    /// `setTitle`.
    pub fn set_title(&mut self, title: Text) {
        self.title = Some(title);
        self.time = self.fade_in + self.stay + self.fade_out;
    }
    /// `setSubtitle`.
    pub fn set_subtitle(&mut self, subtitle: Text) {
        self.subtitle = Some(subtitle);
    }
    /// `setTimes`: negative values keep the current time.
    pub fn set_times(&mut self, fade_in: i32, stay: i32, fade_out: i32) {
        if fade_in >= 0 {
            self.fade_in = fade_in;
        }
        if stay >= 0 {
            self.stay = stay;
        }
        if fade_out >= 0 {
            self.fade_out = fade_out;
        }
        if self.time > 0 {
            self.time = self.fade_in + self.stay + self.fade_out;
        }
    }
    /// `clearTitles`.
    pub fn clear(&mut self) {
        self.title = None;
        self.subtitle = None;
        self.time = 0;
    }
    /// `resetTitleTimes`.
    pub fn reset_times(&mut self) {
        (self.fade_in, self.stay, self.fade_out) = (10, 70, 20);
    }
    /// `setOverlayMessage`.
    pub fn set_overlay(&mut self, text: Text, animate: bool) {
        self.overlay = Some(text);
        self.overlay_time = 60;
        self.overlay_animated = animate;
    }
    /// The title and action bar part of `Hud.tick`.
    pub fn tick(&mut self) {
        if self.overlay_time > 0 {
            self.overlay_time -= 1;
        }
        if self.time > 0 {
            self.time -= 1;
            if self.time <= 0 {
                self.title = None;
                self.subtitle = None;
            }
        }
    }
}

/// The selected item as `Hud.tick` compares it: its item id and hover name.
#[derive(Clone, Debug, PartialEq)]
pub struct HeldName {
    pub item: String,
    /// `ItemStack.getHoverName`.
    pub hover_name: Text,
    /// `getRarity().color()`.
    pub rarity_color: u32,
    /// The stack has `CUSTOM_NAME`.
    pub custom_name: bool,
}

/// `Hud.toolHighlightTimer` and `lastToolHighlight`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolHighlight {
    pub timer: i32,
    pub last: Option<HeldName>,
}
impl ToolHighlight {
    /// The selected-item part of `Hud.tick`; `display_time` is the
    /// `notificationDisplayTime` option.
    pub fn tick(&mut self, selected: Option<HeldName>, display_time: f64) {
        match &selected {
            None => self.timer = 0,
            Some(s) => {
                let changed = self
                    .last
                    .as_ref()
                    .is_none_or(|l| l.item != s.item || l.hover_name != s.hover_name);
                if changed {
                    self.timer = (40.0 * display_time) as i32;
                } else if self.timer > 0 {
                    self.timer -= 1;
                }
            }
        }
        self.last = selected;
    }
}

// ---- subtitles (`SubtitleOverlay`) ------------------------------------------
//
// Closed captions for the sounds the player could hear, with direction arrows
// and a fade over the notification display time. Like vanilla, the overlay
// listens to the sound engine only from the frame that renders it with the
// "Show Subtitles" option on until a frame renders with it off; turning it
// off keeps the captions it has (they show again if the option comes back
// before they expire).
