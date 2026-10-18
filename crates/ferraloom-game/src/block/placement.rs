//! Shared UseOnContext and BlockPlaceContext queries. Contexts carry handles,
//! captured hit/stack data and no long borrow of the canonical level.
use super::{BlockMethod, LevelAccessRef, LevelReaderRef};
use crate::{
    entity::{EntityData, EntityRef},
    inventory::{MenuError, StackRef},
    level::{Level, LevelError},
};
use ferraloom_data::{Block, BlockState};
use ferraloom_protocol::{component::ItemStack, play::InteractionHand};
use ferraloom_util::{BlockPos, Direction, Vec3};

/// Captured ItemStack receiver. StackOperation owns retained roots; copying a
/// view neither acquires a root nor resolves the player's current hand again.
#[derive(Clone, Copy)]
pub enum StackView<'stack> {
    Borrowed(&'stack ItemStack),
    Retained(StackRef),
}
impl StackView<'_> {
    pub fn read<'call>(
        &'call self,
        reader: &'call LevelReaderRef<'_, '_>,
    ) -> Result<&'call ItemStack, LevelError> {
        match self {
            Self::Borrowed(stack) => Ok(stack),
            Self::Retained(reference) => match reader {
                LevelReaderRef::Server(level) => {
                    level.stack(*reference).map_err(|error| match error {
                        MenuError::Level(error) => error,
                        _ => LevelError::Inventory("retained item receiver"),
                    })
                }
                LevelReaderRef::Worldgen(_) => Err(LevelError::UnsupportedWorldgenEnvironment),
            },
        }
    }
}

/// The double-precision BlockHitResult captured by vanilla placement code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacementHit {
    pub pos: BlockPos,
    pub face: Direction,
    pub location: Vec3,
    pub inside: bool,
}
/// Port of UseOnContext. Stack and hit are captured; player methods read the
/// canonical handle again at each call, as vanilla does.
#[derive(Clone)]
pub struct UseOnContext<'stack> {
    pub player: Option<EntityRef>,
    pub hand: InteractionHand,
    pub hit: PlacementHit,
    held_item: StackView<'stack>,
}
impl<'stack> UseOnContext<'stack> {
    pub fn new(
        player: Option<EntityRef>,
        hand: InteractionHand,
        held_item: &'stack ItemStack,
        hit: PlacementHit,
    ) -> Self {
        Self {
            player,
            hand,
            held_item: StackView::Borrowed(held_item),
            hit,
        }
    }
    pub fn new_retained(
        player: Option<EntityRef>,
        hand: InteractionHand,
        held_item: StackRef,
        hit: PlacementHit,
    ) -> Self {
        Self {
            player,
            hand,
            held_item: StackView::Retained(held_item),
            hit,
        }
    }
    pub fn item_in_hand<'call>(
        &'call self,
        reader: &'call LevelReaderRef<'_, '_>,
    ) -> Result<&'call ItemStack, LevelError> {
        self.held_item.read(reader)
    }
    pub fn clicked_pos(&self) -> BlockPos {
        self.hit.pos
    }
    pub fn clicked_face(&self) -> Direction {
        self.hit.face
    }
    pub fn click_location(&self) -> Vec3 {
        self.hit.location
    }
    pub fn is_inside(&self) -> bool {
        self.hit.inside
    }
    pub fn player(&self) -> Option<EntityRef> {
        self.player
    }
    pub fn hand(&self) -> InteractionHand {
        self.hand
    }
    fn player_data<'a>(
        &self,
        reader: &'a LevelReaderRef<'_, '_>,
    ) -> Result<&'a EntityData, LevelError> {
        let player = self.player.ok_or(LevelError::MissingPlacementPlayer)?;
        match reader {
            LevelReaderRef::Server(level) => level.entity::<EntityData>(player),
            LevelReaderRef::Worldgen(_) => Err(LevelError::UnsupportedWorldgenEnvironment),
        }
    }
    /// Entity#getViewXRot(1) reads pitch. Entity#getViewYRot(1) reads yaw,
    /// while LivingEntity's override reads its canonical head yaw.
    fn view_rotation(&self, reader: &LevelReaderRef<'_, '_>) -> Result<(f32, f32), LevelError> {
        let player = self.player.ok_or(LevelError::MissingPlacementPlayer)?;
        match reader {
            LevelReaderRef::Server(level) => {
                let state = level.entity_state(player)?;
                let entity = state.as_entity();
                let yaw = state
                    .as_living()
                    .map_or(entity.yaw, |living| living.y_head_rot);
                Ok((entity.pitch, yaw))
            }
            LevelReaderRef::Worldgen(_) => Err(LevelError::UnsupportedWorldgenEnvironment),
        }
    }
    pub fn horizontal_direction(
        &self,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<Direction, LevelError> {
        if self.player.is_none() {
            Ok(Direction::North)
        } else {
            Ok(Direction::from_y_rot(f64::from(
                self.player_data(reader)?.yaw,
            )))
        }
    }
    pub fn is_secondary_use_active(
        &self,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        if self.player.is_none() {
            Ok(false)
        } else {
            Ok(self.player_data(reader)?.shared_flag(1))
        }
    }
    pub fn rotation(&self, reader: &LevelReaderRef<'_, '_>) -> Result<f32, LevelError> {
        if self.player.is_none() {
            Ok(0.0)
        } else {
            Ok(self.player_data(reader)?.yaw)
        }
    }
}
#[derive(Clone, Copy)]
enum PlacementKind {
    Player,
    Directional(Option<Direction>),
}
/// Port of BlockPlaceContext. The initial replacement query observes
/// replaceClicked=true before the constructor stores the query's result.
pub struct BlockPlaceContext<'stack> {
    use_on: UseOnContext<'stack>,
    relative_pos: BlockPos,
    replace_clicked: bool,
    kind: PlacementKind,
}
impl<'stack> BlockPlaceContext<'stack> {
    pub fn new(
        reader: &LevelReaderRef<'_, '_>,
        use_on: UseOnContext<'stack>,
    ) -> Result<Self, LevelError> {
        let mut context = Self::uninitialized(use_on, PlacementKind::Player);
        let state = reader.get_block_state(context.use_on.hit.pos)?;
        context.replace_clicked = reader.can_be_replaced(state, &context)?;
        Ok(context)
    }
    fn uninitialized(use_on: UseOnContext<'stack>, kind: PlacementKind) -> Self {
        let relative_pos = use_on.hit.pos.relative(use_on.hit.face, 1);
        Self {
            use_on,
            relative_pos,
            replace_clicked: true,
            kind,
        }
    }
    /// Port of BlockPlaceContext#at. The new hit is the center of the target face.
    pub fn at(
        reader: &LevelReaderRef<'_, '_>,
        context: &Self,
        pos: BlockPos,
        face: Direction,
    ) -> Result<Self, LevelError> {
        let step = face.step();
        Self::new(
            reader,
            // Keep the original borrowed or retained receiver; never re-wrap it.
            UseOnContext {
                player: context.use_on.player,
                hand: context.use_on.hand,
                held_item: context.use_on.held_item,
                hit: PlacementHit {
                    pos,
                    face,
                    inside: false,
                    location: Vec3::new(
                        f64::from(pos.x) + 0.5 + f64::from(step.x) * 0.5,
                        f64::from(pos.y) + 0.5 + f64::from(step.y) * 0.5,
                        f64::from(pos.z) + 0.5 + f64::from(step.z) * 0.5,
                    ),
                },
            },
        )
    }
    pub fn clicked_pos(&self) -> BlockPos {
        if matches!(self.kind, PlacementKind::Directional(_)) || self.replace_clicked {
            self.use_on.hit.pos
        } else {
            self.relative_pos
        }
    }
    pub fn clicked_face(&self) -> Direction {
        self.use_on.hit.face
    }
    pub fn click_location(&self) -> Vec3 {
        self.use_on.hit.location
    }
    pub fn is_inside(&self) -> bool {
        self.use_on.hit.inside
    }
    pub fn item_in_hand<'call>(
        &'call self,
        reader: &'call LevelReaderRef<'_, '_>,
    ) -> Result<&'call ItemStack, LevelError> {
        self.use_on.item_in_hand(reader)
    }
    pub fn player(&self) -> Option<EntityRef> {
        self.use_on.player
    }
    pub fn hand(&self) -> InteractionHand {
        self.use_on.hand
    }
    pub fn direction(&self) -> Option<Direction> {
        match self.kind {
            PlacementKind::Player => None,
            PlacementKind::Directional(direction) => direction,
        }
    }
    pub fn can_place(&self, reader: &LevelReaderRef<'_, '_>) -> Result<bool, LevelError> {
        if matches!(self.kind, PlacementKind::Player) && self.replace_clicked {
            return Ok(true);
        }
        reader.can_be_replaced(reader.get_block_state(self.clicked_pos())?, self)
    }
    pub fn replacing_clicked_on_block(
        &self,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        match self.kind {
            PlacementKind::Player => Ok(self.replace_clicked),
            PlacementKind::Directional(_) => self.can_place(reader),
        }
    }
    pub fn nearest_looking_direction(
        &self,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<Direction, LevelError> {
        match self.kind {
            PlacementKind::Directional(_) => Ok(Direction::Down),
            PlacementKind::Player => {
                let (pitch, yaw) = self.use_on.view_rotation(reader)?;
                Ok(Direction::ordered_by_nearest(pitch, yaw)[0])
            }
        }
    }
    pub fn nearest_looking_vertical_direction(
        &self,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<Direction, LevelError> {
        Ok(if self.use_on.player_data(reader)?.pitch < 0.0 {
            Direction::Up
        } else {
            Direction::Down
        })
    }
    pub fn nearest_looking_directions(
        &self,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<[Direction; 6], LevelError> {
        use Direction::*;
        if let PlacementKind::Directional(direction) = self.kind {
            return Ok(
                match direction.ok_or(LevelError::UninitializedPlacementDirection)? {
                    Down => [Down, North, East, South, West, Up],
                    Up => [Down, Up, North, East, South, West],
                    North => [Down, North, East, West, Up, South],
                    South => [Down, South, East, West, Up, North],
                    West => [Down, West, South, Up, North, East],
                    East => [Down, East, South, Up, North, West],
                },
            );
        }
        let (pitch, yaw) = self.use_on.view_rotation(reader)?;
        let mut directions = Direction::ordered_by_nearest(pitch, yaw);
        if !self.replace_clicked {
            let face = self.clicked_face().opposite();
            let index = directions
                .iter()
                .position(|&direction| direction == face)
                .unwrap();
            directions[..=index].rotate_right(1);
        }
        Ok(directions)
    }
    pub fn horizontal_direction(
        &self,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<Direction, LevelError> {
        match self.kind {
            PlacementKind::Player => self.use_on.horizontal_direction(reader),
            PlacementKind::Directional(direction) => {
                let direction = direction.ok_or(LevelError::UninitializedPlacementDirection)?;
                Ok(if direction.axis() == ferraloom_util::Axis::Y {
                    Direction::North
                } else {
                    direction
                })
            }
        }
    }
    pub fn rotation(&self, reader: &LevelReaderRef<'_, '_>) -> Result<f32, LevelError> {
        match self.kind {
            PlacementKind::Player => self.use_on.rotation(reader),
            PlacementKind::Directional(direction) => Ok(direction
                .ok_or(LevelError::UninitializedPlacementDirection)?
                .data_2d() as f32
                * 90.0),
        }
    }
    pub fn is_secondary_use_active(
        &self,
        reader: &LevelReaderRef<'_, '_>,
    ) -> Result<bool, LevelError> {
        match self.kind {
            PlacementKind::Player => self.use_on.is_secondary_use_active(reader),
            PlacementKind::Directional(_) => Ok(false),
        }
    }
}
/// Port of DirectionalPlaceContext. Deref keeps existing falling-block callsites
/// on the same general placement context, including constructor-time dispatch.
pub struct DirectionalPlaceContext<'stack>(BlockPlaceContext<'stack>);
impl<'stack> std::ops::Deref for DirectionalPlaceContext<'stack> {
    type Target = BlockPlaceContext<'stack>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<'stack> DirectionalPlaceContext<'stack> {
    pub fn new(
        reader: &LevelReaderRef<'_, '_>,
        pos: BlockPos,
        direction: Direction,
        held_item: &'stack ItemStack,
        clicked_face: Direction,
    ) -> Result<Self, LevelError> {
        let use_on = UseOnContext::new(
            None,
            InteractionHand::MainHand,
            held_item,
            PlacementHit {
                pos,
                face: clicked_face,
                location: Vec3::new(
                    f64::from(pos.x) + 0.5,
                    f64::from(pos.y),
                    f64::from(pos.z) + 0.5,
                ),
                inside: false,
            },
        );
        let mut context =
            BlockPlaceContext::uninitialized(use_on, PlacementKind::Directional(None));
        reader.can_be_replaced(reader.get_block_state(pos)?, &context)?;
        context.kind = PlacementKind::Directional(Some(direction));
        Ok(Self(context))
    }
}
impl LevelAccessRef<'_, '_> {
    pub fn get_state_for_placement(
        &mut self,
        block: Block,
        context: &BlockPlaceContext<'_>,
    ) -> Result<Option<BlockState>, LevelError> {
        self.as_reader()
            .required_behavior(block, BlockMethod::GetStateForPlacement)?
            .get_state_for_placement(block, context, self)
    }
}
impl LevelReaderRef<'_, '_> {
    pub fn can_be_replaced(
        &self,
        state: BlockState,
        context: &BlockPlaceContext<'_>,
    ) -> Result<bool, LevelError> {
        let behavior = self.required_behavior(state.block(), BlockMethod::CanBeReplaced)?;
        behavior.can_be_replaced(state, context, self)
    }
    pub fn can_survive(&self, state: BlockState, pos: BlockPos) -> Result<bool, LevelError> {
        let behavior = self.required_behavior(state.block(), BlockMethod::CanSurvive)?;
        behavior.can_survive(state, pos, self)
    }
}
impl Level {
    pub fn get_state_for_placement(
        &mut self,
        block: Block,
        context: &BlockPlaceContext<'_>,
    ) -> Result<Option<BlockState>, LevelError> {
        LevelAccessRef::Server(self).get_state_for_placement(block, context)
    }

    pub fn can_be_replaced(
        &self,
        state: BlockState,
        context: &BlockPlaceContext<'_>,
    ) -> Result<bool, LevelError> {
        LevelReaderRef::Server(self).can_be_replaced(state, context)
    }
    pub fn can_survive(&self, state: BlockState, pos: BlockPos) -> Result<bool, LevelError> {
        LevelReaderRef::Server(self).can_survive(state, pos)
    }
    /// Port of Level#setBlockAndUpdate.
    pub fn set_block_and_update(
        &mut self,
        pos: BlockPos,
        state: BlockState,
    ) -> Result<bool, LevelError> {
        self.set_block(pos, state, super::UpdateFlags::ALL, super::UPDATE_LIMIT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        level::{Region, chunk::LevelChunk},
    };
    use ferraloom_util::ChunkPos;
    use std::sync::Arc;
    static BEHAVIORS: Behaviors = Behaviors::EMPTY;
    fn level() -> Level {
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        level
            .add_chunk(LevelChunk::new(
                ChunkPos::new(0, 0),
                0..1,
                ferraloom_chunk::LevelChunkSection::new(
                    ferraloom_chunk::Strategy::block_states(),
                    ferraloom_chunk::Strategy::biomes(Arc::new(
                        ferraloom_chunk::IdMap::new(vec![ferraloom_chunk::BiomeId(0)]).unwrap(),
                    )),
                    ferraloom_chunk::BiomeId(0),
                ),
            ))
            .unwrap();
        level
    }
    #[test]
    fn native_context_construction_and_directional_looking_orders() {
        let mut level = level();
        let pos = BlockPos::new(8, 0, 8);
        for line in include_str!("../../tests/java/level/placement-context.tsv").lines() {
            let (key, expected) = line.split_once('\t').unwrap();
            let actual = if let Some(key) = key.strip_prefix("ordinary:") {
                let (name, held) = key.rsplit_once(':').unwrap();
                let block = Block::from_name(name).unwrap();
                level.set_block_state(pos, block.default_state()).unwrap();
                let item = match held {
                    "0" => ferraloom_data::Item::AIR,
                    "1" => block.as_item(),
                    _ => ferraloom_data::Item::DIRT,
                };
                let stack = ItemStack::new(item, 1, Default::default());
                let reader = LevelReaderRef::Server(&level);
                let context = BlockPlaceContext::new(
                    &reader,
                    UseOnContext::new(
                        None,
                        InteractionHand::MainHand,
                        &stack,
                        PlacementHit {
                            pos,
                            face: Direction::Up,
                            location: Vec3::new(8.25, 0.75, 8.5),
                            inside: false,
                        },
                    ),
                )
                .unwrap();
                let clicked = context.clicked_pos();
                let actual = format!(
                    "{},{},{},{},{},{},{},{:.1}",
                    context.replacing_clicked_on_block(&reader).unwrap(),
                    clicked.x,
                    clicked.y,
                    clicked.z,
                    context.can_place(&reader).unwrap(),
                    context.horizontal_direction(&reader).unwrap().name(),
                    context.is_secondary_use_active(&reader).unwrap(),
                    context.rotation(&reader).unwrap()
                );
                assert_eq!(
                    level.get_state_for_placement(Block::STONE, &context),
                    Ok(Some(Block::STONE.default_state()))
                );
                assert!(matches!(
                    level.get_state_for_placement(Block::OAK_LOG, &context),
                    Err(LevelError::UnsupportedBlockMethod(
                        _,
                        BlockMethod::GetStateForPlacement
                    ))
                ));
                actual
            } else {
                level
                    .set_block_state(pos, Block::AIR.default_state())
                    .unwrap();
                let stack = ItemStack::default();
                let reader = LevelReaderRef::Server(&level);
                let direction =
                    Direction::by_name(key.strip_prefix("directional:").unwrap()).unwrap();
                let context =
                    DirectionalPlaceContext::new(&reader, pos, direction, &stack, Direction::Up)
                        .unwrap();
                format!(
                    "{};{};{:.1};{}",
                    context
                        .nearest_looking_directions(&reader)
                        .unwrap()
                        .map(Direction::name)
                        .join(","),
                    context.horizontal_direction(&reader).unwrap().name(),
                    context.rotation(&reader).unwrap(),
                    context.nearest_looking_direction(&reader).unwrap().name()
                )
            };
            assert_eq!(actual, expected, "{key}");
        }
    }
}
