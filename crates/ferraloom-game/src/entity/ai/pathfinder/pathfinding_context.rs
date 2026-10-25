use super::{PathType, walk_node_evaluator};
use crate::level::{Level, LevelError, path_navigation_region::PathNavigationRegion};
use ferraloom_util::BlockPos;

/// Port of PathfindingContext#getPathTypeFromState for a captured navigation
/// region. The cache belongs to the canonical Level owner; misses use only the
/// bounded reader. No cache or chunk borrow crosses the classification call.
pub fn get_path_type_from_state(
    region: &PathNavigationRegion,
    level: &mut Level,
    pos: BlockPos,
) -> Result<PathType, LevelError> {
    let path_type = {
        let reader = region.reader(level)?;
        if let Some(path_type) = level.path_type_cache().get(pos) {
            return Ok(path_type);
        }
        walk_node_evaluator::get_path_type_from_state(&reader, pos)?
    };
    level.path_type_cache_mut().record_computed(pos, path_type);
    Ok(path_type)
}
