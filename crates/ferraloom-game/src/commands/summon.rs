//! Port of SummonCommand. Construction and registration use the entity owner's
//! canonical loader at this exclusive command call site.
use super::{
    PreparedCommand,
    arguments::{ArgumentValue, coordinates::Coordinates},
    builtins::{self, CommandResult},
    exceptions::error,
    source::EntityAnchor,
};
use crate::entity::EntityRef;
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_data::EntityType;
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{chat::Component, play::Difficulty};
use ferraloom_util::{BlockPos, Vec3};
use std::fmt;

#[derive(Debug)]
pub enum SummonError<E> {
    Syntax(CommandSyntaxException),
    World(E),
}
impl<E: fmt::Display> fmt::Display for SummonError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => error.fmt(formatter),
            Self::World(error) => error.fmt(formatter),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for SummonError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Syntax(error) => error,
            Self::World(error) => error,
        })
    }
}
impl<E> From<CommandSyntaxException> for SummonError<E> {
    fn from(error: CommandSyntaxException) -> Self {
        Self::Syntax(error)
    }
}

/// The host lends the actual source level. A loaded tree is owned until its
/// native UUID preflight and pre-order registration; no entity decoder lives here.
pub trait SummonAccess {
    type Loaded;
    type Error: std::error::Error + 'static;
    fn eye_height(&self, entity: EntityRef) -> Result<f64, Self::Error>;
    fn difficulty(&self) -> Difficulty;
    fn allowed_in_peaceful(&self, kind: EntityType) -> bool;
    /// EntityType.loadEntityRecursive(COMMAND, ignoreChecks=false), invoking
    /// snapTo(position, loaded yaw/pitch) on each root/passenger after its load
    /// and before constructing its passengers. Unsupported factories are errors.
    fn load_entity_recursive(
        &mut self,
        tag: CompoundTag,
        position: Vec3,
    ) -> Result<Option<Self::Loaded>, Self::Error>;
    fn is_mob(&self, entity: &Self::Loaded) -> Result<bool, Self::Error>;
    /// Mob.finalizeSpawn with current local difficulty and COMMAND reason.
    fn finalize_spawn(&mut self, entity: &mut Self::Loaded) -> Result<(), Self::Error>;
    /// None means UUID preflight rejected the tree. Required failures remain
    /// errors; success returns the retained root handle, including a removed root
    /// that native registration declines without failing the command.
    fn try_add_fresh_entity_with_passengers(
        &mut self,
        entity: Self::Loaded,
    ) -> Result<Option<EntityRef>, Self::Error>;
    fn display_name(&self, entity: EntityRef) -> Result<Component, Self::Error>;
}

/// Port of SummonCommand#createEntity. Source coordinates use spawnable bounds,
/// which are independent of the dimension's ordinary build-height range.
pub fn create_entity<A: SummonAccess>(
    kind: EntityType,
    position: Vec3,
    nbt: &CompoundTag,
    finalize: bool,
    access: &mut A,
) -> Result<EntityRef, SummonError<A::Error>> {
    let pos = BlockPos::containing(position.x, position.y, position.z);
    if !(-30_000_000..30_000_000).contains(&pos.x)
        || !(-20_000_000..20_000_000).contains(&pos.y)
        || !(-30_000_000..30_000_000).contains(&pos.z)
    {
        return Err(error("commands.summon.invalidPosition", &[]).into());
    }
    if access.difficulty() == Difficulty::Peaceful && !access.allowed_in_peaceful(kind) {
        return Err(error("commands.summon.failed.peaceful", &[]).into());
    }
    let mut tag = nbt.clone();
    tag.insert("id", Tag::String(kind.name().into()));
    let mut entity = access
        .load_entity_recursive(tag, position)
        .map_err(SummonError::World)?
        .ok_or_else(|| error("commands.summon.failed", &[]))?;
    if finalize && access.is_mob(&entity).map_err(SummonError::World)? {
        access
            .finalize_spawn(&mut entity)
            .map_err(SummonError::World)?;
    }
    access
        .try_add_fresh_entity_with_passengers(entity)
        .map_err(SummonError::World)?
        .ok_or_else(|| error("commands.summon.failed.uuid", &[]).into())
}

/// ResourceArgument#getSummonableEntityType is evaluated before coordinates.
/// An explicitly supplied empty compound still suppresses Mob finalization.
pub fn run<A: SummonAccess>(
    command: &PreparedCommand,
    access: &mut A,
) -> Result<CommandResult, SummonError<A::Error>> {
    let id = command
        .string("entity")
        .expect("entity-type resource argument");
    let kind = EntityType::from_name(id).expect("registered entity-type argument");
    if !kind.data().summonable {
        return Err(error("entity.not_summonable", &[kind.name().into()]).into());
    }
    let position = match command.coordinates("pos") {
        Some(coordinates) => {
            let eye_height = if matches!(coordinates, Coordinates::Local { .. })
                && command.source.anchor == EntityAnchor::Eyes
            {
                command
                    .source
                    .entity
                    .map(|entity| access.eye_height(entity))
                    .transpose()
                    .map_err(SummonError::World)?
            } else {
                None
            };
            coordinates.position(&command.source, eye_height)
        }
        None => command.source.position,
    };
    let empty = CompoundTag::new();
    let nbt = match command.argument("nbt") {
        Some(ArgumentValue::Tag(Tag::Compound(tag))) => tag,
        None => &empty,
        _ => unreachable!("compound-tag argument"),
    };
    let entity = create_entity(
        kind,
        position,
        nbt,
        command.argument("nbt").is_none(),
        access,
    )?;
    let name = access.display_name(entity).map_err(SummonError::World)?;
    Ok(CommandResult::success(
        1,
        Component::translatable("commands.summon.success", vec![builtins::component(name)]),
        true,
    ))
}

#[cfg(test)]
mod tests;
