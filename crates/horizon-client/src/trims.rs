use std::{fs, path::Path};

/// The largest trim texture of the resources (the size of the atlas's trim
/// slots), at least vanilla's 64x32.
pub fn slot_size(root: &Path) -> (u32, u32) {
    let mut size = (64, 32);
    let Ok(namespaces) = fs::read_dir(root.join("assets")) else {
        return size;
    };
    for ns in namespaces.flatten() {
        let Ok(layers) = fs::read_dir(ns.path().join("textures/trims/entity")) else {
            continue;
        };
        for layer in layers.flatten() {
            for f in fs::read_dir(layer.path()).into_iter().flatten().flatten() {
                let path = f.path();
                if path.extension().is_some_and(|e| e == "png")
                    && let Some((w, h)) = png_size(&path)
                {
                    size = (size.0.max(w.min(512)), size.1.max(h.min(512)));
                }
            }
        }
    }
    size
}

/// The width and height in a PNG's IHDR chunk.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    let be = |i: usize| u32::from_be_bytes(head[i..i + 4].try_into().unwrap());
    (&head[12..16] == b"IHDR").then(|| (be(16), be(20)))
}
