//! Owned command sources. Canonical entities and servers are referenced by handles.
use super::permissions::PermissionSet;
use crate::entity::EntityRef;
use ferraloom_protocol::{Uuid, chat::Component, play::MessageSignature};
use ferraloom_util::{Vec3, mth};
use std::{fmt, sync::Arc};

/// Live capabilities available at the native callback fire point. Future result
/// stores extend this host rather than capturing or deferring mutable world data.
pub trait CommandCallbackHost {
    fn send_success(&mut self, source: &CommandSourceStack, message: Component, broadcast: bool);
    fn add_function_result(&mut self, collector: super::functions::CollectorId, value: i32);
}

/// Port of CommandResultCallback. Callbacks run inline, in composition order.
#[derive(Clone, Default)]
pub struct CommandResultCallback(
    Option<Arc<dyn Fn(&mut dyn CommandCallbackHost, bool, i32) + Send + Sync>>,
);
impl fmt::Debug for CommandResultCallback {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.0.is_some() {
            "<callback>"
        } else {
            "<empty>"
        })
    }
}
impl CommandResultCallback {
    pub fn new(
        callback: impl Fn(&mut dyn CommandCallbackHost, bool, i32) + Send + Sync + 'static,
    ) -> Self {
        Self(Some(Arc::new(callback)))
    }
    pub fn on_result(&self, host: &mut dyn CommandCallbackHost, success: bool, value: i32) {
        if let Some(callback) = &self.0 {
            callback(host, success, value);
        }
    }
    pub fn on_success(&self, host: &mut dyn CommandCallbackHost, value: i32) {
        self.on_result(host, true, value);
    }
    pub fn on_failure(&self, host: &mut dyn CommandCallbackHost) {
        self.on_result(host, false, 0);
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
    pub fn chain(self, other: Self) -> Self {
        if self.0.is_none() {
            return other;
        }
        if other.0.is_none() {
            return self;
        }
        Self::new(move |host, success, value| {
            self.on_result(host, success, value);
            other.on_result(host, success, value);
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityAnchor {
    Feet,
    Eyes,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rotation {
    pub pitch: f32,
    pub yaw: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CommandLevel {
    pub dimension: String,
    pub coordinate_scale: f64,
}
impl Default for CommandLevel {
    fn default() -> Self {
        Self {
            dimension: "minecraft:overworld".into(),
            coordinate_scale: 1.,
        }
    }
}
/// Identity of the output sink, distinct from the entity selected by `with_entity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandSource {
    Server,
    Player(EntityRef),
    Block(u64),
    Other(u64),
}
#[derive(Clone, Debug)]
pub enum NamesProvider {
    Entity,
    Constant(Box<Component>),
}
#[derive(Clone, Debug, Default)]
pub struct CommandSigningContext {
    pub arguments: Arc<[(String, SignedArgument)]>,
}
#[derive(Clone, Debug)]
pub struct SignedArgument {
    pub sender: Uuid,
    pub index: i32,
    pub signature: Option<MessageSignature>,
    pub content: String,
    pub timestamp: i64,
    pub salt: i64,
    pub last_seen: Arc<[MessageSignature]>,
}
/// Port of CommandSourceStack. Runtime references are identities, never borrowed world data.
#[derive(Clone, Debug)]
pub struct CommandSourceStack {
    pub source: CommandSource,
    pub position: Vec3,
    pub rotation: Rotation,
    pub level: CommandLevel,
    pub permissions: PermissionSet,
    pub names: NamesProvider,
    pub server: u64,
    pub silent: bool,
    pub entity: Option<EntityRef>,
    pub callback: CommandResultCallback,
    pub anchor: EntityAnchor,
    pub signing_context: CommandSigningContext,
    /// Handle of the runtime's ordered chat task chain. Copies preserve the chain.
    pub chat_message_chainer: u64,
    pub always_accepts: bool,
    pub accepts_success: bool,
    pub accepts_failure: bool,
    pub should_inform_admins: bool,
}
impl CommandSourceStack {
    pub fn server(permission: i32, position: Vec3) -> Self {
        Self {
            source: CommandSource::Server,
            position,
            rotation: Rotation::default(),
            level: CommandLevel::default(),
            permissions: PermissionSet::for_level(permission),
            names: NamesProvider::Constant(Box::new(Component::literal("Server"))),
            server: 0,
            silent: false,
            entity: None,
            callback: CommandResultCallback::default(),
            anchor: EntityAnchor::Feet,
            signing_context: CommandSigningContext::default(),
            chat_message_chainer: 0,
            always_accepts: false,
            accepts_success: true,
            accepts_failure: true,
            should_inform_admins: true,
        }
    }
    pub fn with_source(mut self, source: CommandSource) -> Self {
        self.source = source;
        self
    }
    /// Vanilla changes the entity and names, leaving position, rotation and level alone.
    pub fn with_entity(mut self, entity: EntityRef) -> Self {
        if self.entity != Some(entity) {
            self.entity = Some(entity);
            self.names = NamesProvider::Entity;
        }
        self
    }
    pub fn with_position(mut self, position: Vec3) -> Self {
        self.position = position;
        self
    }
    pub fn with_rotation(mut self, rotation: Rotation) -> Self {
        self.rotation = rotation;
        self
    }
    pub fn with_callback(mut self, callback: CommandResultCallback) -> Self {
        self.callback = callback;
        self
    }
    pub fn with_combined_callback(mut self, callback: CommandResultCallback) -> Self {
        self.callback = self.callback.chain(callback);
        self
    }
    /// Port of the callback-combiner overload of CommandSourceStack#withCallback.
    pub fn with_callback_combiner(
        self,
        callback: CommandResultCallback,
        combine: impl FnOnce(CommandResultCallback, CommandResultCallback) -> CommandResultCallback,
    ) -> Self {
        let combined = combine(self.callback.clone(), callback);
        self.with_callback(combined)
    }
    pub fn with_suppressed_output(mut self) -> Self {
        if !self.always_accepts {
            self.silent = true;
        }
        self
    }
    pub fn with_permission(mut self, permissions: PermissionSet) -> Self {
        self.permissions = permissions;
        self
    }
    pub fn with_maximum_permission(mut self, permissions: PermissionSet) -> Self {
        self.permissions = self.permissions.union(permissions);
        self
    }
    pub fn with_anchor(mut self, anchor: EntityAnchor) -> Self {
        self.anchor = anchor;
        self
    }
    pub fn with_level(mut self, level: CommandLevel) -> Self {
        if self.level.dimension != level.dimension {
            let scale = self.level.coordinate_scale / level.coordinate_scale;
            self.position.x *= scale;
            self.position.z *= scale;
            self.level = level;
        }
        self
    }
    pub fn with_signing_context(
        mut self,
        signing_context: CommandSigningContext,
        chat_message_chainer: u64,
    ) -> Self {
        self.signing_context = signing_context;
        self.chat_message_chainer = chat_message_chainer;
        self
    }
    pub fn anchor_position(&self, eye_height: Option<f64>) -> Vec3 {
        let mut position = self.position;
        if self.anchor == EntityAnchor::Eyes && self.entity.is_some() {
            position.y += eye_height.unwrap_or(0.);
        }
        position
    }
    pub fn facing(self, target: Vec3, eye_height: Option<f64>) -> Self {
        let from = self.anchor_position(eye_height);
        let xd = target.x - from.x;
        let yd = target.y - from.y;
        let zd = target.z - from.z;
        let horizontal = (xd * xd + zd * zd).sqrt();
        self.with_rotation(Rotation {
            pitch: mth::wrap_degrees_f32((-(mth::atan2(yd, horizontal) * 57.2957763671875)) as f32),
            yaw: mth::wrap_degrees_f32((mth::atan2(zd, xd) * 57.2957763671875) as f32 - 90.),
        })
    }
    pub fn display_name(&self, entity_name: impl FnOnce(EntityRef) -> Component) -> Component {
        match &self.names {
            NamesProvider::Constant(name) => name.as_ref().clone(),
            NamesProvider::Entity => self
                .entity
                .map(entity_name)
                .unwrap_or_else(|| Component::literal("")),
        }
    }
    /// Port of CommandSourceStack#getTextName, using the entity's plain name.
    pub fn text_name(&self, entity_name: impl FnOnce(EntityRef) -> String) -> String {
        match &self.names {
            NamesProvider::Constant(name) => super::exceptions::plain_text(name),
            NamesProvider::Entity => self.entity.map(entity_name).unwrap_or_default(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entity_source_and_dimension_changes_preserve_vanilla_fields() {
        let source = CommandSourceStack::server(4, Vec3::new(80., 64., -16.))
            .with_entity(EntityRef::fixture(crate::level::RegionId::FIXTURE, 7));
        assert_eq!(source.source, CommandSource::Server);
        let nether = source.with_level(CommandLevel {
            dimension: "minecraft:the_nether".into(),
            coordinate_scale: 8.,
        });
        assert_eq!(nether.position, Vec3::new(10., 64., -2.));
        assert_eq!(
            nether.entity,
            Some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 7))
        );
        assert_eq!(
            nether
                .with_entity(EntityRef::fixture(crate::level::RegionId::FIXTURE, 9))
                .position,
            Vec3::new(10., 64., -2.)
        );
    }
}

#[cfg(test)]
mod java_tests {
    use super::*;
    use std::sync::Mutex;
    struct Host;
    impl CommandCallbackHost for Host {
        fn add_function_result(&mut self, _: crate::commands::functions::CollectorId, _: i32) {
            panic!("this vector does not accumulate function results");
        }
        fn send_success(&mut self, _: &CommandSourceStack, _: Component, _: bool) {
            panic!("source transformation does not send feedback");
        }
    }

    #[test]
    fn owned_source_transformations_match_java() {
        let vectors: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("vanilla/source-vectors.json")).unwrap();
        for vector in vectors {
            let events = Arc::new(Mutex::new(Vec::new()));
            let callback = |name: &'static str| {
                let events = events.clone();
                CommandResultCallback::new(move |_host, success, value| {
                    events
                        .lock()
                        .unwrap()
                        .push(format!("{name}:{success}:{value}"));
                })
            };
            let base =
                CommandSourceStack::server(4, Vec3::new(80., 64., -16.)).with_rotation(Rotation {
                    pitch: 15.,
                    yaw: -37.,
                });
            let nether = CommandLevel {
                dimension: "minecraft:the_nether".into(),
                coordinate_scale: 8.,
            };
            let operation = vector["operation"].as_str().unwrap();
            let source = match operation {
                "base" => base.clone(),
                "source" => base.clone().with_source(CommandSource::Other(1)),
                "entity" => base
                    .clone()
                    .with_entity(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1)),
                "entity_again" => base
                    .clone()
                    .with_entity(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1))
                    .with_entity(EntityRef::fixture(crate::level::RegionId::FIXTURE, 2)),
                "position" => base.clone().with_position(Vec3::new(-2., 99., 3.)),
                "rotation" => base.clone().with_rotation(Rotation {
                    pitch: -91.,
                    yaw: 540.,
                }),
                "callback" => base.clone().with_callback(callback("first")),
                "combined" => base
                    .clone()
                    .with_callback(callback("first"))
                    .with_callback_combiner(callback("second"), CommandResultCallback::chain),
                "replaced" => base
                    .clone()
                    .with_callback(callback("first"))
                    .with_callback_combiner(callback("second"), |_, right| right),
                "suppressed" => base.clone().with_suppressed_output(),
                "always" => {
                    let mut source = base.clone().with_source(CommandSource::Other(2));
                    source.always_accepts = true;
                    source.with_suppressed_output()
                }
                "permission" => base.clone().with_permission(PermissionSet::for_level(1)),
                "maximum" => base
                    .clone()
                    .with_maximum_permission(PermissionSet::for_level(0)),
                "anchor" => base.clone().with_anchor(EntityAnchor::Eyes),
                "level" => base
                    .clone()
                    .with_entity(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1))
                    .with_level(nether),
                "level_roundtrip" => base
                    .clone()
                    .with_level(nether)
                    .with_level(CommandLevel::default()),
                "signing" => base
                    .clone()
                    .with_signing_context(CommandSigningContext::default(), 1),
                _ => {
                    let (mode, coordinates) = operation.split_once(':').unwrap();
                    let coordinates = coordinates
                        .split(',')
                        .map(|value| value.parse::<f64>().unwrap())
                        .collect::<Vec<_>>();
                    let source = if mode == "eyes_facing" {
                        base.clone()
                            .with_entity(EntityRef::fixture(crate::level::RegionId::FIXTURE, 1))
                            .with_anchor(EntityAnchor::Eyes)
                    } else {
                        base.clone()
                    };
                    source.facing(
                        Vec3::new(coordinates[0], coordinates[1], coordinates[2]),
                        Some(1.625),
                    )
                }
            };
            source.callback.on_success(&mut Host, 17);
            let name = source.display_name(|entity| {
                Component::literal(
                    if entity == EntityRef::fixture(crate::level::RegionId::FIXTURE, 1) {
                        "DisplayA"
                    } else {
                        "DisplayB"
                    },
                )
            });
            let observed = serde_json::json!({
                "operation":operation,
                "position":[source.position.x,source.position.y,source.position.z],
                "rotation":[source.rotation.pitch.to_bits() as i32,source.rotation.yaw.to_bits() as i32],
                "source":match source.source { CommandSource::Server => "normal",CommandSource::Other(2) => "always",_=>"other" },
                "entity":match source.entity {Some(entity) if entity == EntityRef::fixture(crate::level::RegionId::FIXTURE, 1)=>"a",Some(entity) if entity == EntityRef::fixture(crate::level::RegionId::FIXTURE, 2)=>"b",_=>"none"},
                "level":if source.level.dimension == "minecraft:the_nether" {"nether"} else {"overworld"},
                "name":super::super::exceptions::plain_text(&name),
                "text_name":source.text_name(|entity| if entity == EntityRef::fixture(crate::level::RegionId::FIXTURE, 1) {"PlainA".into()} else {"PlainB".into()}),
                "silent":source.silent,
                "eyes":source.anchor == EntityAnchor::Eyes,
                "signing":operation == "signing",
                "chainer":source.chat_message_chainer == 1,
                "permissions":(0..5).map(|level|source.permissions.has_level(level)).collect::<Vec<_>>(),
                "callbacks":*events.lock().unwrap(),
            });
            assert_eq!(observed, vector, "{operation}");
            assert_eq!(base.position, Vec3::new(80., 64., -16.));
            assert!(base.entity.is_none());
        }
    }
}
