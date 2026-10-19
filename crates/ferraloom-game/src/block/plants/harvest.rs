//! SweetBerryBushBlock and CaveVines harvest through canonical BLOCK_INTERACT loot.
use super::super::{BlockBehavior, InteractionResult, LevelAccessRef, UpdateFlags, UseContext};
use crate::{
    entity::player::ServerPlayer,
    level::{Level, LevelError},
};
use ferraloom_data::{
    BlockClass, BlockState, Item, properties as p,
    registries::{game_event::Id as GameEvent, sound_event::Id as SoundEvent},
};
use ferraloom_protocol::{Identifier, play::SoundSource};
use ferraloom_util::{BlockPos, Vec3, random::RandomSource};

pub struct Harvest;
pub static HARVEST: Harvest = Harvest;
impl BlockBehavior for Harvest {
    fn use_item_on(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let item = level
            .entity::<ServerPlayer>(context.player)?
            .get_item_in_hand(context.hand)
            .item();
        if state.get(p::AGE) != Some(3) && item == Item::BONE_MEAL {
            return Ok(InteractionResult::Pass);
        }
        super::super::BASE_BLOCK.use_item_on(state, pos, context, level)
    }
    fn use_without_item(
        &self,
        state: BlockState,
        pos: BlockPos,
        context: &UseContext,
        level: &mut Level,
    ) -> Result<InteractionResult, LevelError> {
        let berry = state.block().class() == BlockClass::SweetBerryBushBlock;
        if if berry {
            state.get(p::AGE).unwrap() <= 1
        } else {
            state.get(p::BERRIES) != Some(true)
        } {
            return Ok(InteractionResult::Pass);
        }
        let table = Identifier::parse(if berry {
            "minecraft:harvest/sweet_berry_bush"
        } else {
            "minecraft:harvest/cave_vine"
        })
        .expect("native harvest table");
        let receiver = level.block_entity_ref(pos)?;
        super::super::drops::drop_from_block_interact_loot_table(
            level,
            &table,
            pos,
            state,
            receiver,
            None,
            Some(context.player),
        )?;
        let pitch = if berry {
            0.8_f32 + level.random().next_float() * 0.4_f32
        } else {
            0.8_f32 + level.random().next_float() * (1.2_f32 - 0.8_f32)
        };
        level.play_sound(
            None,
            Vec3::at_center_of(pos.into()),
            if berry {
                SoundEvent::BLOCK_SWEET_BERRY_BUSH_PICK_BERRIES
            } else {
                SoundEvent::BLOCK_CAVE_VINES_PICK_BERRIES
            },
            SoundSource::Blocks,
            1.0,
            pitch,
        )?;
        let next = if berry {
            state.with(p::AGE, 1).unwrap()
        } else {
            state.with(p::BERRIES, false).unwrap()
        };
        LevelAccessRef::Server(level).set_block(pos, next, UpdateFlags::CLIENTS)?;
        level.game_event(
            GameEvent::BLOCK_CHANGE,
            Vec3::at_center_of(pos.into()),
            Some(context.player),
            Some(next),
        )?;
        Ok(InteractionResult::SUCCESS)
    }
}
