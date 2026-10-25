//! Exact constructor/registry facts from a fresh 26.3 Bootstrap, independently
//! captured by RegistryFactVectors rather than read from generated data.
use ferraloom_data::{
    PropertyValue,
    properties::Instrument,
    registries::{block, item},
};
use ferraloom_util::{BlockPos, Direction};
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
struct Bootstrap {
    items: Vec<ItemFact>,
    blocks: Vec<BlockFact>,
    state_count: usize,
    instruments: Vec<InstrumentFact>,
}
#[derive(Debug, Deserialize)]
struct ItemFact {
    id: u16,
    name: String,
    spawn_egg: bool,
    registered: bool,
    behavior: Option<String>,
    projectile: Option<ProjectileFact>,
}
#[derive(Debug, Deserialize)]
struct ProjectileFact {
    power_bits: u32,
    uncertainty_bits: u32,
    event: Option<i32>,
    positions: Vec<PositionFact>,
}
#[derive(Debug, Deserialize)]
struct PositionFact {
    direction: String,
    bits: [String; 3],
}
#[derive(Debug, Deserialize)]
struct BlockFact {
    id: u16,
    name: String,
    state_count: u16,
    instrument: String,
}
#[derive(Debug, Deserialize)]
struct InstrumentFact {
    name: String,
    sound_id: u16,
    sound_name: String,
    tunable: bool,
    custom_sound: bool,
    works_above: bool,
}
fn bootstrap() -> &'static Bootstrap {
    static NATIVE: LazyLock<Bootstrap> = LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../../tests/java/blocks/redstone/registry_facts.json"
        ))
        .unwrap()
    });
    &NATIVE
}

#[test]
fn dispenser_registry_membership_classes_and_projectile_facts_match_native_bootstrap() {
    let native = &bootstrap().items;
    assert_eq!(item::ALL.len(), native.len());
    for (ordinal, (&item, expected)) in item::ALL.iter().zip(native).enumerate() {
        assert_eq!(usize::from(expected.id), ordinal);
        assert_eq!(item.protocol_id(), expected.id);
        assert_eq!(item.name(), expected.name);
        let actual = item.dispenser_properties();
        assert_eq!(actual.spawn_egg, expected.spawn_egg, "{}", item.name());
        assert_eq!(
            actual.registered_behavior.is_some(),
            expected.registered,
            "{} membership",
            item.name()
        );
        assert_eq!(
            actual.registered_behavior.map(|kind| kind.java_class()),
            expected.behavior.as_deref(),
            "{} class",
            item.name()
        );
        match (actual.projectile, &expected.projectile) {
            (None, None) => {}
            (Some(actual), Some(expected)) => {
                assert_eq!(
                    actual.power.to_bits(),
                    expected.power_bits,
                    "{}",
                    item.name()
                );
                assert_eq!(
                    actual.uncertainty.to_bits(),
                    expected.uncertainty_bits,
                    "{}",
                    item.name()
                );
                assert_eq!(
                    actual.override_dispense_event,
                    expected.event,
                    "{}",
                    item.name()
                );
                assert_eq!(expected.positions.len(), Direction::VALUES.len());
                for (direction, expected) in Direction::VALUES.into_iter().zip(&expected.positions)
                {
                    assert_eq!(direction.name(), expected.direction);
                    let position = super::dispenser::projectile_position(
                        BlockPos::new(8, 64, 8),
                        direction,
                        actual.position,
                    );
                    assert_eq!(
                        [
                            position.x.to_bits(),
                            position.y.to_bits(),
                            position.z.to_bits()
                        ],
                        expected
                            .bits
                            .each_ref()
                            .map(|bits| u64::from_str_radix(bits, 16).unwrap()),
                        "{} {direction:?} position function",
                        item.name()
                    );
                }
            }
            _ => panic!(
                "{} projectile membership: {actual:?} / {expected:?}",
                item.name()
            ),
        }
    }
}

#[test]
fn block_and_instrument_constructor_facts_match_native_bootstrap() {
    let native = bootstrap();
    assert_eq!(block::ALL.len(), native.blocks.len());
    let mut states = 0;
    for (ordinal, (&block, expected)) in block::ALL.iter().zip(&native.blocks).enumerate() {
        assert_eq!(usize::from(expected.id), ordinal);
        assert_eq!(block.protocol_id(), expected.id);
        assert_eq!(block.name(), expected.name);
        assert_eq!(
            block.data().state_count,
            expected.state_count,
            "{}",
            block.name()
        );
        assert_eq!(
            block.note_block_instrument().as_str(),
            expected.instrument,
            "{}",
            block.name()
        );
        states += usize::from(expected.state_count);
    }
    assert_eq!(states, native.state_count);
    let instruments: Vec<_> = (0..=u8::MAX).filter_map(Instrument::from_index).collect();
    assert_eq!(instruments.len(), native.instruments.len());
    for instrument in instruments {
        let expected = native
            .instruments
            .iter()
            .find(|expected| expected.name == instrument.as_str())
            .unwrap();
        let actual = instrument.note_properties();
        assert_eq!(
            actual.sound.protocol_id(),
            expected.sound_id,
            "{instrument:?}"
        );
        assert_eq!(actual.sound.name(), expected.sound_name, "{instrument:?}");
        assert_eq!(actual.tunable, expected.tunable, "{instrument:?}");
        assert_eq!(actual.custom_sound, expected.custom_sound, "{instrument:?}");
        assert_eq!(actual.works_above, expected.works_above, "{instrument:?}");
    }
}
