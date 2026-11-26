//! The game process's RAM and CPU, for the supervisor's status samples (macOS and Linux; `None`
//! elsewhere).

/// What `ps -o <field>= -p <pid>` prints, trimmed (macOS).
fn ps(field: &str, pid: u32) -> Option<String> {
    let out = crate::process::command("ps")
        .args(["-o", &format!("{field}="), "-p", &pid.to_string()])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !text.is_empty()).then_some(text)
}

/// Resident memory of a process, in bytes.
pub fn rss_bytes(pid: u32) -> Option<u64> {
    let kb = if cfg!(target_os = "macos") {
        ps("rss", pid)?.parse::<u64>().ok()?
    } else {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
        line.split_whitespace().nth(1)?.parse::<u64>().ok()?
    };
    Some(kb * 1024)
}

/// CPU time a process has used, user and system, in seconds.
pub fn cpu_seconds(pid: u32) -> Option<f64> {
    if cfg!(target_os = "macos") {
        // "[[dd-]hh:]mm:ss.cc"
        let time = ps("time", pid)?;
        let (rest, days) = match time.split_once('-') {
            Some((d, rest)) => (rest, d.parse::<f64>().ok()?),
            None => (time.as_str(), 0.0),
        };
        let seconds = rest.split(':').try_fold(0.0, |acc, part| {
            part.parse::<f64>().ok().map(|v| acc * 60.0 + v)
        })?;
        return Some(days * 86_400.0 + seconds);
    }
    // utime and stime, fields 14 and 15, in clock ticks (100 per second on Linux).
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    let ticks = fields.get(11)?.parse::<f64>().ok()? + fields.get(12)?.parse::<f64>().ok()?;
    Some(ticks / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn reads_this_process() {
        assert!(rss_bytes(std::process::id()).unwrap() > 1 << 20);
        assert!(cpu_seconds(std::process::id()).unwrap() >= 0.0);
    }
}
