use std::sync::atomic::{AtomicU32, Ordering};

use pumpkin_config::AntiSpamConfig;

use super::SpamType;

/// Java 26.3 uses separate TickThrottlers for chat and commands.
#[derive(Default)]
pub(super) struct SpamCounters {
    chat: AtomicU32,
    command: AtomicU32,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SpamViolation {
    pub count: u32,
    pub threshold: u32,
}

impl SpamCounters {
    /// Charge before checking exemptions so deopping a player retains their debt.
    pub fn record(
        &self,
        config: &AntiSpamConfig,
        spam_type: SpamType,
        is_operator: impl FnOnce() -> bool,
    ) -> Option<SpamViolation> {
        // Ferraloom's global switch suspends accounting and decay; a zero per-type
        // threshold only disables enforcement, as in vanilla.
        if !config.enabled {
            return None;
        }
        let (counter, threshold) = match spam_type {
            SpamType::Chat => (&self.chat, config.chat_threshold_ticks()),
            SpamType::Command => (&self.command, config.command_threshold_ticks()),
        };
        // Saturate rather than reproduce Java's signed-int overflow. Otherwise a
        // bypassed/disabled budget or a large configured cost could wrap below its limit.
        // The update always returns Some, so fetch_update always succeeds.
        let previous = counter
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                Some(count.saturating_add(config.message_cost))
            })
            .unwrap_or_else(|count| count);
        let count = previous.saturating_add(config.message_cost);

        if threshold == 0 || count < threshold || (config.ops_bypass && is_operator()) {
            None
        } else {
            Some(SpamViolation { count, threshold })
        }
    }

    /// Called on player/network ticks, independent of simulation freeze or exemptions.
    pub fn tick(&self, config: &AntiSpamConfig) {
        if !config.enabled || config.decay_per_tick == 0 {
            return;
        }
        for counter in [&self.chat, &self.command] {
            let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                Some(count.saturating_sub(config.decay_per_tick))
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_default_budgets_disconnect_at_the_exact_threshold() {
        let config = AntiSpamConfig::default();
        let counters = SpamCounters::default();
        // DedicatedServerProperties defaults both thresholds to 10 seconds.
        assert_eq!(config.chat_threshold_ticks(), 200);
        assert_eq!(config.command_threshold_ticks(), 200);
        for _ in 0..9 {
            assert_eq!(counters.record(&config, SpamType::Chat, || false), None);
            assert_eq!(counters.record(&config, SpamType::Command, || false), None);
        }
        for spam_type in [SpamType::Chat, SpamType::Command] {
            assert_eq!(
                counters.record(&config, spam_type, || false),
                Some(SpamViolation {
                    count: 200,
                    threshold: 200,
                })
            );
        }
    }

    #[test]
    fn operator_bypass_accumulates_debt_and_deop_retains_it() {
        let config = AntiSpamConfig::default();
        let counters = SpamCounters::default();
        for _ in 0..12 {
            assert_eq!(counters.record(&config, SpamType::Command, || true), None);
        }
        assert_eq!(
            counters.record(&config, SpamType::Command, || false),
            Some(SpamViolation {
                count: 260,
                threshold: 200,
            })
        );
        assert_eq!(counters.record(&config, SpamType::Chat, || false), None);
    }

    #[test]
    fn exemptions_are_only_queried_after_charging_at_the_threshold() {
        let config = AntiSpamConfig {
            spam_threshold: 40,
            ..AntiSpamConfig::default()
        };
        let counters = SpamCounters::default();
        assert_eq!(
            counters.record(&config, SpamType::Command, || panic!("under threshold")),
            None
        );
        assert_eq!(
            counters.record(&config, SpamType::Command, || {
                assert_eq!(counters.command.load(Ordering::Relaxed), 40);
                true
            }),
            None
        );
        let config = AntiSpamConfig {
            ops_bypass: false,
            ..config
        };
        assert_eq!(
            counters.record(&config, SpamType::Command, || panic!("bypass disabled")),
            Some(SpamViolation {
                count: 60,
                threshold: 40,
            })
        );
    }

    #[test]
    fn zero_threshold_disables_only_its_own_enforcement() {
        let mut config = AntiSpamConfig {
            chat_spam_threshold_seconds: Some(0),
            command_spam_threshold_seconds: Some(1),
            ..AntiSpamConfig::default()
        };
        let counters = SpamCounters::default();
        for _ in 0..12 {
            assert_eq!(
                counters.record(&config, SpamType::Chat, || panic!("zero threshold")),
                None
            );
        }
        assert_eq!(
            counters.record(&config, SpamType::Command, || false),
            Some(SpamViolation {
                count: 20,
                threshold: 20,
            })
        );
        counters.tick(&config);
        config.chat_spam_threshold_seconds = Some(10);
        assert_eq!(
            counters.record(&config, SpamType::Chat, || false),
            Some(SpamViolation {
                count: 259,
                threshold: 200,
            })
        );
    }

    #[test]
    fn global_switch_suspends_accounting_and_decay() {
        let mut config = AntiSpamConfig::default();
        let counters = SpamCounters::default();
        assert_eq!(counters.record(&config, SpamType::Chat, || false), None);
        config.enabled = false;
        for _ in 0..12 {
            assert_eq!(
                counters.record(&config, SpamType::Chat, || panic!("globally disabled")),
                None
            );
            assert_eq!(
                counters.record(&config, SpamType::Command, || panic!("globally disabled")),
                None
            );
            counters.tick(&config);
        }
        assert_eq!(counters.chat.load(Ordering::Relaxed), 20);
        assert_eq!(counters.command.load(Ordering::Relaxed), 0);
        config.enabled = true;
        counters.tick(&config);
        assert_eq!(counters.chat.load(Ordering::Relaxed), 19);
    }

    #[test]
    fn decay_applies_to_both_budgets_and_stops_at_zero() {
        let config = AntiSpamConfig::default();
        let counters = SpamCounters::default();
        for spam_type in [SpamType::Chat, SpamType::Command] {
            assert_eq!(counters.record(&config, spam_type, || false), None);
        }
        counters.tick(&config);
        assert_eq!(counters.chat.load(Ordering::Relaxed), 19);
        assert_eq!(counters.command.load(Ordering::Relaxed), 19);
        for _ in 0..25 {
            counters.tick(&config);
        }
        assert_eq!(counters.chat.load(Ordering::Relaxed), 0);
        assert_eq!(counters.command.load(Ordering::Relaxed), 0);

        for _ in 0..9 {
            assert_eq!(counters.record(&config, SpamType::Chat, || false), None);
        }
        counters.tick(&config);
        assert_eq!(counters.record(&config, SpamType::Chat, || false), None);
        assert_eq!(counters.chat.load(Ordering::Relaxed), 199);
    }

    #[test]
    fn configured_cost_decay_and_legacy_threshold_are_used() {
        let mut config = AntiSpamConfig {
            spam_threshold: 21,
            message_cost: 7,
            decay_per_tick: 5,
            ops_bypass: false,
            ..AntiSpamConfig::default()
        };
        let counters = SpamCounters::default();
        for _ in 0..2 {
            assert_eq!(counters.record(&config, SpamType::Chat, || true), None);
        }
        assert_eq!(
            counters.record(&config, SpamType::Chat, || true),
            Some(SpamViolation {
                count: 21,
                threshold: 21,
            })
        );
        counters.tick(&config);
        assert_eq!(counters.chat.load(Ordering::Relaxed), 16);
        config.decay_per_tick = 0;
        counters.tick(&config);
        assert_eq!(counters.chat.load(Ordering::Relaxed), 16);
        config.decay_per_tick = 30;
        counters.tick(&config);
        assert_eq!(counters.chat.load(Ordering::Relaxed), 0);
        config.message_cost = 0;
        assert_eq!(counters.record(&config, SpamType::Chat, || false), None);
        assert_eq!(counters.chat.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn per_type_seconds_override_the_legacy_threshold() {
        let config = AntiSpamConfig {
            spam_threshold: 1,
            chat_spam_threshold_seconds: Some(2),
            command_spam_threshold_seconds: Some(3),
            ..AntiSpamConfig::default()
        };
        let counters = SpamCounters::default();
        assert_eq!(counters.record(&config, SpamType::Chat, || false), None);
        assert_eq!(
            counters.record(&config, SpamType::Chat, || false),
            Some(SpamViolation {
                count: 40,
                threshold: 40,
            })
        );
        for _ in 0..2 {
            assert_eq!(counters.record(&config, SpamType::Command, || false), None);
        }
        assert_eq!(
            counters.record(&config, SpamType::Command, || false),
            Some(SpamViolation {
                count: 60,
                threshold: 60,
            })
        );
    }

    #[test]
    fn large_costs_saturate_without_evading_the_threshold() {
        let config = AntiSpamConfig {
            spam_threshold: u32::MAX,
            message_cost: u32::MAX - 1,
            decay_per_tick: u32::MAX,
            ..AntiSpamConfig::default()
        };
        let counters = SpamCounters::default();
        assert_eq!(counters.record(&config, SpamType::Command, || true), None);
        assert_eq!(counters.record(&config, SpamType::Command, || true), None);
        assert_eq!(
            counters.record(&config, SpamType::Command, || false),
            Some(SpamViolation {
                count: u32::MAX,
                threshold: u32::MAX,
            })
        );
        counters.tick(&config);
        assert_eq!(counters.command.load(Ordering::Relaxed), 0);
    }
}
