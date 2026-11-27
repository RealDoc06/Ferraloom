//! Items drawn through the resolved item model pipeline
//! (`horizon_client::item_render`): GUI icons (`GuiGraphics.renderItem`),
//! dropped items (`ItemEntityRenderer`) and items posed in the world (hands,
//! frames, armour stands). This binds the resolver's host services
//! (`ItemEnv`) and the special renderers' registries to the game.
use super::{Dim, Game};
use horizon_assets::item_model::{
    Components, DisplayContext, GlobalPos, ItemEnv, LevelState, OwnerState, StackView,
};
use horizon_client::{
    item_render::{self, ItemRenderState},
    items::ItemStack,
};
use serde_json::Value;

/// The game's answers to what the resolver cannot read from the stack.
pub(super) struct Env<'a, 'g> {
    game: &'a Game<'g>,
}
impl ItemEnv for Env<'_, '_> {
    fn potion_effect_color(&self, contents: &Value) -> Option<i32> {
        // `PotionContents.getColorOr`: the effect mix when any effect is visible.
        let id = contents["potion"].as_str();
        let effects = id
            .and_then(horizon_client::brewing::potion)
            .is_some_and(|p| !p.effects.is_empty());
        effects.then(|| (0xFF00_0000 | horizon_client::brewing::potion_color(id)) as i32)
    }
    fn grass_color(&self, temperature: f64, downfall: f64) -> i32 {
        self.game
            .pack
            .item_renderer
            .grass_color(temperature, downfall)
    }
    fn bundle_fullness(&self, _stack: &StackView, components: &Components) -> f32 {
        // `BundleContents.weight`: count / max stack size, summed.
        let Some(items) = components
            .get("minecraft:bundle_contents")
            .and_then(Value::as_array)
        else {
            return 0.0;
        };
        let prototypes = &self.game.pack.item_renderer.models.prototypes;
        let mut total = 0.0f64;
        for item in items {
            let id = item["id"].as_str().unwrap_or("");
            let count = item["count"].as_i64().unwrap_or(1) as f64;
            let max = prototypes
                .get(id)
                .and_then(|c| c.get("minecraft:max_stack_size"))
                .and_then(Value::as_i64)
                .unwrap_or(1) as f64;
            total += count / max;
        }
        total as f32
    }
    fn has_shift_down(&self) -> bool {
        self.game.has_shift_down()
    }
    fn local_time(&self, pattern: &str, _locale: &str, _time_zone: Option<&str>) -> String {
        local_time(pattern)
    }
    fn random_float(&self) -> f32 {
        let nanos = self.game.started.elapsed().subsec_nanos();
        (nanos % 1_000_000) as f32 / 1_000_000.0
    }
}
/// The days since 1970-01-01 as a civil (year, month, day) date.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
/// `DateTimeFormatter.ofPattern(pattern).format(now)` for the month, day,
/// year and time letters item models use, in the system time zone
/// (`LocalTime` without a `time_zone`).
pub(super) fn local_time(pattern: &str) -> String {
    let now = chrono::Local::now();
    let secs = now.timestamp() + i64::from(now.offset().local_minus_utc());
    let (y, mo, d) = civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    let (h, mi, se) = (s / 3600, s / 60 % 60, s % 60);
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let mut n = 1;
        while i + n < chars.len() && chars[i + n] == c {
            n += 1;
        }
        let num = |v: i64| format!("{v:0n$}");
        match c {
            'M' => out += &num(i64::from(mo)),
            'd' => out += &num(i64::from(d)),
            'y' | 'u' => out += &if n == 2 { num(y % 100) } else { num(y) },
            'H' => out += &num(h),
            'm' => out += &num(mi),
            's' => out += &num(se),
            '\'' => {
                // Quoted literal text.
                let end = chars[i + 1..].iter().position(|&x| x == '\'');
                if let Some(e) = end {
                    out.extend(&chars[i + 1..i + 1 + e]);
                    i += e + 2;
                    continue;
                }
                n = 1;
            }
            other => out.extend(std::iter::repeat_n(other, n)),
        }
        i += n;
    }
    out
}

impl Game<'_> {
    pub(super) fn item_env(&self) -> Env<'_, '_> {
        Env { game: self }
    }
    /// The client level as item models read it: worked out once a frame (a
    /// screen of items read the same level, its sky sampled once).
    pub(super) fn item_level(&self) -> LevelState {
        let dimension = match self.dim {
            Dim::Overworld => "minecraft:overworld",
            Dim::Nether => "minecraft:the_nether",
            Dim::End => "minecraft:the_end",
        };
        let key = (
            self.frames,
            self.world.game_time,
            self.world.day_time,
            dimension,
        );
        let mut cached = self.item_level_cache.borrow_mut();
        if let Some((at, level)) = cached.as_ref()
            && *at == key
        {
            return level.clone();
        }
        let level = self.level_state_now(dimension);
        *cached = Some((key, level.clone()));
        level
    }
    fn level_state_now(&self, dimension: &'static str) -> LevelState {
        let sky = self
            .dim_environment()
            .sample(self.world.day_time, None, None);
        LevelState {
            dimension: dimension.to_owned(),
            game_time: self.world.game_time,
            sun_angle: sky.sun_angle,
            moon_phase: (self.world.day_time.div_euclid(24_000)).rem_euclid(8) as i32,
            respawn: self.spawn.map(|s| GlobalPos {
                dimension: "minecraft:overworld".to_owned(),
                pos: [s.x.floor() as i32, s.y.floor() as i32, s.z.floor() as i32],
            }),
            runs_normally: true,
        }
    }
    /// `ItemModelResolver.updateForTopItem`.
    pub(super) fn resolve_item(
        &self,
        stack: &ItemStack,
        display: DisplayContext,
        owner: Option<&OwnerState>,
        seed: i32,
    ) -> ItemRenderState {
        let level = self.item_level();
        self.pack.item_renderer.resolve(
            &item_render::stack_view(stack),
            display,
            owner,
            Some(&level),
            seed,
            &self.item_env(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_follow_the_gregorian_calendar() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(19_716), (2023, 12, 25));
        assert_eq!(civil(11_016), (2000, 2, 29));
    }
}
