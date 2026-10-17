//! Owned command sources. Canonical entities and servers are referenced by handles.
use super::permissions::PermissionSet;
use crate::entity::EntityRef;
use ferraloom_protocol::{Uuid, chat::Component};
use ferraloom_util::{Vec3, mth};
use std::{fmt, sync::Arc};

/// Port of CommandResultCallback. Callbacks run inline, in composition order.
#[derive(Clone, Default)]
pub struct CommandResultCallback(Option<Arc<dyn Fn(bool, i32) + Send + Sync>>);
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
    pub fn new(callback: impl Fn(bool, i32) + Send + Sync + 'static) -> Self {
        Self(Some(Arc::new(callback)))
    }
    pub fn on_result(&self, success: bool, value: i32) {
        if let Some(callback) = &self.0 {
            callback(success, value);
        }
    }
    pub fn on_success(&self, value: i32) {
        self.on_result(true, value);
    }
    pub fn on_failure(&self) {
        self.on_result(false, 0);
    }
    pub fn chain(self, other: Self) -> Self {
        if self.0.is_none() {
            return other;
        }
        if other.0.is_none() {
            return self;
        }
        Self::new(move |success, value| {
            self.on_result(success, value);
            other.on_result(success, value);
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
    Constant(Component),
}
#[derive(Clone, Debug, Default)]
pub struct CommandSigningContext {
    pub arguments: Arc<[(String, SignedArgument)]>,
}
#[derive(Clone, Debug)]
pub struct SignedArgument {
    pub sender: Uuid,
    pub index: i32,
    pub signature: Option<Arc<[u8]>>,
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
            names: NamesProvider::Constant(Component::literal("Server")),
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
            NamesProvider::Constant(name) => name.clone(),
            NamesProvider::Entity => self
                .entity
                .map(entity_name)
                .unwrap_or_else(|| Component::literal("")),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entity_source_and_dimension_changes_preserve_vanilla_fields() {
        let source =
            CommandSourceStack::server(4, Vec3::new(80., 64., -16.)).with_entity(EntityRef(7));
        assert_eq!(source.source, CommandSource::Server);
        let nether = source.with_level(CommandLevel {
            dimension: "minecraft:the_nether".into(),
            coordinate_scale: 8.,
        });
        assert_eq!(nether.position, Vec3::new(10., 64., -2.));
        assert_eq!(nether.entity, Some(EntityRef(7)));
        assert_eq!(
            nether.with_entity(EntityRef(9)).position,
            Vec3::new(10., 64., -2.)
        );
    }
}
