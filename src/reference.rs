//! Protocol metadata generated from the pinned official reference, without
//! importing a server implementation. Dynamic IDs are assigned by our lists.

use std::{collections::BTreeMap, io, sync::OnceLock};

use serde_json::Value;

use crate::protocol::{put_string, put_var_int};

pub struct RegistryData {
    pub packets: Vec<Vec<u8>>,
    pub tags: Vec<u8>,
    pub ids: BTreeMap<String, BTreeMap<String, i32>>,
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid compiled reference metadata",
    )
}

fn string(value: &Value) -> io::Result<&str> {
    value.as_str().ok_or_else(invalid)
}
fn array(value: &Value) -> io::Result<&Vec<Value>> {
    value.as_array().ok_or_else(invalid)
}

impl RegistryData {
    fn load() -> io::Result<Self> {
        let source: Value =
            serde_json::from_str(include_str!("../reference/26.3/registries.json"))?;
        let mut packets = Vec::new();
        let mut ids = BTreeMap::new();
        for registry in array(&source["registries"])? {
            let name = string(&registry["id"])?;
            let entries = array(&registry["entries"])?;
            let mut payload = Vec::new();
            put_string(&mut payload, name)?;
            put_var_int(
                &mut payload,
                entries.len().try_into().map_err(|_| invalid())?,
            );
            let mut raw_ids = BTreeMap::new();
            for (index, entry) in entries.iter().enumerate() {
                let identifier = string(entry)?;
                put_string(&mut payload, identifier)?;
                payload.push(0); // The acknowledged core pack supplies this entry's NBT.
                if raw_ids
                    .insert(identifier.to_owned(), index as i32)
                    .is_some()
                {
                    return Err(invalid());
                }
            }
            if ids.insert(name.to_owned(), raw_ids).is_some() {
                return Err(invalid());
            }
            packets.push(payload);
        }
        let tag_registries = array(&source["tags"])?;
        let mut tags = Vec::new();
        put_var_int(
            &mut tags,
            tag_registries.len().try_into().map_err(|_| invalid())?,
        );
        for registry in tag_registries {
            put_string(&mut tags, string(&registry["registry"])?)?;
            let entries = array(&registry["tags"])?;
            put_var_int(&mut tags, entries.len().try_into().map_err(|_| invalid())?);
            for tag in entries {
                put_string(&mut tags, string(&tag["id"])?)?;
                let members = array(&tag["entries"])?;
                put_var_int(&mut tags, members.len().try_into().map_err(|_| invalid())?);
                for member in members {
                    let id = member
                        .as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .filter(|n| *n >= 0)
                        .ok_or_else(invalid)?;
                    put_var_int(&mut tags, id);
                }
            }
        }
        Ok(Self { packets, tags, ids })
    }

    pub fn id(&self, registry: &str, identifier: &str) -> io::Result<i32> {
        self.ids
            .get(registry)
            .and_then(|ids| ids.get(identifier))
            .copied()
            .ok_or_else(invalid)
    }
}

pub fn registries() -> io::Result<&'static RegistryData> {
    static DATA: OnceLock<Result<RegistryData, String>> = OnceLock::new();
    DATA.get_or_init(|| RegistryData::load().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|message| io::Error::new(io::ErrorKind::InvalidData, message.clone()))
}
