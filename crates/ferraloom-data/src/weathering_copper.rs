//! Bootstrapped WeatheringCopper ages and NEXT_BY_BLOCK. The inverse and first
//! stage are derived once, rather than maintained as independent mappings.
use crate::{Block, BlockState};

/// Port of WeatheringCopper.WeatherState, in native ordinal order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum WeatherState {
    Unaffected,
    Exposed,
    Weathered,
    Oxidized,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WeatheringCopperProperties {
    pub age: WeatherState,
    pub next: Option<Block>,
    pub previous: Option<Block>,
    pub first: Block,
}
include!(concat!(env!("OUT_DIR"), "/weathering_copper.rs"));

impl Block {
    pub const fn weathering_copper_properties(self) -> Option<&'static WeatheringCopperProperties> {
        WEATHERING_COPPER_PROPERTIES[self.protocol_id() as usize].as_ref()
    }
    /// Port of WeatheringCopper#getNext(Block).
    pub fn next_weathering(self) -> Option<Self> {
        self.weathering_copper_properties()?.next
    }
    /// Port of WeatheringCopper#getPrevious(Block).
    pub fn previous_weathering(self) -> Option<Self> {
        self.weathering_copper_properties()?.previous
    }
    /// Port of WeatheringCopper#getFirst(Block), including non-copper inputs.
    pub fn first_weathering(self) -> Self {
        self.weathering_copper_properties()
            .map_or(self, |copper| copper.first)
    }
}
impl BlockState {
    /// Port of WeatheringCopper#getNext(BlockState), preserving shared properties.
    pub fn next_weathering(self) -> Option<Self> {
        self.block()
            .next_weathering()
            .map(|next| next.default_state().with_properties_of(self))
    }
    /// Port of WeatheringCopper#getPrevious(BlockState).
    pub fn previous_weathering(self) -> Option<Self> {
        self.block()
            .previous_weathering()
            .map(|previous| previous.default_state().with_properties_of(self))
    }
    /// Port of WeatheringCopper#getFirst(BlockState).
    pub fn first_weathering(self) -> Self {
        self.block()
            .first_weathering()
            .default_state()
            .with_properties_of(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NATIVE: &str = include_str!("../tests/fixtures/weathering_copper/native.tsv");

    #[test]
    fn copper_registry_ages_and_progression_match_independent_native_bootstrap() {
        let mut checked = 0;
        let mut members = 0;
        for line in NATIVE.lines().filter(|line| line.starts_with("block\t")) {
            let f: Vec<_> = line.split('\t').collect();
            let block = Block::from_protocol_id(f[1].parse().unwrap()).unwrap();
            assert_eq!(block.name(), f[2], "{line}");
            let properties = block.weathering_copper_properties();
            assert_eq!(
                properties.map_or(-1, |p| i64::from(p.age as u8)),
                f[3].parse::<i64>().unwrap(),
                "{line}"
            );
            assert_eq!(
                block
                    .next_weathering()
                    .map_or(-1, |b| i64::from(b.protocol_id())),
                f[4].parse::<i64>().unwrap(),
                "{line}"
            );
            assert_eq!(
                block
                    .previous_weathering()
                    .map_or(-1, |b| i64::from(b.protocol_id())),
                f[5].parse::<i64>().unwrap(),
                "{line}"
            );
            assert_eq!(
                i64::from(block.first_weathering().protocol_id()),
                f[6].parse::<i64>().unwrap(),
                "{line}"
            );
            members += usize::from(properties.is_some());
            checked += 1;
        }
        assert_eq!(checked, crate::BLOCKS.len());
        assert_eq!(members, 60);
    }
    #[test]
    fn copper_state_transforms_preserve_native_properties_and_random_tick_gates() {
        let mut checked = 0;
        for line in NATIVE.lines().filter(|line| line.starts_with("state\t")) {
            let f: Vec<_> = line.split('\t').collect();
            let state = BlockState::from_id(f[1].parse().unwrap()).unwrap();
            assert_eq!(
                state.next_weathering().map_or(-1, |s| i64::from(s.id())),
                f[2].parse::<i64>().unwrap(),
                "{line}"
            );
            assert_eq!(
                state
                    .previous_weathering()
                    .map_or(-1, |s| i64::from(s.id())),
                f[3].parse::<i64>().unwrap(),
                "{line}"
            );
            assert_eq!(
                i64::from(state.first_weathering().id()),
                f[4].parse::<i64>().unwrap(),
                "{line}"
            );
            assert_eq!(state.data().random_ticking, f[5] == "true", "{line}");
            checked += 1;
        }
        assert_eq!(
            checked,
            crate::BLOCK_STATES
                .iter()
                .filter(|s| s.block.weathering_copper_properties().is_some())
                .count()
        );
        assert_eq!(checked, 1380);
    }
}
