//! BaseFireBlock/FireBlock lifecycle and LavaFluid's native ignition walk.
use crate::{
    block::{BlockBehavior, BlockMethod, LevelReaderRef, UPDATE_LIMIT, UpdateFlags},
    entity::{
        EntityData, EntityRef, EntityState,
        inside_block::{InsideBlockAction, InsideBlockEffectType},
    },
    level::{Level, LevelError, environment::AttributeKey},
};
use ferraloom_data::{Block, BlockClass, BlockState, properties as p, tags::BlockTags};
use ferraloom_util::{Axis, BlockPos, Direction, Vec3, VoxelShape, random::RandomSource};

pub(super) struct Fire;
pub(super) static FIRE: Fire = Fire;
const INCREASED_FIRE_BURNOUT: AttributeKey<bool> = AttributeKey::new(
    ferraloom_data::registries::environment_attribute::Id::GAMEPLAY_INCREASED_FIRE_BURNOUT,
);
pub(super) fn supports(block: Block, method: BlockMethod) -> bool {
    matches!(
        block.class(),
        BlockClass::FireBlock | BlockClass::SoulFireBlock
    ) && (matches!(
        method,
        BlockMethod::OnPlace
            | BlockMethod::EntityInside
            | BlockMethod::OutlineShape
            | BlockMethod::PlayerWillDestroy
    ) || method == BlockMethod::Tick && block.class() == BlockClass::FireBlock)
}

fn schedule_fire(pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    let delay = 30 + level.random().next_int_bound(10);
    level.schedule_block_tick_after(pos, Block::FIRE, delay)
}
fn state_with_age(pos: BlockPos, age: u8, level: &Level) -> Result<BlockState, LevelError> {
    let state = super::fire_powder::fire_state(pos, &LevelReaderRef::Server(level))?;
    Ok(if state.block() == Block::FIRE {
        state.with(p::AGE, age).unwrap()
    } else {
        state
    })
}
fn valid_fire_location(pos: BlockPos, level: &Level) -> Result<bool, LevelError> {
    for side in Direction::VALUES {
        if level
            .get_block_state(pos.relative(side, 1))?
            .can_burn_in_fire()
        {
            return Ok(true);
        }
    }
    Ok(false)
}
fn near_rain(pos: BlockPos, level: &Level) -> Result<bool, LevelError> {
    for at in [pos, pos.west(1), pos.east(1), pos.north(1), pos.south(1)] {
        if level.is_raining_at(at)? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn ignite_odds(pos: BlockPos, level: &Level) -> Result<i32, LevelError> {
    if !level.get_block_state(pos)?.is_air() {
        return Ok(0);
    }
    let mut odds = 0;
    for side in Direction::VALUES {
        odds = odds.max(i32::from(
            level
                .get_block_state(pos.relative(side, 1))?
                .fire_ignite_odds(),
        ));
    }
    Ok(odds)
}

/// Port of TntBlock#prime's source-less overload, used by FireBlock#checkBurnOut.
fn prime_tnt(pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    if !level.rules().tnt_explodes {
        return Ok(());
    }
    let at = Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    );
    let tnt = crate::entity::item::PrimedTnt::new_at(level, at, None)?;
    let handle = level.next_entity_ref();
    level.add_entity(handle, EntityState::PrimedTnt(Box::new(tnt)))?;
    level.play_sound(
        None,
        at,
        ferraloom_data::registries::sound_event::Id::ENTITY_TNT_PRIMED,
        ferraloom_protocol::play::SoundSource::Blocks,
        1.0,
        1.0,
    )?;
    level.game_event(
        ferraloom_data::registries::game_event::Id::PRIME_FUSE,
        Vec3::new(at.x, at.y + 0.5, at.z),
        None,
        None,
    )
}
/// Port of FireBlock#checkBurnOut. Capture the old block after the first random
/// draw and before callbacks, so TNT ignition survives its synchronous removal.
fn check_burn_out(
    pos: BlockPos,
    chance: i32,
    age: u8,
    level: &mut Level,
) -> Result<(), LevelError> {
    let odds = i32::from(level.get_block_state(pos)?.fire_burn_odds());
    if level.random().next_int_bound(chance) < odds {
        let old = level.get_block_state(pos)?;
        if level.random().next_int_bound(i32::from(age) + 10) < 5 && !level.is_raining_at(pos)? {
            let age = (i32::from(age) + level.random().next_int_bound(5) / 4).min(15) as u8;
            let state = state_with_age(pos, age, level)?;
            level.set_block_and_update(pos, state)?;
        } else {
            level.remove_block(pos, false)?;
        }
        if old.block().is_instance_of(BlockClass::TntBlock) {
            prime_tnt(pos, level)?;
        }
    }
    Ok(())
}

impl BlockBehavior for Fire {
    /// Port of BaseFireBlock#onPlace and FireBlock#onPlace. Fire still schedules
    /// after the base creates a portal or removes an unsupported fire cell.
    fn on_place(
        &self,
        state: BlockState,
        pos: BlockPos,
        old: BlockState,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if old.block() != state.block() {
            let dimension = level.dimension_key()?;
            if dimension.namespace() == "minecraft"
                && matches!(dimension.path(), "overworld" | "the_nether")
                && let Some(shape) = super::portal_shape::PortalShape::find_empty(
                    pos,
                    Axis::X,
                    &LevelReaderRef::Server(level),
                )?
            {
                shape.create_portal_blocks(level)?;
            } else if !super::fire_powder::FIRE_POWDER.can_survive(
                state,
                pos,
                &LevelReaderRef::Server(level),
            )? {
                level.remove_block(pos, false)?;
            }
        }
        if state.block() == Block::FIRE {
            schedule_fire(pos, level)?;
        }
        Ok(())
    }
    /// Port of FireBlock#tick, retaining the pre-mutation age for all burnout
    /// calculations and east/west/down/up/north/south neighbor processing order.
    fn tick(
        &self,
        mut state: BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        schedule_fire(pos, level)?;
        if !level.can_spread_fire_around(pos)? {
            return Ok(());
        }
        if !super::fire_powder::FIRE_POWDER.can_survive(
            state,
            pos,
            &LevelReaderRef::Server(level),
        )? {
            level.remove_block(pos, false)?;
        }
        let below = level.get_block_state(pos.below(1))?;
        let infinite = level.is_infiniburn(below)?;
        let age = state.get(p::AGE).unwrap();
        if !infinite
            && level.is_raining()
            && near_rain(pos, level)?
            && level.random().next_float() < 0.2_f32 + f32::from(age) * 0.03_f32
        {
            level.remove_block(pos, false)?;
            return Ok(());
        }
        let new_age = (i32::from(age) + level.random().next_int_bound(3) / 2).min(15) as u8;
        if age != new_age {
            state = state.with(p::AGE, new_age).unwrap();
            level.set_block(pos, state, UpdateFlags::NONE, UPDATE_LIMIT)?;
        }
        if !infinite {
            if !valid_fire_location(pos, level)? {
                let below_pos = pos.below(1);
                let below = level.get_block_state(below_pos)?;
                if !LevelReaderRef::Server(level).is_face_sturdy(below, below_pos, Direction::Up)?
                    || age > 3
                {
                    level.remove_block(pos, false)?;
                }
                return Ok(());
            }
            if age == 15
                && level.random().next_int_bound(4) == 0
                && !level.get_block_state(pos.below(1))?.can_burn_in_fire()
            {
                level.remove_block(pos, false)?;
                return Ok(());
            }
        }
        let increased = level.get_value(INCREASED_FIRE_BURNOUT, pos)?;
        let extra = if increased { -50 } else { 0 };
        for (side, chance) in [
            (Direction::East, 300),
            (Direction::West, 300),
            (Direction::Down, 250),
            (Direction::Up, 250),
            (Direction::North, 300),
            (Direction::South, 300),
        ] {
            check_burn_out(pos.relative(side, 1), chance + extra, age, level)?;
        }
        for x in -1..=1 {
            for z in -1..=1 {
                for y in -1..=4 {
                    if x == 0 && y == 0 && z == 0 {
                        continue;
                    }
                    let rate = 100 + if y > 1 { (y - 1) * 100 } else { 0 };
                    let at = pos.offset(x, y, z);
                    let ignite = ignite_odds(at, level)?;
                    if ignite > 0 {
                        let mut odds = (ignite + 40 + i32::from(level.difficulty() as u8) * 7)
                            / (i32::from(age) + 30);
                        if increased {
                            odds /= 2;
                        }
                        if odds > 0
                            && level.random().next_int_bound(rate) <= odds
                            && (!level.is_raining() || !near_rain(at, level)?)
                        {
                            let age = (i32::from(age) + level.random().next_int_bound(5) / 4)
                                .min(15) as u8;
                            let state = state_with_age(at, age, level)?;
                            level.set_block_and_update(at, state)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    /// Port of BaseFireBlock#entityInside. Deferred ignition, freeze clearing
    /// and damage share the entity core's canonical movement-effect collector.
    fn entity_inside(
        &self,
        state: BlockState,
        _pos: BlockPos,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let effects = &mut level
            .entity_mut::<EntityData>(entity)?
            .inside_effect_collector;
        effects.apply(InsideBlockEffectType::ClearFreeze);
        effects.apply(InsideBlockEffectType::FireIgnite);
        effects.run_after(
            InsideBlockEffectType::FireIgnite,
            InsideBlockAction::InFireHurt(if state.block() == Block::SOUL_FIRE {
                2.0
            } else {
                1.0
            }),
        );
        Ok(())
    }
    /// FireBlock and BaseFireBlock's shapes depend only on the state's face
    /// properties; use the extracted native shape, including its representation.
    fn shape(
        &self,
        state: BlockState,
        _pos: BlockPos,
        _level: &LevelReaderRef<'_, '_>,
    ) -> Result<VoxelShape, LevelError> {
        Ok(state.cached_shapes().outline.get().clone())
    }
    fn player_will_destroy(
        &self,
        state: BlockState,
        pos: BlockPos,
        player: EntityRef,
        level: &mut Level,
    ) -> Result<BlockState, LevelError> {
        level.level_event(None, 1009, pos, 0)?;
        crate::block::BASE_BLOCK.player_will_destroy(state, pos, player, level)
    }
}

fn is_flammable(pos: BlockPos, level: &Level) -> Result<bool, LevelError> {
    if pos.y >= level.min_build_height()
        && pos.y <= level.max_build_height()
        && !level.has_chunk_at(pos)
    {
        return Ok(false);
    }
    Ok(level.get_block_state(pos)?.ignited_by_lava())
}
fn flammable_neighbors(pos: BlockPos, level: &Level) -> Result<bool, LevelError> {
    for side in Direction::VALUES {
        if is_flammable(pos.relative(side, 1), level)? {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Port of LavaFluid#randomTick. Use the level's same random stream through
/// synchronous setBlock callbacks, including FireBlock's extra scheduling draw.
pub(crate) fn lava_random_tick(pos: BlockPos, level: &mut Level) -> Result<(), LevelError> {
    if !level.can_spread_fire_around(pos)? {
        return Ok(());
    }
    let passes = level.random().next_int_bound(3);
    if passes > 0 {
        let mut at = pos;
        for _ in 0..passes {
            let x = level.random().next_int_bound(3) - 1;
            let z = level.random().next_int_bound(3) - 1;
            at = at.offset(x, 1, z);
            if !level.is_loaded(at) {
                return Ok(());
            }
            let state = level.get_block_state(at)?;
            if state.is_air() {
                if flammable_neighbors(at, level)? {
                    let fire = super::fire_powder::fire_state(at, &LevelReaderRef::Server(level))?;
                    level.set_block_and_update(at, fire)?;
                    return Ok(());
                }
            } else if level
                .block_tags()?
                .contains_key(BlockTags::BLOCKS_LAVA_FIRE_SPREAD, state.block())
            {
                return Ok(());
            }
        }
    } else {
        for _ in 0..3 {
            let x = level.random().next_int_bound(3) - 1;
            let z = level.random().next_int_bound(3) - 1;
            let at = pos.offset(x, 0, z);
            if !level.is_loaded(at) {
                return Ok(());
            }
            if level.get_block_state(at.above(1))?.is_air() && is_flammable(at, level)? {
                // Native passes the support position to getState, not its above.
                let fire = super::fire_powder::fire_state(at, &LevelReaderRef::Server(level))?;
                level.set_block_and_update(at.above(1), fire)?;
            }
        }
    }
    Ok(())
}
