//! Daylight detector data is exactly the BlockEntity base. The block chooses
//! its ticker using the owning dimension's static skylight fact.
use super::{BlockEntity, BlockEntityData, BlockEntityRef};
use crate::level::{Level, LevelError};

/// Port of DaylightDetectorBlockEntity.
#[derive(Clone, Debug)]
pub struct DaylightDetectorBlockEntity {
    pub parent: BlockEntityData,
}
impl DaylightDetectorBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self { parent }
    }
}
pub(super) struct DaylightDetectorBehavior;
pub(super) static DAYLIGHT_DETECTOR_ENTITY: DaylightDetectorBehavior = DaylightDetectorBehavior;
impl BlockEntity for DaylightDetectorBehavior {
    fn tick(&self, receiver: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        crate::block::redstone::device::tick_daylight_detector(receiver, level)
    }
}

#[cfg(test)]
mod tests {
    use crate::block::entity::{create, load_static, new_block_entity};
    use crate::level::LevelError;
    use ferraloom_data::{Block, BlockState};
    use ferraloom_nbt::parse_compound;
    use ferraloom_util::BlockPos;

    #[test]
    fn native_crafter_constructor_factory_load_and_dimension_tickers_match() {
        let pos = BlockPos::new(8, 64, 8);
        let mut cases = 0;
        for row in include_str!("../../../tests/java/blocks/entities/factory-tickers.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            match fields[0] {
                "factory" => {
                    let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
                    let entity = match fields[2] {
                        "typed" => create(pos, state).unwrap(),
                        "factory" => new_block_entity(pos, state).unwrap().unwrap(),
                        _ => load_static(pos, state, &parse_compound(fields[3]).unwrap())
                            .unwrap()
                            .unwrap(),
                    };
                    assert_eq!(
                        entity.save_with_full_metadata().unwrap(),
                        parse_compound(fields[4]).unwrap(),
                        "{row}"
                    );
                    assert_eq!(
                        entity.has_server_ticker(None).unwrap(),
                        fields[5] == "true",
                        "{row}"
                    );
                }
                "daylight" => {
                    let sky = fields[2].parse().unwrap();
                    let state = BlockState::from_id(fields[3].parse().unwrap()).unwrap();
                    let entity = new_block_entity(pos, state).unwrap().unwrap();
                    assert_eq!(
                        entity.save_with_full_metadata().unwrap(),
                        parse_compound(fields[5]).unwrap(),
                        "{row}"
                    );
                    assert_eq!(
                        entity.has_server_ticker(Some(sky)).unwrap(),
                        fields[4] == "true",
                        "{row}"
                    );
                    assert_eq!(
                        entity.has_server_ticker(None),
                        Err(LevelError::UnsupportedLightQuery)
                    );
                }
                _ => panic!("unknown native case {row}"),
            }
            cases += 1;
        }
        assert_eq!(cases, 432);
        for (block, ticks) in [
            (Block::END_PORTAL, false),
            (Block::FURNACE, true),
            (Block::POTENT_SULFUR, false),
        ] {
            let entity = create(pos, block.default_state()).unwrap();
            assert_eq!(entity.has_server_ticker(None), Ok(ticks));
        }
    }
}

#[cfg(test)]
mod ticker_tests;
