use serde_json::Value;
use std::{collections::HashMap, fs, path::Path};

/// `SampledFloat` as sounds use it: constants and `MultipliedFloats`.
#[derive(Clone, Debug, PartialEq)]
pub enum Sampled {
    Constant(f32),
    Product(Vec<Sampled>),
}
/// `Sound`.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    /// The file identifier (`namespace:path`, under `sounds/` with `.ogg`).
    pub location: String,
    pub volume: Sampled,
    pub pitch: Sampled,
    pub weight: i32,
    /// `Sound.Type.SOUND_EVENT` (a reference to another event).
    pub event: bool,
    pub stream: bool,
    pub preload: bool,
    pub attenuation_distance: i32,
    /// The `EMPTY_SOUND` / `INTENTIONALLY_EMPTY_SOUND` singletons (vanilla
    /// compares them by identity).
    pub sentinel: Sentinel,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sentinel {
    #[default]
    None,
    Empty,
    IntentionallyEmpty,
}
impl Sound {
    fn file(location: &str) -> Self {
        Self {
            location: location.to_owned(),
            volume: Sampled::Constant(1.0),
            pitch: Sampled::Constant(1.0),
            weight: 1,
            event: false,
            stream: false,
            preload: false,
            attenuation_distance: 16,
            sentinel: Sentinel::None,
        }
    }
    /// `Sound.getPath`: the resource path (`namespace`, `sounds/<path>.ogg`).
    pub fn path(&self) -> (&str, String) {
        let (namespace, path) = split(&self.location);
        (namespace, format!("sounds/{path}.ogg"))
    }
}
/// `WeighedSoundEvents`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeighedSoundEvents {
    /// Files, or references to events (`Sound.event`).
    pub sounds: Vec<Sound>,
    /// The subtitle translation key.
    pub subtitle: Option<String>,
}
/// A `sounds.json` entry: event, `replace`, subtitle and sounds.
type Registration = (String, bool, Option<String>, Vec<Sound>);
/// Parse a `sounds.json`; `None` when Gson would reject the whole file (a
/// malformed entry anywhere in it).
fn parse_file(namespace: &str, text: &[u8]) -> Option<Vec<Registration>> {
    let Ok(Value::Object(map)) = serde_json::from_slice::<Value>(text) else {
        return None;
    };
    let mut parsed = Vec::new();
    for (name, def) in &map {
        let Value::Object(object) = def else {
            return None;
        };
        let replace = json_bool(object.get("replace").unwrap_or(&Value::Null), false);
        let subtitle = match object.get("subtitle") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => return None,
        };
        let mut sounds = Vec::new();
        if let Some(list) = object.get("sounds") {
            let Value::Array(list) = list else {
                return None;
            };
            for s in list {
                sounds.push(parse_sound(s)?);
            }
        }
        parsed.push((format!("{namespace}:{name}"), replace, subtitle, sounds));
    }
    Some(parsed)
}
/// Whether vanilla's `SoundManager` reads this `sounds.json` at all.
pub fn is_valid_file(text: &[u8]) -> bool {
    parse_file("minecraft", text).is_some()
}
/// The sound event registry (`SoundManager.registry`).
#[derive(Clone, Debug, Default)]
pub struct SoundRegistry {
    events: HashMap<String, WeighedSoundEvents>,
}
/// `Identifier.parse` with the default namespace.
fn split(id: &str) -> (&str, &str) {
    match id.split_once(':') {
        Some((namespace, path)) => (
            if namespace.is_empty() {
                "minecraft"
            } else {
                namespace
            },
            path,
        ),
        None => ("minecraft", id),
    }
}
pub fn normalize(id: &str) -> String {
    let (namespace, path) = split(id);
    format!("{namespace}:{path}")
}
/// `GsonHelper.getAsFloat`: Gson parses the number's text as a float.
fn json_float(v: &Value, fallback: f32) -> f32 {
    match v {
        Value::Number(n) => n.to_string().parse().unwrap_or(fallback),
        Value::String(s) => s.parse().unwrap_or(fallback),
        _ => fallback,
    }
}
fn json_int(v: &Value, fallback: i32) -> i32 {
    match v {
        Value::Number(n) => n
            .as_i64()
            .map(|i| i as i32)
            .or_else(|| n.as_f64().map(|f| f as i32))
            .unwrap_or(fallback),
        Value::String(s) => s.parse().unwrap_or(fallback),
        _ => fallback,
    }
}
pub(crate) fn json_bool(v: &Value, fallback: bool) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::String(s) => s == "true",
        _ => fallback,
    }
}
/// `SoundEventRegistrationSerializer.getSound`; None for an invalid entry
/// (vanilla rejects the whole file).
fn parse_sound(v: &Value) -> Option<Sound> {
    if let Value::String(name) = v {
        return Some(Sound::file(&normalize(name)));
    }
    let name = v.get("name")?.as_str()?;
    let event = match v.get("type") {
        None => false,
        Some(t) => match t.as_str()? {
            "file" => false,
            "event" => true,
            _ => return None,
        },
    };
    let volume = json_float(v.get("volume").unwrap_or(&Value::Null), 1.0);
    let pitch = json_float(v.get("pitch").unwrap_or(&Value::Null), 1.0);
    let weight = json_int(v.get("weight").unwrap_or(&Value::Null), 1);
    if !(volume > 0.0 && pitch > 0.0 && weight > 0) {
        return None;
    }
    Some(Sound {
        location: normalize(name),
        volume: Sampled::Constant(volume),
        pitch: Sampled::Constant(pitch),
        weight,
        event,
        stream: json_bool(v.get("stream").unwrap_or(&Value::Null), false),
        preload: json_bool(v.get("preload").unwrap_or(&Value::Null), false),
        attenuation_distance: json_int(v.get("attenuation_distance").unwrap_or(&Value::Null), 16),
        sentinel: Sentinel::None,
    })
}
impl SoundRegistry {
    /// Every `assets/<namespace>/sounds.json` under a resource root, files
    /// validated against `assets/<namespace>/sounds/`.
    pub fn load(root: &Path) -> Self {
        let assets = root.join("assets");
        let mut namespaces: Vec<String> = fs::read_dir(&assets)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .collect();
        namespaces.sort();
        let mut registry = Self::default();
        for namespace in namespaces {
            let Ok(text) = fs::read(assets.join(&namespace).join("sounds.json")) else {
                continue;
            };
            let exists = |ns: &str, path: &str| assets.join(ns).join(path).is_file();
            registry.add_file(&namespace, &text, &exists);
        }
        registry
    }
    /// One `sounds.json` (`Preparations.handleRegistration` for each entry).
    /// `exists(namespace, path)` says whether a sound file is present.
    pub fn add_file(&mut self, namespace: &str, text: &[u8], exists: &dyn Fn(&str, &str) -> bool) {
        let Some(parsed) = parse_file(namespace, text) else {
            return;
        };
        for (location, replace, subtitle, sounds) in parsed {
            let entry = match self.events.get_mut(&location) {
                Some(entry) if !replace => entry,
                _ => {
                    self.events.insert(
                        location.clone(),
                        WeighedSoundEvents {
                            sounds: Vec::new(),
                            subtitle,
                        },
                    );
                    self.events.get_mut(&location).expect("inserted")
                }
            };
            for sound in sounds {
                if !sound.event {
                    let (ns, path) = sound.path();
                    if !exists(ns, &path) {
                        continue;
                    }
                }
                entry.sounds.push(sound);
            }
        }
    }
    /// Every registered event id.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.events.keys()
    }
}
