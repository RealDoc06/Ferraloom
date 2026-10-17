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
        if let Some(list) = tag.get_list("Pos") {
            if let [Tag::Double(x), Tag::Double(y), Tag::Double(z)] = list.0.as_slice() {
                self.position = Vec3::new(*x, *y, *z);
            }
        }
        if let Some(list) = tag.get_list("Rotation") {
            if let [Tag::Float(yaw), Tag::Float(pitch)] = list.0.as_slice() {
                self.yaw = *yaw;
                self.pitch = *pitch;
            }
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
    if [target.x, target.y, target.z]
        .iter()
        .any(|v| !v.is_finite())
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
    let (from, spectator, too_quickly) = {
        let player = level.entity_mut::<ServerPlayer>(entity)?;
        player.received_moves += 1;
        let packets = if player.received_moves > 5 {
            1
        } else {
            player.received_moves
        };
        (
            player.position,
            player.game_mode == GameType::Spectator,
            target.distance_to_sqr(player.first_good) - player.velocity.length_sqr()
                > 100. * f64::from(packets),
        )
    };
    if too_quickly {
        return Ok(MovementResult::TooQuickly);
    }
    // Vanilla's AABB terrain collision path, with no vehicles or step-up yet.
    if !spectator && collides(level, target)? && !collides(level, from)? {
        return Ok(MovementResult::Wrongly);
    }
    let player = level.entity_mut::<ServerPlayer>(entity)?;
    player.position = target;
    player.yaw = mth::wrap_degrees_f32(yaw);
    player.pitch = mth::wrap_degrees_f32(pitch);
    player.on_ground = on_ground;
    player.last_good = target;
    Ok(MovementResult::Accepted)
}
fn collides(level: &Level, position: Vec3) -> Result<bool, LevelError> {
    let min = [
        position.x - 0.3 + 1e-5,
        position.y + 1e-5,
        position.z - 0.3 + 1e-5,
    ];
    let max = [
        position.x + 0.3 - 1e-5,
        position.y + 1.8 - 1e-5,
        position.z + 0.3 - 1e-5,
    ];
    for x in mth::floor(min[0])..=mth::floor(max[0]) {
        for y in mth::floor(min[1])..=mth::floor(max[1]) {
            for z in mth::floor(min[2])..=mth::floor(max[2]) {
                let state = level.get_block_state(BlockPos::new(x, y, z))?;
                for shape in state.data().collision.boxes() {
                    let offset = [f64::from(x), f64::from(y), f64::from(z)];
                    if (0..3).all(|i| {
                        max[i] > shape.min[i] + offset[i] && min[i] < shape.max[i] + offset[i]
                    }) {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}
pub struct PlayerBehavior;
impl crate::entity::Entity for PlayerBehavior {}
