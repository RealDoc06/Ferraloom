//! Ports of TickRateManager and ServerTickRateManager. The server supplies a
//! monotonic nanosecond clock and drains ordered packets/reports at call sites.
use std::ops::Deref;

#[derive(Clone, Debug)]
pub struct TickRateManager {
    tickrate: f32,
    nanoseconds_per_tick: i64,
    frozen_ticks_to_run: i32,
    run_game_elements: bool,
    is_frozen: bool,
}
impl Default for TickRateManager {
    fn default() -> Self {
        Self {
            tickrate: 20.0,
            nanoseconds_per_tick: 50_000_000,
            frozen_ticks_to_run: 0,
            run_game_elements: true,
            is_frozen: false,
        }
    }
}
impl TickRateManager {
    pub fn set_tick_rate(&mut self, rate: f32) {
        // Java Math.max propagates NaN, unlike Rust's f32::max.
        self.tickrate = if rate < 1.0 { 1.0 } else { rate };
        self.nanoseconds_per_tick = (1_000_000_000.0 / f64::from(self.tickrate)) as i64;
    }
    pub fn tickrate(&self) -> f32 {
        self.tickrate
    }
    pub fn milliseconds_per_tick(&self) -> f32 {
        self.nanoseconds_per_tick as f32 / 1_000_000.0_f32
    }
    pub fn nanoseconds_per_tick(&self) -> i64 {
        self.nanoseconds_per_tick
    }
    pub fn runs_normally(&self) -> bool {
        self.run_game_elements
    }
    pub fn is_stepping_forward(&self) -> bool {
        self.frozen_ticks_to_run > 0
    }
    pub fn frozen_ticks_to_run(&self) -> i32 {
        self.frozen_ticks_to_run
    }
    pub fn set_frozen_ticks_to_run(&mut self, ticks: i32) {
        self.frozen_ticks_to_run = ticks;
    }
    pub fn is_frozen(&self) -> bool {
        self.is_frozen
    }
    pub fn set_frozen(&mut self, frozen: bool) {
        self.is_frozen = frozen;
    }
    /// Compute this tick's run flag before decrementing the step count. The
    /// final stepped tick runs even though isSteppingForward now returns false.
    pub fn tick(&mut self) {
        self.run_game_elements = !self.is_frozen || self.frozen_ticks_to_run > 0;
        if self.frozen_ticks_to_run > 0 {
            self.frozen_ticks_to_run -= 1;
        }
    }
    /// Port of TickRateManager#isEntityFrozen. The entity subsystem supplies
    /// vanilla's transitive player-passenger count from canonical storage.
    pub fn is_entity_frozen(&self, is_player: bool, player_passengers: i32) -> bool {
        !self.run_game_elements && !is_player && player_passengers <= 0
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum TickRateEvent {
    State {
        tick_rate: f32,
        is_frozen: bool,
    },
    Step {
        ticks: i32,
    },
    SprintReport {
        completed_ticks: i64,
        ticks_per_second: i32,
        milliseconds_per_tick: String,
    },
    /// Port of MinecraftServer#onTickRateChanged's deadline/autosave reset hook.
    RateChanged,
}
#[derive(Default)]
pub struct ServerTickRateManager {
    base: TickRateManager,
    remaining_sprint_ticks: i64,
    sprint_tick_start_time: i64,
    sprint_time_spent: i64,
    scheduled_current_sprint_ticks: i64,
    previous_is_frozen: bool,
    events: Vec<TickRateEvent>,
}
impl Deref for ServerTickRateManager {
    type Target = TickRateManager;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
impl ServerTickRateManager {
    pub fn tick(&mut self) {
        self.base.tick();
    }
    pub fn set_frozen(&mut self, frozen: bool) {
        self.base.set_frozen(frozen);
        self.events.push(self.state_event());
    }
    pub fn set_tick_rate(&mut self, rate: f32) {
        self.base.set_tick_rate(rate);
        self.events.push(TickRateEvent::RateChanged);
        self.events.push(self.state_event());
    }
    pub fn is_sprinting(&self) -> bool {
        self.scheduled_current_sprint_ticks > 0
    }
    pub fn step_game_if_paused(&mut self, ticks: i32) -> bool {
        if !self.is_frozen() {
            return false;
        }
        self.base.set_frozen_ticks_to_run(ticks);
        self.events.push(TickRateEvent::Step { ticks });
        true
    }
    pub fn stop_stepping(&mut self) -> bool {
        if self.frozen_ticks_to_run() <= 0 {
            return false;
        }
        self.base.set_frozen_ticks_to_run(0);
        self.events.push(TickRateEvent::Step { ticks: 0 });
        true
    }
    pub fn request_game_to_sprint(&mut self, ticks: i32) -> bool {
        let interrupted = self.remaining_sprint_ticks > 0;
        self.sprint_time_spent = 0;
        self.scheduled_current_sprint_ticks = i64::from(ticks);
        self.remaining_sprint_ticks = i64::from(ticks);
        self.previous_is_frozen = self.is_frozen();
        self.set_frozen(false);
        interrupted
    }
    pub fn stop_sprinting(&mut self) -> bool {
        if self.remaining_sprint_ticks <= 0 {
            return false;
        }
        self.finish_tick_sprint();
        true
    }
    /// Call only at MinecraftServer's outer sprint check, before tickServer.
    pub fn check_should_sprint_this_tick(&mut self, now_nanos: i64) -> bool {
        if !self.runs_normally() {
            return false;
        }
        if self.remaining_sprint_ticks > 0 {
            self.sprint_tick_start_time = now_nanos;
            self.remaining_sprint_ticks -= 1;
            true
        } else {
            self.finish_tick_sprint();
            false
        }
    }
    pub fn end_tick_work(&mut self, now_nanos: i64) {
        self.sprint_time_spent = self
            .sprint_time_spent
            .wrapping_add(now_nanos.wrapping_sub(self.sprint_tick_start_time));
    }
    fn finish_tick_sprint(&mut self) {
        let completed = self
            .scheduled_current_sprint_ticks
            .wrapping_sub(self.remaining_sprint_ticks);
        let milliseconds = (self.sprint_time_spent as f64).max(1.0) / 1_000_000.0;
        let ticks_per_second = (1000.0 * completed as f64 / milliseconds) as i32;
        let ms_per_tick = if completed == 0 {
            f64::from(self.milliseconds_per_tick())
        } else {
            milliseconds / completed as f64
        };
        self.scheduled_current_sprint_ticks = 0;
        self.sprint_time_spent = 0;
        self.events.push(TickRateEvent::SprintReport {
            completed_ticks: completed,
            ticks_per_second,
            milliseconds_per_tick: format_report_milliseconds(ms_per_tick),
        });
        self.remaining_sprint_ticks = 0;
        self.set_frozen(self.previous_is_frozen);
        self.events.push(TickRateEvent::RateChanged);
    }
    fn state_event(&self) -> TickRateEvent {
        TickRateEvent::State {
            tick_rate: self.tickrate(),
            is_frozen: self.is_frozen(),
        }
    }
    pub fn joining_events(&self) -> [TickRateEvent; 2] {
        [
            self.state_event(),
            TickRateEvent::Step {
                ticks: self.frozen_ticks_to_run(),
            },
        ]
    }
    /// The server drains immediately before command feedback, preserving
    /// vanilla's tick-state packet, sprint-report and feedback ordering.
    pub fn take_events(&mut self) -> Vec<TickRateEvent> {
        std::mem::take(&mut self.events)
    }
}

// Decimal half-up formatting, matching Formatter's two fractional digits rather
// than Rust's tie-to-even fixed formatter. Sprint times are nonnegative finite
// milliseconds; shortest decimal digits preserve near-tie division results.
fn format_report_milliseconds(value: f64) -> String {
    let decimal = value.to_string();
    let (whole, fraction) = decimal.split_once('.').unwrap_or((&decimal, ""));
    let mut whole = whole
        .parse::<u64>()
        .expect("nonnegative finite sprint milliseconds");
    let mut digits = fraction.bytes();
    let mut hundredths = u16::from(digits.next().unwrap_or(b'0') - b'0') * 10
        + u16::from(digits.next().unwrap_or(b'0') - b'0');
    if digits.next().unwrap_or(b'0') >= b'5' {
        hundredths += 1;
    }
    if hundredths == 100 {
        whole += 1;
        hundredths = 0;
    }
    format!("{whole}.{hundredths:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(manager: &TickRateManager) -> String {
        format!(
            "{}:{}:{}:{}",
            manager.runs_normally(),
            manager.is_frozen(),
            manager.is_stepping_forward(),
            manager.frozen_ticks_to_run()
        )
    }
    fn vector(name: &str) -> &'static str {
        include_str!("../../tests/java/level/tick-rate.tsv")
            .lines()
            .find_map(|line| {
                let (key, value) = line.split_once('\t')?;
                (key == name).then_some(value)
            })
            .unwrap()
    }
    #[test]
    fn rate_math_and_freeze_step_state_match_real_vanilla() {
        for line in include_str!("../../tests/java/level/tick-rate.tsv")
            .lines()
            .filter(|line| line.starts_with("rate:"))
        {
            let (input, expected) = line.split_once('\t').unwrap();
            let rate = f32::from_bits(input.strip_prefix("rate:").unwrap().parse().unwrap());
            let mut manager = TickRateManager::default();
            manager.set_tick_rate(rate);
            assert_eq!(
                format!(
                    "{}:{}:{}",
                    manager.tickrate().to_bits(),
                    manager.nanoseconds_per_tick(),
                    manager.milliseconds_per_tick().to_bits()
                ),
                expected
            );
        }
        let mut manager = TickRateManager::default();
        assert_eq!(state(&manager), vector("initial"));
        manager.set_frozen(true);
        assert_eq!(state(&manager), vector("freeze-before-tick"));
        manager.tick();
        assert_eq!(state(&manager), vector("frozen-tick"));
        manager.set_frozen_ticks_to_run(2);
        assert_eq!(state(&manager), vector("step-request"));
        for name in ["step-one", "step-two", "step-done"] {
            manager.tick();
            assert_eq!(state(&manager), vector(name));
        }
        assert!(manager.is_entity_frozen(false, 0));
        assert!(!manager.is_entity_frozen(true, 0));
        assert!(!manager.is_entity_frozen(false, 1));
        manager.set_frozen_ticks_to_run(3);
        manager.set_frozen(false);
        manager.tick();
        assert_eq!(state(&manager), vector("unfreeze-with-steps"));
    }
    #[test]
    fn sprint_reports_match_java_formatter_and_restore_frozen_state_after_final_work() {
        for line in include_str!("../../tests/java/level/tick-rate.tsv")
            .lines()
            .filter(|line| line.starts_with("sprint:"))
        {
            let (input, expected) = line.split_once('\t').unwrap();
            let mut numbers = input
                .strip_prefix("sprint:")
                .unwrap()
                .split(':')
                .map(|v| v.parse::<i64>().unwrap());
            let completed = numbers.next().unwrap();
            let nanos = numbers.next().unwrap();
            let mut manager = ServerTickRateManager {
                scheduled_current_sprint_ticks: completed,
                sprint_time_spent: nanos,
                ..Default::default()
            };
            manager.finish_tick_sprint();
            let events = manager.take_events();
            let TickRateEvent::SprintReport {
                ticks_per_second,
                milliseconds_per_tick,
                ..
            } = &events[0]
            else {
                panic!("expected report")
            };
            assert_eq!(
                format!("{ticks_per_second}:{milliseconds_per_tick}"),
                expected,
                "{input}"
            );
        }
        let mut manager = ServerTickRateManager::default();
        assert!(!manager.step_game_if_paused(1));
        manager.set_frozen(true);
        manager.tick();
        assert!(manager.step_game_if_paused(2));
        manager.tick();
        manager.tick();
        assert!(manager.runs_normally());
        assert!(!manager.is_stepping_forward());
        manager.tick();
        assert!(!manager.runs_normally());
        manager.take_events();
        assert!(!manager.request_game_to_sprint(2));
        // The outer loop checks the previous tick's run flag, just as vanilla.
        assert!(!manager.check_should_sprint_this_tick(0));
        manager.tick();
        for now in [10, 30] {
            assert!(manager.check_should_sprint_this_tick(now));
            manager.tick();
            manager.end_tick_work(now + 5);
        }
        assert!(manager.is_sprinting());
        assert!(!manager.stop_sprinting());
        assert!(!manager.check_should_sprint_this_tick(50));
        assert!(!manager.is_sprinting());
        assert!(manager.is_frozen());
        let events = manager.take_events();
        assert!(matches!(
            events.as_slice(),
            [
                TickRateEvent::State {
                    is_frozen: false,
                    ..
                },
                TickRateEvent::SprintReport {
                    completed_ticks: 2,
                    ..
                },
                TickRateEvent::State {
                    is_frozen: true,
                    ..
                },
                TickRateEvent::RateChanged
            ]
        ));
    }
}
