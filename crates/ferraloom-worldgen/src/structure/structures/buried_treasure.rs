use crate::structure::{BoundingBox, piece::StructurePieceData};
use crate::{
    Error,
    feature::FeatureContext,
    region::{Heightmap, WorldGenRegion},
};
use ferraloom_data::{Block, BlockState};
use ferraloom_nbt::CompoundTag;
use ferraloom_util::{Direction, Rotation};

/// Port of BuriedTreasurePieces.BuriedTreasurePiece.
#[derive(Clone, Debug)]
pub struct BuriedTreasurePiece {
    pub data: StructurePieceData,
}
impl BuriedTreasurePiece {
    pub fn new(position: [i32; 3]) -> Self {
        Self {
            data: StructurePieceData::new(BoundingBox::point(position), 0, None),
        }
    }
    pub fn to_nbt(&self) -> CompoundTag {
        self.data
            .to_nbt(&"minecraft:btp".parse().expect("built-in piece id"))
    }
    pub fn from_nbt(tag: &CompoundTag) -> Result<Self, Error> {
        Ok(Self {
            data: StructurePieceData::from_nbt(tag)?,
        })
    }
    pub fn post_process(
        &mut self,
        context: &mut FeatureContext<'_, '_>,
        bounds: BoundingBox,
    ) -> Result<(), Error> {
        let x = self.data.bounds.min[0];
        let z = self.data.bounds.min[2];
        let mut p = [
            x,
            context.region.height_at(Heightmap::OceanFloorWg, x, z)?,
            z,
        ];
        while p[1] > context.region.min_y() {
            let current = context.region.block(p)?;
            let below = context.region.block([x, p[1] - 1, z])?;
            if matches!(
                below.block(),
                Block::SANDSTONE | Block::STONE | Block::ANDESITE | Block::GRANITE | Block::DIORITE
            ) {
                let soft = if !current.is_air() && !is_liquid(current) {
                    current
                } else {
                    Block::SAND.default_state()
                };
                for direction in Direction::VALUES {
                    let neighbor = relative(p, direction);
                    let state = context.region.block(neighbor)?;
                    if state.is_air() || is_liquid(state) {
                        let below_neighbor =
                            context.region.block(relative(neighbor, Direction::Down))?;
                        let fill = if (below_neighbor.is_air() || is_liquid(below_neighbor))
                            && direction != Direction::Up
                        {
                            below
                        } else {
                            soft
                        };
                        context.region.set_block(neighbor, fill, 3)?;
                    }
                }
                self.data.bounds = BoundingBox::point(p);
                if bounds.contains(p) && context.region.block(p)?.block() != Block::CHEST {
                    let state = reorient_chest(context.region, p)?;
                    context.region.set_block(p, state, 2)?;
                    // The adapter resolves the pending chest and consumes its
                    // loot seed. Unsupported game-backed cases stay explicit.
                    context.region.set_loot_table(
                        p,
                        &"minecraft:chests/buried_treasure".parse()?,
                        context.random,
                    )?;
                }
                return Ok(());
            }
            p[1] -= 1;
        }
        Ok(())
    }
}
fn relative(p: [i32; 3], direction: Direction) -> [i32; 3] {
    let d = direction.step();
    [
        p[0].wrapping_add(d.x),
        p[1].wrapping_add(d.y),
        p[2].wrapping_add(d.z),
    ]
}
fn is_liquid(state: BlockState) -> bool {
    matches!(state.block(), Block::WATER | Block::LAVA)
}
/// Port of StructurePiece#reorient, using the extracted isSolidRender fact.
pub(in crate::structure) fn reorient_chest(
    region: &WorldGenRegion<'_>,
    p: [i32; 3],
) -> Result<BlockState, Error> {
    let mut solid = None;
    for direction in [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ] {
        let state = region.block(relative(p, direction))?;
        if state.block() == Block::CHEST {
            return Ok(Block::CHEST.default_state());
        }
        if state.data().solid_render {
            if solid.is_some() {
                solid = None;
                break;
            }
            solid = Some(direction);
        }
    }
    let facing = if let Some(solid) = solid {
        solid.opposite()
    } else {
        let mut facing = Direction::North;
        if region.block(relative(p, facing))?.data().solid_render {
            facing = facing.opposite();
        }
        if region.block(relative(p, facing))?.data().solid_render {
            facing = facing.clockwise();
        }
        if region.block(relative(p, facing))?.data().solid_render {
            facing = facing.opposite();
        }
        facing
    };
    Ok(Block::CHEST.default_state().rotate(match facing {
        Direction::North => Rotation::None,
        Direction::East => Rotation::Clockwise90,
        Direction::South => Rotation::Clockwise180,
        Direction::West => Rotation::CounterClockwise90,
        _ => unreachable!("horizontal chest facing"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        feature::FeatureRegistry,
        region::{ChunkStatus, RegionChunk, WorldgenBlockBehavior},
        structure::processor::tests::generator,
    };
    use ferraloom_util::random::{Algorithm, RandomSource, WorldgenRandom};
    use serde_json::Value;
    use std::io::Read;
    fn point(value: &Value) -> [i32; 3] {
        std::array::from_fn(|i| value[i].as_i64().unwrap() as i32)
    }
    // The Java accessor never instantiates block entities, even after chest
    // writes. Ignore pending NBT placeholders and consume no loot RNG here;
    // real chest entity construction and loot assignment stay game-backed.
    struct AbsentBlockEntities;
    impl WorldgenBlockBehavior for AbsentBlockEntities {
        fn has_block_entity(&self, _state: BlockState) -> bool {
            false
        }
        fn post_process_pos(
            &self,
            _state: BlockState,
            _region: &WorldGenRegion<'_>,
            _p: [i32; 3],
        ) -> Result<Option<[i32; 3]>, Error> {
            Ok(None)
        }
        fn set_loot_table(
            &self,
            _region: &mut WorldGenRegion<'_>,
            _p: [i32; 3],
            _table: &crate::Identifier,
            _random: &mut WorldgenRandom,
        ) -> Result<(), Error> {
            Ok(())
        }
    }
    #[test]
    fn bounded_buried_treasure_fill_and_piece_mutation_match_vanilla() {
        let (_, generator) = generator();
        let features = FeatureRegistry::default();
        let mut text = String::new();
        flate2::read::GzDecoder::new(&include_bytes!("../testdata/buried_treasure.json.gz")[..])
            .read_to_string(&mut text)
            .unwrap();
        let cases: Value = serde_json::from_str(&text).unwrap();
        for case in cases.as_array().unwrap() {
            let mut chunk = generator.new_chunk([0, 0]);
            for entry in case["initial"].as_array().unwrap() {
                let p = point(&entry["pos"]);
                chunk.set_block(
                    p[0] as u8,
                    p[1],
                    p[2] as u8,
                    BlockState::from_id(entry["state"].as_u64().unwrap() as u16).unwrap(),
                );
            }
            // The Java mock supplies this fixed, already-primed WG heightmap.
            chunk.ocean_floor.fill(26);
            let mut chunks = [RegionChunk::new(chunk, ChunkStatus::Terrain)];
            let mut region = WorldGenRegion::new(
                &mut chunks,
                [0, 0],
                &[ChunkStatus::Terrain],
                0,
                None,
                &AbsentBlockEntities,
                0,
                63,
            )
            .unwrap();
            let mut random =
                WorldgenRandom::new(Algorithm::Legacy.new_instance(case["seed"].as_i64().unwrap()));
            let bounds = &case["bounds"];
            let bounds = BoundingBox::new(
                point(bounds),
                std::array::from_fn(|i| bounds[i + 3].as_i64().unwrap() as i32),
            );
            let mut piece = BuriedTreasurePiece::new([9, 90, 9]);
            piece
                .post_process(
                    &mut FeatureContext {
                        region: &mut region,
                        random: &mut random,
                        origin: [0, 0, 0],
                        generator: &generator,
                        registry: &features,
                    },
                    bounds,
                )
                .unwrap();
            assert_eq!(
                piece.to_nbt(),
                ferraloom_nbt::parse_compound(case["nbt"].as_str().unwrap()).unwrap(),
                "mode={}",
                case["mode"]
            );
            assert_eq!(random.next_int(), case["next"].as_i64().unwrap() as i32);
            for entry in case["writes"].as_array().unwrap() {
                assert_eq!(
                    region.block(point(&entry["pos"])).unwrap().id(),
                    entry["state"].as_u64().unwrap() as u16,
                    "mode={}",
                    case["mode"]
                );
            }
            let saved = piece.to_nbt();
            assert_eq!(
                BuriedTreasurePiece::from_nbt(&saved).unwrap().to_nbt(),
                saved
            );
        }
    }
}
