use super::*;
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
