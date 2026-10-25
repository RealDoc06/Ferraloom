//! Port of ParticleArgument; persistent options use the shared ParticleTypes codec.
use super::{identifier, nbt};
use crate::commands::{
    context::CommandBuildContext,
    exceptions::{error, error_at},
};
use ferraloom_brigadier::{CommandSyntaxException, StringReader};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::entity_data::Particle;

pub fn parse(
    reader: &mut StringReader,
    context: &CommandBuildContext,
) -> Result<Particle, CommandSyntaxException> {
    let id = identifier(reader)?;
    if !context.contains("minecraft:particle_type", &id) {
        return Err(error_at(reader, "particle.notFound", &[id]));
    }
    let options = if reader.can_read() && reader.peek() == b'{' as u16 {
        nbt(reader, false)?
    } else {
        Tag::Compound(CompoundTag::new())
    };
    crate::particles::codec::decode_options(&id, &options)
        .map_err(|message| error("particle.invalidOptions", &[message.to_string()]))
}
