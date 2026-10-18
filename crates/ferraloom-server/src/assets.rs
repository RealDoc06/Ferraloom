//! Official, SHA-1 pinned 26.3 server data. No Mojang resources are embedded.
use anyhow::{Context, Result, ensure};
use sha1::{Digest, Sha1};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
pub const VANILLA_SHA1: &str = "33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c";
pub const VANILLA_URL: &str =
    "https://piston-data.mojang.com/v1/objects/33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c/server.jar";

pub async fn vanilla_jar(cache: &Path) -> Result<PathBuf> {
    let path = std::env::var_os("FERRALOOM_VANILLA_JAR")
        .map_or_else(|| cache.join("vanilla/26.3/server.jar"), PathBuf::from);
    if path.is_file() {
        verify(&fs::read(&path)?)?;
        return Ok(path);
    }
    fs::create_dir_all(path.parent().context("jar has no parent")?)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let bytes = client
        .get(VANILLA_URL)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    verify(&bytes)?;
    let temp = path.with_extension("jar.part");
    fs::write(&temp, &bytes)?;
    fs::rename(temp, &path)?;
    Ok(path)
}
fn verify(bytes: &[u8]) -> Result<()> {
    ensure!(
        format!("{:x}", Sha1::digest(bytes)) == VANILLA_SHA1,
        "26.3 server jar SHA-1 mismatch"
    );
    Ok(())
}
