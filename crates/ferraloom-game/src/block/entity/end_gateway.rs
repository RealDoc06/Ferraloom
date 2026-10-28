//! TheEndGatewayBlockEntity state and server timer; exit terrain/teleporting is unported.
use super::{BlockEntity, BlockEntityData, BlockEntityRef, BlockEntityState};
use crate::level::{Level, LevelError, events::BlockEvent};
use ferraloom_nbt::{CompoundTag, Tag};
use ferraloom_util::BlockPos;

/// Port of TheEndGatewayBlockEntity; its stateless EndPortal parent is shared data.
#[derive(Clone, Debug)]
pub struct TheEndGatewayBlockEntity {
    pub parent: BlockEntityData,
    pub age: i64,
    pub teleport_cooldown: i32,
    pub exit_portal: Option<BlockPos>,
    pub exact_teleport: bool,
}
impl TheEndGatewayBlockEntity {
    pub fn new(parent: BlockEntityData) -> Self {
        Self {
            parent,
            age: 0,
            teleport_cooldown: 0,
            exit_portal: None,
            exact_teleport: false,
        }
    }
    pub const fn is_spawning(&self) -> bool {
        self.age < 200
    }
    pub const fn is_cooling_down(&self) -> bool {
        self.teleport_cooldown > 0
    }
    /// Port of TheEndGatewayBlockEntity#setExitPosition; no notification here.
    pub fn set_exit_position(&mut self, position: BlockPos, exact: bool) {
        self.exit_portal = Some(position);
        self.exact_teleport = exact;
    }
}

// BlockPos.CODEC uses Number.intValue, unlike TagValueInput's floor-based getters.
fn codec_int(tag: &Tag) -> Option<i32> {
    Some(match tag {
        Tag::Byte(v) => i32::from(*v),
        Tag::Short(v) => i32::from(*v),
        Tag::Int(v) => *v,
        Tag::Long(v) => *v as i32,
        Tag::Float(v) => *v as i32,
        Tag::Double(v) => *v as i32,
        _ => return None,
    })
}
/// BlockPos.CODEC through ValueInput#read accepts a surplus numeric list's
/// first-three partial, then Level#isInSpawnableBounds filters the position.
fn exit_portal(tag: &Tag) -> Option<BlockPos> {
    let pos = match tag {
        Tag::IntArray(v) => BlockPos::new(*v.first()?, *v.get(1)?, *v.get(2)?),
        Tag::ByteArray(v) => BlockPos::new(
            i32::from(*v.first()?),
            i32::from(*v.get(1)?),
            i32::from(*v.get(2)?),
        ),
        Tag::LongArray(v) => {
            BlockPos::new(*v.first()? as i32, *v.get(1)? as i32, *v.get(2)? as i32)
        }
        Tag::List(v) => {
            let mut values = v.iter().map(codec_int);
            let pos = BlockPos::new(values.next()??, values.next()??, values.next()??);
            if values.any(|value| value.is_none()) {
                return None;
            }
            pos
        }
        _ => return None,
    };
    ((-30_000_000..30_000_000).contains(&pos.x)
        && (-30_000_000..30_000_000).contains(&pos.z)
        && (-20_000_000..20_000_000).contains(&pos.y))
    .then_some(pos)
}

/// Port of TheEndGatewayBlockEntity#triggerCooldown for the server Level.
pub fn trigger_cooldown(receiver: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
    level.with_block_entity_callback(receiver, |level| {
        let entity = level.block_entity_mut::<TheEndGatewayBlockEntity>(receiver)?;
        entity.teleport_cooldown = 40;
        let block = entity.parent.block_state.block();
        level.block_event(BlockEvent {
            pos: receiver.pos,
            block,
            param_a: 1,
            param_b: 0,
        });
        receiver.set_changed(level)
    })
}
/// Port of TheEndGatewayBlockEntity#portalTick, preserving both native change
/// callbacks at the 2400-tick transition and rereading the original receiver.
pub fn server_tick(receiver: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
    level.with_block_entity_callback(receiver, |level| {
        let entity = level.block_entity_mut::<TheEndGatewayBlockEntity>(receiver)?;
        let spawning = entity.is_spawning();
        let cooling = entity.is_cooling_down();
        entity.age = entity.age.wrapping_add(1);
        let trigger = if cooling {
            entity.teleport_cooldown -= 1;
            false
        } else {
            entity.age % 2400 == 0
        };
        if trigger {
            trigger_cooldown(receiver, level)?;
        }
        let entity = level.block_entity::<TheEndGatewayBlockEntity>(receiver)?;
        if spawning != entity.is_spawning() || cooling != entity.is_cooling_down() {
            receiver.set_changed(level)?;
        }
        Ok(())
    })
}

pub(super) struct EndGatewayBehavior;
pub(super) static END_GATEWAY_ENTITY: EndGatewayBehavior = EndGatewayBehavior;
impl BlockEntity for EndGatewayBehavior {
    fn load_additional(
        &self,
        state: &mut BlockEntityState,
        tag: &CompoundTag,
        _registries: &dyn ferraloom_protocol::codec::RegistryContext,
    ) {
        let BlockEntityState::EndGateway(entity) = state else {
            unreachable!()
        };
        entity.age = tag.get_long_or("Age", 0);
        entity.exit_portal = tag.get("exit_portal").and_then(exit_portal);
        entity.exact_teleport = tag.get_byte_or("ExactTeleport", 0) != 0;
    }
    fn save_additional(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        let BlockEntityState::EndGateway(entity) = state else {
            unreachable!()
        };
        let mut tag = CompoundTag::new();
        tag.insert("Age", Tag::Long(entity.age));
        if let Some(pos) = entity.exit_portal {
            tag.insert("exit_portal", Tag::IntArray(vec![pos.x, pos.y, pos.z]));
        }
        if entity.exact_teleport {
            tag.insert("ExactTeleport", Tag::Byte(1));
        }
        Ok(tag)
    }
    fn get_update_tag(&self, state: &BlockEntityState) -> ferraloom_protocol::Result<CompoundTag> {
        self.save_additional(state)
    }
    fn sends_update_packet(&self) -> bool {
        true
    }
    fn has_server_ticker(&self) -> bool {
        true
    }
    fn tick(&self, receiver: BlockEntityRef, level: &mut Level) -> Result<(), LevelError> {
        server_tick(receiver, level)
    }
    fn trigger_event(&self, state: &mut BlockEntityState, event: i32, _value: i32) -> bool {
        let BlockEntityState::EndGateway(entity) = state else {
            unreachable!()
        };
        if event == 1 {
            entity.teleport_cooldown = 40;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests;
