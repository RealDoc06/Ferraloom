//! Port of StatusProtocols and ServerStatus's lenient optional JSON fields.
use crate::chat::Component;
use crate::codec::{state_packets, wire_struct};
use crate::common::{clientbound as cb, serverbound as sb};
use crate::{Error, Reader, Result, Uuid, Wire, Writer};
use base64::{
    Engine, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use serde_json::{Map, Value};

wire_struct!(/// Port of ServerboundStatusRequestPacket.
StatusRequest {});
#[derive(Clone, Debug, PartialEq)]
pub struct StatusResponse {
    pub status: ServerStatus,
}
impl Wire for StatusResponse {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            status: ServerStatus::from_json(&r.string(32767)?)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(&self.status.to_json()?, 32767)
    }
}
/// Port of ServerStatus. Malformed optional fields fall back independently.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerStatus {
    pub description: Component,
    pub players: Option<Players>,
    pub version: Option<Version>,
    pub favicon: Option<Favicon>,
    pub enforces_secure_chat: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Players {
    pub max: i32,
    pub online: i32,
    pub sample: Vec<NameAndId>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct NameAndId {
    pub id: Uuid,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Version {
    pub name: String,
    pub protocol: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Favicon {
    pub icon_bytes: Vec<u8>,
}
fn base64() -> GeneralPurpose {
    GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    )
}
fn string(map: &Value, key: &str) -> Option<String> {
    map.get(key)?.as_str().map(str::to_owned)
}
fn integer(map: &Value, key: &str) -> Option<i32> {
    let value = map.get(key)?.as_number()?;
    // JsonOps' Number.intValue uses Java's truncating conversion.
    Some(
        value
            .as_i64()
            .map_or_else(|| value.as_f64().unwrap() as i32, |v| v as i32),
    )
}
impl ServerStatus {
    pub fn from_json(json: &str) -> Result<Self> {
        let value = crate::chat::json::parse(json)?;
        if !value.is_object() {
            return Err(Error::invalid("status", "Expected JSON object"));
        }
        let description = value
            .get("description")
            .and_then(|v| Component::from_json(&v.to_string()).ok())
            .unwrap_or_default();
        let players = value.get("players").and_then(|p| {
            Some(Players {
                max: integer(p, "max")?,
                online: integer(p, "online")?,
                sample: p
                    .get("sample")
                    .and_then(Value::as_array)
                    .and_then(|values| {
                        values
                            .iter()
                            .map(|v| {
                                Some(NameAndId {
                                    id: crate::codec::uuid_from_string(&string(v, "id")?).ok()?,
                                    name: string(v, "name")?,
                                })
                            })
                            .collect::<Option<Vec<_>>>()
                    })
                    .unwrap_or_default(),
            })
        });
        let version = value.get("version").and_then(|v| {
            Some(Version {
                name: string(v, "name")?,
                protocol: integer(v, "protocol")?,
            })
        });
        let favicon = value
            .get("favicon")
            .and_then(Value::as_str)
            .and_then(|text| {
                base64()
                    .decode(
                        text.strip_prefix("data:image/png;base64,")?
                            .replace('\n', ""),
                    )
                    .ok()
                    .map(|icon_bytes| Favicon { icon_bytes })
            });
        let enforces_secure_chat = value
            .get("enforcesSecureChat")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(Self {
            description,
            players,
            version,
            favicon,
            enforces_secure_chat,
        })
    }
    pub fn to_json(&self) -> Result<String> {
        let mut map = Map::new();
        if self.description != Component::default() {
            map.insert(
                "description".into(),
                serde_json::from_str(&self.description.to_json()?)
                    .map_err(|e| Error::invalid("status", e.to_string()))?,
            );
        }
        if let Some(players) = &self.players {
            let mut p = Map::new();
            p.insert("max".into(), players.max.into());
            p.insert("online".into(), players.online.into());
            if !players.sample.is_empty() {
                p.insert(
                    "sample".into(),
                    Value::Array(
                        players
                            .sample
                            .iter()
                            .map(|profile| {
                                let mut v = Map::new();
                                v.insert("id".into(), profile.id.to_string().into());
                                v.insert("name".into(), profile.name.clone().into());
                                Value::Object(v)
                            })
                            .collect(),
                    ),
                );
            }
            map.insert("players".into(), Value::Object(p));
        }
        if let Some(version) = &self.version {
            let mut v = Map::new();
            v.insert("name".into(), version.name.clone().into());
            v.insert("protocol".into(), version.protocol.into());
            map.insert("version".into(), Value::Object(v));
        }
        if let Some(favicon) = &self.favicon {
            map.insert(
                "favicon".into(),
                format!(
                    "data:image/png;base64,{}",
                    base64().encode(&favicon.icon_bytes)
                )
                .into(),
            );
        }
        if self.enforces_secure_chat {
            map.insert("enforcesSecureChat".into(), Value::Bool(true));
        }
        Ok(Value::Object(map).to_string())
    }
}
state_packets!(ServerboundPacket, ferraloom_data::packets::status::serverbound::Packet, {
    Request(StatusRequest) => StatusRequest, Ping(sb::PingRequest) => PingRequest,
});
state_packets!(ClientboundPacket, ferraloom_data::packets::status::clientbound::Packet, {
    Response(Box<StatusResponse>) => StatusResponse, Pong(cb::PongResponse) => PongResponse,
});
