//! Port of ProfilePublicKey, SignedMessageChain and LastSeenMessagesValidator.
use anyhow::{Context, Result, ensure};
use base64::Engine;
use ferraloom_protocol::{
    Uuid,
    play::{
        LastSeenMessagesPacked, LastSeenMessagesUpdate, MessageSignature, PackedMessageSignature,
        RemoteChatSessionData, SignedMessageBodyPacked,
    },
};
use rsa::{Pkcs1v15Sign, RsaPublicKey, pkcs8::DecodePublicKey, sha2::Sha256};
use serde::Deserialize;
use sha1::{Digest, Sha1};
use std::time::Duration;

#[derive(Default)]
pub struct ServicesKeySet {
    keys: Vec<RsaPublicKey>,
}
impl ServicesKeySet {
    /// Authlib MinecraftServicesKeyInfo uses SHA1withRSA for player certificates.
    pub async fn fetch() -> Result<Self> {
        #[derive(Deserialize)]
        struct Key {
            #[serde(rename = "publicKey")]
            public_key: String,
        }
        #[derive(Deserialize)]
        struct Response {
            #[serde(rename = "playerCertificateKeys")]
            keys: Vec<Key>,
        }
        let response: Response = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(5))
            .build()?
            .get("https://api.minecraftservices.com/publickeys")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let keys = response
            .keys
            .into_iter()
            .map(|key| {
                Ok(RsaPublicKey::from_public_key_der(
                    &base64::engine::general_purpose::STANDARD.decode(key.public_key)?,
                )?)
            })
            .collect::<Result<_>>()?;
        Ok(Self { keys })
    }
    pub fn can_validate(&self) -> bool {
        !self.keys.is_empty()
    }
    pub fn validate(&self, uuid: Uuid, data: &RemoteChatSessionData) -> Result<RsaPublicKey> {
        let certificate = &data.profile_public_key;
        let mut payload = uuid.to_byte_array().to_vec();
        payload.extend_from_slice(&certificate.expires_at.to_be_bytes());
        payload.extend_from_slice(&certificate.key.0);
        let digest = Sha1::digest(payload);
        ensure!(
            self.keys.iter().any(|key| key
                .verify(
                    Pkcs1v15Sign::new::<Sha1>(),
                    &digest,
                    &certificate.key_signature.0
                )
                .is_ok()),
            "multiplayer.disconnect.invalid_public_key_signature"
        );
        Ok(RsaPublicKey::from_public_key_der(&certificate.key.0)?)
    }
}
#[derive(Clone)]
struct Tracked {
    signature: MessageSignature,
    pending: bool,
}
pub struct LastSeenMessagesValidator {
    tracked: Vec<Option<Tracked>>,
    last_pending: Option<MessageSignature>,
}
impl Default for LastSeenMessagesValidator {
    fn default() -> Self {
        Self {
            tracked: vec![None; 20],
            last_pending: None,
        }
    }
}
impl LastSeenMessagesValidator {
    pub fn add_pending(&mut self, signature: &MessageSignature) -> Result<()> {
        if self.last_pending.as_ref() != Some(signature) {
            self.tracked.push(Some(Tracked {
                signature: signature.clone(),
                pending: true,
            }));
            self.last_pending = Some(signature.clone());
        }
        ensure!(
            self.tracked.len() <= 4096,
            "multiplayer.disconnect.too_many_pending_chats"
        );
        Ok(())
    }
    pub fn offset(&mut self, offset: i32) -> Result<()> {
        let offset = usize::try_from(offset).context("negative chat acknowledgement")?;
        ensure!(
            offset <= self.tracked.len() - 20,
            "invalid chat acknowledgement offset"
        );
        self.tracked.drain(..offset);
        Ok(())
    }
    pub fn apply(&mut self, update: &LastSeenMessagesUpdate) -> Result<Vec<MessageSignature>> {
        self.offset(update.offset.0)?;
        ensure!(
            update.acknowledged.bit_length() <= 20,
            "invalid chat acknowledgement window"
        );
        let mut entries = Vec::new();
        for index in 0..20 {
            if update.acknowledged.get(index) {
                let entry = self.tracked[index]
                    .as_mut()
                    .context("acknowledged unknown chat message")?;
                entry.pending = false;
                entries.push(entry.signature.clone());
            } else {
                ensure!(
                    self.tracked[index]
                        .as_ref()
                        .is_none_or(|entry| entry.pending),
                    "ignored acknowledged chat message"
                );
                self.tracked[index] = None;
            }
        }
        let checksum = entries.iter().fold(1i32, |checksum, signature| {
            checksum
                .wrapping_mul(31)
                .wrapping_add(signature.0.iter().fold(1i32, |hash, &byte| {
                    hash.wrapping_mul(31).wrapping_add(i32::from(byte as i8))
                }))
        }) as i8;
        let checksum = if checksum == 0 { 1 } else { checksum };
        ensure!(
            update.checksum == 0 || update.checksum == checksum,
            "chat acknowledgement checksum mismatch"
        );
        Ok(entries)
    }
}
pub struct ChatSession {
    pub data: RemoteChatSessionData,
    key: RsaPublicKey,
    index: Option<i32>,
    last_timestamp: i64,
}
impl ChatSession {
    pub fn new(uuid: Uuid, data: RemoteChatSessionData, services: &ServicesKeySet) -> Result<Self> {
        let key = services.validate(uuid, &data)?;
        Ok(Self {
            data,
            key,
            index: Some(0),
            last_timestamp: 0,
        })
    }
    pub fn unpack(
        &mut self,
        uuid: Uuid,
        content: String,
        timestamp: i64,
        salt: i64,
        signature: Option<MessageSignature>,
        last_seen: Vec<MessageSignature>,
    ) -> Result<ValidatedMessage> {
        let signature = signature.context("chat.disabled.missingProfileKey")?;
        ensure!(
            self.data.profile_public_key.expires_at >= now_millis(),
            "chat.disabled.expiredProfileKey"
        );
        let index = self.index.context("chat.disabled.chain_broken")?;
        if timestamp < self.last_timestamp {
            self.index = None;
            anyhow::bail!("chat.disabled.out_of_order_chat");
        }
        self.last_timestamp = timestamp;
        let payload = signature_payload(
            uuid,
            self.data.session_id,
            index,
            &content,
            timestamp,
            salt,
            &last_seen,
        );
        if self
            .key
            .verify(
                Pkcs1v15Sign::new::<Sha256>(),
                &Sha256::digest(payload),
                &signature.0,
            )
            .is_err()
        {
            self.index = None;
            anyhow::bail!("chat.disabled.invalid_signature");
        }
        self.index = index.checked_add(1);
        Ok(ValidatedMessage {
            sender: uuid,
            index,
            signature: Some(signature),
            content,
            timestamp,
            salt,
            last_seen,
        })
    }
}
pub struct ValidatedMessage {
    pub sender: Uuid,
    pub index: i32,
    pub signature: Option<MessageSignature>,
    pub content: String,
    pub timestamp: i64,
    pub salt: i64,
    pub last_seen: Vec<MessageSignature>,
}
impl ValidatedMessage {
    pub fn unsigned(sender: Uuid, content: String) -> Self {
        Self {
            sender,
            index: 0,
            signature: None,
            content,
            timestamp: now_millis(),
            salt: 0,
            last_seen: Vec::new(),
        }
    }
}
#[derive(Default)]
pub struct MessageSignatureCache {
    entries: Vec<MessageSignature>,
}
impl MessageSignatureCache {
    /// Each recipient packs against its own pre-send cache, then pushes this body.
    pub fn pack(&mut self, message: &ValidatedMessage) -> SignedMessageBodyPacked {
        let packed = message
            .last_seen
            .iter()
            .map(|signature| {
                self.entries
                    .iter()
                    .position(|old| old == signature)
                    .map_or_else(
                        || PackedMessageSignature::Full(Box::new(signature.clone())),
                        |id| PackedMessageSignature::Cached(id as i32),
                    )
            })
            .collect();
        let mut incoming = message.last_seen.clone();
        if let Some(signature) = &message.signature {
            incoming.push(signature.clone());
        }
        if !incoming.is_empty() {
            let fresh = incoming.clone();
            for old in &self.entries {
                if !fresh.contains(old) {
                    incoming.insert(0, old.clone());
                }
            }
            incoming.reverse();
            incoming.truncate(128);
            self.entries = incoming;
        }
        SignedMessageBodyPacked {
            content: ferraloom_protocol::common::Utf8(message.content.clone()),
            timestamp: message.timestamp,
            salt: message.salt,
            last_seen: LastSeenMessagesPacked(packed),
        }
    }
}
fn signature_payload(
    sender: Uuid,
    session: Uuid,
    index: i32,
    content: &str,
    timestamp: i64,
    salt: i64,
    last_seen: &[MessageSignature],
) -> Vec<u8> {
    let mut payload = 1i32.to_be_bytes().to_vec();
    payload.extend_from_slice(&sender.to_byte_array());
    payload.extend_from_slice(&session.to_byte_array());
    payload.extend_from_slice(&index.to_be_bytes());
    payload.extend_from_slice(&salt.to_be_bytes());
    payload.extend_from_slice(&timestamp.div_euclid(1000).to_be_bytes());
    payload.extend_from_slice(&(content.len() as i32).to_be_bytes());
    payload.extend_from_slice(content.as_bytes());
    payload.extend_from_slice(&(last_seen.len() as i32).to_be_bytes());
    for signature in last_seen {
        payload.extend_from_slice(&signature.0);
    }
    payload
}
fn now_millis() -> i64 {
    time::OffsetDateTime::now_utc()
        .unix_timestamp_nanos()
        .div_euclid(1_000_000) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferraloom_protocol::{
        codec::{BitSet, VarInt},
        common::ByteArray,
        play::ProfilePublicKeyData,
    };
    use rsa::{RsaPrivateKey, pkcs8::EncodePublicKey};
    #[test]
    fn signed_chain_rejects_tampering_and_acknowledgement_retraction() {
        let private = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap();
        let public = RsaPublicKey::from(&private);
        let uuid = Uuid::offline_player("Signer");
        let session_id = Uuid::new(7, 8);
        let expires = now_millis() + 60000;
        let der = public.to_public_key_der().unwrap().as_bytes().to_vec();
        let mut certificate = uuid.to_byte_array().to_vec();
        certificate.extend_from_slice(&expires.to_be_bytes());
        certificate.extend_from_slice(&der);
        let data = RemoteChatSessionData {
            session_id,
            profile_public_key: ProfilePublicKeyData {
                expires_at: expires,
                key: ByteArray(der),
                key_signature: ByteArray(
                    private
                        .sign(Pkcs1v15Sign::new::<Sha1>(), &Sha1::digest(certificate))
                        .unwrap(),
                ),
            },
        };
        let services = ServicesKeySet { keys: vec![public] };
        let mut chain = ChatSession::new(uuid, data, &services).unwrap();
        let timestamp = now_millis();
        let payload = signature_payload(uuid, session_id, 0, "hello", timestamp, 9, &[]);
        let signature = MessageSignature(
            private
                .sign(Pkcs1v15Sign::new::<Sha256>(), &Sha256::digest(payload))
                .unwrap()
                .try_into()
                .unwrap(),
        );
        let validated = chain
            .unpack(
                uuid,
                "hello".into(),
                timestamp,
                9,
                Some(signature.clone()),
                Vec::new(),
            )
            .unwrap();
        assert_eq!(validated.index, 0);
        assert!(
            chain
                .unpack(
                    uuid,
                    "tampered".into(),
                    timestamp,
                    9,
                    Some(signature.clone()),
                    Vec::new()
                )
                .is_err()
        );
        assert!(chain.index.is_none());
        let mut validator = LastSeenMessagesValidator::default();
        validator.add_pending(&signature).unwrap();
        let update = LastSeenMessagesUpdate {
            offset: VarInt(1),
            acknowledged: BitSet::from_words(vec![1 << 19]),
            checksum: 0,
        };
        assert_eq!(validator.apply(&update).unwrap(), vec![signature]);
        assert!(
            validator
                .apply(&LastSeenMessagesUpdate {
                    offset: VarInt(0),
                    acknowledged: BitSet::default(),
                    checksum: 0
                })
                .is_err()
        );
    }
}
