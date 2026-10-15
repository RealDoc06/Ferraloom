use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};

/// Contents vanilla writes into `session.lock`: a UTF-8 snowman.
const SENTINEL: &[u8] = "☃".as_bytes();

/// Lock `<world>/session.lock` without blocking, like vanilla's `DirectoryLock`.
/// Call before anything touches the world and keep the returned file open until
/// the final save finishes; dropping it releases the lock.
///
/// On Unix the lock is process-associated, so the process must never open and
/// close another handle to `session.lock` while holding it: closing any handle
/// to the file releases the lock.
pub fn acquire(world: &Path) -> Result<File> {
    fs::create_dir_all(world)
        .with_context(|| format!("Cannot create world directory {}", world.display()))?;
    let path = world.join("session.lock");
    // Never truncate: another process may hold the lock on this file.
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("Cannot open {}", path.display()))?;
    if !try_lock(&file).with_context(|| format!("Cannot lock {}", path.display()))? {
        bail!(
            "{} is already locked, possibly by another Minecraft server using this world",
            path.display()
        );
    }
    // Keep an existing lock file as it is; only a file we just created is empty.
    if file.metadata()?.len() == 0 {
        file.write_all(SENTINEL)
            .and_then(|()| file.sync_all())
            .with_context(|| format!("Cannot write {}", path.display()))?;
    }
    Ok(file)
}

/// Take the whole-file `F_SETLK` write lock Java's `FileChannel.tryLock` takes.
/// Linux keeps `flock(2)` locks (used by `File::try_lock`) separate from these,
/// so vanilla would not see them.
#[cfg(unix)]
fn try_lock(file: &File) -> io::Result<bool> {
    use rustix::{
        fs::{FlockOperation, fcntl_lock},
        io::Errno,
    };
    match fcntl_lock(file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(true),
        Err(Errno::ACCESS | Errno::AGAIN) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// On Windows `File::try_lock` is `LockFileEx` over every byte, which overlaps the
/// range Java's `FileChannel.tryLock` locks. Untested against vanilla there.
#[cfg(not(unix))]
fn try_lock(file: &File) -> io::Result<bool> {
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(fs::TryLockError::WouldBlock) => Ok(false),
        Err(fs::TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_vanilla_sentinel_only_into_new_lock_files() {
        let directory = tempfile::tempdir().unwrap();
        let world = directory.path().join("world");
        let path = world.join("session.lock");
        drop(acquire(&world).unwrap());
        assert_eq!(fs::read(&path).unwrap(), SENTINEL);
        fs::write(&path, "existing").unwrap();
        drop(acquire(&world).unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"existing");
    }

    /// Python's `fcntl.lockf` issues the same whole-file `F_SETLK` write lock as
    /// Java's `FileChannel.tryLock`, so it stands in for a vanilla server here.
    #[cfg(target_os = "linux")]
    #[test]
    fn conflicts_with_posix_record_locks_in_other_processes() {
        use std::{
            io::{BufRead, BufReader},
            process::{Command, Stdio},
        };

        const PROBE: &str = "import fcntl, sys\n\
            f = open(sys.argv[1], 'r+')\n\
            try:\n    fcntl.lockf(f, fcntl.LOCK_EX | fcntl.LOCK_NB)\n\
            except OSError:\n    sys.exit(3)\n\
            print('locked', flush=True)\n\
            sys.stdin.read()\n";
        let probe = |path: &Path, stdin: Stdio| {
            Command::new("python3")
                .args(["-c", PROBE])
                .arg(path)
                .stdin(stdin)
                .stdout(Stdio::piped())
                .spawn()
                .expect("python3 is required for the cross-process lock test")
        };
        let world = tempfile::tempdir().unwrap();
        let path = world.path().join("session.lock");

        let lock = acquire(world.path()).unwrap();
        let status = probe(&path, Stdio::null()).wait().unwrap();
        assert_eq!(
            status.code(),
            Some(3),
            "another process locked a held world"
        );
        drop(lock);

        let mut holder = probe(&path, Stdio::piped());
        let mut line = String::new();
        BufReader::new(holder.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line, "locked\n");
        let error = acquire(world.path()).unwrap_err();
        assert!(error.to_string().contains("already locked"), "{error:#}");
        drop(holder.stdin.take());
        assert!(holder.wait().unwrap().success());

        assert_eq!(fs::read(&path).unwrap(), SENTINEL);
        drop(acquire(world.path()).unwrap());
    }
}
