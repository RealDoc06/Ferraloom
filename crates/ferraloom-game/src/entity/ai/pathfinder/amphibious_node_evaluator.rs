use super::{
    Capabilities, MobAccess, NodeEvaluator, NodeId, PathData, Target,
    walk_node_evaluator::{WalkNodeEvaluator, prepared::WalkKind},
};
use crate::level::{Level, LevelError, path_navigation_region::PathNavigationRegion};
use ferraloom_util::BlockPos;

/// Port of AmphibiousNodeEvaluator, including temporary canonical malus writes.
pub struct AmphibiousNodeEvaluator<'a, M>(WalkNodeEvaluator<'a, M>);
impl<'a, M: MobAccess> AmphibiousNodeEvaluator<'a, M> {
    pub fn new(
        region: &'a PathNavigationRegion,
        level: &'a mut Level,
        mob: &'a mut M,
        prefers_shallow_swimming: bool,
    ) -> Self {
        Self(WalkNodeEvaluator::with_kind(
            region,
            level,
            mob,
            WalkKind::Amphibious {
                prefers_shallow: prefers_shallow_swimming,
            },
        ))
    }
    pub fn capabilities(&self) -> &Capabilities {
        self.0.capabilities()
    }
    pub fn capabilities_mut(&mut self) -> &mut Capabilities {
        self.0.capabilities_mut()
    }
    pub fn get_path_type(&mut self, pos: BlockPos) -> Result<super::PathType, LevelError> {
        self.0.get_path_type(pos)
    }
    pub fn get_path_type_of_mob(&mut self, pos: BlockPos) -> Result<super::PathType, LevelError> {
        self.0.get_path_type_of_mob(pos)
    }
}
impl<M: MobAccess> NodeEvaluator for AmphibiousNodeEvaluator<'_, M> {
    type Error = LevelError;
    fn prepare(&mut self, data: &mut PathData) -> Result<(), LevelError> {
        self.0.prepare(data)
    }
    fn done(&mut self, data: &mut PathData) -> Result<(), LevelError> {
        self.0.done(data)
    }
    fn get_start(&mut self, data: &mut PathData) -> Result<Option<NodeId>, LevelError> {
        self.0.get_start(data)
    }
    fn get_target(&mut self, pos: BlockPos, data: &mut PathData) -> Result<Target, LevelError> {
        self.0.get_target(pos, data)
    }
    fn get_neighbors(
        &mut self,
        node: NodeId,
        out: &mut Vec<NodeId>,
        data: &mut PathData,
    ) -> Result<(), LevelError> {
        self.0.get_neighbors(node, out, data)
    }
}
