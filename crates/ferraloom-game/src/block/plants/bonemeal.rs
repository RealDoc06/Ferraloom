//! Port of BonemealableBlock. Item behavior can query this family lookup without
//! storing mutable behavior objects or losing the world's canonical RNG stream.
use super::{
    super::{LevelAccessRef, LevelReaderRef, UpdateFlags},
    bamboo, crops, dripleaf, feature_bonemeal, grass_bonemeal, growing, hanging_moss, moss,
    multiface_spreader, nylium, pitcher, sapling, sea_pickle, spread, survival,
    tree_grower::TreeGrower,
};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockClass, BlockState, properties as p};
use ferraloom_util::{BlockPos, core::Plane, random::RandomSource};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BonemealSource {
    Interaction,
    Mob,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BonemealType {
    NeighborSpreader,
    Grower,
}
/// Canonical gameplay calls pass Level#getRandom in vanilla. These methods borrow
/// that stream through LevelAccessRef, allowing mutations to reenter block code.
pub trait BonemealableBlock: Sync {
    fn is_valid_bonemeal_target(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
        source: BonemealSource,
    ) -> Result<bool, LevelError>;
    fn is_bonemeal_success(
        &self,
        _state: BlockState,
        _pos: BlockPos,
        _level: &mut LevelAccessRef<'_, '_>,
        _source: BonemealSource,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    fn perform_bonemeal(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
        source: BonemealSource,
    ) -> Result<(), LevelError>;
    fn bonemeal_type(&self) -> BonemealType {
        BonemealType::Grower
    }
    fn particle_pos(&self, pos: BlockPos) -> BlockPos {
        match self.bonemeal_type() {
            BonemealType::NeighborSpreader => pos.above(1),
            BonemealType::Grower => pos,
        }
    }
}
pub struct PlantBonemeal {
    kind: BonemealType,
    particle_offset: i32,
}
static PLANT_BONEMEAL: PlantBonemeal = PlantBonemeal {
    kind: BonemealType::Grower,
    particle_offset: 0,
};
static NEIGHBOR_BONEMEAL: PlantBonemeal = PlantBonemeal {
    kind: BonemealType::NeighborSpreader,
    particle_offset: 1,
};
static ROOTED_BONEMEAL: PlantBonemeal = PlantBonemeal {
    kind: BonemealType::Grower,
    particle_offset: -1,
};
/// None for an unported class is distinct from not implementing vanilla's
/// interface; callers can test BlockClass::BonemealableBlock before dispatch.
pub fn bonemealable_behavior(block: Block) -> Option<&'static dyn BonemealableBlock> {
    use BlockClass as C;
    if matches!(
        block.class(),
        C::GrassBlock | C::NetherrackBlock | C::NyliumBlock | C::BonemealableFeaturePlacerBlock
    ) {
        return Some(&NEIGHBOR_BONEMEAL);
    }
    if matches!(block.class(), C::RootedDirtBlock | C::MangroveLeavesBlock) {
        return Some(&ROOTED_BONEMEAL);
    }
    matches!(
        block.class(),
        C::MushroomBlock
            | C::NetherFungusBlock
            | C::SaplingBlock
            | C::MangrovePropaguleBlock
            | C::AzaleaBlock
            | C::MossyCarpetBlock
            | C::BigDripleafBlock
            | C::BigDripleafStemBlock
            | C::SmallDripleafBlock
            | C::StemBlock
            | C::KelpBlock
            | C::KelpPlantBlock
            | C::WeepingVinesBlock
            | C::WeepingVinesPlantBlock
            | C::TwistingVinesBlock
            | C::TwistingVinesPlantBlock
            | C::PitcherCropBlock
            | C::SeaPickleBlock
            | C::HangingMossBlock
            | C::BambooStalkBlock
            | C::BambooSaplingBlock
            | C::FireflyBushBlock
            | C::CropBlock
            | C::CarrotBlock
            | C::PotatoBlock
            | C::BeetrootBlock
            | C::TorchflowerCropBlock
            | C::TallGrassBlock
            | C::ShortDryGrassBlock
            | C::TallDryGrassBlock
            | C::SweetBerryBushBlock
            | C::BushBlock
            | C::SeagrassBlock
            | C::CaveVinesBlock
            | C::CaveVinesPlantBlock
            | C::GlowLichenBlock
            | C::TallFlowerBlock
            | C::FlowerBedBlock
    )
    .then_some(&PLANT_BONEMEAL)
}
pub(super) fn inside_height(level: &LevelReaderRef<'_, '_>, pos: BlockPos) -> bool {
    match level {
        LevelReaderRef::Server(level) => {
            pos.y >= level.min_build_height() && pos.y <= level.max_build_height()
        }
        LevelReaderRef::PathNavigation(reader) => {
            pos.y >= reader.min_build_height() && pos.y <= reader.max_build_height()
        }
        LevelReaderRef::Worldgen(region) => {
            pos.y >= region.min_y() && pos.y < region.min_y() + region.height()
        }
    }
}
fn grown_grass(state: BlockState) -> BlockState {
    if state.block() == Block::FERN {
        Block::LARGE_FERN.default_state()
    } else {
        Block::TALL_GRASS.default_state()
    }
}
impl BonemealableBlock for PlantBonemeal {
    fn bonemeal_type(&self) -> BonemealType {
        self.kind
    }
    fn particle_pos(&self, pos: BlockPos) -> BlockPos {
        pos.above(self.particle_offset)
    }
    fn is_valid_bonemeal_target(
        &self,
        state: BlockState,
        pos: BlockPos,
        access: &mut LevelAccessRef<'_, '_>,
        _source: BonemealSource,
    ) -> Result<bool, LevelError> {
        use BlockClass as C;
        match state.block().class() {
            C::MushroomBlock => return Ok(feature_bonemeal::mushroom_target(state, pos, access)),
            C::MangrovePropaguleBlock => {
                return Ok(state.get(p::HANGING) != Some(true) || state.get(p::AGE) != Some(4));
            }
            C::SaplingBlock => {
                let LevelAccessRef::Server(level) = access else {
                    return Ok(false);
                };
                let grower = TreeGrower::for_block(state.block()).expect("sapling grower");
                if !grower.can_grow(state, pos, &mut LevelAccessRef::Server(level))? {
                    return Ok(false);
                }
                let height = grower
                    .minimum_height(&level.context().features)
                    .unwrap_or(0);
                return Ok(inside_height(
                    &LevelReaderRef::Server(level),
                    pos.above(height),
                ));
            }
            C::AzaleaBlock => {
                let LevelAccessRef::Server(level) = access else {
                    return Ok(false);
                };
                let height = TreeGrower::Azalea
                    .minimum_height(&level.context().features)
                    .unwrap_or(0);
                return Ok(inside_height(
                    &LevelReaderRef::Server(level),
                    pos.above(height.wrapping_add(2)),
                ) && LevelReaderRef::Server(level)
                    .get_fluid_state(pos.above(1))?
                    .empty);
            }
            _ => {}
        }
        let level = &access.as_reader();
        match state.block().class() {
            C::GrassBlock => {
                Ok(level.get_block_state(pos.above(1))?.is_air()
                    && inside_height(level, pos.above(1)))
            }
            C::NetherFungusBlock | C::NyliumBlock | C::BonemealableFeaturePlacerBlock => {
                feature_bonemeal::target(state, pos, level)
            }
            C::MangroveLeavesBlock => Ok(level.get_block_state(pos.below(1))?.is_air()),
            C::NetherrackBlock => nylium::netherrack_target(pos, level),
            C::RootedDirtBlock => {
                Ok(level.get_block_state(pos.below(1))?.is_air()
                    && inside_height(level, pos.below(1)))
            }
            C::SmallDripleafBlock | C::TallFlowerBlock | C::FlowerBedBlock => Ok(true),
            C::BigDripleafBlock | C::BigDripleafStemBlock => {
                let Some(head) = dripleaf::head_pos(state, pos, level)? else {
                    return Ok(false);
                };
                dripleaf::can_grow_into(level, head.above(1))
            }
            C::StemBlock => Ok(state.get(p::AGE) != Some(7)),
            C::KelpBlock
            | C::KelpPlantBlock
            | C::WeepingVinesBlock
            | C::WeepingVinesPlantBlock
            | C::TwistingVinesBlock
            | C::TwistingVinesPlantBlock => {
                let Some(head) = growing::head_pos(state, pos, level)? else {
                    return Ok(false);
                };
                let (block, _, direction) = growing::family(state.block());
                let forward = head.relative(direction, 1);
                Ok(
                    growing::can_grow_into(block, level.get_block_state(forward)?)
                        && inside_height(level, forward),
                )
            }
            C::PitcherCropBlock => {
                let Some((pos, lower)) = pitcher::lower_half(state, pos, level)? else {
                    return Ok(false);
                };
                pitcher::can_grow(lower, pos, lower.get(p::AGE).unwrap() + 1, level)
            }
            C::SeaPickleBlock => Ok(state.get(p::WATERLOGGED) == Some(true)
                && level.block_tags()?.contains_key(
                    ferraloom_data::tags::BlockTags::CORAL_BLOCKS,
                    level.get_block_state(pos.below(1))?.block(),
                )),
            C::MossyCarpetBlock => {
                Ok(state.get(p::BOTTOM) == Some(true)
                    && !moss::bonemeal_topper(level, pos)?.is_air())
            }

            C::HangingMossBlock => {
                let growth = hanging_moss::growth_position(state, pos, level)?;
                Ok(level.get_block_state(growth)?.is_air() && inside_height(level, growth))
            }

            C::BambooSaplingBlock => {
                Ok(level.get_block_state(pos.above(1))?.is_air()
                    && inside_height(level, pos.above(1)))
            }
            C::BambooStalkBlock => {
                let above = bamboo::height(level, pos, ferraloom_util::Direction::Up)?;
                let below = bamboo::height(level, pos, ferraloom_util::Direction::Down)?;
                let top = level.get_block_state(pos.above(above))?;
                let growth = pos.above(above + 1);
                Ok(above + below + 1 < 16
                    && top.get(p::STAGE) != Some(1)
                    && inside_height(level, growth)
                    && level.get_block_state(growth)?.is_air())
            }

            C::CropBlock
            | C::CarrotBlock
            | C::PotatoBlock
            | C::BeetrootBlock
            | C::TorchflowerCropBlock => {
                Ok(state.get(p::AGE).unwrap() < crops::max_age(state.block()))
            }
            C::SweetBerryBushBlock => Ok(state.get(p::AGE).unwrap() < 3),
            C::ShortDryGrassBlock => Ok(true),
            C::TallGrassBlock => Ok(level.can_survive(grown_grass(state), pos)?
                && level.get_block_state(pos.above(1))?.is_air()
                && inside_height(level, pos.above(1))),
            C::SeagrassBlock => Ok(level.get_block_state(pos.above(1))?.block() == Block::WATER),
            C::CaveVinesBlock | C::CaveVinesPlantBlock => Ok(state.get(p::BERRIES) == Some(false)),
            C::BushBlock | C::FireflyBushBlock | C::TallDryGrassBlock => {
                let target = if state.block().class() == C::TallDryGrassBlock {
                    Block::SHORT_DRY_GRASS.default_state()
                } else {
                    state
                };
                for &d in Plane::Horizontal.faces() {
                    let at = pos.relative(d, 1);
                    if level.get_block_state(at)?.is_air() && level.can_survive(target, at)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            C::GlowLichenBlock => {
                for from in ferraloom_util::Direction::VALUES {
                    for toward in ferraloom_util::Direction::VALUES {
                        if multiface_spreader::get_spread_from_face_toward_direction(
                            level, state, pos, from, toward,
                        )?
                        .is_some()
                        {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            _ => unreachable!("registered bonemeal class"),
        }
    }
    fn is_bonemeal_success(
        &self,
        state: BlockState,
        _pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
        _source: BonemealSource,
    ) -> Result<bool, LevelError> {
        if state.block().class() == BlockClass::MangrovePropaguleBlock
            && state.get(p::HANGING) == Some(true)
        {
            return Ok(state.get(p::AGE) != Some(4));
        }
        if matches!(
            state.block().class(),
            BlockClass::SaplingBlock | BlockClass::MangrovePropaguleBlock | BlockClass::AzaleaBlock
        ) {
            return Ok(f64::from(level.random()?.next_float()) < 0.45);
        }
        if matches!(
            state.block().class(),
            BlockClass::NetherFungusBlock | BlockClass::MushroomBlock
        ) {
            return Ok(f64::from(level.random()?.next_float()) < 0.4);
        }
        Ok(true)
    }
    fn perform_bonemeal(
        &self,
        state: BlockState,
        pos: BlockPos,
        level: &mut LevelAccessRef<'_, '_>,
        source: BonemealSource,
    ) -> Result<(), LevelError> {
        use BlockClass as C;
        match state.block().class() {
            C::FlowerBedBlock if state.get(p::FLOWER_AMOUNT).unwrap() < 4 => {
                level.set_block(
                    pos,
                    state
                        .with(p::FLOWER_AMOUNT, state.get(p::FLOWER_AMOUNT).unwrap() + 1)
                        .unwrap(),
                    UpdateFlags::CLIENTS,
                )?;
            }
            C::TallFlowerBlock | C::FlowerBedBlock => {
                let LevelAccessRef::Server(level) = level else {
                    return Err(LevelError::UnsupportedWorldgenBehavior);
                };
                super::super::drops::pop_resource(
                    level,
                    pos,
                    ferraloom_protocol::component::ItemStack::new(
                        state.block().as_item(),
                        1,
                        Default::default(),
                    ),
                )?;
            }
            C::GrassBlock => grass_bonemeal::perform(pos, level, source)?,
            C::MushroomBlock => feature_bonemeal::grow_mushroom(state, pos, level)?,
            C::NetherFungusBlock | C::NyliumBlock | C::BonemealableFeaturePlacerBlock => {
                feature_bonemeal::perform(state, pos, level)?;
            }
            C::MangrovePropaguleBlock
                if state.get(p::HANGING) == Some(true) && state.get(p::AGE) != Some(4) =>
            {
                level.set_block(
                    pos,
                    state.with(p::AGE, state.get(p::AGE).unwrap() + 1).unwrap(),
                    UpdateFlags::CLIENTS,
                )?;
            }
            C::SaplingBlock | C::MangrovePropaguleBlock | C::AzaleaBlock => {
                let LevelAccessRef::Server(level) = level else {
                    return Err(LevelError::UnsupportedWorldgenBehavior);
                };
                if state.block().class() == C::AzaleaBlock {
                    TreeGrower::Azalea.grow_tree(state, pos, level)?;
                } else {
                    sapling::advance_tree(state, pos, level)?;
                }
            }
            C::MangroveLeavesBlock => {
                level.set_block(
                    pos.below(1),
                    Block::MANGROVE_PROPAGULE
                        .default_state()
                        .with(p::HANGING, true)
                        .unwrap()
                        .with(p::AGE, 0)
                        .unwrap(),
                    UpdateFlags::CLIENTS,
                )?;
            }
            C::NetherrackBlock => nylium::spread_to_netherrack(pos, level)?,
            C::RootedDirtBlock => {
                level.set_block(
                    pos.below(1),
                    Block::HANGING_ROOTS.default_state(),
                    UpdateFlags::ALL,
                )?;
            }
            C::BigDripleafBlock | C::BigDripleafStemBlock | C::SmallDripleafBlock => {
                dripleaf::bonemeal(state, pos, level)?
            }
            C::StemBlock => {
                let age = (state.get(p::AGE).unwrap()
                    + (level.random()?.next_int_bound(4) + 2) as u8)
                    .min(7);
                let grown = state.with(p::AGE, age).unwrap();
                level.set_block(pos, grown, UpdateFlags::CLIENTS)?;
                if age == 7 {
                    spread::grow_stem(grown, pos, level)?;
                }
            }
            C::KelpBlock
            | C::KelpPlantBlock
            | C::WeepingVinesBlock
            | C::WeepingVinesPlantBlock
            | C::TwistingVinesBlock
            | C::TwistingVinesPlantBlock => {
                if let Some(head) = growing::head_pos(state, pos, &level.as_reader())? {
                    let state = level.get_block_state(head)?;
                    growing::bonemeal(state, head, level)?;
                }
            }
            C::PitcherCropBlock => {
                if let Some((pos, lower)) = pitcher::lower_half(state, pos, &level.as_reader())? {
                    pitcher::grow(lower, pos, level)?;
                }
            }
            C::SeaPickleBlock => sea_pickle::grow(state, pos, level)?,
            C::MossyCarpetBlock => {
                let topper = moss::bonemeal_topper(&level.as_reader(), pos)?;
                if !topper.is_air() {
                    level.set_block(pos.above(1), topper, UpdateFlags::ALL)?;
                }
            }

            C::HangingMossBlock => {
                let at = hanging_moss::growth_position(state, pos, &level.as_reader())?;
                if level.get_block_state(at)?.is_air() {
                    level.set_block(at, state.with(p::TIP, true).unwrap(), UpdateFlags::ALL)?;
                }
            }

            C::BambooSaplingBlock => bamboo::grow_sapling(pos, level)?,
            C::BambooStalkBlock => {
                let above = bamboo::height(&level.as_reader(), pos, ferraloom_util::Direction::Up)?;
                let below =
                    bamboo::height(&level.as_reader(), pos, ferraloom_util::Direction::Down)?;
                let count = 1 + level.random()?.next_int_bound(2);
                for offset in 0..count {
                    let above = above + offset;
                    let total = above + below + 1;
                    let at = pos.above(above);
                    let state = level.get_block_state(at)?;
                    let growth = at.above(1);
                    if total >= 16
                        || state.get(p::STAGE) == Some(1)
                        || !level.get_block_state(growth)?.is_air()
                        || !inside_height(&level.as_reader(), growth)
                    {
                        return Ok(());
                    }
                    bamboo::grow(state, at, total, level)?;
                }
            }

            C::CropBlock
            | C::CarrotBlock
            | C::PotatoBlock
            | C::BeetrootBlock
            | C::TorchflowerCropBlock => crops::grow_crops(state, pos, level)?,
            C::SweetBerryBushBlock => {
                level.set_block(
                    pos,
                    state
                        .with(p::AGE, (state.get(p::AGE).unwrap() + 1).min(3))
                        .unwrap(),
                    UpdateFlags::CLIENTS,
                )?;
            }
            C::ShortDryGrassBlock => {
                level.set_block(pos, Block::TALL_DRY_GRASS.default_state(), UpdateFlags::ALL)?;
            }
            C::TallGrassBlock => {
                survival::place_double_plant(level, grown_grass(state), pos, UpdateFlags::CLIENTS)?
            }
            C::SeagrassBlock => survival::place_double_plant(
                level,
                Block::TALL_SEAGRASS.default_state(),
                pos,
                UpdateFlags::CLIENTS,
            )?,
            C::CaveVinesBlock | C::CaveVinesPlantBlock => {
                level.set_block(
                    pos,
                    state.with(p::BERRIES, true).unwrap(),
                    UpdateFlags::CLIENTS,
                )?;
            }
            C::BushBlock | C::FireflyBushBlock | C::TallDryGrassBlock => {
                let target = if state.block().class() == C::TallDryGrassBlock {
                    Block::SHORT_DRY_GRASS.default_state()
                } else {
                    state
                };
                let mut directions: [ferraloom_util::Direction; 4] = Plane::Horizontal
                    .faces()
                    .try_into()
                    .expect("four horizontal faces");
                for i in (2..=directions.len()).rev() {
                    let j = level.random()?.next_int_bound(i as i32) as usize;
                    directions.swap(i - 1, j);
                }
                for d in directions {
                    let at = pos.relative(d, 1);
                    if level.get_block_state(at)?.is_air()
                        && level.as_reader().can_survive(target, at)?
                    {
                        level.set_block(at, target.block().default_state(), UpdateFlags::ALL)?;
                        break;
                    }
                }
            }
            C::GlowLichenBlock => {
                multiface_spreader::spread_using_level_random(level, state, pos)?;
            }
            _ => unreachable!("registered bonemeal class"),
        }
        Ok(())
    }
}
