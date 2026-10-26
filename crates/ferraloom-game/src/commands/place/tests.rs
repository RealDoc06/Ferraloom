use super::*;
use crate::commands::{CommandSourceStack, Commands, source::Rotation as SourceRotation};
use ferraloom_util::random::{LegacyRandomSource, RandomSource};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, io};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vector {
    command: String,
    found: bool,
    placed: bool,
    has_chunk: bool,
    in_world: bool,
    missing: Vec<i32>,
    size: [i32; 3],
    permission: i32,
    value: Option<i32>,
    error: Option<String>,
    cursor: Option<i32>,
    #[serde(rename = "error_component")]
    error_component: Option<Value>,
    trace: Value,
    feedback: Value,
}
struct Host<'a> {
    vector: &'a Vector,
    trace: RefCell<Vec<Value>>,
}
impl Host<'_> {
    fn record(&self, value: impl Into<Value>) {
        self.trace.borrow_mut().push(value.into());
    }
}
fn pos(pos: BlockPos) -> [i32; 3] {
    [pos.x, pos.y, pos.z]
}
impl PlaceAccess for Host<'_> {
    type Error = io::Error;
    fn eye_height(&self, _: EntityRef) -> Result<f64, Self::Error> {
        self.record("eye_height");
        Ok(1.25)
    }
    fn has_chunk_at(&self, position: BlockPos) -> Result<bool, Self::Error> {
        self.record(json!({"call":"has_chunk","pos":pos(position)}));
        Ok(self.vector.has_chunk)
    }
    fn in_world_bounds(&self, position: BlockPos) -> bool {
        self.record(json!({"call":"in_world","pos":pos(position)}));
        self.vector.in_world
    }
    fn is_loaded(&self, position: BlockPos) -> Result<bool, Self::Error> {
        self.record(json!({"call":"loaded","pos":pos(position)}));
        Ok(self.vector.missing != pos(position))
    }
}
impl TemplateAccess for Host<'_> {
    type Template = ();
    fn template(&mut self, id: &Identifier) -> Result<Option<()>, Self::Error> {
        self.record(json!({"call":"template","id":id.to_string()}));
        Ok(self.vector.found.then_some(()))
    }
    fn size(&self, _: &()) -> [i32; 3] {
        self.record("size");
        self.vector.size
    }
    fn create_random(&mut self, seed: i64) -> Result<Random, Self::Error> {
        self.record(json!({"call":"random","seed":seed}));
        // Native capture controls createRandom(0)'s clock seed, not its draws.
        Ok(Random::Legacy(LegacyRandomSource::new(if seed == 0 {
            42
        } else {
            seed
        })))
    }
    fn place_template(
        &mut self,
        _: &(),
        position: BlockPos,
        mut settings: StructurePlaceSettings,
        processors: &[StructureProcessor],
        mut random: Random,
        flags: u32,
    ) -> Result<bool, Self::Error> {
        let rotation = match settings.rotation {
            Rotation::None => "NONE",
            Rotation::Clockwise90 => "CLOCKWISE_90",
            Rotation::Clockwise180 => "CLOCKWISE_180",
            Rotation::CounterClockwise90 => "COUNTERCLOCKWISE_90",
        };
        let mirror = match settings.mirror {
            Mirror::None => "NONE",
            Mirror::LeftRight => "LEFT_RIGHT",
            Mirror::FrontBack => "FRONT_BACK",
        };
        let processors = processors
            .iter()
            .map(|processor| match processor {
                StructureProcessor::BlockRot {
                    integrity,
                    rottable: None,
                } => json!({"integrity":integrity}),
                other => panic!("unexpected template command processor: {other:?}"),
            })
            .collect::<Vec<_>>();
        let settings_first_long =
            settings.with_random(pos(position), &mut random, |random| random.next_long());
        let placement_first_long = random.next_long();
        self.record(json!({
            "call":"place","pos":pos(position),"reference":pos(position),
            "rotation":rotation,"mirror":mirror,"known_shape":settings.known_shape,
            "ignore_entities":settings.ignore_entities,"finalize_entities":settings.finalize_entities,
            "waterlogging":settings.liquid_settings == ferraloom_worldgen::structure::template::LiquidSettings::ApplyWaterlogging,
            "processors":processors,"flags":flags,
            "settings_first_long":settings_first_long,"placement_first_long":placement_first_long,
        }));
        Ok(self.vector.placed)
    }
}

#[test]
fn native_template_arguments_loaded_area_settings_seeds_feedback_and_errors_match() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../tests/java/commands/place/vectors.json"
    ))
    .unwrap();
    assert_eq!(vectors.len(), 38);
    let commands = Commands::default();
    for vector in &vectors {
        let mut source = CommandSourceStack::server(
            vector.permission,
            ferraloom_util::Vec3::new(80.25, 64.5, -16.75),
        );
        source.rotation = SourceRotation {
            pitch: -20.,
            yaw: 35.,
        };
        source.anchor = EntityAnchor::Eyes;
        source.entity = Some(EntityRef::fixture(crate::level::RegionId::FIXTURE, 42));
        let mut host = Host {
            vector,
            trace: RefCell::default(),
        };
        let result = commands
            .prepare(&vector.command, source)
            .map_err(PlaceError::Syntax)
            .and_then(|command| template(&command, &mut host));
        let feedback = match result {
            Ok(result) => {
                assert_eq!(Some(result.value), vector.value, "{}", vector.command);
                assert!(vector.error.is_none(), "{}", vector.command);
                host.record("success");
                result.feedback.into_iter().map(|feedback| json!({"broadcast":feedback.broadcast,"message":serde_json::from_str::<Value>(&feedback.message.to_json().unwrap()).unwrap()})).collect::<Vec<_>>()
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
    }
}
