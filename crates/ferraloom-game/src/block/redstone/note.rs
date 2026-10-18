//! Port of NoteBlock's instrument selection, redstone edge and sound event.
use crate::{
    block::{BlockBehavior, LevelAccessRef, LevelReaderRef},
    entity::EntityRef,
    level::{Level, LevelError, events::BlockEvent, redstone::Orientation},
};
use ferraloom_data::{Block, BlockState, properties as p, registries::game_event::Id as GameEvent};
use ferraloom_protocol::{entity_data::Particle, play::SoundSource};
use ferraloom_util::{Axis, BlockPos, Direction, Vec3, random::RandomSource};

pub struct NoteBlock;
pub static NOTE: NoteBlock = NoteBlock;

/// Port of NoteBlock#setInstrument. Heads above take precedence; a head below
/// supplies the harp instrument, including when the space above is occupied.
pub(super) fn set_instrument(
    reader: &LevelReaderRef<'_, '_>,
    pos: BlockPos,
    state: BlockState,
) -> Result<BlockState, LevelError> {
    let above = reader
        .get_block_state(pos.above(1))?
        .block()
        .note_block_instrument();
    let instrument = if above.note_properties().works_above {
        above
    } else {
        let below = reader
            .get_block_state(pos.below(1))?
            .block()
            .note_block_instrument();
        if below.note_properties().works_above {
            p::Instrument::Harp
        } else {
            below
        }
    };
    Ok(state.with(p::INSTRUMENT, instrument).unwrap())
}
/// Port of NoteBlock#getPitchFromNote; the pow operation is double precision
/// and narrows once, before the sound call.
pub fn pitch_from_note(note: i32) -> f32 {
    2.0_f64.powf(f64::from(note.wrapping_sub(12)) / 12.0) as f32
}
fn play_note(
    level: &mut Level,
    pos: BlockPos,
    state: BlockState,
    source: Option<EntityRef>,
) -> Result<(), LevelError> {
    if state
        .get(p::INSTRUMENT)
        .unwrap()
        .note_properties()
        .works_above
        || level.get_block_state(pos.above(1))?.is_air()
    {
        level.block_event(BlockEvent {
            pos,
            block: state.block(),
            param_a: 0,
            param_b: 0,
        });
        level.game_event(
            GameEvent::NOTE_BLOCK_PLAY,
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
            source,
            None,
        )?;
    }
    Ok(())
}
impl BlockBehavior for NoteBlock {
    fn update_shape(
        &self,
        state: BlockState,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor: BlockState,
        level: &mut LevelAccessRef<'_, '_>,
    ) -> Result<BlockState, LevelError> {
        if direction.axis() == Axis::Y {
            set_instrument(&level.as_reader(), pos, state)
        } else {
            Ok(state)
        }
    }
    fn neighbor_changed(
        &self,
        state: BlockState,
        pos: BlockPos,
        _source: Block,
        _orientation: Option<Orientation>,
        _moved: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let signal = level.has_neighbor_signal(pos)?;
        if signal != state.get(p::POWERED).unwrap() {
            if signal {
                play_note(level, pos, state, None)?;
            }
            level.set_block_and_update(pos, state.with(p::POWERED, signal).unwrap())?;
        }
        Ok(())
    }
    fn trigger_event(
        &self,
        state: BlockState,
        pos: BlockPos,
        _a: i32,
        _b: i32,
        level: &mut Level,
    ) -> Result<bool, LevelError> {
        let properties = state.get(p::INSTRUMENT).unwrap().note_properties();
        let pitch = if properties.tunable {
            let note = state.get(p::NOTE).unwrap();
            level.add_particle(
                Particle::Note,
                Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y) + 1.2,
                    f64::from(pos.z) + 0.5,
                ),
                Vec3::new(f64::from(note) / 24.0, 0.0, 0.0),
            );
            pitch_from_note(i32::from(note))
        } else {
            1.0
        };
        if properties.custom_sound {
            if let Some(handle) = level.block_entity_ref(pos.above(1))?
                && level
                    .block_entity::<crate::block::entity::BlockEntityData>(handle)?
                    .kind
                    == crate::block::entity::BlockEntityType::SKULL
            {
                return Err(LevelError::Inventory("SkullBlockEntity#getNoteBlockSound"));
            }
            return Ok(false);
        }
        let seed = level.random().next_long();
        level.play_seeded_sound(
            None,
            Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ),
            properties.sound,
            SoundSource::Records,
            3.0,
            pitch,
            seed,
        )?;
        Ok(true)
    }
}
