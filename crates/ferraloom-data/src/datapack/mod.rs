//! Immutable server-data resources and generic registry tag loading.

use serde_json::Value;
mod block_state;
mod config;
pub mod metadata;
pub mod repository;
pub use block_state::decode_block_state;
pub use config::{DataPackConfig, FeatureFlags, WorldDataConfiguration};
pub mod tags;
pub use tags::{StaticTagElement, TagElement, TagId, TagKey, TagSet};

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    io::{self, Cursor, Read, Seek},
    path::Path,
    str::FromStr,
    sync::Arc,
};

/// Port of Identifier. Resource paths use `/` independently of the host OS.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Identifier {
    namespace: String,
    path: String,
}

impl Identifier {
    pub fn new(namespace: impl Into<String>, path: impl Into<String>) -> Result<Self, Error> {
        let namespace = namespace.into();
        let path = path.into();
        let valid = |c: u8| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c);
        if namespace == ".."
            || !namespace.bytes().all(valid)
            || !path.bytes().all(|c| valid(c) || c == b'/')
        {
            return Err(Error::InvalidIdentifier(format!("{namespace}:{path}")));
        }
        Ok(Self { namespace, path })
    }
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn with_path(&self, path: impl Into<String>) -> Result<Self, Error> {
        Self::new(self.namespace.clone(), path)
    }
}
impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path
            .cmp(&other.path)
            .then_with(|| self.namespace.cmp(&other.namespace))
    }
}
impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl FromStr for Identifier {
    type Err = Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (namespace, path) = value.split_once(':').unwrap_or(("minecraft", value));
        Self::new(
            if namespace.is_empty() {
                "minecraft"
            } else {
                namespace
            },
            path,
        )
    }
}
impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Zip(zip::result::ZipError),
    Json(serde_json::Error),
    InvalidIdentifier(String),
    InvalidTag(String),
    InvalidData(String),
    MissingResource(Identifier),
    UnsupportedPack { pack: String, operation: String },
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Zip(e) => e.fmt(f),
            Self::Json(e) => e.fmt(f),
            Self::InvalidIdentifier(s) => write!(f, "invalid resource identifier: {s}"),
            Self::InvalidData(s) => write!(f, "invalid resource data: {s}"),
            Self::InvalidTag(s) => write!(f, "invalid tag: {s}"),
            Self::MissingResource(id) => write!(f, "missing resource: {id}"),
            Self::UnsupportedPack { pack, operation } => {
                write!(f, "Unsupported datapack {pack}: {operation}")
            }
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

/// A snapshot of a pack's `data/` tree. Eager reads keep shared access lock-free.
#[derive(Clone, Debug)]
pub struct Pack {
    name: String,
    resources: BTreeMap<Identifier, Arc<[u8]>>,
}
impl Pack {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            resources: BTreeMap::new(),
        }
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn insert(&mut self, id: Identifier, data: impl Into<Arc<[u8]>>) {
        self.resources.insert(id, data.into());
    }
    pub fn from_directory(name: impl Into<String>, root: &Path) -> Result<Self, Error> {
        fn visit(pack: &mut Pack, base: &Path, current: &Path) -> Result<(), Error> {
            if !current.exists() {
                return Ok(());
            }
            for entry in fs::read_dir(current)? {
                let entry = entry?;
                let ty = entry.file_type()?;
                if ty.is_dir() {
                    visit(pack, base, &entry.path())?;
                } else if ty.is_file() {
                    let path = entry.path();
                    let relative = path
                        .strip_prefix(base)
                        .expect("descendant")
                        .to_string_lossy()
                        .replace('\\', "/");
                    if let Some(id) = resource_id(&relative) {
                        pack.insert(id, fs::read(path)?);
                    }
                }
            }
            Ok(())
        }
        let mut pack = Self::new(name);
        visit(&mut pack, root, &root.join("data"))?;
        Ok(pack)
    }
    pub fn from_zip(name: impl Into<String>, reader: impl Read + Seek) -> Result<Self, Error> {
        let mut archive = zip::ZipArchive::new(reader)?;
        let mut pack = Self::new(name);
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if !file.is_file() {
                continue;
            }
            if let Some(id) = resource_id(file.name()) {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)?;
                pack.insert(id, bytes);
            }
        }
        Ok(pack)
    }
    /// Accepts both the extracted server jar and Mojang's executable bundle.
    pub fn vanilla_jar(path: &Path) -> Result<Self, Error> {
        Self::from_zip("vanilla", Cursor::new(vanilla_archive(path)?))
    }
    pub(super) fn from_zip_root(
        name: impl Into<String>,
        reader: impl Read + Seek,
        prefix: &str,
    ) -> Result<Self, Error> {
        let mut archive = zip::ZipArchive::new(reader)?;
        let mut pack = Self::new(name);
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if file.is_file()
                && let Some(id) = file.name().strip_prefix(prefix).and_then(resource_id)
            {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)?;
                pack.insert(id, bytes);
            }
        }
        Ok(pack)
    }
    pub(super) fn overlay(&mut self, overlay: Self) {
        self.resources.extend(overlay.resources);
    }
}
/// Shared executable-bundle ingress for the base pack and built-in feature packs.
fn vanilla_archive(path: &Path) -> Result<Arc<[u8]>, Error> {
    let mut archive = zip::ZipArchive::new(fs::File::open(path)?)?;
    if archive
        .file_names()
        .any(|name| name.starts_with("data/minecraft/"))
    {
        let mut file = archive.into_inner();
        file.rewind()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        return Ok(bytes.into());
    }
    let mut versions = String::new();
    archive
        .by_name("META-INF/versions.list")?
        .read_to_string(&mut versions)?;
    let jar = versions
        .lines()
        .find_map(|line| line.split('\t').nth(2))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "empty server bundle"))?;
    let mut bytes = Vec::new();
    archive
        .by_name(&format!("META-INF/versions/{jar}"))?
        .read_to_end(&mut bytes)?;
    Ok(bytes.into())
}
fn resource_id(path: &str) -> Option<Identifier> {
    let (namespace, path) = path.strip_prefix("data/")?.split_once('/')?;
    Identifier::new(namespace, path).ok()
}

/// Port of MultiPackResourceManager: packs are supplied lowest priority first.
#[derive(Clone, Debug, Default)]
pub struct ResourceManager {
    packs: Vec<Pack>,
}
#[derive(Clone, Copy, Debug)]
pub struct Resource<'a> {
    pub source: &'a str,
    pub bytes: &'a [u8],
}
impl Resource<'_> {
    pub fn json(&self) -> Result<Value, Error> {
        strict_json(self.bytes)
    }
}
// Resource#openAsReader uses the JDK UTF-8 decoder's replacement policy.
// An encoded surrogate (ED A0..BF xx) consumes its complete continuation prefix
// as one malformed input; Rust's from_utf8_lossy instead replaces its bytes.
pub fn read_utf8(bytes: &[u8]) -> String {
    let mut output = String::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0xed
            && bytes
                .get(index + 1)
                .is_some_and(|byte| (0xa0..=0xbf).contains(byte))
        {
            output.push_str(&String::from_utf8_lossy(&bytes[start..index]));
            output.push('\u{fffd}');
            index += 2;
            if bytes
                .get(index)
                .is_some_and(|byte| (0x80..=0xbf).contains(byte))
            {
                index += 1;
            }
            start = index;
        } else {
            index += 1;
        }
    }
    output.push_str(&String::from_utf8_lossy(&bytes[start..]));
    output
}

/// Port of StrictJsonParser. Gson retains the final duplicate object key and
/// skips the end-of-document check for a null root; streaming preserves both.
fn strict_json(bytes: &[u8]) -> Result<Value, Error> {
    let text = read_utf8(bytes);
    let bytes = text.as_bytes();
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    decoder.disable_recursion_limit();
    let mut stream = decoder.into_iter::<Value>();
    let value = stream.next().transpose()?.unwrap_or(Value::Null);
    if !value.is_null()
        && bytes[stream.byte_offset()..]
            .iter()
            .any(|b| !b.is_ascii_whitespace())
    {
        return Err(Error::InvalidData(
            "JSON document has trailing content".into(),
        ));
    }
    Ok(value)
}

impl ResourceManager {
    pub fn new(packs: Vec<Pack>) -> Self {
        Self { packs }
    }
    pub fn get(&self, id: &Identifier) -> Option<Resource<'_>> {
        self.packs.iter().rev().find_map(|pack| {
            pack.resources.get(id).map(|bytes| Resource {
                source: &pack.name,
                bytes,
            })
        })
    }
    pub fn require(&self, id: &Identifier) -> Result<Resource<'_>, Error> {
        self.get(id)
            .ok_or_else(|| Error::MissingResource(id.clone()))
    }
    pub fn stack(&self, id: &Identifier) -> Vec<Resource<'_>> {
        self.packs
            .iter()
            .filter_map(|pack| {
                pack.resources.get(id).map(|bytes| Resource {
                    source: &pack.name,
                    bytes,
                })
            })
            .collect()
    }
    pub fn list(&self, prefix: &str, suffix: &str) -> BTreeSet<Identifier> {
        self.packs
            .iter()
            .flat_map(|p| p.resources.keys())
            .filter(|id| id.path.starts_with(prefix) && id.path.ends_with(suffix))
            .cloned()
            .collect()
    }
    /// Port of FileToIdConverter#json. Keys omit the registry prefix and `.json`.
    pub fn registry(&self, directory: &str) -> BTreeMap<Identifier, Resource<'_>> {
        let prefix = format!("{directory}/");
        self.list(&prefix, ".json")
            .into_iter()
            .map(|file| {
                let id = file
                    .with_path(
                        file.path
                            .strip_prefix(&prefix)
                            .unwrap()
                            .strip_suffix(".json")
                            .unwrap(),
                    )
                    .unwrap();
                (id, self.get(&file).unwrap())
            })
            .collect()
    }
}

/// Port of TagEntry. Missing optional elements and tags contribute no values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagEntry {
    pub id: Identifier,
    pub tag: bool,
    pub required: bool,
}
impl TagEntry {
    pub fn decode(value: &Value) -> Result<Self, Error> {
        let (id, required) = if let Some(id) = value.as_str() {
            (id, true)
        } else {
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::InvalidTag("expected entry id".into()))?;
            let required = match value.get("required") {
                None => true,
                Some(v) => v
                    .as_bool()
                    .ok_or_else(|| Error::InvalidTag("required must be boolean".into()))?,
            };
            (id, required)
        };
        let tag = id.starts_with('#');
        Ok(Self {
            id: id.strip_prefix('#').unwrap_or(id).parse()?,
            tag,
            required,
        })
    }
}
#[derive(Clone, Debug)]
pub struct TagDiagnostic {
    pub tag: Identifier,
    pub source: String,
    pub message: String,
}
#[derive(Debug)]
pub struct Tags<T> {
    pub values: BTreeMap<Identifier, Vec<T>>,
    pub diagnostics: Vec<TagDiagnostic>,
}

/// Port of TagLoader#load/build. Invalid files and unbuildable tags are reported
/// individually; successfully built tags remain available to other registries.
pub fn load_tags<T: Clone + Eq>(
    manager: &ResourceManager,
    registry: &str,
    lookup: impl Fn(&Identifier, bool) -> Option<T>,
) -> Tags<T> {
    let directory = format!("tags/{registry}/");
    let mut builders: BTreeMap<Identifier, Vec<(TagEntry, String)>> = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for file in manager.list(&directory, ".json") {
        let id = file
            .with_path(
                file.path
                    .strip_prefix(&directory)
                    .unwrap()
                    .strip_suffix(".json")
                    .unwrap(),
            )
            .unwrap();
        for resource in manager.stack(&file) {
            let json = match resource.json() {
                Ok(json) => json,
                Err(error) => {
                    diagnostics.push(TagDiagnostic {
                        tag: id.clone(),
                        source: resource.source.into(),
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            // TagLoader creates the builder after parsing JSON, before TagFile's
            // codec runs. A malformed TagFile can therefore leave an empty tag.
            builders.entry(id.clone()).or_default();
            let decode = || -> Result<(bool, Vec<TagEntry>), Error> {
                let replace = match json.get("replace") {
                    None => false,
                    Some(v) => v
                        .as_bool()
                        .ok_or_else(|| Error::InvalidTag("replace must be boolean".into()))?,
                };
                let entries = json
                    .get("values")
                    .and_then(Value::as_array)
                    .ok_or_else(|| Error::InvalidTag("expected values list".into()))?
                    .iter()
                    .map(TagEntry::decode)
                    .collect::<Result<_, _>>()?;
                Ok((replace, entries))
            };
            match decode() {
                Ok((replace, entries)) => {
                    let builder = builders.entry(id.clone()).or_default();
                    if replace {
                        builder.clear();
                    }
                    builder.extend(
                        entries
                            .into_iter()
                            .map(|entry| (entry, resource.source.to_owned())),
                    );
                }
                Err(e) => diagnostics.push(TagDiagnostic {
                    tag: id.clone(),
                    source: resource.source.into(),
                    message: e.to_string(),
                }),
            }
        }
    }
    // DependencySorter adds required edges first, then optional edges, dropping
    // edges that would introduce a cycle. This also permits optional cycles.
    fn reaches(
        edges: &BTreeMap<Identifier, BTreeSet<Identifier>>,
        from: &Identifier,
        to: &Identifier,
    ) -> bool {
        from == to
            || edges
                .get(from)
                .is_some_and(|deps| deps.iter().any(|dep| reaches(edges, dep, to)))
    }
    let mut edges: BTreeMap<Identifier, BTreeSet<Identifier>> = BTreeMap::new();
    for required in [true, false] {
        for (id, entries) in &builders {
            for (entry, _) in entries
                .iter()
                .filter(|(e, _)| e.tag && e.required == required)
            {
                if !reaches(&edges, &entry.id, id) {
                    edges
                        .entry(id.clone())
                        .or_default()
                        .insert(entry.id.clone());
                }
            }
        }
    }
    fn visit(
        id: &Identifier,
        edges: &BTreeMap<Identifier, BTreeSet<Identifier>>,
        seen: &mut BTreeSet<Identifier>,
        order: &mut Vec<Identifier>,
    ) {
        if seen.insert(id.clone()) {
            if let Some(deps) = edges.get(id) {
                for dep in deps {
                    visit(dep, edges, seen, order);
                }
            }
            order.push(id.clone());
        }
    }
    let mut order = Vec::new();
    let mut seen = BTreeSet::new();
    for id in builders.keys() {
        visit(id, &edges, &mut seen, &mut order);
    }
    let mut values: BTreeMap<Identifier, Vec<T>> = BTreeMap::new();
    for id in order {
        let Some(entries) = builders.get(&id) else {
            continue;
        };
        let mut output = Vec::new();
        let mut missing = false;
        for (entry, source) in entries {
            let resolved = if entry.tag {
                values.get(&entry.id).cloned()
            } else {
                lookup(&entry.id, entry.required).map(|v| vec![v])
            };
            if let Some(resolved) = resolved {
                for value in resolved {
                    if !output.contains(&value) {
                        output.push(value);
                    }
                }
            } else if entry.required {
                missing = true;
                diagnostics.push(TagDiagnostic {
                    tag: id.clone(),
                    source: source.clone(),
                    message: format!("missing {}{}", if entry.tag { "#" } else { "" }, entry.id),
                });
            }
        }
        if !missing {
            values.insert(id, output);
        }
    }
    Tags {
        values,
        diagnostics,
    }
}

#[cfg(test)]
mod tests;
