//! Region-owned simulation. See `docs/design/game-core.md` for porting rules.
//!
//! This foundation implements ownership and narrow tick/damage primitives, not
//! complete cow AI, block placement updates, or the server tick loop.

pub mod block;
pub mod commands;
pub mod entity;
pub mod gamerules;
pub mod level;
pub mod ticks;

/// Immutable virtual dispatch tables. Unsupported behavior is an explicit error.
pub struct Behaviors {
    entities: [Option<&'static dyn entity::Entity>; ferraloom_data::ENTITY_TYPES.len()],
    blocks: [Option<&'static dyn block::BlockBehavior>; ferraloom_data::BLOCKS.len()],
}

impl Behaviors {
    pub const EMPTY: Self = Self {
        entities: [None; ferraloom_data::ENTITY_TYPES.len()],
        blocks: [None; ferraloom_data::BLOCKS.len()],
    };

    pub const fn with_entity(
        mut self,
        kind: ferraloom_data::EntityType,
        behavior: &'static dyn entity::Entity,
    ) -> Self {
        self.entities[kind.protocol_id() as usize] = Some(behavior);
        self
    }

    pub const fn with_block(
        mut self,
        block: ferraloom_data::Block,
        behavior: &'static dyn block::BlockBehavior,
    ) -> Self {
        self.blocks[block.protocol_id() as usize] = Some(behavior);
        self
    }

    pub fn entity(&self, kind: ferraloom_data::EntityType) -> Option<&'static dyn entity::Entity> {
        self.entities[kind.protocol_id() as usize]
    }

    pub fn block(&self, block: ferraloom_data::Block) -> Option<&'static dyn block::BlockBehavior> {
        self.blocks[block.protocol_id() as usize]
    }
}

#[cfg(test)]
mod tests;
