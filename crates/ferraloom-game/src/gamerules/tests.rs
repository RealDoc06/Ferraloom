use super::*;
#[test]
fn saved_rule_partial_values_and_errors_match_java() {
    #[derive(serde::Deserialize)]
    struct Vector {
        input: String,
        success: bool,
        partial: Option<serde_json::Value>,
        error: Option<String>,
    }
    let vectors: Vec<Vector> =
        serde_json::from_str(include_str!("../commands/vanilla/saved-rule-vectors.json")).unwrap();
    for vector in vectors {
        let tag = ferraloom_nbt::parse(&vector.input).unwrap();
        let result = GameRuleMap::decode_partial(&tag);
        assert_eq!(result.error, vector.error, "{}", vector.input);
        assert_eq!(result.error.is_none(), vector.success, "{}", vector.input);
        let actual = result.value.map(|map| {
            serde_json::Value::Object(
                map.iter()
                    .map(|(rule, value)| {
                        (
                            rule.definition().id.into(),
                            match value {
                                GameRuleValue::Boolean(value) => serde_json::json!(value),
                                GameRuleValue::Integer(value) => serde_json::json!(value),
                            },
                        )
                    })
                    .collect(),
            )
        });
        assert_eq!(actual, vector.partial, "{}", vector.input);
    }
}
#[test]
fn all_definitions_match_bootstrapped_java_registry() {
    let expected: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../commands/vanilla/rules.json")).unwrap();
    assert_eq!(expected.len(), GameRule::ALL.len());
    let rules = GameRules::default();
    for (rule, expected) in GameRule::ALL.into_iter().zip(expected) {
        let definition = rule.definition();
        assert_eq!(definition.id, expected["name"]);
        assert_eq!(definition.category.id(), expected["category"]);
        assert_eq!(
            rules.get(rule).is_some(),
            expected["enabled"].as_bool().unwrap()
        );
        match definition.kind {
            RuleType::Boolean { default } => {
                assert_eq!(serde_json::json!(default), expected["default"])
            }
            RuleType::Integer { default, min, max } => {
                assert_eq!(serde_json::json!(default), expected["default"]);
                assert_eq!(serde_json::json!(min), expected["min"]);
                assert_eq!(serde_json::json!(max), expected["max"]);
            }
        }
    }
}
#[test]
fn parsing_bounds_and_partial_deserialization() {
    assert_eq!(
        GameRule::MaxSnowAccumulationHeight.deserialize("8"),
        Ok(GameRuleValue::Integer(8))
    );
    assert_eq!(
        GameRule::MaxSnowAccumulationHeight.deserialize("9"),
        Err(DeserializeError::Invalid)
    );
    assert_eq!(
        GameRule::MaxBlockModifications.deserialize("0"),
        Err(DeserializeError::Invalid)
    );
    assert_eq!(
        GameRule::FireSpreadRadiusAroundPlayer.deserialize("-1"),
        Ok(GameRuleValue::Integer(-1))
    );
    assert_eq!(
        GameRule::Pvp.deserialize("TRUE"),
        Err(DeserializeError::Invalid)
    );
    assert_eq!(
        GameRule::Pvp.deserialize("true "),
        Err(DeserializeError::TrailingCharacters(
            GameRuleValue::Boolean(true)
        ))
    );
}
#[test]
fn native_saved_data_round_trip_and_feature_filtering() {
    let mut rules = GameRules::default();
    assert!(
        rules
            .set(GameRule::MaxMinecartSpeed, GameRuleValue::Integer(10))
            .is_err()
    );
    let mut enabled = rules.copy(RuleFeatures {
        minecart_improvements: true,
    });
    enabled
        .set(GameRule::MaxMinecartSpeed, GameRuleValue::Integer(1000))
        .unwrap();
    rules
        .set(GameRule::KeepInventory, GameRuleValue::Boolean(true))
        .unwrap();
    let map = GameRuleMap::decode(&rules.map().encode().unwrap()).unwrap();
    assert_eq!(GameRules::new(RuleFeatures::default(), map), rules);
    assert!(
        enabled
            .copy(RuleFeatures::default())
            .get(GameRule::MaxMinecartSpeed)
            .is_none()
    );
}
#[test]
fn callbacks_fire_for_unchanged_values_and_have_vanilla_event_ids() {
    let mut rules = GameRules::default();
    for (rule, enabled, expected) in [
        (
            GameRule::ImmediateRespawn,
            false,
            GameRuleEffect::GameEvent {
                event: 11,
                enabled: false,
            },
        ),
        (
            GameRule::LimitedCrafting,
            true,
            GameRuleEffect::GameEvent {
                event: 12,
                enabled: true,
            },
        ),
        (
            GameRule::ReducedDebugInfo,
            true,
            GameRuleEffect::ReducedDebugInfo { event: 22 },
        ),
        (
            GameRule::AdvanceTime,
            true,
            GameRuleEffect::SynchronizeClocks,
        ),
    ] {
        assert_eq!(
            rules
                .set(rule, GameRuleValue::Boolean(enabled))
                .unwrap()
                .effect,
            Some(expected)
        );
    }
}

#[test]
fn every_rule_value_parser_matches_java_including_partial_values() {
    #[derive(serde::Deserialize)]
    struct Vector {
        rule: String,
        input: String,
        value: Option<serde_json::Value>,
        partial: Option<serde_json::Value>,
        error: Option<String>,
    }
    let vectors: Vec<Vector> =
        serde_json::from_str(include_str!("../commands/vanilla/rule-parsing.json")).unwrap();
    let value = |json: &serde_json::Value| match json {
        serde_json::Value::Bool(value) => GameRuleValue::Boolean(*value),
        _ => GameRuleValue::Integer(json.as_i64().unwrap() as i32),
    };
    for vector in vectors {
        let rule = GameRule::by_id(&vector.rule).unwrap();
        let expected = if let Some(result) = &vector.value {
            assert!(vector.error.is_none());
            Ok(value(result))
        } else if let Some(partial) = &vector.partial {
            assert_eq!(
                vector.error.as_deref(),
                Some("Failed to deserialize; trailing characters")
            );
            Err(DeserializeError::TrailingCharacters(value(partial)))
        } else {
            assert_eq!(vector.error.as_deref(), Some("Failed to deserialize"));
            Err(DeserializeError::Invalid)
        };
        assert_eq!(
            rule.deserialize(&vector.input),
            expected,
            "{} {:?}",
            vector.rule,
            vector.input
        );
    }
}
