//! Port of PlaceCommand. The host lends the canonical template manager and level.
use super::{
    PreparedCommand,
    arguments::{ArgumentValue, coordinates::Coordinates},
    builtins::{self, CommandResult},
    exceptions::error,
    source::EntityAnchor,
};
use crate::entity::EntityRef;
use ferraloom_brigadier::CommandSyntaxException;
use ferraloom_protocol::{Identifier, chat::Component};
use ferraloom_util::{BlockPos, ChunkPos, Mirror, Rotation, random::Random};
use ferraloom_worldgen::structure::{
    processor::StructureProcessor,
    template::{PlacementRandom, StructurePlaceSettings},
};
use std::fmt;

#[derive(Debug)]
pub enum PlaceError<E> {
    Syntax(CommandSyntaxException),
    World(E),
}
impl<E: fmt::Display> fmt::Display for PlaceError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(error) => error.fmt(formatter),
            Self::World(error) => error.fmt(formatter),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for PlaceError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Syntax(error) => error,
            Self::World(error) => error,
        })
    }
}
impl<E> From<CommandSyntaxException> for PlaceError<E> {
    fn from(error: CommandSyntaxException) -> Self {
        Self::Syntax(error)
    }
}

pub trait TemplateAccess {
    type Template;
    type Error: std::error::Error + 'static;
    fn eye_height(&self, entity: EntityRef) -> Result<f64, Self::Error>;
    fn has_chunk_at(&self, pos: BlockPos) -> Result<bool, Self::Error>;
    fn in_world_bounds(&self, pos: BlockPos) -> bool;
    fn is_loaded(&self, pos: BlockPos) -> Result<bool, Self::Error>;
    /// None includes native manager lookup failure, including an invalid file path.
    fn template(&mut self, id: &Identifier) -> Result<Option<Self::Template>, Self::Error>;
    fn size(&self, template: &Self::Template) -> [i32; 3];
    /// StructureBlockEntity#createRandom: a fresh LegacyRandomSource, using
    /// Util.getMillis only for seed zero. It never draws from Level.random.
    fn create_random(&mut self, seed: i64) -> Result<Random, Self::Error>;
    fn place_template(
        &mut self,
        template: &Self::Template,
        position: BlockPos,
        settings: StructurePlaceSettings,
        processors: &[StructureProcessor],
        random: Random,
        flags: u32,
    ) -> Result<bool, Self::Error>;
}

/// Port of PlaceCommand#checkLoaded. ChunkPos.rangeClosed visits X first and
/// checks world positions at Y=0; the unrotated size's inclusive end is native.
fn check_loaded<A: TemplateAccess>(
    access: &A,
    from: ChunkPos,
    to: ChunkPos,
) -> Result<(), PlaceError<A::Error>> {
    let dx = if from.x < to.x { 1 } else { -1 };
    let dz = if from.z < to.z { 1 } else { -1 };
    let mut z = from.z;
    loop {
        let mut x = from.x;
        loop {
            if !access
                .is_loaded(BlockPos::new(x << 4, 0, z << 4))
                .map_err(PlaceError::World)?
            {
                return Err(error("argument.pos.unloaded", &[]).into());
            }
            if x == to.x {
                break;
            }
            x += dx;
        }
        if z == to.z {
            return Ok(());
        }
        z += dz;
    }
}

fn position<A: TemplateAccess>(
    command: &PreparedCommand,
    access: &A,
) -> Result<BlockPos, PlaceError<A::Error>> {
    let Some(coordinates) = command.coordinates("pos") else {
        let p = command.source.position;
        return Ok(BlockPos::containing(p.x, p.y, p.z));
    };
    let eye_height = if matches!(coordinates, Coordinates::Local { .. })
        && command.source.anchor == EntityAnchor::Eyes
    {
        command
            .source
            .entity
            .map(|entity| access.eye_height(entity))
            .transpose()
            .map_err(PlaceError::World)?
    } else {
        None
    };
    let p = coordinates.position(&command.source, eye_height);
    let pos = BlockPos::containing(p.x, p.y, p.z);
    // BlockPosArgument#getLoadedBlockPos runs before the manager lookup only
    // when the position argument exists; the implicit source position skips it.
    if !access.has_chunk_at(pos).map_err(PlaceError::World)? {
        Err(error("argument.pos.unloaded", &[]).into())
    } else if !access.in_world_bounds(pos) {
        Err(error("argument.pos.outofworld", &[]).into())
    } else {
        Ok(pos)
    }
}

/// Port of PlaceCommand#placeTemplate. The two seed-derived streams are created
/// separately, and integrity=1 retains positional palette selection.
pub fn template<A: TemplateAccess>(
    command: &PreparedCommand,
    access: &mut A,
) -> Result<CommandResult, PlaceError<A::Error>> {
    let pos = position(command, access)?;
    let id: Identifier = command
        .string("template")
        .expect("template identifier argument")
        .parse()
        .expect("validated template identifier");
    let Some(template) = access.template(&id).map_err(PlaceError::World)? else {
        return Err(error("commands.place.template.invalid", &[id.to_string().into()]).into());
    };
    let size = access.size(&template);
    let end = BlockPos::new(
        pos.x.wrapping_add(size[0]),
        pos.y.wrapping_add(size[1]),
        pos.z.wrapping_add(size[2]),
    );
    check_loaded(access, ChunkPos::containing(pos), ChunkPos::containing(end))?;
    let mut settings = StructurePlaceSettings {
        rotation: match command.string("rotation").unwrap_or("none") {
            "none" => Rotation::None,
            "clockwise_90" => Rotation::Clockwise90,
            "180" => Rotation::Clockwise180,
            "counterclockwise_90" => Rotation::CounterClockwise90,
            _ => unreachable!("template rotation argument"),
        },
        mirror: match command.string("mirror").unwrap_or("none") {
            "none" => Mirror::None,
            "front_back" => Mirror::FrontBack,
            "left_right" => Mirror::LeftRight,
            _ => unreachable!("template mirror argument"),
        },
        known_shape: command.literals.iter().any(|literal| literal == "strict"),
        ..Default::default()
    };
    let integrity = match command.argument("integrity") {
        Some(ArgumentValue::Float(value)) => *value,
        None => 1.0,
        _ => unreachable!("integrity float argument"),
    };
    let seed = i64::from(command.integer("seed").unwrap_or(0));
    let mut processors = Vec::new();
    if integrity < 1.0 {
        processors.push(StructureProcessor::BlockRot {
            integrity,
            rottable: None,
        });
        settings.random =
            PlacementRandom::Fixed(access.create_random(seed).map_err(PlaceError::World)?);
    }
    let random = access.create_random(seed).map_err(PlaceError::World)?;
    let flags = 2 | if settings.known_shape { 816 } else { 0 };
    if !access
        .place_template(&template, pos, settings, &processors, random, flags)
        .map_err(PlaceError::World)?
    {
        return Err(error("commands.place.template.failed", &[]).into());
    }
    Ok(CommandResult::success(
        1,
        Component::translatable(
            "commands.place.template.success",
            vec![
                builtins::component(Component::literal(id.to_string())),
                builtins::integer(pos.x),
                builtins::integer(pos.y),
                builtins::integer(pos.z),
            ],
        ),
        true,
    ))
}

#[cfg(test)]
mod tests;
