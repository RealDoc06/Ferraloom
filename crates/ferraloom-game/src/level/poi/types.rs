//! Port of PoiTypes#bootstrap and its immutable block-state reverse index.
use ferraloom_data::{Block, BlockClass, BlockState, registries::point_of_interest_type};
pub use point_of_interest_type::Id as PoiTypeId;
use std::sync::OnceLock;

/// Port of PoiType. State membership is built once from the real state registry.
#[derive(Debug)]
pub struct PoiType {
    pub matching_states: Vec<BlockState>,
    pub max_tickets: i32,
    pub valid_range: i32,
}
#[derive(Debug)]
pub struct PoiTypes {
    types: Vec<PoiType>,
    by_state: Vec<Option<PoiTypeId>>,
}
impl PoiTypes {
    pub fn builtin() -> &'static Self {
        static TYPES: OnceLock<PoiTypes> = OnceLock::new();
        TYPES.get_or_init(Self::bootstrap)
    }
    fn bootstrap() -> Self {
        let mut types = Vec::new();
        let mut by_state = vec![None; ferraloom_data::BLOCK_STATES.len()];
        for &id in point_of_interest_type::ALL {
            let block = match id {
                PoiTypeId::ARMORER => Some(Block::BLAST_FURNACE),
                PoiTypeId::BUTCHER => Some(Block::SMOKER),
                PoiTypeId::CARTOGRAPHER => Some(Block::CARTOGRAPHY_TABLE),
                PoiTypeId::CLERIC => Some(Block::BREWING_STAND),
                PoiTypeId::FARMER => Some(Block::COMPOSTER),
                PoiTypeId::FISHERMAN => Some(Block::BARREL),
                PoiTypeId::FLETCHER => Some(Block::FLETCHING_TABLE),
                PoiTypeId::LIBRARIAN => Some(Block::LECTERN),
                PoiTypeId::MASON => Some(Block::STONECUTTER),
                PoiTypeId::SHEPHERD => Some(Block::LOOM),
                PoiTypeId::TOOLSMITH => Some(Block::SMITHING_TABLE),
                PoiTypeId::WEAPONSMITH => Some(Block::GRINDSTONE),
                PoiTypeId::MEETING => Some(Block::BELL),
                PoiTypeId::BEEHIVE => Some(Block::BEEHIVE),
                PoiTypeId::BEE_NEST => Some(Block::BEE_NEST),
                PoiTypeId::NETHER_PORTAL => Some(Block::NETHER_PORTAL),
                PoiTypeId::LODESTONE => Some(Block::LODESTONE),
                PoiTypeId::TEST_INSTANCE => Some(Block::TEST_INSTANCE_BLOCK),
                PoiTypeId::HOME | PoiTypeId::LEATHERWORKER | PoiTypeId::LIGHTNING_ROD => None,
                _ => panic!("unported POI type {}", id.name()),
            };
            let matching_states = (0..ferraloom_data::BLOCK_STATES.len())
                .filter_map(|index| {
                    let state = BlockState::from_id(index as u16).expect("registered state");
                    let matches = if let Some(block) = block {
                        state.block() == block
                    } else if id == PoiTypeId::HOME {
                        state.block().is_instance_of(BlockClass::BedBlock)
                            && state
                                .properties()
                                .any(|(name, value)| name == "part" && value == "head")
                    } else if id == PoiTypeId::LEATHERWORKER {
                        [
                            Block::CAULDRON,
                            Block::LAVA_CAULDRON,
                            Block::WATER_CAULDRON,
                            Block::POWDER_SNOW_CAULDRON,
                        ]
                        .contains(&state.block())
                    } else {
                        state.block().is_instance_of(BlockClass::LightningRodBlock)
                    };
                    if matches {
                        assert!(
                            by_state[index].replace(id).is_none(),
                            "POI state registered twice"
                        );
                        Some(state)
                    } else {
                        None
                    }
                })
                .collect();
            let max_tickets = if id == PoiTypeId::MEETING {
                32
            } else if [
                PoiTypeId::BEEHIVE,
                PoiTypeId::BEE_NEST,
                PoiTypeId::NETHER_PORTAL,
                PoiTypeId::LODESTONE,
                PoiTypeId::TEST_INSTANCE,
                PoiTypeId::LIGHTNING_ROD,
            ]
            .contains(&id)
            {
                0
            } else {
                1
            };
            types.push(PoiType {
                matching_states,
                max_tickets,
                valid_range: if id == PoiTypeId::MEETING { 6 } else { 1 },
            });
        }
        Self { types, by_state }
    }
    pub fn get(&self, id: PoiTypeId) -> &PoiType {
        &self.types[id.protocol_id() as usize]
    }
    pub fn for_state(&self, state: BlockState) -> Option<PoiTypeId> {
        self.by_state[state.id() as usize]
    }
    pub fn has_poi(&self, state: BlockState) -> bool {
        self.for_state(state).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_registry_matches_native_bootstrap_dump_exactly() {
        let registry = PoiTypes::builtin();
        let mut seen = Vec::new();
        for row in include_str!("../../../tests/java/level/poi-types.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            let id = PoiTypeId::from_name(fields[1]).expect("native registry key");
            assert_eq!(id.protocol_id(), fields[0].parse::<u16>().unwrap());
            let data = registry.get(id);
            assert_eq!(data.max_tickets, fields[2].parse::<i32>().unwrap(), "{row}");
            assert_eq!(data.valid_range, fields[3].parse::<i32>().unwrap(), "{row}");
            let states: Vec<u16> = fields[4]
                .split(',')
                .map(|value| value.parse().unwrap())
                .collect();
            assert_eq!(
                data.matching_states
                    .iter()
                    .map(|state| state.id())
                    .collect::<Vec<_>>(),
                states,
                "{}",
                id.name()
            );
            for state in &data.matching_states {
                assert_eq!(registry.for_state(*state), Some(id));
            }
            seen.push(id);
        }
        assert_eq!(seen, point_of_interest_type::ALL);
    }
}
