//! Bonemeal feature lookup preserves vanilla's optional registry holders.
use super::super::{LevelAccessRef, LevelReaderRef};
use super::{bonemeal::inside_height, tree_grower::place_feature};
use crate::level::LevelError;
use ferraloom_data::{Block, BlockClass, BlockState};
use ferraloom_util::BlockPos;

pub(super) fn target(
    state: BlockState,
    pos: BlockPos,
    level: &LevelReaderRef<'_, '_>,
) -> Result<bool, LevelError> {
    if state.block().class() == BlockClass::NetherFungusBlock {
        let required = if state.block() == Block::WARPED_FUNGUS {
            Block::WARPED_NYLIUM
        } else {
            Block::CRIMSON_NYLIUM
        };
        return Ok(level.get_block_state(pos.below(1))?.block() == required
            && inside_height(level, pos.above(1)));
    }
    Ok(level.get_block_state(pos.above(1))?.is_air()
        && (state.block().class() != BlockClass::NyliumBlock || inside_height(level, pos.above(1))))
}

pub(super) fn perform(
    state: BlockState,
    pos: BlockPos,
    access: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let name = match state.block() {
        Block::WARPED_FUNGUS => "warped_fungus_planted",
        Block::CRIMSON_FUNGUS => "crimson_fungus_planted",
        Block::MOSS_BLOCK => "moss_patch_bonemeal",
        Block::PALE_MOSS_BLOCK => "pale_moss_patch_bonemeal",
        Block::WARPED_NYLIUM | Block::CRIMSON_NYLIUM => "nylium_bonemeal",
        _ => unreachable!("registered feature block"),
    };
    let origin = if state.block().class() == BlockClass::NetherFungusBlock {
        pos
    } else {
        pos.above(1)
    };
    if state.block().class() == BlockClass::NyliumBlock
        && !inside_height(&access.as_reader(), origin)
    {
        return Ok(());
    }
    let LevelAccessRef::Server(level) = access else {
        return Err(LevelError::UnsupportedWorldgenBehavior);
    };
    let id = name.parse().expect("builtin feature identifier");
    if level.context().features.features.contains_key(&id) {
        place_feature(level, &id, origin)?;
    }
    Ok(())
}

fn mushroom_feature(state: BlockState) -> ferraloom_worldgen::Identifier {
    if state.block() == Block::BROWN_MUSHROOM {
        "huge_brown_mushroom"
    } else {
        "huge_red_mushroom"
    }
    .parse()
    .expect("builtin feature identifier")
}

/// Port of MushroomBlock#isValidBonemealTarget. Only the typed huge-mushroom
/// feature exposes foliage radius; an unrelated feature is not a valid target.
pub(super) fn mushroom_target(
    state: BlockState,
    pos: BlockPos,
    access: &LevelAccessRef<'_, '_>,
) -> bool {
    let LevelAccessRef::Server(level) = access else {
        return false;
    };
    level
        .context()
        .features
        .features
        .get(&mushroom_feature(state))
        .and_then(|feature| feature.huge_mushroom_foliage_radius())
        .is_some_and(|radius| {
            inside_height(
                &LevelReaderRef::Server(level),
                pos.above(4_i32.wrapping_add(radius)),
            )
        })
}

/// Port of MushroomBlock#growMushroom, including native fluid replacement and
/// restoration on false. A failed feature's other writes are retained.
pub(super) fn grow_mushroom(
    state: BlockState,
    pos: BlockPos,
    access: &mut LevelAccessRef<'_, '_>,
) -> Result<(), LevelError> {
    let LevelAccessRef::Server(level) = access else {
        return Err(LevelError::UnsupportedWorldgenBehavior);
    };
    let id = mushroom_feature(state);
    if level.context().features.features.contains_key(&id) {
        level.remove_block(pos, false)?;
        if !place_feature(level, &id, pos)? {
            LevelAccessRef::Server(level).set_block(pos, state, super::super::UpdateFlags::ALL)?;
        }
    }
    Ok(())
}
