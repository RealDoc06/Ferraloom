//! Port of DedicatedServerProperties and Settings. Unknown properties survive a reload.
use anyhow::{Context, Result, bail};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug)]
pub struct ServerProperties {
    values: BTreeMap<String, String>,
}

#[derive(Clone, Copy)]
enum Kind {
    Text,
    Boolean,
    Int,
    Long,
    Difficulty,
    GameMode,
    WorldSize,
    BroadcastRange,
    Permission,
}
use Kind::*;
const KEYS: &[(&str, &str, Kind)] = &[
    ("online-mode", "true", Boolean),
    ("prevent-proxy-connections", "false", Boolean),
    ("server-ip", "", Text),
    ("allow-flight", "false", Boolean),
    ("motd", "A Minecraft Server", Text),
    ("enable-code-of-conduct", "false", Boolean),
    ("bug-report-link", "", Text),
    ("force-gamemode", "false", Boolean),
    ("enforce-whitelist", "false", Boolean),
    ("difficulty", "easy", Difficulty),
    ("gamemode", "survival", GameMode),
    ("level-name", "world", Text),
    ("server-port", "25565", Int),
    ("management-server-enabled", "false", Boolean),
    ("management-server-host", "localhost", Text),
    ("management-server-port", "0", Int),
    ("management-server-secret", "", Text),
    ("management-server-tls-enabled", "true", Boolean),
    ("management-server-tls-keystore", "", Text),
    ("management-server-tls-keystore-password", "", Text),
    ("management-server-allowed-origins", "", Text),
    ("enable-query", "false", Boolean),
    ("query.port", "25565", Int),
    ("enable-rcon", "false", Boolean),
    ("rcon.port", "25575", Int),
    ("rcon.password", "", Text),
    ("hardcore", "false", Boolean),
    ("use-native-transport", "true", Boolean),
    ("spawn-protection", "16", Int),
    ("op-permission-level", "4", Permission),
    ("function-permission-level", "2", Permission),
    ("max-tick-time", "60000", Long),
    ("max-chained-neighbor-updates", "1000000", Int),
    ("rate-limit", "0", Int),
    ("command-spam-threshold-seconds", "10", Int),
    ("chat-spam-threshold-seconds", "10", Int),
    ("view-distance", "10", Int),
    ("simulation-distance", "10", Int),
    ("max-players", "20", Int),
    ("network-compression-threshold", "256", Int),
    ("broadcast-rcon-to-ops", "true", Boolean),
    ("broadcast-console-to-ops", "true", Boolean),
    ("max-world-size", "29999984", WorldSize),
    ("sync-chunk-writes", "true", Boolean),
    ("region-file-compression", "deflate", Text),
    ("enable-jmx-monitoring", "false", Boolean),
    ("enable-status", "true", Boolean),
    ("hide-online-players", "false", Boolean),
    ("entity-broadcast-range-percentage", "100", BroadcastRange),
    ("text-filtering-config", "", Text),
    ("text-filtering-version", "0", Int),
    ("player-idle-timeout", "0", Int),
    ("status-heartbeat-interval", "0", Int),
    ("white-list", "true", Boolean),
    ("enforce-secure-profile", "true", Boolean),
    ("log-ips", "true", Boolean),
    ("pause-when-empty-seconds", "60", Int),
    ("accepts-transfers", "false", Boolean),
    ("level-seed", "", Text),
    ("generate-structures", "true", Boolean),
    ("generator-settings", "{}", Text),
    ("level-type", "minecraft:normal", Text),
    ("resource-pack-id", "", Text),
    ("resource-pack", "", Text),
    ("resource-pack-sha1", "", Text),
    ("require-resource-pack", "false", Boolean),
    ("resource-pack-prompt", "", Text),
    ("initial-enabled-packs", "vanilla", Text),
    ("initial-disabled-packs", "", Text),
];

impl Default for ServerProperties {
    fn default() -> Self {
        Self::from_values(BTreeMap::new())
    }
}
impl ServerProperties {
    pub fn load(path: &Path) -> Result<Self> {
        let values = match fs::read(path) {
            Ok(bytes) => parse(
                &std::string::String::from_utf8(bytes.clone())
                    .unwrap_or_else(|_| bytes.into_iter().map(char::from).collect()),
            )?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e).context("reading server.properties"),
        };
        let properties = Self::from_values(values);
        let _: serde_json::Map<std::string::String, serde_json::Value> =
            serde_json::from_str(properties.get("generator-settings"))
                .context("invalid generator-settings JSON object")?;
        properties.store(path)?;
        Ok(properties)
    }
    pub fn from_values(mut values: BTreeMap<std::string::String, std::string::String>) -> Self {
        for &(key, default, kind) in KEYS {
            let input = values.get(key).map_or(default, |v| v.as_str());
            let value = match kind {
                Text => input.to_owned(),
                Boolean => input.eq_ignore_ascii_case("true").to_string(),
                Int => input
                    .parse::<i32>()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|_| default.into()),
                Long => input
                    .parse::<i64>()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|_| default.into()),
                Difficulty | GameMode => {
                    let names = if matches!(kind, Difficulty) {
                        ["peaceful", "easy", "normal", "hard"]
                    } else {
                        ["survival", "creative", "adventure", "spectator"]
                    };
                    input
                        .parse::<i32>()
                        .ok()
                        .map(|n| {
                            names[if matches!(kind, Difficulty) {
                                n.rem_euclid(4) as usize
                            } else {
                                usize::try_from(n).ok().filter(|&n| n < 4).unwrap_or(0)
                            }]
                        })
                        .or_else(|| names.iter().copied().find(|&n| n == input))
                        .unwrap_or(default)
                        .to_owned()
                }
                WorldSize => input
                    .parse::<i32>()
                    .map(|v| v.clamp(1, 29_999_984).to_string())
                    .unwrap_or_else(|_| default.into()),
                BroadcastRange => input
                    .parse::<i32>()
                    .map(|v| v.clamp(10, 1000).to_string())
                    .unwrap_or_else(|_| default.into()),
                Permission => input
                    .parse::<i32>()
                    .map(|v| v.clamp(0, 4).to_string())
                    .unwrap_or_else(|_| default.into()),
            };
            values.insert(key.into(), value);
        }
        values
            .entry("level-type".into())
            .and_modify(|value| *value = value.to_lowercase());
        if values["generator-settings"].is_empty() {
            values.insert("generator-settings".into(), "{}".into());
        }
        values.remove("announce-player-achievements");
        if let Some(hash) = values.remove("resource-pack-hash") {
            if values["resource-pack-sha1"].is_empty() {
                values.insert("resource-pack-sha1".into(), hash);
            }
        }
        if values["management-server-secret"].is_empty() {
            use rand::Rng;
            let secret: std::string::String = rand::thread_rng()
                .sample_iter(&rand::distributions::Alphanumeric)
                .take(40)
                .map(char::from)
                .collect();
            values.insert("management-server-secret".into(), secret);
        }
        Self { values }
    }
    pub fn store(&self, path: &Path) -> Result<()> {
        let mut text = std::string::String::from("#Minecraft server properties\n");
        for (key, value) in &self.values {
            text.push_str(&format!("{}={}\n", escape(key), escape(value)));
        }
        fs::write(path, text)?;
        Ok(())
    }
    pub fn get(&self, key: &str) -> &str {
        self.values.get(key).map_or("", |v| v.as_str())
    }
    pub fn boolean(&self, key: &str) -> bool {
        self.get(key) == "true"
    }
    pub fn int(&self, key: &str) -> i32 {
        self.get(key).parse().expect("normalized integer property")
    }
    pub fn long(&self, key: &str) -> i64 {
        self.get(key).parse().expect("normalized long property")
    }
    pub fn set(&mut self, key: &str, value: impl ToString) {
        self.values.insert(key.into(), value.to_string());
        *self = Self::from_values(std::mem::take(&mut self.values));
    }
    pub fn seed(&self) -> i64 {
        let seed = self.get("level-seed").trim();
        if seed.is_empty() {
            rand::random()
        } else {
            seed.parse().unwrap_or_else(|_| {
                i64::from(
                    seed.encode_utf16()
                        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c))),
                )
            })
        }
    }
}

fn escape(value: &str) -> std::string::String {
    value
        .chars()
        .enumerate()
        .flat_map(|(i, c)| match c {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            ' ' | '=' | ':' | '#' | '!' if i == 0 || c != ' ' => vec!['\\', c],
            _ => vec![c],
        })
        .collect()
}
fn unescape(value: &str) -> Result<std::string::String> {
    let mut chars = value.chars();
    let mut units = Vec::new();
    while let Some(c) = chars.next() {
        let c = if c == '\\' {
            match chars.next() {
                Some('u') => {
                    let digits: std::string::String = chars.by_ref().take(4).collect();
                    units.push(
                        u16::from_str_radix(&digits, 16)
                            .context("malformed property unicode escape")?,
                    );
                    continue;
                }
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                Some('f') => '\u{c}',
                Some(c) => c,
                None => continue,
            }
        } else {
            c
        };
        units.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
    }
    Ok(std::string::String::from_utf16(&units)?)
}
/// Port of Properties#load: continuation lines, escaped separators and Unicode.
fn parse(text: &str) -> Result<BTreeMap<std::string::String, std::string::String>> {
    let mut result = BTreeMap::new();
    let mut logical = std::string::String::new();
    for line in text.lines() {
        let line = line.trim_start_matches([' ', '\t', '\u{c}']);
        if logical.is_empty() && (line.is_empty() || line.starts_with(['#', '!'])) {
            continue;
        }
        logical.push_str(line);
        let slashes = logical.chars().rev().take_while(|&c| c == '\\').count();
        if slashes % 2 == 1 {
            logical.pop();
            continue;
        }
        let mut escaped = false;
        let separator = logical.char_indices().find_map(|(i, c)| {
            if escaped {
                escaped = false;
                None
            } else if c == '\\' {
                escaped = true;
                None
            } else if matches!(c, '=' | ':' | ' ' | '\t' | '\u{c}') {
                Some(i)
            } else {
                None
            }
        });
        let (key, value) = separator.map_or((logical.as_str(), ""), |i| {
            let rest = logical[i..].trim_start_matches([' ', '\t', '\u{c}']);
            let rest = rest
                .strip_prefix(['=', ':'])
                .unwrap_or(rest)
                .trim_start_matches([' ', '\t', '\u{c}']);
            (&logical[..i], rest)
        });
        result.insert(unescape(key)?, unescape(value)?);
        logical.clear();
    }
    if !logical.is_empty() {
        result.insert(unescape(&logical)?, std::string::String::new());
    }
    Ok(result)
}

pub fn check_eula(root: &Path) -> Result<()> {
    let path = root.join("eula.txt");
    if path.exists()
        && parse(&fs::read_to_string(&path)?)?
            .get("eula")
            .is_some_and(|s| s.eq_ignore_ascii_case("true"))
    {
        return Ok(());
    }
    fs::write(
        path,
        "#You must agree to the Minecraft EULA (https://aka.ms/MinecraftEULA).\neula=false\n",
    )?;
    bail!("You need to agree to the EULA in eula.txt in order to run the server.")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_properties_and_validation() {
        let values = parse("motd : hello\\nworld\\u0021\nview-distance=broken\nwhite-list=FALSE\nmax-world-size=0\nlevel-seed=a\\\n b\n").unwrap();
        let p = ServerProperties::from_values(values);
        assert_eq!(p.get("motd"), "hello\nworld!");
        assert_eq!(p.int("view-distance"), 10);
        assert!(!p.boolean("white-list"));
        assert_eq!(p.int("max-world-size"), 1);
        assert_eq!(p.get("level-seed"), "ab");
        assert_eq!(
            parse(&format!("motd={}\n", escape(p.get("motd")))).unwrap()["motd"],
            p.get("motd")
        );
        assert!(ServerProperties::default().boolean("white-list"));
    }
}
