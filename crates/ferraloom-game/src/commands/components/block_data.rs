//! Port of BlockDataSource#getData. Reads never acquire an unloaded chunk.
use super::ResolutionError;
use crate::{
    commands::{
        CommandSourceStack,
        arguments::coordinates::{CoordinateKind, Coordinates},
        exceptions,
        source::EntityAnchor,
    },
    level::Level,
};
use ferraloom_brigadier::StringReader;
use ferraloom_nbt::CompoundTag;
use ferraloom_util::BlockPos;

/// Resolve the compiled block coordinates at the component's capture point and
/// save the canonical receiver with its id, position, components and payload.
pub fn block_data(
    level: &Level,
    source: &CommandSourceStack,
    coordinates: &str,
) -> Result<Vec<CompoundTag>, ResolutionError> {
    let mut reader = StringReader::new(coordinates);
    let position = Coordinates::parse(&mut reader, CoordinateKind::BlockPos)?;
    if reader.can_read() {
        return Err(exceptions::error(
            "command.trailing_data",
            &[format!(
                "{}[{}]",
                reader.slice(0, reader.cursor()),
                reader.slice(reader.cursor(), reader.total_length())
            )],
        )
        .into());
    }
    let eye_height =
        if matches!(position, Coordinates::Local { .. }) && source.anchor == EntityAnchor::Eyes {
            source
                .entity
                .map(|entity| {
                    level
                        .entity_state(entity)
                        .map(|state| f64::from(state.as_entity().eye_height))
                        .map_err(ResolutionError::BlockData)
                })
                .transpose()?
        } else {
            None
        };
    let position = position.position(source, eye_height);
    let pos = BlockPos::containing(position.x, position.y, position.z);
    if !level.is_loaded(pos) {
        return Ok(Vec::new());
    }
    let Some(handle) = level
        .block_entity_ref(pos)
        .map_err(ResolutionError::BlockData)?
    else {
        return Ok(Vec::new());
    };
    let receiver = level
        .block_entity_state(handle)
        .map_err(ResolutionError::BlockData)?;
    Ok(vec![
        receiver
            .save_with_full_metadata()
            .map_err(ResolutionError::Serialization)?,
    ])
}

#[cfg(test)]
mod tests;
