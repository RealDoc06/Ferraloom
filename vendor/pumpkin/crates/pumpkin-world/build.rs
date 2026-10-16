use std::{
    env, fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use sha1::{Digest, Sha1};

/// Embed the presets and template test fixture; keep official downloads outside Git.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=PUMPKIN_MINECRAFT_SERVER_JAR");
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let root = manifest
        .ancestors()
        .nth(4)
        .ok_or("Cannot locate Ferraloom root")?;
    let jar = env::var_os("PUMPKIN_MINECRAFT_SERVER_JAR").map_or_else(
        || root.join(".cache/vanilla/26.3/server.jar"),
        PathBuf::from,
    );
    println!("cargo:rerun-if-changed={}", jar.display());
    let bytes = fs::read(&jar).map_err(|error| {
        format!(
            "Read {}: {error}. Run python3 tools/oracle.py fetch first.",
            jar.display()
        )
    })?;
    let expected = [
        0x33, 0x68, 0x0f, 0x5f, 0x2a, 0xc3, 0x28, 0x64, 0xd6, 0xd7, 0xcf, 0x5e, 0x56, 0xa7, 0x05,
        0xfd, 0xb3, 0xe0, 0x5f, 0x4c,
    ];
    if Sha1::digest(&bytes)[..] != expected {
        return Err("World presets require the exact official Minecraft 26.3 server".into());
    }
    let mut outer = zip::ZipArchive::new(Cursor::new(bytes))?;
    let mut inner_bytes = Vec::new();
    outer
        .by_name("META-INF/versions/26.3/server-26.3.jar")?
        .read_to_end(&mut inner_bytes)?;
    let mut inner = zip::ZipArchive::new(Cursor::new(inner_bytes))?;
    let build_output = PathBuf::from(env::var("OUT_DIR")?);
    let mut template = inner.by_name("data/minecraft/structure/igloo/top.nbt")?;
    let mut fixture = fs::File::create(build_output.join("igloo-top.nbt"))?;
    std::io::copy(&mut template, &mut fixture)?;
    drop(template);
    let output = build_output.join("vanilla-worldgen");
    let prefix = "data/minecraft/worldgen/";
    for index in 0..inner.len() {
        let mut entry = inner.by_index(index)?;
        let Some(name) = entry.name().strip_prefix(prefix) else {
            continue;
        };
        if !name.starts_with("world_preset/") && !name.starts_with("flat_level_generator_preset/") {
            continue;
        }
        if Path::new(name)
            .extension()
            .is_none_or(|extension| extension != "json")
            || name.contains("..")
        {
            continue;
        }
        let destination = output.join(name);
        fs::create_dir_all(destination.parent().ok_or("Invalid preset path")?)?;
        let mut file = fs::File::create(destination)?;
        std::io::copy(&mut entry, &mut file)?;
    }
    Ok(())
}
