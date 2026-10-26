use super::*;
use crate::commands::{
    CommandSourceStack, Commands, context::CommandBuildContext, source::Rotation as SourceRotation,
};
use ferraloom_util::{
    Vec3,
    random::{LegacyRandomSource, RandomSource},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, io};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vector {
    command: String,
    placed: bool,
    has_chunk: bool,
    in_world: bool,
    missing: Vec<i32>,
    permission: i32,
    value: Option<i32>,
    error: Option<String>,
    cursor: Option<i32>,
    #[serde(rename = "error_component")]
    error_component: Option<Value>,
    trace: Value,
    feedback: Value,
    #[serde(rename = "next_long")]
    next_long: i64,
}
struct Host<'a> {
    vector: &'a Vector,
    trace: RefCell<Vec<Value>>,
    random: LegacyRandomSource,
}
impl Host<'_> {
    fn record(&self, value: impl Into<Value>) {
        self.trace.borrow_mut().push(value.into());
    }
}
fn point(p: BlockPos) -> [i32; 3] {
    [p.x, p.y, p.z]
}
impl PlaceAccess for Host<'_> {
    type Error = io::Error;
    fn eye_height(&self, _: EntityRef) -> Result<f64, Self::Error> {
        self.record("eye_height");
        Ok(1.25)
    }
    fn has_chunk_at(&self, p: BlockPos) -> Result<bool, Self::Error> {
        self.record(json!({"call":"has_chunk","pos":point(p)}));
        Ok(self.vector.has_chunk)
    }
    fn in_world_bounds(&self, p: BlockPos) -> bool {
        self.record(json!({"call":"in_world","pos":point(p)}));
        self.vector.in_world
    }
    fn is_loaded(&self, p: BlockPos) -> Result<bool, Self::Error> {
        self.record(json!({"call":"loaded","pos":point(p)}));
        Ok(self.vector.missing != point(p))
    }
}
impl FeatureAccess for Host<'_> {
    fn place_registered_feature(
        &mut self,
        id: &Identifier,
        position: BlockPos,
    ) -> Result<bool, Self::Error> {
        let draw = self.random.next_long();
        self.record(
            json!({"call":"feature","id":id.to_string(),"pos":point(position),"draw":draw}),
        );
        Ok(self.vector.placed)
    }
}

#[test]
fn native_registered_feature_area_traversal_results_feedback_and_live_rng_match() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/place/features.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 12);
    let mut context = CommandBuildContext::default();
    context.registries.insert(
        "minecraft:worldgen/feature".into(),
        vec!["minecraft:oak".into()],
    );
    let commands = Commands::new(context);
    for vector in vectors {
        let mut source =
            CommandSourceStack::server(vector.permission, Vec3::new(80.25, 64.5, -16.75));
        source.rotation = SourceRotation {
            pitch: -20.,
            yaw: 35.,
        };
        source.anchor = EntityAnchor::Eyes;
        source.entity = Some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 42));
        let mut host = Host {
            vector: &vector,
            trace: RefCell::default(),
            random: LegacyRandomSource::new(42),
        };
        let result = commands
            .prepare(&vector.command, source)
            .map_err(PlaceError::Syntax)
            .and_then(|command| feature(&command, &mut host));
        let feedback = match result {
            Ok(result) => {
                assert_eq!(Some(result.value), vector.value, "{}", vector.command);
                assert!(vector.error.is_none(), "{}", vector.command);
                host.record("success");
                result.feedback.into_iter().map(|feedback|json!({"broadcast":feedback.broadcast,"message":serde_json::from_str::<Value>(&feedback.message.to_json().unwrap()).unwrap()})).collect::<Vec<_>>()
            }
            Err(PlaceError::Syntax(error)) => {
                assert_eq!(
                    error.raw_message.text(),
                    vector.error.as_deref().unwrap(),
                    "{}",
                    vector.command
                );
                assert_eq!(
                    Some(error.cursor.map(|c| c as i32).unwrap_or(-1)),
                    vector.cursor,
                    "{}",
                    vector.command
                );
                assert_eq!(
                    serde_json::from_str::<Value>(
                        &crate::commands::exceptions::error_component(&error)
                            .to_json()
                            .unwrap()
                    )
                    .unwrap(),
                    *vector.error_component.as_ref().unwrap(),
                    "{}",
                    vector.command
                );
                Vec::new()
            }
            Err(error) => panic!("{}: {error}", vector.command),
        };
        assert_eq!(json!(feedback), vector.feedback, "{}", vector.command);
        assert_eq!(
            json!(*host.trace.borrow()),
            vector.trace,
            "{}",
            vector.command
        );
        assert_eq!(
            host.random.next_long(),
            vector.next_long,
            "{}",
            vector.command
        );
    }
}
