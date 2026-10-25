//! Registration metadata, compared with an independent vanilla 26.3 bootstrap.
use ferraloom_data::{Block, registries::block_entity_type};
use ferraloom_game::block::entity::registry;
use serde_json::json;

#[test]
fn every_block_entity_registration_matches_the_vanilla_bootstrap_dump() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("java/blocks/entities/registry-bootstrap.json")).unwrap();
    let actual: Vec<_> = block_entity_type::ALL.iter().map(|&kind| json!({
        "id": kind.protocol_id(),
        "name": kind.name(),
        "only_op_can_set_nbt": registry::only_op_can_set_nbt(kind),
        "is_nameable": kind.is_nameable(),
        "valid_blocks": registry::valid_blocks(kind).iter().map(|block| block.protocol_id()).collect::<Vec<_>>(),
    })).collect();
    assert_eq!(actual.len(), 49);
    assert_eq!(serde_json::Value::Array(actual), expected);
    for row in expected.as_array().unwrap() {
        let kind =
            block_entity_type::Id::from_protocol_id(row["id"].as_u64().unwrap() as u16).unwrap();
        for &block in ferraloom_data::registries::block::ALL {
            let expected = row["valid_blocks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id.as_u64() == Some(u64::from(block.protocol_id())));
            assert_eq!(
                registry::is_valid(kind, block.default_state()),
                expected,
                "{} {}",
                kind.name(),
                block.name()
            );
        }
    }
    assert_eq!(
        registry::for_block(Block::CHEST),
        Some(block_entity_type::Id::CHEST)
    );
}
