use super::*;
use crate::commands::arguments::selectors::{SelectorEntity, SelectorPredicate};
use ferraloom_util::{Uuid, Vec3};

struct Backend {
    document: CompoundTag,
    data_reads: usize,
    warnings: usize,
    scores_available: bool,
}
impl EntityQuery for Backend {
    fn candidates(&self, _: bool) -> Vec<EntityRef> {
        vec![EntityRef(1), EntityRef(2)]
    }
    fn info(&self, _: EntityRef) -> Option<SelectorEntity<'_>> {
        Some(SelectorEntity {
            name: "Alice".into(),
            uuid: Uuid::NIL,
            player: true,
            dimension: "minecraft:overworld",
            position: Vec3::ZERO,
            width: 0.6,
            height: 1.8,
        })
    }
    fn display_name(&self, _: EntityRef) -> Result<Component, CommandSyntaxException> {
        Ok(Component::literal("Display Alice"))
    }
    fn matches(
        &mut self,
        _: EntityRef,
        predicate: &SelectorPredicate,
    ) -> Result<bool, CommandSyntaxException> {
        Ok(match predicate {
            SelectorPredicate::Alive => true,
            SelectorPredicate::Type {
                value,
                inverted,
                tag: false,
            } => (value == "minecraft:player") != *inverted,
            _ => panic!("unexpected fixture predicate: {predicate:?}"),
        })
    }
    fn shuffle(&mut self, _: &mut [(EntityRef, f64)]) {
        panic!("fixture has no random selector");
    }
}
impl ComponentResolver for Backend {
    fn formatted_score(
        &mut self,
        holder: &str,
        objective: &str,
    ) -> Result<Component, ResolutionError> {
        if !self.scores_available {
            return Err(ResolutionError::Unsupported(UnsupportedContent::Score));
        }
        Ok(if objective == "o" {
            match holder {
                "Alice" => Component::literal("37"),
                "fake holder" => Component::literal("-4"),
                _ => Component::default(),
            }
        } else {
            Component::default()
        })
    }
    fn nbt_data(
        &mut self,
        _: &CommandSourceStack,
        source: &DataSource,
    ) -> Result<Vec<CompoundTag>, ResolutionError> {
        assert!(matches!(source, DataSource::Storage(id) if id.as_str()=="test:data"));
        self.data_reads += 1;
        Ok(vec![self.document.clone()])
    }
    fn invalid_interpreted_component(&mut self, _: &Tag, _: &str) {
        self.warnings += 1;
    }
}
#[derive(serde::Deserialize)]
struct Vector {
    input: serde_json::Value,
    nbt: String,
    source: bool,
    depth: i32,
    limit: i32,
    discard: bool,
    reject: bool,
    budget: i32,
    output: Option<serde_json::Value>,
    error: Option<String>,
    count: i32,
}
#[test]
fn native_dynamic_components_recursion_styles_and_nbt_text_match() {
    let vectors: Vec<Vector> =
        serde_json::from_str(include_str!("../vanilla/component-resolution-vectors.json")).unwrap();
    let source = CommandSourceStack::server(4, Vec3::ZERO).with_entity(EntityRef(1));
    for (index, vector) in vectors.into_iter().enumerate() {
        let component = Component::from_json(&vector.input.to_string()).unwrap();
        let mut backend = Backend {
            document: ferraloom_nbt::parse_compound(&vector.nbt).unwrap(),
            data_reads: 0,
            warnings: 0,
            scores_available: true,
        };
        let mut context = ResolutionContext::new(vector.source.then_some(&source));
        context.depth_limit = vector.limit;
        context.depth_limit_behavior = if vector.discard {
            LimitBehavior::DiscardRemaining
        } else {
            LimitBehavior::StopProcessingAndCopyRemaining
        };
        context.resolved_component_limit = vector.budget;
        if vector.reject {
            context.object_info_validator = &|_| false;
        }
        match resolve_at_depth(&mut context, &component, &mut backend, vector.depth) {
            Ok(component) => assert_eq!(
                Some(
                    serde_json::from_str::<serde_json::Value>(&component.to_json().unwrap())
                        .unwrap()
                ),
                vector.output,
                "case {index}: {}",
                vector.input
            ),
            Err(ResolutionError::Syntax(error)) => assert_eq!(
                Some(error.raw_message.text().to_owned()),
                vector.error,
                "case {index}"
            ),
            Err(error) => panic!("case {index}: {error}"),
        }
        assert_eq!(
            context.resolved_component_count(),
            vector.count,
            "case {index}"
        );
        assert_eq!(backend.data_reads, vector.count as usize, "case {index}");
        assert_eq!(
            backend.warnings,
            usize::from(vector.nbt.contains("invalid:1")),
            "case {index}"
        );
    }
}
#[test]
fn missing_live_score_service_remains_explicitly_unsupported() {
    let source = CommandSourceStack::server(4, Vec3::ZERO).with_entity(EntityRef(1));
    let mut context = ResolutionContext::new(Some(&source));
    let mut backend = Backend {
        document: CompoundTag::new(),
        data_reads: 0,
        warnings: 0,
        scores_available: false,
    };
    let component = Component::new(Contents::Score {
        name: "*".into(),
        objective: "o".into(),
    });
    assert!(matches!(
        resolve(&mut context, &component, &mut backend),
        Err(ResolutionError::Unsupported(UnsupportedContent::Score))
    ));
}

#[test]
fn required_selector_name_failure_retains_the_level_error() {
    let failure = crate::level::LevelError::MissingEntity(EntityRef(99));
    let syntax = crate::commands::arguments::selectors::selector_read_error(failure);
    assert!(
        matches!(ResolutionError::from(syntax), ResolutionError::EntityData(error) if error == failure)
    );
}
