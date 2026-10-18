//! Audited plant shapes. The listed static overrides inspect constants/state only;
//! their native extracted shapes are exact. Offset-bearing shapes run the ported
//! position transform rather than treating the ZERO baseline as a live answer.
use super::super::{BlockBehavior, BlockCollisionContext, LevelReaderRef};
use crate::level::LevelError;
use ferraloom_data::{BlockClass, BlockState, properties as p};
use ferraloom_util::{BlockPos, VoxelShape, mth, shapes};
pub struct PlantShapes;
pub static PLANT_SHAPES: PlantShapes = PlantShapes;
fn column(width: f64, min: f64, max: f64) -> VoxelShape {
    let inset = (16.0 - width) / 32.0;
    shapes::create(
        inset,
        min / 16.0,
        inset,
        1.0 - inset,
        max / 16.0,
        1.0 - inset,
    )
}
/// Port of BlockBehaviour.Properties#offsetType(XZ). Preserve f32 division before
/// Java's double promotion for subtraction and multiplication.
fn offset(shape: VoxelShape, pos: BlockPos) -> VoxelShape {
    let seed = mth::get_seed(pos.x, 0, pos.z);
    let x = (f64::from((seed & 15) as f32 / 15.0) - 0.5) * 0.5;
    let z = (f64::from(((seed >> 8) & 15) as f32 / 15.0) - 0.5) * 0.5;
    shape.move_by(x, 0.0, z)
}
pub fn supports(class: BlockClass) -> bool {
    use BlockClass as C;
    matches!(
        class,
        C::FlowerBlock
            | C::WitherRoseBlock
            | C::EyeblossomBlock
            | C::BambooStalkBlock
            | C::BambooSaplingBlock
            | C::MangrovePropaguleBlock
            | C::TallGrassBlock
            | C::DryVegetationBlock
            | C::ShortDryGrassBlock
            | C::TallDryGrassBlock
            | C::NetherRootsBlock
            | C::NetherSproutsBlock
            | C::MushroomBlock
            | C::NetherFungusBlock
            | C::SaplingBlock
            | C::AzaleaBlock
            | C::CactusFlowerBlock
            | C::SeagrassBlock
            | C::TallSeagrassBlock
            | C::SmallDripleafBlock
            | C::SugarCaneBlock
            | C::BushBlock
            | C::LeafLitterBlock
            | C::FlowerBedBlock
            | C::SnowLayerBlock
            | C::MossyCarpetBlock
            | C::SeaPickleBlock
            | C::VineBlock
            | C::HangingRootsBlock
            | C::FireflyBushBlock
            | C::SporeBlossomBlock
            | C::BigDripleafStemBlock
            | C::BigDripleafBlock
            | C::PitcherCropBlock
            | C::CropBlock
            | C::TorchflowerCropBlock
            | C::BeetrootBlock
            | C::CactusBlock
            | C::StemBlock
            | C::AttachedStemBlock
            | C::NetherWartBlock
            | C::SweetBerryBushBlock
    )
}
impl BlockBehavior for PlantShapes {
    fn shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        use BlockClass as C;
        Ok(match state.block().class() {
            C::FlowerBlock | C::WitherRoseBlock | C::EyeblossomBlock => {
                offset(column(6.0, 0.0, 10.0), pos)
            }
            C::BambooStalkBlock => offset(
                column(
                    if state.get(p::LEAVES) == Some(p::Leaves::Large) {
                        10.0
                    } else {
                        6.0
                    },
                    0.0,
                    16.0,
                ),
                pos,
            ),
            C::BambooSaplingBlock => offset(column(8.0, 0.0, 12.0), pos),
            C::MangrovePropaguleBlock => {
                let age = if state.get(p::HANGING) == Some(true) {
                    state.get(p::AGE).unwrap()
                } else {
                    4
                };
                offset(
                    column(2.0, [13.0, 10.0, 7.0, 3.0, 0.0][usize::from(age)], 16.0),
                    pos,
                )
            }
            _ => state.cached_shapes().outline.get().clone(),
        })
    }
    fn collision_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        _context: &BlockCollisionContext<'_>,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Ok(if state.block().class() == BlockClass::BambooStalkBlock {
            offset(column(3.0, 0.0, 16.0), pos)
        } else {
            state.cached_shapes().collision.get().clone()
        })
    }
}
