//! Port of SignBlockEntity and HangingSignBlockEntity's two-sided text state.
use crate::block::entity::{BlockEntity, BlockEntityData, BlockEntityState};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_protocol::{
    Uuid,
    chat::Component,
    component::{
        ComponentTypeId, DataComponent, DataComponentMap, DyeColor, Persistent, SignText, Unit,
    },
};
use ferraloom_util::{Vec3, mth};

fn empty_text() -> SignText {
    SignText {
        messages: std::array::from_fn(|_| Component::literal("")),
        filtered_messages: None,
        color: DyeColor::Black,
        has_glowing_text: false,
    }
}
#[derive(Clone, Debug)]
pub struct SignBlockEntity {
    pub parent: BlockEntityData,
    pub front_text: SignText,
    pub back_text: SignText,
    pub is_waxed: bool,
    pub allow_op_features: bool,
    pub allowed_player_editor: Option<Uuid>,
}
impl SignBlockEntity {
    /// Port of SignBlockEntity#getSlotPlayerIsFacing. Wall signs measure from
    /// their offset board center; hanging and standing signs use block center.
    pub fn player_is_facing_front(&self, position: Vec3) -> bool {
        use ferraloom_data::{BlockClass, properties as p};
        let state = self.parent.block_state;
        let facing = state.get(p::FACING).map(|_| super::menu::facing(state));
        let rotation = if let Some(facing) = facing {
            facing.to_y_rot()
        } else {
            // RotationSegment#convertToDegrees returns the signed segment angle.
            // Keep that float subtraction before degreesDifferenceAbs: wrapping
            // only the final difference changes rounding at the 90-degree edge.
            let degrees = state.get(p::ROTATION).expect("standing sign rotation") as f32 * 22.5;
            if degrees >= 180.0 {
                degrees - 360.0
            } else {
                degrees
            }
        };
        let (mut x, mut z) = (0.5, 0.5);
        if state.block().class() == BlockClass::WallSignBlock {
            let offset = facing.expect("wall sign facing").step();
            x -= f64::from(offset.x) * 7.0 / 16.0;
            z -= f64::from(offset.z) * 7.0 / 16.0;
        }
        let xd = position.x - (f64::from(self.parent.pos.x) + x);
        let zd = position.z - (f64::from(self.parent.pos.z) + z);
        let player_rotation = (mth::atan2(zd, xd) * 57.2957763671875) as f32 - 90.0;
        mth::degrees_difference_abs(rotation, player_rotation) <= 90.0
    }
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            front_text: empty_text(),
            back_text: empty_text(),
            is_waxed: false,
            allow_op_features: false,
            allowed_player_editor: None,
        }
    }
    /// In-memory adapter for SignBlockEntity#updateSignText. Live callers use
    /// the handle function below so both notifications occur in native order.
    pub fn update_sign_text(
        &mut self,
        player: Uuid,
        front: bool,
        lines: &[(String, String); 4],
        filter: bool,
    ) -> bool {
        if self.is_waxed || self.allowed_player_editor != Some(player) || !self.parent.has_level {
            return false;
        }
        self.update_messages(front, lines, filter);
        self.allowed_player_editor = None;
        true
    }
    fn update_messages(&mut self, front: bool, lines: &[(String, String); 4], filter: bool) {
        let text = if front {
            &mut self.front_text
        } else {
            &mut self.back_text
        };
        let old = if filter {
            text.filtered_messages.as_ref().unwrap_or(&text.messages)
        } else {
            &text.messages
        };
        let mut raw = std::array::from_fn(|i| {
            let mut line = Component::literal(if filter { &lines[i].1 } else { &lines[i].0 });
            line.style = old[i].style.clone();
            line
        });
        let filtered = std::array::from_fn(|i| {
            let mut line = Component::literal(&lines[i].1);
            line.style = old[i].style.clone();
            line
        });
        if filter {
            raw = filtered.clone();
        }
        text.filtered_messages = (filtered != raw).then_some(filtered);
        text.messages = raw;
    }
}

/// Port of SignBlockEntity#updateSignText, including markUpdated inside setText
/// before clearing the editor, then the second sendBlockUpdated afterwards.
pub fn update_sign_text(
    handle: crate::block::entity::BlockEntityRef,
    player: Uuid,
    front: bool,
    lines: &[(String, String); 4],
    filter: bool,
    level: &mut crate::level::Level,
) -> Result<bool, crate::level::LevelError> {
    level.with_block_entity_callback(handle, |level| {
        let sign = level.block_entity_mut::<SignBlockEntity>(handle)?;
        if sign.is_waxed || sign.allowed_player_editor != Some(player) || !sign.parent.has_level {
            return Ok(false);
        }
        sign.update_messages(front, lines, filter);
        handle.set_changed(level)?;
        level.block_entity_updated(handle)?;
        level
            .block_entity_mut::<SignBlockEntity>(handle)?
            .allowed_player_editor = None;
        level.block_entity_updated(handle)?;
        Ok(true)
    })
}
pub struct SignBehavior;
pub static SIGN_ENTITY: SignBehavior = SignBehavior;
impl BlockEntity for SignBehavior {
    fn has_server_ticker(&self) -> bool {
        true
    }
    /// Port of SignBlockEntity#tick / clearInvalidPlayerWhoMayEdit.
    fn tick(
        &self,
        handle: crate::block::entity::BlockEntityRef,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        use crate::entity::player::ServerPlayer;
        let Some(editor) = level
            .block_entity::<SignBlockEntity>(handle)?
            .allowed_player_editor
        else {
            return Ok(());
        };
        let valid = level
            .entity_by_uuid(editor)
            .and_then(|player| level.entity::<ServerPlayer>(player).ok())
            .is_some_and(|player| player.is_within_block_interaction_range(handle.pos, 4.0));
        if !valid {
            level
                .block_entity_mut::<SignBlockEntity>(handle)?
                .allowed_player_editor = None;
        }
        Ok(())
    }
    fn remove_components_from_tag(&self, tag: &mut CompoundTag) {
        for key in ["front_text", "back_text", "is_waxed"] {
            tag.remove(key);
        }
    }
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::Sign(sign) = state else {
            unreachable!()
        };
        sign.allow_op_features = tag.get_boolean_or("allow_op_features", false);
        sign.is_waxed = tag.get_boolean_or("is_waxed", false);
        sign.front_text = tag
            .get("front_text")
            .and_then(|text| SignText::from_nbt(text).ok())
            .unwrap_or_else(empty_text);
        sign.back_text = tag
            .get("back_text")
            .and_then(|text| SignText::from_nbt(text).ok())
            .unwrap_or_else(empty_text);
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::Sign(sign) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        if sign.allow_op_features {
            tag.insert("allow_op_features", Tag::Byte(1));
        }
        tag.insert("front_text", sign.front_text.to_nbt()?);
        tag.insert("back_text", sign.back_text.to_nbt()?);
        tag.insert("is_waxed", Tag::Byte(i8::from(sign.is_waxed)));
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        self.save_additional(state)
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
    fn apply_implicit_components(
        &self,
        state: &mut BlockEntityState,
        map: &DataComponentMap,
    ) -> &'static [ComponentTypeId] {
        let BlockEntityState::Sign(sign) = state else {
            unreachable!()
        };
        sign.front_text = match map.get(ComponentTypeId::SIGN_TEXT_FRONT) {
            Some(DataComponent::SignTextFront(text)) => (**text).clone(),
            _ => empty_text(),
        };
        sign.back_text = match map.get(ComponentTypeId::SIGN_TEXT_BACK) {
            Some(DataComponent::SignTextBack(text)) => (**text).clone(),
            _ => empty_text(),
        };
        sign.is_waxed = map.get(ComponentTypeId::WAXED).is_some();
        &[
            ComponentTypeId::SIGN_TEXT_FRONT,
            ComponentTypeId::SIGN_TEXT_BACK,
            ComponentTypeId::WAXED,
        ]
    }
    fn collect_implicit_components(&self, state: &BlockEntityState, map: &mut DataComponentMap) {
        let BlockEntityState::Sign(sign) = state else {
            unreachable!()
        };
        super::storage::set(
            map,
            DataComponent::SignTextFront(Box::new(sign.front_text.clone())),
        );
        super::storage::set(
            map,
            DataComponent::SignTextBack(Box::new(sign.back_text.clone())),
        );
        if sign.is_waxed {
            super::storage::set(map, DataComponent::Waxed(Unit));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::entity::{BlockEntityType, create};
    use ferraloom_data::BlockState;
    use ferraloom_util::BlockPos;
    #[test]
    fn sign_editor_ticker_matches_native_missing_and_distant_players() {
        use crate::{
            Behaviors,
            entity::{
                EntityRef, EntityState,
                player::{GameType, ServerPlayer},
            },
            level::{Level, Region, chunk::LevelChunk},
        };
        use ferraloom_data::Block;
        use ferraloom_util::ChunkPos;
        use std::sync::Arc;
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut checked = 0;
        for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("sign-editor-tick\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let mut level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            let biome = ferraloom_chunk::BiomeId(0);
            let section = ferraloom_chunk::LevelChunkSection::new(
                ferraloom_chunk::Strategy::block_states(),
                ferraloom_chunk::Strategy::biomes(Arc::new(
                    ferraloom_chunk::IdMap::new(vec![biome]).unwrap(),
                )),
                biome,
            );
            level
                .add_chunk(LevelChunk::new(ChunkPos::new(0, 0), 0..2, section))
                .unwrap();
            let state = Block::OAK_SIGN.default_state();
            level.set_block_state(BlockPos::ZERO, state).unwrap();
            level
                .set_block_entity(create(BlockPos::ZERO, state).unwrap())
                .unwrap();
            let handle = level.block_entity_ref(BlockPos::ZERO).unwrap().unwrap();
            let editor = Uuid::new(1, 2);
            level
                .block_entity_mut::<SignBlockEntity>(handle)
                .unwrap()
                .allowed_player_editor = Some(editor);
            if fields[1] == "true" {
                level
                    .add_entity(
                        EntityRef::fixture(crate::level::RegionId::FIXTURE, 1),
                        EntityState::Player(Box::new(ServerPlayer::new(
                            editor,
                            "sign".into(),
                            1,
                            Vec3::new(fields[2].parse().unwrap(), 0.0, 0.5),
                            GameType::Survival,
                        ))),
                    )
                    .unwrap();
            }
            SIGN_ENTITY.tick(handle, &mut level).unwrap();
            assert_eq!(
                level
                    .block_entity::<SignBlockEntity>(handle)
                    .unwrap()
                    .allowed_player_editor
                    .is_some(),
                fields[3] == "true",
                "{line}"
            );
            checked += 1;
        }
        assert_eq!(checked, 8);
    }
    #[test]
    fn sign_facing_matches_native_rotations_and_offset_wall_boards() {
        let mut checked = 0;
        for line in include_str!("../../../tests/java/blocks/entities/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("sign-facing\t"))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let state = BlockState::from_id(fields[1].parse().unwrap()).unwrap();
            let entity = create(BlockPos::ZERO, state).unwrap();
            assert!(matches!(
                entity.data().kind,
                BlockEntityType::SIGN | BlockEntityType::HANGING_SIGN
            ));
            let BlockEntityState::Sign(sign) = entity else {
                panic!("sign fixture");
            };
            let x = f64::from_bits(u64::from_str_radix(fields[2], 16).unwrap());
            let z = f64::from_bits(u64::from_str_radix(fields[3], 16).unwrap());
            assert_eq!(
                sign.player_is_facing_front(Vec3::new(x, 0.0, z)),
                fields[4] == "true",
                "{line}"
            );
            checked += 1;
        }
        assert_eq!(checked, 320);
    }
}
