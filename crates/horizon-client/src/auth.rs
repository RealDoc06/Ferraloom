//! The Microsoft account the launcher signed in, handed over in a session
//! file (docs/microsoft-account.md), and the Minecraft services call
//! online-mode servers need: authlib's MinecraftServicesSessionService.joinServer
//! through the services discovery document. The access token stays in this
//! process's memory: it is never logged, printed or written anywhere.
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// MinecraftServicesEnvironment.PROD.
pub const DEFAULT_DISCOVERY: &str = "https://discovery.minecraftservices.com/minecraft/client";

/// A UUID with or without dashes.
pub fn parse_uuid(text: &str) -> Option<u128> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    (hex.len() == 32)
        .then(|| u128::from_str_radix(&hex, 16).ok())
        .flatten()
}

/// Why joining a server through the session service failed, like the
/// AuthenticationException subclasses authlib throws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JoinError {
    /// No answer, or a 5xx (AuthenticationUnavailableException).
    Unavailable,
    /// 401 or ForbiddenOperationException (InvalidCredentialsException).
    InvalidSession,
    /// InsufficientPrivilegesException: multiplayer is off for the account.
    InsufficientPrivileges,
    /// multiplayer.access.banned or FORCED_USERNAME_CHANGE.
    Banned,
    /// Anything else, with the service's message.
    Other(String),
}
impl JoinError {
    /// MinecraftClientHttpException.toAuthenticationException.
    pub fn from_response(status: u16, body: &str) -> Self {
        let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
        let error = v.get("error").and_then(|e| e.as_str()).unwrap_or("");
        match error {
            "ForbiddenOperationException" => return Self::InvalidSession,
            "multiplayer.access.banned" | "FORCED_USERNAME_CHANGE" => return Self::Banned,
            "InsufficientPrivilegesException" => return Self::InsufficientPrivileges,
            _ => {}
        }
        match status {
            401 => Self::InvalidSession,
            500.. => Self::Unavailable,
            _ => Self::Other(
                v.get("errorMessage")
                    .and_then(|m| m.as_str())
                    .map_or_else(|| format!("Status: {status}"), str::to_owned),
            ),
        }
    }
}

/// The services discovery document URL: `HORIZON_API_DISCOVERY` overrides
/// it like authlib's `-Dminecraft.api.discovery.host`.
pub fn discovery_url() -> String {
    std::env::var("HORIZON_API_DISCOVERY")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| DEFAULT_DISCOVERY.to_owned())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent(&format!("Horizon/{}", horizon_core::release::VERSION))
        .build()
}

/// The discovery document, fetched once per game (a failure is retried next time).
pub(crate) fn discovery() -> Result<Arc<serde_json::Value>, JoinError> {
    static CACHE: Mutex<Option<Arc<serde_json::Value>>> = Mutex::new(None);
    if let Some(d) = CACHE.lock().unwrap().clone() {
        return Ok(d);
    }
    let doc: serde_json::Value = agent()
        .get(&discovery_url())
        .call()
        .ok()
        .and_then(|r| r.into_string().ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or(JoinError::Unavailable)?;
    let doc = Arc::new(doc);
    *CACHE.lock().unwrap() = Some(doc.clone());
    Ok(doc)
}

/// MinecraftServicesDiscoveryService.getUrl(service, endpoint).
pub fn endpoint(doc: &serde_json::Value, service: &str, name: &str) -> Option<String> {
    doc.pointer(&format!("/discovery/{service}/endpoints/{name}/uri"))
        .and_then(|u| u.as_str())
        .map(str::to_owned)
}

/// What the session service says about a player joining this server
/// (MinecraftServicesSessionService.hasJoinedServer).
#[derive(Clone, Debug, PartialEq)]
pub struct JoinedProfile {
    pub uuid: u128,
    pub properties: Vec<horizon_protocol::packets::login::Property>,
}

/// HttpDiscoveryService.buildQuery: `URLEncoder.encode(value, UTF-8)`.
fn url_encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'*' | b'_' => {
                out.push(char::from(b));
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The session service's verify endpoint, asked by an online-mode server
/// whether `name` joined with `server_hash` (and from `ip`, when the server
/// prevents proxy connections). `Ok(None)`: it didn't, or the service said
/// no; `Err(Unavailable)`: no answer.
pub fn has_joined(
    name: &str,
    server_hash: &str,
    ip: Option<std::net::IpAddr>,
) -> Result<Option<JoinedProfile>, JoinError> {
    let doc = discovery()?;
    let url = endpoint(&doc, "session", "verify").ok_or(JoinError::Unavailable)?;
    let mut query = format!(
        "username={}&serverId={}",
        url_encode(name),
        url_encode(server_hash)
    );
    if let Some(ip) = ip {
        query.push_str(&format!("&ip={}", url_encode(&ip.to_string())));
    }
    let separator = if url.contains('?') { '&' } else { '?' };
    let body = match agent().get(&format!("{url}{separator}{query}")).call() {
        Ok(response) => response.into_string().unwrap_or_default(),
        Err(ureq::Error::Status(status, response)) => {
            let error =
                JoinError::from_response(status, &response.into_string().unwrap_or_default());
            return match error {
                JoinError::Unavailable => Err(error),
                _ => Ok(None),
            };
        }
        Err(ureq::Error::Transport(_)) => return Err(JoinError::Unavailable),
    };
    Ok(parse_joined(&body))
}

/// HasJoinedMinecraftServerResponse: `id` and `properties`; no id is no profile.
pub fn parse_joined(body: &str) -> Option<JoinedProfile> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let uuid = parse_uuid(v.get("id")?.as_str()?)?;
    let properties = v
        .get("properties")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some(horizon_protocol::packets::login::Property {
                name: p.get("name")?.as_str()?.to_owned(),
                value: p.get("value")?.as_str()?.to_owned(),
                signature: p
                    .get("signature")
                    .and_then(|s| s.as_str())
                    .map(str::to_owned),
            })
        })
        .collect();
    Some(JoinedProfile { uuid, properties })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: u128 = 0x069a79f444e94726a5befca90e38aaf5;

    #[test]
    fn verify_answers() {
        assert_eq!(url_encode("Tester_1"), "Tester_1");
        assert_eq!(url_encode("-5a f"), "-5a+f");
        assert_eq!(url_encode("::1"), "%3A%3A1");
        let body = r#"{"id":"069a79f444e94726a5befca90e38aaf5","name":"Notch","properties":[{"name":"textures","value":"e30=","signature":"c2ln"}]}"#;
        let p = parse_joined(body).unwrap();
        assert_eq!(p.uuid, UUID);
        assert_eq!(p.properties.len(), 1);
        assert_eq!(p.properties[0].signature.as_deref(), Some("c2ln"));
        assert_eq!(parse_joined(""), None);
        assert_eq!(parse_joined(r#"{"name":"Notch"}"#), None);
    }

    #[test]
    fn discovery_endpoints() {
        let doc = serde_json::json!({"discovery": {"session": {"endpoints": {"join": {"uri": "https://s/join"}}}}});
        assert_eq!(
            endpoint(&doc, "session", "join").as_deref(),
            Some("https://s/join")
        );
        assert_eq!(endpoint(&doc, "session", "verify"), None);
    }
}
