//! DedicatedServer spawn protection published at the exclusive server call site.
use std::collections::HashSet;

use ferraloom_data::datapack::Identifier;
use ferraloom_util::{BlockPos, Uuid};

use super::{Level, LevelError};
use crate::entity::{EntityRef, EntityState, player::ServerPlayer};

/// Owned inputs of DedicatedServer#isUnderSpawnProtection. Operator membership,
/// rather than command permission level, is vanilla's bypass on dedicated servers.
#[derive(Clone, Debug)]
pub struct SpawnProtectionSnapshot {
    dimension: Identifier,
    spawn_dimension: Identifier,
    spawn: BlockPos,
    radius: i32,
    operators: HashSet<Uuid>,
}
impl SpawnProtectionSnapshot {
    pub fn new(
        dimension: Identifier,
        spawn_dimension: Identifier,
        spawn: BlockPos,
        radius: i32,
        operators: impl IntoIterator<Item = Uuid>,
    ) -> Self {
        Self {
            dimension,
            spawn_dimension,
            spawn,
            radius,
            operators: operators.into_iter().collect(),
        }
    }

    /// Port of DedicatedServer#isUnderSpawnProtection's dedicated-server cases.
    fn protects(&self, player: Uuid, pos: BlockPos) -> bool {
        if self.dimension != self.spawn_dimension
            || self.operators.is_empty()
            || self.operators.contains(&player)
            || self.radius <= 0
        {
            return false;
        }
        // Java int subtraction and Mth.abs retain MIN_VALUE on overflow.
        let dx = pos.x.wrapping_sub(self.spawn.x).wrapping_abs();
        let dz = pos.z.wrapping_sub(self.spawn.z).wrapping_abs();
        dx.max(dz) <= self.radius
    }
}

impl Level {
    /// Publish after loading properties/operators and after each op or shared-spawn change.
    pub fn set_spawn_protection(&mut self, snapshot: SpawnProtectionSnapshot) {
        self.region_mut().spawn_protection = Some(snapshot);
    }

    /// Port of DedicatedServer#isUnderSpawnProtection, using canonical player identity.
    pub fn is_under_spawn_protection(
        &self,
        player: EntityRef,
        pos: BlockPos,
    ) -> Result<bool, LevelError> {
        let player = self.entity::<ServerPlayer>(player)?;
        let snapshot = self
            .region()
            .spawn_protection
            .as_ref()
            .ok_or(LevelError::MissingSpawnProtectionContext)?;
        Ok(snapshot.protects(player.uuid, pos))
    }

    /// Port of ServerLevel#mayInteract (the base Level implementation permits all).
    /// Non-player entities bypass both checks, as in vanilla's instanceof branch.
    pub fn may_interact(&self, entity: EntityRef, pos: BlockPos) -> Result<bool, LevelError> {
        if !matches!(self.entity_state(entity)?, EntityState::Player(_)) {
            return Ok(true);
        }
        Ok(!self.is_under_spawn_protection(entity, pos)?
            && self.entity_world_border().is_within_block(pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::{experience_orb::ExperienceOrb, player::GameType},
        level::{Region, border::WorldBorder},
    };
    use ferraloom_util::Vec3;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Vector {
        dimension: String,
        spawn_dimension: String,
        radius: i32,
        has_ops: bool,
        player_op_level: i32,
        spawn: [i32; 3],
        pos: [i32; 3],
        player: bool,
        border_size: f64,
        protected: bool,
        may_interact: bool,
    }
    const PLAYER: EntityRef = EntityRef(1);
    const OTHER: EntityRef = EntityRef(2);
    const UUID: Uuid = Uuid::new(1, 2);

    fn level() -> Level {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        level
            .add_entity(
                PLAYER,
                EntityState::Player(Box::new(ServerPlayer::new(
                    UUID,
                    "Player".into(),
                    1,
                    Vec3::ZERO,
                    GameType::Survival,
                ))),
            )
            .unwrap();
        level
            .add_entity(
                OTHER,
                EntityState::ExperienceOrb(Box::new(ExperienceOrb::new(2, 0))),
            )
            .unwrap();
        level
    }

    #[test]
    fn native_dedicated_protection_and_server_level_interaction_vectors() {
        let vectors: Vec<Vector> =
            serde_json::from_str(include_str!("vanilla/spawn-protection-vectors.json")).unwrap();
        let mut level = level();
        for vector in vectors {
            let mut operators = Vec::new();
            if vector.has_ops {
                operators.push(Uuid::new(3, 4));
            }
            if vector.player_op_level >= 0 {
                operators.push(UUID);
            }
            level.set_spawn_protection(SpawnProtectionSnapshot::new(
                format!("minecraft:{}", vector.dimension).parse().unwrap(),
                format!("minecraft:{}", vector.spawn_dimension)
                    .parse()
                    .unwrap(),
                BlockPos::new(vector.spawn[0], vector.spawn[1], vector.spawn[2]),
                vector.radius,
                operators,
            ));
            let mut border = WorldBorder::default();
            border.set_size(vector.border_size);
            level.set_entity_world_border(border);
            let pos = BlockPos::new(vector.pos[0], vector.pos[1], vector.pos[2]);
            assert_eq!(
                level.is_under_spawn_protection(PLAYER, pos),
                Ok(vector.protected),
                "{vector:?}"
            );
            assert_eq!(
                level.may_interact(if vector.player { PLAYER } else { OTHER }, pos),
                Ok(vector.may_interact),
                "{vector:?}"
            );
        }
    }

    #[test]
    fn missing_publication_fails_player_query_and_nonplayers_bypass_it() {
        let mut level = level();
        let pos = BlockPos::new(0, 64, 0);
        assert_eq!(
            level.may_interact(PLAYER, pos),
            Err(LevelError::MissingSpawnProtectionContext)
        );
        assert_eq!(level.may_interact(OTHER, pos), Ok(true));
        assert_eq!(
            level.may_interact(EntityRef(999), pos),
            Err(LevelError::MissingEntity(EntityRef(999)))
        );
        for operators in [vec![Uuid::new(3, 4)], vec![UUID], vec![]] {
            let protected = operators == [Uuid::new(3, 4)];
            level.set_spawn_protection(SpawnProtectionSnapshot::new(
                "minecraft:overworld".parse().unwrap(),
                "minecraft:overworld".parse().unwrap(),
                pos,
                16,
                operators,
            ));
            assert_eq!(level.may_interact(PLAYER, pos), Ok(!protected));
        }
        // A shared spawn change is observed by the next game query, without a tick.
        level.set_spawn_protection(SpawnProtectionSnapshot::new(
            "minecraft:overworld".parse().unwrap(),
            "minecraft:overworld".parse().unwrap(),
            BlockPos::new(100, 64, 100),
            16,
            [Uuid::new(3, 4)],
        ));
        assert_eq!(level.may_interact(PLAYER, pos), Ok(true));
    }
}
