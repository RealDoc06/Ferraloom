//! Opt-in SIGPROF sampling, with tick-thread self and inclusive frame counts.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Write,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, PartialEq)]
struct Settings {
    warmup: u64,
    seconds: u64,
    hz: i32,
}
impl Settings {
    fn parse(value: &str) -> Option<Self> {
        let parts: Vec<_> = value.split(':').collect();
        if !(2..=3).contains(&parts.len()) {
            return None;
        }
        let settings = Self {
            warmup: parts[0].parse().ok()?,
            seconds: parts[1].parse().ok()?,
            hz: parts.get(2).unwrap_or(&"99").parse().ok()?,
        };
        (settings.seconds > 0 && (1..=10000).contains(&settings.hz)).then_some(settings)
    }
}

/// Start one delayed profile when FERRALOOM_PROFILE is set on the dedicated server.
pub fn start() {
    if let Err(error) = rustix::thread::set_name(c"Ferraloom tick") {
        log::warn!("tick thread name: {error}");
    }
    let Ok(value) = std::env::var("FERRALOOM_PROFILE") else {
        return;
    };
    let Some(settings) = Settings::parse(&value) else {
        log::error!(
            "FERRALOOM_PROFILE expects warmup_s:seconds[:hz] (positive seconds, hz 1..10000)"
        );
        return;
    };
    if let Err(error) = std::thread::Builder::new()
        .name("Profile writer".into())
        .spawn(move || {
            if let Err(error) = collect(settings) {
                log::error!("CPU profile: {error}");
            }
        })
    {
        log::error!("CPU profiler thread: {error}");
    }
}

fn collect(settings: Settings) -> Result<(), Box<dyn std::error::Error>> {
    std::thread::sleep(Duration::from_secs(settings.warmup));
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(settings.hz)
        .build()?;
    log::info!(
        "CPU profile sampling for {}s at {} Hz",
        settings.seconds,
        settings.hz
    );
    std::thread::sleep(Duration::from_secs(settings.seconds));
    let report = guard.report().build()?;
    drop(guard);
    let stem = format!(
        "profile-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()
    );
    report.flamegraph(File::create(format!("{stem}.svg"))?)?;
    let mut out = File::create(format!("{stem}.txt"))?;
    let mut threads = BTreeMap::new();
    let mut inclusive = BTreeMap::new();
    let mut own = BTreeMap::new();
    let mut tick = 0;
    for (stack, &count) in &report.data {
        *threads
            .entry((stack.thread_name.clone(), stack.thread_id))
            .or_insert(0) += count;
        if stack.thread_name != "Ferraloom tick" {
            continue;
        }
        tick += count;
        let frames: Vec<_> = stack
            .frames
            .iter()
            .flatten()
            .map(|symbol| {
                format!(
                    "{} ({}:{})",
                    symbol.name(),
                    symbol
                        .filename
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    symbol.lineno.unwrap_or(0)
                )
            })
            .collect();
        if let Some(leaf) = frames.first() {
            *own.entry(leaf.clone()).or_insert(0) += count;
        }
        for frame in frames.into_iter().collect::<BTreeSet<_>>() {
            *inclusive.entry(frame).or_insert(0) += count;
        }
    }
    let total: isize = threads.values().sum();
    writeln!(
        out,
        "SIGPROF {} Hz, {}s requested; percentages are captured CPU samples, not wall time.\nThread shares ({} samples):",
        settings.hz, settings.seconds, total
    )?;
    for ((name, id), count) in threads {
        writeln!(
            out,
            "{:.2}% {count} {name} tid={id}",
            100.0 * count as f64 / total.max(1) as f64
        )?;
    }
    for (heading, counts) in [("SELF", own), ("INCLUSIVE", inclusive)] {
        writeln!(
            out,
            "\n{heading}: tick thread only ({tick} samples), top 40"
        )?;
        let mut rows: Vec<_> = counts.into_iter().collect();
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (frame, count) in rows.into_iter().take(40) {
            writeln!(
                out,
                "{:.2}% {count} {frame}",
                100.0 * count as f64 / tick.max(1) as f64
            )?;
        }
    }
    log::info!("CPU profile written: {stem}.svg and {stem}.txt");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_settings() {
        assert_eq!(
            Settings::parse("90:180"),
            Some(Settings {
                warmup: 90,
                seconds: 180,
                hz: 99
            })
        );
        assert_eq!(
            Settings::parse("0:2:100"),
            Some(Settings {
                warmup: 0,
                seconds: 2,
                hz: 100
            })
        );
        for invalid in [
            "",
            "1",
            "1:0",
            "1:2:0",
            "1:2:-1",
            "1:2:10001",
            "1:2:3:4",
            "x:2",
        ] {
            assert_eq!(Settings::parse(invalid), None);
        }
    }
}
