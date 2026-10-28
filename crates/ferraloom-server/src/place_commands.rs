//! PlaceCommand's synchronous adapter over the shared manager and live Level.
use super::*;
use ferraloom_game::{
    block::containers::structure::StructureBlockEntity,
    commands::place::{self, PlaceAccess, PlaceError, TemplateAccess},
    entity::EntityData,
    level::Level,
};
use ferraloom_util::random::Random;
use ferraloom_worldgen::structure::{
    processor::StructureProcessor,
    rule::BlockTags,
    template::{StructurePlaceSettings, StructureTemplate},
};

#[derive(Debug)]
enum PlaceRuntimeError {
    Level(LevelError),
    World(ferraloom_worldgen::Error),
}
impl std::fmt::Display for PlaceRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Level(error) => error.fmt(formatter),
            Self::World(error) => error.fmt(formatter),
        }
    }
}
impl std::error::Error for PlaceRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Level(error) => error,
            Self::World(error) => error,
        })
    }
}
struct TemplateHost<'a> {
    level: &'a mut Level,
    tags: &'a BlockTags,
}
impl PlaceAccess for TemplateHost<'_> {
    type Error = PlaceRuntimeError;
    fn eye_height(&self, entity: EntityRef) -> std::result::Result<f64, Self::Error> {
        Ok(f64::from(
            self.level
                .entity::<EntityData>(entity)
                .map_err(PlaceRuntimeError::Level)?
                .eye_height,
        ))
    }
    fn has_chunk_at(&self, pos: BlockPos) -> std::result::Result<bool, Self::Error> {
        Ok(self.level.has_chunk_at(pos))
    }
    fn in_world_bounds(&self, pos: BlockPos) -> bool {
        // Level#isInWorldBounds, rather than the wider chunk-pyramid bounds.
        self.level.is_in_valid_bounds(pos)
            && (-30_000_000..30_000_000).contains(&pos.x)
            && (-30_000_000..30_000_000).contains(&pos.z)
    }
    fn is_loaded(&self, pos: BlockPos) -> std::result::Result<bool, Self::Error> {
        Ok(self.level.is_loaded(pos))
    }
}
impl TemplateAccess for TemplateHost<'_> {
    type Template = Arc<StructureTemplate>;
    fn template(
        &mut self,
        id: &protocol::Identifier,
    ) -> std::result::Result<Option<Self::Template>, Self::Error> {
        let manager = self
            .level
            .context()
            .structure_templates()
            .map_err(PlaceRuntimeError::Level)?;
        let id = id.as_str().parse().expect("validated template identifier");
        // Manager#get's only error is a cached/source lookup miss. Failure to
        // install the required canonical manager was propagated above.
        Ok(manager.get(&id).ok())
    }
    fn size(&self, template: &Self::Template) -> [i32; 3] {
        template.size
    }
    fn create_random(&mut self, seed: i64) -> std::result::Result<Random, Self::Error> {
        Ok(StructureBlockEntity::create_random(seed))
    }
    fn place_template(
        &mut self,
        template: &Self::Template,
        position: BlockPos,
        mut settings: StructurePlaceSettings,
        processors: &[StructureProcessor],
        mut random: Random,
        flags: u32,
    ) -> std::result::Result<bool, Self::Error> {
        let position = [position.x, position.y, position.z];
        let mut random = ferraloom_worldgen::feature::FeatureRandom::Live(&mut random);
        template
            .place_in_world(
                self.level,
                position,
                position,
                &mut settings,
                processors,
                self.tags,
                &mut random,
                flags,
            )
            .map_err(PlaceRuntimeError::World)
    }
}
impl MinecraftServer {
    pub(in crate::server) fn place_template_command(
        &mut self,
        command: &PreparedCommand,
    ) -> Result<CommandResult> {
        let registries = self.world_root.services.worldgen().clone();
        let dimension = command
            .source
            .level
            .dimension
            .parse()
            .context("Invalid command source dimension")?;
        let result = place::template(
            command,
            &mut TemplateHost {
                level: self
                    .get_level_mut(&dimension)
                    .with_context(|| format!("Unknown command source dimension {dimension}"))?,
                tags: &registries.tags,
            },
        );
        // Block/entity effects remain at the command call site, before success
        // feedback, including writes performed before a required failure.
        self.apply_level_effects()?;
        result.map_err(|error| match error {
            PlaceError::Syntax(error) => anyhow::Error::new(error),
            PlaceError::World(error) => anyhow::Error::new(error),
        })
    }
}
