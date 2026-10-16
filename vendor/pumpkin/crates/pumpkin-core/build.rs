use std::{path::PathBuf, process::Command};

fn git_path(name: &str) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--path-format=absolute", "--git-path", name])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    Some(PathBuf::from(value.trim()))
}

/// Git resolves linked-worktree metadata outside the vendored package directory.
fn watch_git_revision() {
    if let Some(head) = git_path("HEAD").filter(|path| path.is_file()) {
        println!("cargo::rerun-if-changed={}", head.display());
    }
    let Ok(reference) = Command::new("git")
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .output()
    else {
        return;
    };
    if !reference.status.success() {
        return; // A detached revision is stored directly in HEAD.
    }
    let Ok(reference) = String::from_utf8(reference.stdout) else {
        return;
    };
    let Some(path) = git_path(reference.trim()) else {
        return;
    };
    if path.is_file() {
        println!("cargo::rerun-if-changed={}", path.display());
    } else {
        if let Some(packed) = git_path("packed-refs").filter(|path| path.is_file()) {
            println!("cargo::rerun-if-changed={}", packed.display());
        }
        // A packed or unborn branch can acquire a loose ref on its next commit.
        if let Some(directory) = path.ancestors().skip(1).find(|path| path.is_dir()) {
            println!("cargo::rerun-if-changed={}", directory.display());
        }
    }
}

fn main() {
    // Get short hash (7 chars) for display
    let short_output = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output();

    let git_hash_short = match short_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    };

    // Get full hash for hover text
    let full_output = Command::new("git").args(["rev-parse", "HEAD"]).output();

    let git_hash_full = match full_output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    };

    watch_git_revision();
    println!("cargo::rustc-env=GIT_HASH={git_hash_short}");
    println!("cargo::rustc-env=GIT_HASH_FULL={git_hash_full}");
}
