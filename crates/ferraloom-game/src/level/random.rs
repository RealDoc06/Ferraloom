//! Construction of Level's nondeterministically seeded random streams.
use ferraloom_util::random::{
    LegacyRandomSource, Random, RandomSource, SingleThreadedRandomSource,
};
use std::{
    sync::{
        OnceLock,
        atomic::{AtomicI64, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};
static SEED_UNIQUIFIER: AtomicI64 = AtomicI64::new(8_682_522_807_148_012);

// System.nanoTime has an arbitrary process/platform origin. Anchor Rust's
// monotonic clock once; its absolute origin is deliberately nondeterministic.
fn nano_time() -> i64 {
    static ORIGIN: OnceLock<(Instant, i64)> = OnceLock::new();
    let (instant, offset) = ORIGIN.get_or_init(|| {
        (
            Instant::now(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as i64,
        )
    });
    offset.wrapping_add(instant.elapsed().as_nanos() as i64)
}
/// Port of Util#getMillis.
pub(crate) fn get_millis() -> i64 {
    nano_time() / 1_000_000
}
/// Port of RandomSource#create and RandomSupport#generateUniqueSeed. This is
/// Level#random, independent of the world seed; tests can install a fixed stream.
pub(super) fn new_level_random() -> Random {
    let previous = SEED_UNIQUIFIER
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |seed| {
            Some(seed.wrapping_mul(1_181_783_497_276_652_981))
        })
        .unwrap();
    let seed = previous.wrapping_mul(1_181_783_497_276_652_981) ^ nano_time();
    Random::Legacy(LegacyRandomSource::new(seed))
}
/// RandomSource#createThreadSafe uses the same unique seed factory and Legacy
/// algorithm. Region ownership replaces its Java synchronization, without locks.
pub(super) fn new_sound_random() -> Random {
    new_level_random()
}
/// Level initializes randValue from a temporary SingleThreadedRandomSource,
/// seeded by Netty's nondeterministic thread-local generator in Java. It does
/// not draw from the canonical Level#random stream.
pub(super) fn new_position_seed() -> i32 {
    SingleThreadedRandomSource::new(nano_time()).next_int()
}
