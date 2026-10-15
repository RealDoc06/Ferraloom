use std::{fs, io::ErrorKind, num::NonZero, path::Path};

use anyhow::{Context, Result, ensure};
use pumpkin_config::PumpkinConfig;
use toml::Value;

use crate::Args;

/// Spellings upstream accepts in place of a field name, as `(table, alias, field)`.
/// Mirrors the `#[serde(alias)]` attributes of the pinned `pumpkin-config`; any
/// other key the typed config does not keep is rejected as unknown.
#[rustfmt::skip]
const ALIASES: &[(&str, &str, &str)] = &[
    ("", "accepts-transfers", "accepts_transfers"),
    ("chat.anti_spam", "chat-spam-threshold-seconds", "chat_spam_threshold_seconds"),
    ("chat.anti_spam", "command-spam-threshold-seconds", "command_spam_threshold_seconds"),
    ("networking.java", "keep-alive-time", "keep_alive_time"),
    ("networking.java", "keep_alive_interval", "keep_alive_time"),
    ("networking.java", "keep-alive-interval", "keep_alive_time"),
    ("networking.java.authentication", "fallback_urls", "fallbacks"),
    ("networking.java.authentication", "profile_by_name_fallback_urls", "profile_by_name_fallbacks"),
    ("networking.java.authentication", "profile_by_uuid_fallback_urls", "profile_by_uuid_fallbacks"),
    ("networking.java.packet_limiter", "max-packet-rate", "max_packet_rate"),
    ("networking.java.packet_limiter", "burst-capacity", "burst_capacity"),
    ("networking.java.packet_limiter", "kick-message", "kick_message"),
    ("networking.bedrock.packet_limiter", "max-packet-rate", "max_packet_rate"),
    ("networking.bedrock.packet_limiter", "burst-capacity", "burst_capacity"),
    ("networking.bedrock.packet_limiter", "kick-message", "kick_message"),
];

/// Load `<data_dir>/pumpkin.toml` strictly: malformed files and unknown keys never
/// start a server with defaults. A missing file is created with Ferraloom defaults.
pub fn load(data_dir: &Path) -> Result<PumpkinConfig> {
    let path = data_dir.join("pumpkin.toml");
    match fs::read_to_string(&path) {
        Ok(source) => {
            parse(&source).with_context(|| format!("Invalid configuration in {}", path.display()))
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            let mut config = PumpkinConfig::default();
            config.advanced.networking.java.motd = "Ferraloom".into();
            config.advanced.networking.java.max_players = 20;
            config.advanced.networking.java.view_distance =
                NonZero::new(10).context("Invalid default view distance")?;
            config.advanced.networking.bedrock.enabled = false;
            config.advanced.plugins.enabled = false;
            config.telemetry.enabled = false;
            fs::write(&path, toml::to_string_pretty(&config)?)
                .with_context(|| format!("Cannot write {}", path.display()))?;
            Ok(config)
        }
        Err(error) => Err(error).with_context(|| format!("Cannot read {}", path.display())),
    }
}

fn parse(source: &str) -> Result<PumpkinConfig> {
    let document: Value = toml::from_str(source).context("Invalid TOML")?;
    let mut config: PumpkinConfig = document.clone().try_into()?;
    // Upstream structs use `#[serde(default)]` and `flatten`, which silently drop
    // unknown keys. Serializing the parsed config back keeps every field and map
    // entry it understood, so anything missing there was ignored.
    let understood = Value::try_from(&config).context("Cannot serialize configuration")?;
    let mut unknown = Vec::new();
    collect_unknown_keys(&document, &understood, "", &mut unknown);
    ensure!(unknown.is_empty(), "Unknown keys: {}", unknown.join(", "));

    let has_motd = document
        .get("networking")
        .and_then(|value| value.get("java"))
        .and_then(|value| value.get("motd"))
        .is_some();
    if !has_motd {
        config.advanced.networking.java.motd = "Ferraloom".into();
    }
    Ok(config)
}

/// Push the dotted path of every key in `input` that has no counterpart in `understood`.
fn collect_unknown_keys(input: &Value, understood: &Value, path: &str, unknown: &mut Vec<String>) {
    match (input, understood) {
        (Value::Table(input), Value::Table(understood)) => {
            for (key, value) in input {
                let field = ALIASES
                    .iter()
                    .find(|&&(table, alias, _)| table == path && alias == key.as_str())
                    .map_or(key.as_str(), |&(_, _, field)| field);
                let key_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match understood.get(field) {
                    Some(understood) => collect_unknown_keys(value, understood, &key_path, unknown),
                    None => unknown.push(key_path),
                }
            }
        }
        (Value::Array(input), Value::Array(understood)) => {
            for (index, (input, understood)) in input.iter().zip(understood).enumerate() {
                collect_unknown_keys(input, understood, &format!("{path}[{index}]"), unknown);
            }
        }
        _ => {}
    }
}

/// Apply command-line overrides and Ferraloom's vanilla-first runtime policy,
/// then reject settings the engine would otherwise only assert on at startup.
pub fn configure_runtime(config: &mut PumpkinConfig, args: &Args) -> Result<()> {
    config.advanced.networking.java.enabled = true;
    config.advanced.networking.bedrock.enabled = false;
    config.advanced.plugins.enabled = false;
    config.telemetry.enabled = false;
    let proxy = &config.advanced.networking.proxy;
    ensure!(
        !(proxy.enabled
            || proxy.velocity.enabled
            || proxy.bungeecord.enabled
            || proxy.vine.enabled),
        "Proxy forwarding is not supported: it trusts player identities from the proxy instead of \
         authenticating them. Set enabled = false in [networking.proxy] and its velocity, \
         bungeecord, and vine sections"
    );
    let java = &mut config.advanced.networking.java;
    if let Some(bind) = args.bind {
        java.address = bind;
    }
    if args.offline {
        java.online_mode = false;
        config.basic.allow_chat_reports = false;
    }
    ensure!(
        java.keep_alive_time > 0,
        "keep_alive_time must be greater than zero"
    );
    ensure!(
        (2..=64).contains(&java.view_distance.get()),
        "view_distance must be between 2 and 64"
    );
    ensure!(
        (2..=64).contains(&java.simulation_distance.get()),
        "simulation_distance must be between 2 and 64"
    );
    ensure!(
        !java.online_mode || java.encryption,
        "Online mode requires encryption"
    );
    ensure!(
        !config.basic.allow_chat_reports || java.online_mode,
        "Chat reports require online mode"
    );
    ensure!(
        config.basic.tps.is_finite() && config.basic.tps > 0.0,
        "tps must be a finite positive number"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use pumpkin_config::chunk::ChunkConfig;

    use super::*;

    fn unknown_keys(source: &str) -> String {
        format!(
            "{:#}",
            parse(source)
                .err()
                .expect("configuration should be rejected")
        )
    }

    #[test]
    fn config_errors_do_not_fall_back_or_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pumpkin.toml");
        for invalid in [
            "[networking.java\n",
            "[networking.java]\nadress = '0.0.0.0:25565'\n",
        ] {
            fs::write(&path, invalid).unwrap();
            assert!(load(directory.path()).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), invalid);
        }
    }

    #[test]
    fn rejects_unknown_keys_at_every_depth() {
        let error = unknown_keys(
            "whitelist = true\n\
             [networking.java]\nadress = '127.0.0.1:25565'\n\
             [networking.java.authentication.textures]\nenabeld = false\n\
             [commands.overrides.tp]\npermision_level = 2\n",
        );
        for key in [
            "whitelist",
            "networking.java.adress",
            "networking.java.authentication.textures.enabeld",
            "commands.overrides.tp.permision_level",
        ] {
            assert!(error.contains(key), "{key} missing from: {error}");
        }
        // Aliases are scoped to the table that declares them.
        assert!(unknown_keys("keep-alive-time = 5\n").contains("keep-alive-time"));
        // Variant fields only count for the variant that has them.
        assert!(
            unknown_keys("[world.chunk]\ntype = 'linear'\nwrite_in_place = true\n")
                .contains("world.chunk.write_in_place")
        );
    }

    #[test]
    fn accepts_documented_aliases_user_maps_and_enum_variants() {
        let config = parse(
            "accepts-transfers = false\nwhite_list = true\n\
             [networking.java]\nkeep-alive-interval = 30\n\
             [networking.java.packet_limiter]\nmax-packet-rate = 120.0\n\
             [networking.java.authentication]\nfallback_urls = ['https://auth.example']\n\
             [chat.anti_spam]\nchat-spam-threshold-seconds = 3\n\
             [commands.overrides.gamemode]\npermission_level = 4\n\
             [commands.overrides.tp]\nenabled = false\n\
             [server_links.custom]\n\
             [world.chunk]\ntype = 'anvil'\nwrite_in_place = true\n\
             [world.chunk.compression]\nalgorithm = 'ZLib'\nlevel = 6\n",
        )
        .unwrap();
        assert!(!config.basic.accepts_transfers);
        assert!(config.basic.white_list);
        let java = &config.advanced.networking.java;
        assert_eq!(java.keep_alive_time, 30);
        assert_eq!(java.packet_limiter.max_packet_rate, 120.0);
        assert_eq!(java.authentication.fallbacks, ["https://auth.example"]);
        assert_eq!(
            config.advanced.chat.anti_spam.chat_spam_threshold_seconds,
            Some(3)
        );
        assert_eq!(config.advanced.commands.overrides.len(), 2);
        assert!(matches!(
            config.advanced.world.chunk,
            ChunkConfig::Anvil(ref anvil) if anvil.write_in_place
        ));
        assert!(parse("[world.chunk]\ntype = 'linear'\n").is_ok());
    }

    #[test]
    fn generated_default_config_reloads_strictly() {
        let directory = tempfile::tempdir().unwrap();
        load(directory.path()).unwrap();
        let config = load(directory.path()).unwrap();
        assert_eq!(config.advanced.networking.java.motd, "Ferraloom");
        assert!(!config.advanced.networking.proxy.enabled);
    }

    #[test]
    fn local_overrides_preserve_configured_motd_and_do_not_persist_offline() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pumpkin.toml");
        let original = "[networking.java]\nmotd = 'Doc server'\n[telemetry]\nenabled = true\n[plugins]\nenabled = true\n[networking.bedrock]\nenabled = true\n";
        fs::write(&path, original).unwrap();
        let mut config = load(directory.path()).unwrap();
        let args =
            Args::try_parse_from(["ferraloom", "--offline", "--bind", "127.0.0.1:0"]).unwrap();
        configure_runtime(&mut config, &args).unwrap();
        assert_eq!(config.advanced.networking.java.motd, "Doc server");
        assert!(!config.advanced.networking.java.online_mode);
        assert!(!config.advanced.networking.bedrock.enabled);
        assert!(!config.advanced.plugins.enabled);
        assert!(!config.telemetry.enabled);
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn shipped_example_is_valid_and_online() {
        let mut config = parse(include_str!("../examples/pumpkin.toml")).unwrap();
        let args = Args::try_parse_from(["ferraloom"]).unwrap();
        configure_runtime(&mut config, &args).unwrap();
        assert!(config.advanced.networking.java.online_mode);
    }

    #[test]
    fn rejects_proxy_forwarding_even_in_offline_mode() {
        let args = Args::try_parse_from(["ferraloom", "--offline"]).unwrap();
        for section in ["proxy", "proxy.velocity", "proxy.bungeecord", "proxy.vine"] {
            let mut config = parse(&format!("[networking.{section}]\nenabled = true\n")).unwrap();
            let error = configure_runtime(&mut config, &args).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("Proxy forwarding is not supported")
            );
        }
    }

    #[test]
    fn invalid_tick_rate_and_online_encryption_fail_before_startup() {
        let mut config = PumpkinConfig::default();
        let args = Args::try_parse_from(["ferraloom"]).unwrap();
        config.basic.tps = f32::NAN;
        assert!(configure_runtime(&mut config, &args).is_err());
        config.basic.tps = 20.0;
        config.advanced.networking.java.encryption = false;
        assert!(configure_runtime(&mut config, &args).is_err());
    }
}
