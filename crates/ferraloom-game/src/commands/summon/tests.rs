use super::*;
use crate::commands::{CommandSourceStack, Commands, source::Rotation};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, io};

const ROOT: EntityRef = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
const ACTOR: EntityRef = EntityRef::fixture(crate::level::RegionId::FIXTURE, 42);
#[derive(Deserialize)]
struct Vector {
    command: String,
    peaceful: bool,
    load: bool,
    duplicate: bool,
    passengers: usize,
    permission: u8,
    eyes: bool,
    mob: bool,
    allowed: bool,
    display: Value,
    value: Option<i32>,
    error: Option<String>,
    cursor: Option<i32>,
    error_component: Option<Value>,
    trace: Value,
    feedback: Value,
    original_after: Option<String>,
}
fn vectors() -> Vec<Vector> {
    serde_json::from_str(include_str!(
        "../../../tests/java/commands/summon/vectors.json"
    ))
    .unwrap()
}
struct Loaded {
    position: Vec3,
}
struct World<'a> {
    vector: &'a Vector,
    trace: RefCell<Vec<Value>>,
    required_error: Option<&'static str>,
}
impl World<'_> {
    fn record(&self, value: impl Into<Value>) {
        self.trace.borrow_mut().push(value.into());
    }
}
impl SummonAccess for World<'_> {
    type Loaded = Loaded;
    type Error = io::Error;
    fn eye_height(&self, entity: EntityRef) -> Result<f64, Self::Error> {
        assert_eq!(entity, ACTOR);
        self.record("eye_height");
        Ok(1.25)
    }
    fn difficulty(&self) -> Difficulty {
        self.record("difficulty");
        if self.vector.peaceful {
            Difficulty::Peaceful
        } else {
            Difficulty::Normal
        }
    }
    fn allowed_in_peaceful(&self, _: EntityType) -> bool {
        self.vector.allowed
    }
    fn load_entity_recursive(
        &mut self,
        tag: CompoundTag,
        position: Vec3,
    ) -> Result<Option<Loaded>, Self::Error> {
        self.record(json!({"call":"load","tag":Tag::Compound(tag).to_string(),"reason":"COMMAND","ignore_checks":false}));
        if let Some(kind) = self.required_error {
            return Err(io::Error::new(io::ErrorKind::Unsupported, kind));
        }
        if !self.vector.load {
            return Ok(None);
        }
        for index in 0..=self.vector.passengers {
            self.record(json!({"call":"snap","index":index,"pos":[position.x,position.y,position.z],"yaw":10.5 + index as f32 * 10.,"pitch":-25.25 + index as f32 * 10.}));
        }
        Ok(Some(Loaded { position }))
    }
    fn is_mob(&self, _: &Loaded) -> Result<bool, Self::Error> {
        Ok(self.vector.mob)
    }
    fn finalize_spawn(&mut self, entity: &mut Loaded) -> Result<(), Self::Error> {
        let pos = BlockPos::containing(entity.position.x, entity.position.y, entity.position.z);
        self.record(json!({"call":"local_difficulty","pos":[pos.x,pos.y,pos.z]}));
        self.record("finalize");
        Ok(())
    }
    fn try_add_fresh_entity_with_passengers(
        &mut self,
        _: Loaded,
    ) -> Result<Option<EntityRef>, Self::Error> {
        self.record("register");
        Ok((!self.vector.duplicate).then_some(ROOT))
    }
    fn display_name(&self, entity: EntityRef) -> Result<Component, Self::Error> {
        assert_eq!(entity, ROOT);
        self.record("display_name");
        Ok(Component::from_json(&self.vector.display.to_string()).unwrap())
    }
}
fn source(permission: u8, eyes: bool) -> CommandSourceStack {
    let mut source =
        CommandSourceStack::server(i32::from(permission), Vec3::new(80.25, 64.5, -16.75));
    source.rotation = Rotation {
        pitch: -20.,
        yaw: 35.,
    };
    source.entity = Some(ACTOR);
    source.anchor = if eyes {
        EntityAnchor::Eyes
    } else {
        EntityAnchor::Feet
    };
    source
}

#[test]
fn native_summon_arguments_bounds_finalization_order_uuid_failure_and_feedback_match() {
    let vectors = vectors();
    assert_eq!(vectors.len(), 29);
    let commands = Commands::default();
    for vector in &vectors {
        let mut world = World {
            vector,
            trace: RefCell::default(),
            required_error: None,
        };
        let command = commands.prepare(&vector.command, source(vector.permission, vector.eyes));
        let result = match &command {
            Ok(command) => run(command, &mut world),
            Err(error) => Err(SummonError::Syntax(error.clone())),
        };
        let feedback = match result {
            Ok(result) => {
                assert_eq!(Some(result.value), vector.value, "{}", vector.command);
                assert!(vector.error.is_none(), "{}", vector.command);
                world.record("success");
                result.feedback.into_iter().map(|feedback| json!({"broadcast":feedback.broadcast,"message":serde_json::from_str::<Value>(&feedback.message.to_json().unwrap()).unwrap()})).collect::<Vec<_>>()
            }
            Err(SummonError::Syntax(error)) => {
                assert_eq!(
                    error.raw_message.text(),
                    vector.error.as_deref().expect("native error"),
                    "{}",
                    vector.command
                );
                assert_eq!(
                    Some(error.cursor.map(|cursor| cursor as i32).unwrap_or(-1)),
                    vector.cursor,
                    "{}",
                    vector.command
                );
                assert_eq!(
                    serde_json::from_str::<Value>(
                        &super::super::exceptions::error_component(&error)
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
            json!(*world.trace.borrow()),
            vector.trace,
            "{}",
            vector.command
        );
        if let (Ok(command), Some(expected)) = (command, &vector.original_after) {
            let Some(ArgumentValue::Tag(Tag::Compound(actual))) = command.argument("nbt") else {
                panic!("native input compound")
            };
            assert_eq!(
                *actual,
                ferraloom_nbt::parse_compound(expected).unwrap(),
                "id overwrite must not mutate the original input"
            );
        }
    }
}

#[test]
fn unsupported_factory_preserves_the_required_error_and_never_reports_success() {
    let vector = vectors().remove(0);
    let mut world = World {
        vector: &vector,
        trace: RefCell::default(),
        required_error: Some("minecraft:experience_orb"),
    };
    let command = Commands::default()
        .prepare(&vector.command, source(4, false))
        .unwrap();
    let error = run(&command, &mut world).unwrap_err();
    assert!(
        matches!(&error, SummonError::World(error) if error.kind() == io::ErrorKind::Unsupported && error.to_string() == "minecraft:experience_orb")
    );
    assert_eq!(
        std::error::Error::source(&error).unwrap().to_string(),
        "minecraft:experience_orb"
    );
    assert!(
        !world
            .trace
            .borrow()
            .iter()
            .any(|event| event == "register" || event == "success")
    );
}
