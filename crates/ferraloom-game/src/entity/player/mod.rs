//! ServerPlayer gameplay state. Connections retain handles, never another player copy.
use crate::{
    entity::{EntityData, EntityDataView, EntityState, LivingEntity},
    level::{Level, LevelError},
};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_util::{BlockPos, ChunkPos, Uuid, Vec3, mth};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameType {
    Survival,
    Creative,
    Adventure,
    Spectator,
}
impl GameType {
    pub fn id(self) -> i32 {
        match self {
            Self::Survival => 0,
            Self::Creative => 1,
            Self::Adventure => 2,
            Self::Spectator => 3,
        }
    }
    pub fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Creative,
            2 => Self::Adventure,
            3 => Self::Spectator,
            _ => Self::Survival,
        }
    }
}
/// Port of ServerPlayer's join, position, game-mode and persistence state.
#[derive(Debug)]
pub struct ServerPlayer {
    pub parent: LivingEntity,
    pub uuid: Uuid,
    pub name: String,
    pub network_id: i32,
    pub position: Vec3,
    pub velocity: Vec3,
    pub known_movement: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub game_mode: GameType,
    pub flying: bool,
    pub view_distance: i32,
    pub loaded: bool,
    pub connection_ticks: u64,
    pub saved: CompoundTag,
    pub first_good: Vec3,
    pub last_good: Vec3,
    pub received_moves: u32,
}
impl ServerPlayer {
    /// Port of Player#getDisplayName and #decorateDisplayNameComponent (without a team).
    pub fn display_name(&self) -> ferraloom_protocol::chat::Component {
        use ferraloom_protocol::{
            Identifier,
            chat::{ClickEvent, Component, HoverEvent},
        };
        let mut name = Component::literal(&self.name);
        name.style.click_event = Some(ClickEvent::SuggestCommand(format!("/tell {} ", self.name)));
        name.style.hover_event = Some(HoverEvent::ShowEntity {
            entity_type: Identifier::parse("minecraft:player").expect("built-in player identifier"),
            uuid: self.uuid,
            name: Some(Box::new(Component::literal(&self.name))),
        });
        name.style.insertion = Some(self.name.clone());
        name
    }

    pub fn new(
        uuid: Uuid,
        name: String,
        network_id: i32,
        position: Vec3,
        game_mode: GameType,
    ) -> Self {
        Self {
            parent: LivingEntity {
                parent: EntityData::default(),
                health: 20.,
                last_hurt: 0.,
                damage_cooldown_time: 0,
                hurt_time: 0,
                hurt_duration: 0,
            },
            uuid,
            name,
            network_id,
            position,
            velocity: Vec3::ZERO,
            known_movement: Vec3::ZERO,
            yaw: 0.,
            pitch: 0.,
            on_ground: false,
            game_mode,
            flying: game_mode == GameType::Spectator,
            view_distance: 10,
            loaded: false,
            connection_ticks: 0,
            saved: CompoundTag::new(),
            first_good: position,
            last_good: position,
            received_moves: 0,
        }
    }
    pub fn chunk_position(&self) -> ChunkPos {
        ChunkPos::containing(BlockPos::new(
            mth::floor(self.position.x),
            mth::floor(self.position.y),
            mth::floor(self.position.z),
        ))
    }
    /// Port of ServerGamePacketListenerImpl#resetPosition at connection tick.
    pub fn do_tick(&mut self) {
        self.connection_ticks += 1;
        self.first_good = self.position;
        self.last_good = self.position;
        self.received_moves = 0;
    }
    pub fn load(&mut self, tag: CompoundTag) {
        if let Some(list) = tag.get_list("Pos")
            && let [Tag::Double(x), Tag::Double(y), Tag::Double(z)] = list.0.as_slice()
        {
            self.position = Vec3::new(*x, *y, *z);
        }
        if let Some(list) = tag.get_list("Rotation")
            && let [Tag::Float(yaw), Tag::Float(pitch)] = list.0.as_slice()
        {
            self.yaw = *yaw;
            self.pitch = *pitch;
        }
        self.game_mode = GameType::from_id(tag.get_int_or("playerGameType", self.game_mode.id()));
        self.on_ground = tag.get_boolean_or("OnGround", false);
        self.first_good = self.position;
        self.last_good = self.position;
        self.saved = tag;
    }
    /// Port of Entity#saveWithoutId, Player#addAdditionalSaveData and ServerPlayer.
    pub fn save(&self) -> CompoundTag {
        let mut tag = self.saved.clone();
        tag.insert("DataVersion", Tag::Int(5023));
        tag.insert("UUID", Tag::IntArray(self.uuid.to_int_array().to_vec()));
        tag.insert("Dimension", Tag::String("minecraft:overworld".into()));
        tag.insert(
            "Pos",
            Tag::List(ListTag(vec![
                Tag::Double(self.position.x),
                Tag::Double(self.position.y),
                Tag::Double(self.position.z),
            ])),
        );
        tag.insert(
            "Motion",
            Tag::List(ListTag(vec![
                Tag::Double(self.velocity.x),
                Tag::Double(self.velocity.y),
                Tag::Double(self.velocity.z),
            ])),
        );
        tag.insert(
            "Rotation",
            Tag::List(ListTag(vec![Tag::Float(self.yaw), Tag::Float(self.pitch)])),
        );
        tag.insert("playerGameType", Tag::Int(self.game_mode.id()));
        tag.insert("Health", Tag::Float(self.parent.health));
        tag.insert("OnGround", Tag::Byte(i8::from(self.on_ground)));
        tag.insert("Air", Tag::Short(300));
        tag.insert("Fire", Tag::Short(-20));
        if !tag.contains_key("Inventory") {
            tag.insert("Inventory", Tag::List(ListTag::default()));
        }
        tag.insert("foodLevel", Tag::Int(20));
        tag.insert("foodSaturationLevel", Tag::Float(5.));
        let creative = matches!(self.game_mode, GameType::Creative | GameType::Spectator);
        let mut abilities = CompoundTag::new();
        abilities.insert("invulnerable", Tag::Byte(i8::from(creative)));
        abilities.insert("mayfly", Tag::Byte(i8::from(creative)));
        abilities.insert("flying", Tag::Byte(i8::from(self.flying)));
        abilities.insert(
            "instabuild",
            Tag::Byte(i8::from(self.game_mode == GameType::Creative)),
        );
        abilities.insert("mayBuild", Tag::Byte(1));
        abilities.insert("flySpeed", Tag::Float(0.05));
        abilities.insert("walkSpeed", Tag::Float(0.1));
        tag.insert("abilities", Tag::Compound(abilities));
        tag
    }
}
impl EntityDataView for ServerPlayer {
    fn view(state: &EntityState) -> Option<&Self> {
        match state {
            EntityState::Player(p) => Some(p),
            _ => None,
        }
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        match state {
            EntityState::Player(p) => Some(p),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementResult {
    Accepted,
    Invalid,
    TooQuickly,
    Wrongly,
}
/// Port of ServerGamePacketListenerImpl#handleMovePlayer's ordinary terrain path.
/// Missing chunks stay errors; callers resolve the synchronous load before retrying.
pub fn move_player(
    level: &mut Level,
    entity: crate::entity::EntityRef,
    target: Vec3,
    yaw: f32,
    pitch: f32,
    on_ground: bool,
) -> Result<MovementResult, LevelError> {
    if [target.x, target.y, target.z].iter().any(|v| v.is_nan())
        || !yaw.is_finite()
        || !pitch.is_finite()
    {
        return Ok(MovementResult::Invalid);
    }
    let target = Vec3::new(
        target.x.clamp(-30_000_000., 30_000_000.),
        target.y.clamp(-20_000_000., 20_000_000.),
        target.z.clamp(-30_000_000., 30_000_000.),
    );
    let (from, mode, too_quickly) = {
        let player = level.entity_mut::<ServerPlayer>(entity)?;
        player.received_moves += 1;
        let packets = if player.received_moves > 5 {
            1
        } else {
            player.received_moves
        };
        (
            player.position,
            player.game_mode,
            target.distance_to_sqr(player.first_good) - player.velocity.length_sqr()
                > 100. * f64::from(packets),
        )
    };
    if too_quickly {
        return Ok(MovementResult::TooQuickly);
    }
    // Entity#collideBoundingBox clips the swept movement before the listener
    // checks its horizontal discrepancy and newly introduced terrain collisions.
    if mode != GameType::Spectator {
        let shapes = terrain_shapes(level, from, target)?;
        let old = player_box(from);
        let requested = player_box(target);
        let mut clipped = old;
        let mut delta = [target.x - from.x, target.y - from.y, target.z - from.z];
        let horizontal = if delta[0].abs() < delta[2].abs() {
            [2, 0]
        } else {
            [0, 2]
        };
        for axis in [1, horizontal[0], horizontal[1]] {
            for shape in &shapes {
                if (0..3)
                    .filter(|&i| i != axis)
                    .all(|i| clipped.1[i] > shape.0[i] + 1e-7 && clipped.0[i] < shape.1[i] - 1e-7)
                {
                    if delta[axis] > 0. && clipped.1[axis] <= shape.0[axis] + 1e-7 {
                        delta[axis] = delta[axis].min(shape.0[axis] - clipped.1[axis]);
                    } else if delta[axis] < 0. && clipped.0[axis] >= shape.1[axis] - 1e-7 {
                        delta[axis] = delta[axis].max(shape.1[axis] - clipped.0[axis]);
                    }
                }
            }
            clipped.0[axis] += delta[axis];
            clipped.1[axis] += delta[axis];
        }
        let dx = target.x - from.x - delta[0];
        let dz = target.z - from.z - delta[2];
        // Vanilla 26.3's listener clears the vertical discrepancy unconditionally.
        let wrongly = dx * dx + dz * dz > 0.0625 && mode != GameType::Creative;
        let old_collides = shapes.iter().any(|shape| intersects(old, *shape));
        let new_collision = shapes
            .iter()
            .any(|shape| intersects(requested, *shape) && !intersects(old, *shape));
        if (wrongly && !old_collides) || new_collision {
            return Ok(MovementResult::Wrongly);
        }
    }
    let player = level.entity_mut::<ServerPlayer>(entity)?;
    player.position = target;
    player.yaw = mth::wrap_degrees_f32(yaw);
    player.pitch = mth::wrap_degrees_f32(pitch);
    player.on_ground = on_ground;
    player.last_good = target;
    Ok(MovementResult::Accepted)
}
type CollisionBox = ([f64; 3], [f64; 3]);
fn player_box(position: Vec3) -> CollisionBox {
    (
        [
            position.x - 0.3 + 1e-5,
            position.y + 1e-5,
            position.z - 0.3 + 1e-5,
        ],
        [
            position.x + 0.3 - 1e-5,
            position.y + 1.8 - 1e-5,
            position.z + 0.3 - 1e-5,
        ],
    )
}
fn intersects(a: CollisionBox, b: CollisionBox) -> bool {
    (0..3).all(|i| a.1[i] > b.0[i] && a.0[i] < b.1[i])
}
fn terrain_shapes(level: &Level, from: Vec3, to: Vec3) -> Result<Vec<CollisionBox>, LevelError> {
    let a = player_box(from);
    let b = player_box(to);
    let min = std::array::from_fn::<_, 3, _>(|i| a.0[i].min(b.0[i]));
    let max = std::array::from_fn::<_, 3, _>(|i| a.1[i].max(b.1[i]));
    let mut shapes = Vec::new();
    for x in mth::floor(min[0])..=mth::floor(max[0]) {
        for y in mth::floor(min[1])..=mth::floor(max[1]) {
            for z in mth::floor(min[2])..=mth::floor(max[2]) {
                let state = level.get_block_state(BlockPos::new(x, y, z))?;
                let offset = [f64::from(x), f64::from(y), f64::from(z)];
                for shape in state.data().collision.boxes() {
                    shapes.push((
                        std::array::from_fn(|i| shape.min[i] + offset[i]),
                        std::array::from_fn(|i| shape.max[i] + offset[i]),
                    ));
                }
            }
        }
    }
    Ok(shapes)
}
pub struct PlayerBehavior;
impl crate::entity::Entity for PlayerBehavior {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Behaviors,
        entity::{EntityRef, EntityState},
        level::{Region, chunk::LevelChunk},
    };
    use ferraloom_chunk::{BiomeId, IdMap, LevelChunkSection, Strategy};
    use ferraloom_data::Block;
    use std::sync::Arc;
    #[test]
    fn display_name_matches_java_player_component() {
        let player = ServerPlayer::new(
            Uuid::new(0x0123456789abcdef, 0x0123456789abcdef),
            "Doc".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        );
        let expected =
            ferraloom_protocol::chat::Component::from_json(include_str!("display-name.json"))
                .unwrap();
        assert_eq!(player.display_name(), expected);
    }
    #[test]
    fn movement_sweep_cannot_tunnel_through_terrain() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let biome = Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap()));
        let section = LevelChunkSection::new(Strategy::block_states(), biome, BiomeId(0));
        let mut level = Level::new(Region::new(&BEHAVIORS));
        level
            .add_chunk(LevelChunk::new(ChunkPos::new(0, 0), 0..16, section))
            .unwrap();
        for y in 64..67 {
            level
                .set_block_state(BlockPos::new(4, y, 1), Block::STONE.default_state())
                .unwrap();
        }
        let entity = EntityRef(1);
        let player = ServerPlayer::new(
            Uuid::NIL,
            "Walker".into(),
            1,
            Vec3::new(1.5, 64., 1.5),
            GameType::Survival,
        );
        level
            .add_entity(entity, EntityState::Player(Box::new(player)))
            .unwrap();
        assert_eq!(
            move_player(&mut level, entity, Vec3::new(7.5, 64., 1.5), 0., 0., true).unwrap(),
            MovementResult::Wrongly
        );
        assert_eq!(
            level.entity::<ServerPlayer>(entity).unwrap().position.x,
            1.5
        );
        assert_eq!(
            move_player(
                &mut level,
                entity,
                Vec3::new(f64::NAN, 64., 1.5),
                0.,
                0.,
                true
            )
            .unwrap(),
            MovementResult::Invalid
        );
    }
}
