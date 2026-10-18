//! ServerPlayer gameplay state. Connections retain handles, never another player copy.
mod drop_item;
pub use drop_item::{drop_item, drop_item_stack};
mod contact;
pub mod cooldowns;
pub mod equipment;
mod experience;
mod interaction;
mod notifications;
pub use notifications::{build_limit_packet, send_build_limit_message};
mod spawn;
pub use experience::{give_experience_levels, give_experience_points, send_experience_update};
pub use interaction::swing_and_reset_attack_strength;
pub mod inventory;
use crate::{
    entity::{Avatar, EntityData, EntityDataView, EntityRef, EntityState},
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
    pub parent: Avatar,
    // GameProfile identity is immutable; entity NBT cannot replace it.
    profile_id: Uuid,
    pub inventory: inventory::Inventory,
    pub menus: crate::inventory::MenuSession,
    pub recipe_book: crate::crafting::ServerRecipeBook,
    pub advancements: crate::advancements::PlayerAdvancements,
    pub stats: crate::stats::ServerStatsCounter,
    pub experience_level: i32,
    pub experience_progress: f32,
    pub total_experience: i32,
    last_level_up_time: i32,
    last_sent_experience: i32,
    pub name: String,
    pub known_movement: Vec3,
    pub game_mode: GameType,
    pub flying: bool,
    pub may_build: bool,
    pub instabuild: bool,
    pub last_client_input: ferraloom_protocol::play::Input,
    pub text_filtering_enabled: bool,
    pub cooldowns: cooldowns::ServerItemCooldowns,
    interaction_range_modifiers: [std::sync::Arc<super::ai::attributes::AttributeModifier>; 2],
    pub view_distance: i32,
    pub loaded: bool,
    pub connection_ticks: u64,
    pub teleport: super::teleport::TeleportState,
    pub first_good: Vec3,
    pub last_good: Vec3,
    pub received_moves: u32,
}
impl ServerPlayer {
    /// Port of Player#getGameProfile's identity, also used by Player#nameAndId.
    pub fn profile_id(&self) -> Uuid {
        self.profile_id
    }
    /// Port of Player#getDisplayName (without a team).
    pub fn display_name(&self) -> ferraloom_protocol::chat::Component {
        self.decorate_display_name_component(ferraloom_protocol::chat::Component::literal(
            &self.name,
        ))
    }
    /// Port of Player#decorateDisplayNameComponent, preserving the formatted name's other styles.
    pub fn decorate_display_name_component(
        &self,
        mut name: ferraloom_protocol::chat::Component,
    ) -> ferraloom_protocol::chat::Component {
        use ferraloom_protocol::{
            Identifier,
            chat::{ClickEvent, Component, HoverEvent},
        };
        name.style.click_event = Some(ClickEvent::SuggestCommand(format!("/tell {} ", self.name)));
        name.style.hover_event = Some(HoverEvent::ShowEntity {
            entity_type: Identifier::parse("minecraft:player").expect("built-in player identifier"),
            uuid: self.uuid,
            name: Some(Box::new(Component::literal(&self.name))),
        });
        name.style.insertion = Some(self.name.clone());
        name
    }

    /// Port of Player#getLuck: the canonical attribute value narrowed to float.
    pub fn luck(&self) -> f32 {
        self.attributes
            .value(ferraloom_data::registries::attribute::Id::LUCK)
            .expect("player attribute supplier defines LUCK") as f32
    }

    /// Port of Player#mayBuild. This ability can differ from the game mode.
    pub fn may_build(&self) -> bool {
        self.may_build
    }

    /// Port of Abilities.instabuild.
    pub fn instabuild(&self) -> bool {
        self.instabuild
    }

    /// Port of Player#preventsBlockDrops, independent of the game mode.
    pub fn prevents_block_drops(&self) -> bool {
        self.instabuild
    }

    /// Port of GameType#updatePlayerAbilities for the currently ported fields.
    /// Runtime game-mode transitions also handle their packets and other effects.
    pub fn update_game_mode_abilities(&mut self) {
        self.may_build = matches!(self.game_mode, GameType::Survival | GameType::Creative);
        self.instabuild = self.game_mode == GameType::Creative;
        match self.game_mode {
            GameType::Creative => {}
            GameType::Spectator => self.flying = true,
            GameType::Survival | GameType::Adventure => self.flying = false,
        }
    }

    pub fn new(
        uuid: Uuid,
        name: String,
        network_id: i32,
        position: Vec3,
        game_mode: GameType,
    ) -> Self {
        Self {
            parent: {
                let mut base = EntityData::from_random(
                    ferraloom_data::EntityType::PLAYER,
                    network_id,
                    crate::level::entity::new_entity_random(),
                );
                base.uuid = uuid;
                base.set_pos(position);
                let mut avatar = Avatar::with_parent(base);
                use super::synched_data as data;
                use ferraloom_protocol::{codec::VarInt, entity_data::OptionalUnsignedInt};
                avatar.entity_data.define(data::PLAYER_ABSORPTION, 0.0);
                avatar.entity_data.define(data::PLAYER_SCORE, VarInt(0));
                avatar
                    .entity_data
                    .define(data::SHOULDER_PARROT_LEFT, OptionalUnsignedInt(None));
                avatar
                    .entity_data
                    .define(data::SHOULDER_PARROT_RIGHT, OptionalUnsignedInt(None));
                avatar
            },
            profile_id: uuid,
            name,
            inventory: Default::default(),
            menus: Default::default(),
            recipe_book: Default::default(),
            advancements: Default::default(),
            stats: Default::default(),
            experience_level: 0,
            experience_progress: 0.0,
            total_experience: 0,
            last_level_up_time: 0,
            last_sent_experience: -99_999_999,
            known_movement: Vec3::ZERO,
            game_mode,
            flying: game_mode == GameType::Spectator,
            may_build: matches!(game_mode, GameType::Survival | GameType::Creative),
            instabuild: game_mode == GameType::Creative,
            last_client_input: ferraloom_protocol::play::Input {
                forward: false,
                backward: false,
                left: false,
                right: false,
                jump: false,
                shift: false,
                sprint: false,
            },
            text_filtering_enabled: false,
            cooldowns: Default::default(),
            interaction_range_modifiers: interaction::range_modifiers(),
            view_distance: 10,
            loaded: false,
            connection_ticks: 0,
            teleport: Default::default(),
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
        self.try_load(tag)
            .expect("invalid saved player entity data");
    }
    pub fn try_load(
        &mut self,
        tag: CompoundTag,
    ) -> Result<(), super::persistence::EntityLoadError> {
        if tag.contains_key("recipeBook") {
            return Err(super::persistence::EntityLoadError::RecipeManagerRequired);
        }
        self.load_player_data(tag)
    }

    /// Port of ServerPlayer#readAdditionalSaveData's recipe hydration against the
    /// live manager. Optional malformed recipeBook data is diagnosed and skipped
    /// as ValueInput#read does, without replacing the existing canonical book.
    pub fn try_load_with_recipes(
        &mut self,
        tag: CompoundTag,
        manager: &crate::crafting::RecipeManager,
    ) -> Result<(), super::persistence::EntityLoadError> {
        self.load_player_data(tag)?;
        if let Some(nbt) = self.parent.parent.saved.get("recipeBook")
            && let Err(error) = self.recipe_book.load_untrusted(nbt, manager)
        {
            eprintln!(
                "Failed to load recipeBook for player {}: {error}",
                self.uuid
            );
        }
        Ok(())
    }

    fn load_player_data(
        &mut self,
        tag: CompoundTag,
    ) -> Result<(), super::persistence::EntityLoadError> {
        self.parent.parent.load_nbt(tag)?;
        // Player#readAdditionalSaveData restores the profile UUID after super.
        self.uuid = self.profile_id;
        self.experience_progress = self.saved.get_float_or("XpP", 0.0);
        self.experience_level = self.saved.get_int_or("XpLevel", 0);
        self.total_experience = self.saved.get_int_or("XpTotal", 0);
        self.set_score(self.saved.get_int_or("Score", 0));
        if let Some(abilities) = self.saved.get_compound("abilities") {
            self.instabuild = abilities.get_boolean_or("instabuild", false);
        }
        self.game_mode =
            GameType::from_id(self.saved.get_int_or("playerGameType", self.game_mode.id()));
        // ServerPlayer reads saved abilities before applying its loaded game mode.
        self.update_game_mode_abilities();
        let inventory = self
            .saved
            .get_list("Inventory")
            .cloned()
            .unwrap_or_default();
        let selected = self.saved.get_int_or("SelectedItemSlot", 0);
        self.inventory.load(&inventory);
        self.inventory.set_selected_slot(selected as usize);
        self.first_good = self.position;
        self.last_good = self.position;
        Ok(())
    }
    /// PlayerDataStorage's payload uses the same full player save chain.
    pub fn save(&self) -> CompoundTag {
        self.save_without_id()
    }
    /// Port of Entity#saveWithoutId, Player#addAdditionalSaveData and ServerPlayer.
    pub fn save_without_id(&self) -> CompoundTag {
        let mut tag = self.parent.parent.save_without_id();
        // Player#addAdditionalSaveData writes the version even for predicate NBT.
        tag.insert("DataVersion", Tag::Int(5023));
        tag.insert("recipeBook", self.recipe_book.to_nbt());
        tag.insert("XpP", Tag::Float(self.experience_progress));
        tag.insert("XpLevel", Tag::Int(self.experience_level));
        tag.insert("XpTotal", Tag::Int(self.total_experience));
        tag.insert("Score", Tag::Int(self.score()));
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
        self.parent.parent.add_additional_save_data(&mut tag);
        tag.insert("OnGround", Tag::Byte(i8::from(self.on_ground)));
        tag.insert(
            "Inventory",
            Tag::List(self.inventory.save().expect("canonical inventory stacks")),
        );
        tag.insert(
            "SelectedItemSlot",
            Tag::Int(self.inventory.selected_slot() as i32),
        );
        tag.insert("foodLevel", Tag::Int(20));
        tag.insert("foodSaturationLevel", Tag::Float(5.));
        let creative = matches!(self.game_mode, GameType::Creative | GameType::Spectator);
        let mut abilities = CompoundTag::new();
        abilities.insert("invulnerable", Tag::Byte(i8::from(creative)));
        abilities.insert("mayfly", Tag::Byte(i8::from(creative)));
        abilities.insert("flying", Tag::Byte(i8::from(self.flying)));
        abilities.insert("instabuild", Tag::Byte(i8::from(self.instabuild)));
        abilities.insert("mayBuild", Tag::Byte(i8::from(self.may_build)));
        abilities.insert("flySpeed", Tag::Float(0.05));
        abilities.insert("walkSpeed", Tag::Float(0.1));
        tag.insert("abilities", Tag::Compound(abilities));
        tag
    }
}
/// Embedded-parent projection preserving the bring-up's public field access.
impl std::ops::Deref for ServerPlayer {
    type Target = Avatar;
    fn deref(&self) -> &Self::Target {
        &self.parent
    }
}
impl std::ops::DerefMut for ServerPlayer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.parent
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
/// Server-owned predicates used by the listener's movement checks. The tick-rate
/// manager is canonical; this value never stores another freeze counter.
pub struct MovementChecks<'a> {
    pub tick_rate: &'a crate::level::tick_rate::TickRateManager,
    pub singleplayer_owner: bool,
    pub changing_dimension: bool,
    pub player_movement_check: bool,
    pub elytra_movement_check: bool,
    pub post_impulse_grace: bool,
}
impl<'a> MovementChecks<'a> {
    pub fn dedicated(tick_rate: &'a crate::level::tick_rate::TickRateManager) -> Self {
        Self {
            tick_rate,
            singleplayer_owner: false,
            changing_dimension: false,
            player_movement_check: true,
            elytra_movement_check: true,
            post_impulse_grace: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PlayerMovement {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
}
/// Compatibility entry for the bring-up's normal-rate dedicated-server path.
/// Packet routing supplies the canonical manager and packet collision flag to
/// move_player_with_checks; all geometry is the actual Entity#move implementation.
pub fn move_player(
    level: &mut Level,
    entity: crate::entity::EntityRef,
    target: Vec3,
    yaw: f32,
    pitch: f32,
    on_ground: bool,
) -> Result<MovementResult, LevelError> {
    let tick_rate = crate::level::tick_rate::TickRateManager::default();
    move_player_with_checks(
        level,
        entity,
        PlayerMovement {
            position: target,
            yaw,
            pitch,
            on_ground,
            horizontal_collision: false,
        },
        &MovementChecks::dedicated(&tick_rate),
    )
}
/// Port of ServerGamePacketListenerImpl#handlePlayerPositionChange. Missing
/// ownership remains a synchronous-load error, including correction queries.
pub fn move_player_with_checks(
    level: &mut Level,
    entity: crate::entity::EntityRef,
    input: PlayerMovement,
    checks: &MovementChecks<'_>,
) -> Result<MovementResult, LevelError> {
    use crate::entity::{
        Entity, EntityData,
        movement::{self, MoverType},
    };
    if [input.position.x, input.position.y, input.position.z]
        .iter()
        .any(|v| v.is_nan())
        || !input.yaw.is_finite()
        || !input.pitch.is_finite()
    {
        return Ok(MovementResult::Invalid);
    }
    let target = Vec3::new(
        mth::clamp(input.position.x, -30_000_000.0, 30_000_000.0),
        mth::clamp(input.position.y, -20_000_000.0, 20_000_000.0),
        mth::clamp(input.position.z, -30_000_000.0, 30_000_000.0),
    );
    let yaw = mth::wrap_degrees_f32(input.yaw);
    let pitch = mth::wrap_degrees_f32(input.pitch);
    let player = level.entity::<ServerPlayer>(entity)?;
    if player.vehicle.get().is_some() {
        let player = level.entity_mut::<ServerPlayer>(entity)?;
        player.set_y_rot(yaw);
        player.set_x_rot(pitch);
        return Ok(MovementResult::Accepted);
    }
    let start = player.position;
    let old_box = player.bounding_box;
    let distance = target.distance_to_sqr(player.first_good);
    let expected = player.velocity.length_sqr();
    if player.pose() == ferraloom_protocol::entity_data::Pose::Sleeping {
        if distance > 1.0 {
            let player = level.entity_mut::<ServerPlayer>(entity)?;
            player.set_y_rot(yaw);
            player.set_x_rot(pitch);
            return Ok(MovementResult::Wrongly);
        }
        return Ok(MovementResult::Accepted);
    }
    let fall_flying = player.shared_flag(7);
    let mode = player.game_mode;
    if checks.tick_rate.runs_normally() {
        let player = level.entity_mut::<ServerPlayer>(entity)?;
        player.received_moves = player.received_moves.wrapping_add(1);
        let packets = if player.received_moves > 5 {
            1
        } else {
            player.received_moves
        };
        if !checks.singleplayer_owner
            && !checks.changing_dimension
            && checks.player_movement_check
            && (!fall_flying || checks.elytra_movement_check)
        {
            let limit = if fall_flying { 300.0_f32 } else { 100.0_f32 };
            if distance - expected > f64::from(limit * packets as f32) {
                return Ok(MovementResult::TooQuickly);
            }
        }
    }
    let requested = target - level.entity::<ServerPlayer>(entity)?.last_good;
    let moved_up = requested.y > 0.0;
    if level.entity::<EntityData>(entity)?.on_ground && !input.on_ground && moved_up {
        jump_from_ground(entity, level)?;
    }
    PlayerBehavior.move_by(entity, level, MoverType::Player, requested)?;
    let discrepancy = target - level.entity::<EntityData>(entity)?.position;
    // Vanilla's OR condition clears every finite vertical discrepancy.
    let wrongly = discrepancy.x * discrepancy.x + discrepancy.z * discrepancy.z > 0.0625
        && !checks.changing_dimension
        && !matches!(mode, GameType::Creative | GameType::Spectator)
        && !checks.post_impulse_grace;
    let no_physics = level.entity::<EntityData>(entity)?.no_physics;
    let accepted = no_physics
        || (!wrongly || !level.no_collision(Some(entity), old_box, false)?)
            && !colliding_with_anything_new(entity, level, old_box, target)?;
    if accepted {
        level.set_entity_pos(entity, target)?;
        {
            let player = level.entity_mut::<ServerPlayer>(entity)?;
            player.set_y_rot(yaw);
            player.set_x_rot(pitch);
            player.horizontal_collision = input.horizontal_collision;
        }
        let delta = target - start;
        movement::set_on_ground(entity, input.on_ground, Some(delta), level)?;
        movement::check_supporting_block(entity, input.on_ground, Some(delta), level)?;
        let pos = movement::get_on_pos(entity, 0.2_f32, level)?;
        let state = level.get_block_state(pos)?;
        movement::check_fall_damage(entity, delta.y, input.on_ground, state, pos, level)?;
        let player = level.entity_mut::<ServerPlayer>(entity)?;
        player.known_movement = delta;
        if moved_up {
            player.fall_distance = 0.0;
        }
        player.last_good = target;
        Ok(MovementResult::Accepted)
    } else {
        level.set_entity_pos(entity, start)?;
        {
            let player = level.entity_mut::<ServerPlayer>(entity)?;
            player.set_y_rot(yaw);
            player.set_x_rot(pitch);
            player.velocity = Vec3::ZERO;
        }
        movement::check_supporting_block(entity, input.on_ground, Some(Vec3::ZERO), level)?;
        let pos = movement::get_on_pos(entity, 0.2_f32, level)?;
        let state = level.get_block_state(pos)?;
        movement::check_fall_damage(entity, 0.0, input.on_ground, state, pos, level)?;
        level
            .entity_mut::<EntityData>(entity)?
            .remove_latest_movement_recording();
        Ok(MovementResult::Wrongly)
    }
}
fn colliding_with_anything_new(
    entity: crate::entity::EntityRef,
    level: &Level,
    old: ferraloom_util::Aabb,
    target: Vec3,
) -> Result<bool, LevelError> {
    use crate::{
        block::BlockCollisionContext,
        entity::{EntityData, collision},
    };
    use ferraloom_util::shapes::{self, BooleanOp};
    let base = level.entity::<EntityData>(entity)?;
    let epsilon = f64::from(1e-5_f32);
    let new = base
        .bounding_box
        .move_vec(target - base.position)
        .deflate(epsilon, epsilon, epsilon);
    let old_shape = shapes::create_aabb(old.deflate(epsilon, epsilon, epsilon));
    let mut context = super::collision_context::EntityCollisionView::capture(entity, level, false)?;
    context.entity_bottom = old.min_y;
    context.placement = true;
    let mut shapes = collision::entity_collisions(level, Some(entity), new)?;
    shapes.extend(
        collision::block_collisions_from_context(
            level,
            &BlockCollisionContext::Entity(context),
            new,
        )?
        .into_iter()
        .map(|(_, shape)| shape),
    );
    Ok(shapes
        .iter()
        .any(|shape| !shapes::join_is_not_empty(shape, &old_shape, BooleanOp::And)))
}
/// Port of LivingEntity#jumpFromGround with the player's base jump attribute.
/// Effect/attribute ports feed the same primitive when those modifiers land.
fn jump_from_ground(entity: crate::entity::EntityRef, level: &mut Level) -> Result<(), LevelError> {
    let power = 0.42_f32 * crate::entity::movement::block_jump_factor(entity, level)?;
    if power <= 1e-5_f32 {
        return Ok(());
    }
    let player = level.entity_mut::<ServerPlayer>(entity)?;
    player.velocity.y = ferraloom_util::java::max(f64::from(power), player.velocity.y);
    if player.shared_flag(3) {
        let angle = player.yaw * 0.017_453_292_f32;
        player.velocity = player.velocity.add_xyz(
            f64::from(-mth::sin(f64::from(angle))) * 0.2,
            0.0,
            f64::from(mth::cos(f64::from(angle))) * 0.2,
        );
    }
    player.needs_sync = true;
    Ok(())
}
pub struct PlayerBehavior;
impl crate::entity::Entity for PlayerBehavior {
    /// Port of PlayerEquipment#get's selected main-hand projection.
    fn item_by_slot<'level>(
        &self,
        entity: crate::entity::EntityRef,
        slot: ferraloom_protocol::component::EquipmentSlot,
        level: &'level Level,
    ) -> Result<Option<&'level ferraloom_protocol::component::ItemStack>, LevelError> {
        let player = level.entity::<ServerPlayer>(entity)?;
        Ok(Some(
            if slot == ferraloom_protocol::component::EquipmentSlot::Mainhand {
                player.get_main_hand_item()
            } else {
                player.equipment.get(slot)
            },
        ))
    }
    fn can_freeze(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        super::living::freezing::can_freeze(self, entity, level)
    }
    fn name(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_protocol::chat::Component, LevelError> {
        Ok(ferraloom_protocol::chat::Component::literal(
            &level.entity::<ServerPlayer>(entity)?.name,
        ))
    }
    fn display_name(
        &self,
        entity: EntityRef,
        level: &Level,
        scoreboard: &crate::scoreboard::Scoreboard,
    ) -> Result<ferraloom_protocol::chat::Component, LevelError> {
        super::names::player_display_name(self, entity, level, scoreboard)
    }
    fn y_head_rot(
        &self,
        entity: crate::entity::EntityRef,
        level: &Level,
    ) -> Result<f32, LevelError> {
        Ok(level.entity::<ServerPlayer>(entity)?.y_head_rot)
    }
    fn dimensions(
        &self,
        entity: crate::entity::EntityRef,
        pose: ferraloom_protocol::entity_data::Pose,
        level: &Level,
    ) -> Result<crate::entity::EntityDimensions, LevelError> {
        super::living::dimensions(self, entity, pose, level)
    }
    fn default_dimensions(
        &self,
        _entity: crate::entity::EntityRef,
        pose: ferraloom_protocol::entity_data::Pose,
        _level: &Level,
    ) -> Result<crate::entity::EntityDimensions, LevelError> {
        Ok(Avatar::default_dimensions(pose))
    }
    fn push(
        &self,
        entity: crate::entity::EntityRef,
        other: crate::entity::EntityRef,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        crate::entity::pushing::living_push(self, entity, other, level)
    }
    fn is_pushable(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<bool, crate::level::LevelError> {
        crate::entity::living::climbable::is_pushable(self, entity, level)
    }
    /// Port of ServerPlayer#pushEntities: frozen players do not push neighbors.
    fn push_entities(
        &self,
        entity: crate::entity::EntityRef,
        level: &mut Level,
        tick_rate: &crate::level::tick_rate::TickRateManager,
    ) -> Result<(), LevelError> {
        if tick_rate.runs_normally() {
            crate::entity::pushing::living_push_entities(self, entity, level)?;
        }
        Ok(())
    }
    fn passenger_riding_position(
        &self,
        entity: crate::entity::EntityRef,
        passenger: crate::entity::EntityRef,
        level: &Level,
    ) -> Result<Vec3, LevelError> {
        super::living::passenger_riding_position(self, entity, passenger, level)
    }
    /// Player#rideTick's server dismount gate precedes LivingEntity#rideTick.
    fn ride_tick(
        &self,
        entity: crate::entity::EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        self.ride_tick_with_tick_rate(
            entity,
            level,
            &crate::level::tick_rate::TickRateManager::default(),
        )
    }

    fn ride_tick_with_tick_rate(
        &self,
        entity: crate::entity::EntityRef,
        level: &mut Level,
        tick_rate: &crate::level::tick_rate::TickRateManager,
    ) -> Result<(), LevelError> {
        let player = level.entity::<ServerPlayer>(entity)?;
        if player.shared_flag(1) && player.vehicle.get().is_some() {
            crate::entity::stop_riding(entity, level)
        } else {
            crate::entity::living_ride_tick_with_tick_rate(self, entity, level, tick_rate)
        }
    }

    /// Player#getMoveSimulationType is AUTHORITATIVE_SIDE_AND_SERVER.
    fn can_simulate_movement(
        &self,
        _entity: crate::entity::EntityRef,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }

    fn on_above_bubble_column(
        &self,
        entity: crate::entity::EntityRef,
        drag_down: bool,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !level.entity::<ServerPlayer>(entity)?.flying {
            crate::entity::bubble_column::on_above_bubble_column(
                self, entity, drag_down, pos, level,
            )?;
        }
        Ok(())
    }

    fn on_inside_bubble_column(
        &self,
        entity: crate::entity::EntityRef,
        drag_down: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !level.entity::<ServerPlayer>(entity)?.flying {
            crate::entity::bubble_column::on_inside_bubble_column(self, entity, drag_down, level)?;
        }
        Ok(())
    }

    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        references: &mut Vec<crate::entity::EntityRef>,
    ) {
        crate::entity::trace_base_entity_refs(state.as_entity(), references);
    }

    fn known_movement(
        &self,
        entity: crate::entity::EntityRef,
        level: &Level,
    ) -> Result<Vec3, LevelError> {
        super::teleport::player_known_movement(entity, level)
    }
    fn teleport_relative(
        &self,
        entity: crate::entity::EntityRef,
        delta: Vec3,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::teleport::teleport_player(
            entity,
            super::teleport::position_move_rotation(delta, Vec3::ZERO, 0.0, 0.0),
            super::teleport::Relative::ALL,
            level,
        )
    }
    fn is_alive(
        &self,
        entity: crate::entity::EntityRef,
        level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(!level.entity::<ServerPlayer>(entity)?.removed
            && level.entity::<ServerPlayer>(entity)?.health() > 0.0)
    }
    fn is_spectator(
        &self,
        entity: crate::entity::EntityRef,
        level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(level.entity::<ServerPlayer>(entity)?.game_mode == GameType::Spectator)
    }
    /// Port of Player#isSwimming. The final read invokes Entity's parent body.
    fn is_swimming(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        if level.entity::<ServerPlayer>(entity)?.flying || self.is_spectator(entity, level)? {
            return Ok(false);
        }
        Ok(level.entity::<super::EntityData>(entity)?.is_swimming())
    }
    fn sound_source(&self) -> ferraloom_protocol::play::SoundSource {
        ferraloom_protocol::play::SoundSource::Players
    }
    fn swim_sound(&self) -> ferraloom_data::registries::sound_event::Id {
        ferraloom_data::registries::sound_event::Id::ENTITY_PLAYER_SWIM
    }
    fn swim_splash_sound(&self) -> ferraloom_data::registries::sound_event::Id {
        ferraloom_data::registries::sound_event::Id::ENTITY_PLAYER_SPLASH
    }
    fn swim_high_speed_splash_sound(&self) -> ferraloom_data::registries::sound_event::Id {
        ferraloom_data::registries::sound_event::Id::ENTITY_PLAYER_SPLASH_HIGH_SPEED
    }
    fn play_sound(
        &self,
        entity: crate::entity::EntityRef,
        sound: ferraloom_data::registries::sound_event::Id,
        volume: f32,
        pitch: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        crate::entity::sounds::play_sound(self, entity, sound, volume, pitch, true, level)
    }
    fn play_step_sound(
        &self,
        entity: crate::entity::EntityRef,
        pos: ferraloom_util::BlockPos,
        state: ferraloom_data::BlockState,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        crate::entity::sounds::player_step_sound(self, entity, pos, state, level)
    }
    fn do_water_splash_effect(
        &self,
        entity: crate::entity::EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !self.is_spectator(entity, level)? {
            crate::entity::sounds::water_splash_effect(self, entity, level)?;
        }
        Ok(())
    }
    fn fire_immune_ticks(&self) -> i32 {
        20
    }
    fn default_gravity(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f64, crate::level::LevelError> {
        crate::entity::living::attribute(
            entity,
            ferraloom_data::registries::attribute::Id::GRAVITY,
            level,
        )
    }
    fn max_up_step(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f32, crate::level::LevelError> {
        Ok(crate::entity::living::attribute(
            entity,
            ferraloom_data::registries::attribute::Id::STEP_HEIGHT,
            level,
        )? as f32)
    }
    fn bounciness(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f64, crate::level::LevelError> {
        crate::entity::living::attribute(
            entity,
            ferraloom_data::registries::attribute::Id::BOUNCINESS,
            level,
        )
    }
    fn air_drag(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f32, crate::level::LevelError> {
        crate::entity::living::air_drag(entity, level)
    }
    fn block_speed_factor(
        &self,
        entity: crate::entity::EntityRef,
        level: &crate::level::Level,
    ) -> Result<f32, crate::level::LevelError> {
        crate::entity::living::block_speed_factor(entity, level)
    }
    fn ignite_for_ticks(
        &self,
        entity: crate::entity::EntityRef,
        ticks: i32,
        level: &mut crate::level::Level,
    ) -> Result<(), crate::level::LevelError> {
        crate::entity::living::ignite_for_ticks(entity, ticks, level)
    }
    fn tick(&self, entity: crate::entity::EntityRef, level: &mut Level) -> Result<(), LevelError> {
        crate::inventory::tick_container_menu(entity, level)?;
        let (spectator, passenger) = {
            let player = level.entity::<ServerPlayer>(entity)?;
            (
                player.game_mode == GameType::Spectator,
                player.vehicle.get().is_some(),
            )
        };
        level
            .entity_mut::<crate::entity::EntityData>(entity)?
            .no_physics = spectator;
        if spectator || passenger {
            crate::entity::movement::set_on_ground(entity, false, None, level)?;
        }
        crate::entity::base_tick(self, entity, level)?;
        // Known slice-1 gap before Player#aiStep's touch point: LivingEntity
        // server travel, block effects, freezing/damage and pushing are unported.
        // Slice 2 ports these predecessors; live pickup parity remains pending.
        contact::touch_entities(entity, level)?;
        cooldowns::tick(entity, level)?;
        level
            .entity_mut::<ServerPlayer>(entity)?
            .update_interaction_range_attributes();
        Ok(())
    }

    fn maybe_back_off_from_edge(
        &self,
        entity: crate::entity::EntityRef,
        level: &Level,
        delta: Vec3,
        mover: crate::entity::movement::MoverType,
    ) -> Result<Vec3, LevelError> {
        let max_step = self.max_up_step(entity, level)?;
        crate::entity::movement::player_back_off_from_edge(
            entity,
            level,
            delta,
            mover,
            max_step,
            level.entity::<ServerPlayer>(entity)?.flying,
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn player_load_restores_immutable_profile_identity_after_entity_nbt() {
        use super::*;
        let mut actual = Vec::new();
        for scenario in 0..3 {
            let mut player = ServerPlayer::new(
                Uuid::new(1, 2),
                "profile".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            );
            player.uuid = Uuid::new(5, 6);
            let mut tag = CompoundTag::new();
            if scenario != 0 {
                let saved = if scenario == 1 {
                    Uuid::new(3, 4)
                } else {
                    Uuid::new(1, 2)
                };
                tag.insert("UUID", Tag::IntArray(saved.to_int_array().to_vec()));
            }
            player.try_load(tag).unwrap();
            let saved = player.save();
            let Some(Tag::IntArray(words)) = saved.get("UUID") else {
                panic!("saved UUID")
            };
            let saved = Uuid::from_int_array(words.as_slice().try_into().unwrap());
            actual.push(format!(
                "player-profile-uuid\t{scenario}\t{},{},{}",
                player.uuid,
                player.profile_id(),
                saved
            ));
        }
        let expected: Vec<_> = include_str!("../../../tests/java/entity/vectors.tsv")
            .lines()
            .filter(|line| line.starts_with("player-profile-uuid\t"))
            .collect();
        assert_eq!(actual, expected);
    }
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
    fn player_health_persistence_hydrates_attributes_before_clamping_health() {
        let mut player = ServerPlayer::new(
            Uuid::new(1, 2),
            "health".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        );
        player
            .attributes
            .get_instance(ferraloom_data::registries::attribute::Id::MAX_HEALTH)
            .unwrap()
            .set_base_value(40.0);
        player.set_health(35.0);
        let saved = player.save();
        let mut loaded = ServerPlayer::new(
            Uuid::new(1, 2),
            "health".into(),
            2,
            Vec3::ZERO,
            GameType::Survival,
        );
        loaded
            .try_load_with_recipes(
                saved,
                &crate::crafting::RecipeManager::new(Vec::new()).unwrap(),
            )
            .unwrap();
        assert_eq!(loaded.health(), 35.0);
        assert_eq!(loaded.max_health(), 40.0);
        assert_eq!(
            *loaded.entity_data.get(super::super::synched_data::HEALTH),
            35.0
        );
        assert_eq!(
            loaded.parent.main_arm(),
            ferraloom_protocol::entity_data::HumanoidArm::Right
        );
    }

    #[test]
    fn player_recipe_book_load_requires_live_manager_and_filters_unknown_ids() {
        use super::super::persistence::EntityLoadError;
        use crate::crafting::{Recipe, RecipeHolder, RecipeManager};
        use ferraloom_data::{
            Item,
            datapack::{TagSet, Tags},
        };
        let tags = TagSet::<Item>::from_tags(
            Tags {
                values: Default::default(),
                diagnostics: Vec::new(),
            },
            ferraloom_data::ITEMS.len(),
        )
        .unwrap();
        let kept: ferraloom_data::datapack::Identifier = "minecraft:kept".parse().unwrap();
        let manager = RecipeManager::new(vec![RecipeHolder {
            id: kept.clone(),
            value: Recipe::decode(
                &serde_json::json!({"type":"crafting_special_repairitem"}),
                &tags,
            )
            .unwrap(),
        }])
        .unwrap();
        let make_player = || {
            ServerPlayer::new(
                Uuid::new(1, 2),
                "recipe".into(),
                1,
                Vec3::ZERO,
                GameType::Survival,
            )
        };
        let mut original = make_player();
        original.recipe_book.add(kept.clone());
        let mut saved = original.save();
        let book = saved
            .get_mut("recipeBook")
            .unwrap()
            .as_compound_mut()
            .unwrap();
        book.insert("isGuiOpen", Tag::Byte(1));
        let ids = Tag::List(ListTag(vec![
            Tag::String("minecraft:kept".into()),
            Tag::String("minecraft:unknown".into()),
        ]));
        book.insert("recipes", ids.clone());
        book.insert("toBeDisplayed", ids);
        let mut loaded = make_player();
        assert_eq!(
            loaded.try_load(saved.clone()),
            Err(EntityLoadError::RecipeManagerRequired)
        );
        loaded.try_load_with_recipes(saved, &manager).unwrap();
        assert_eq!(loaded.recipe_book.known(), std::slice::from_ref(&kept));
        assert!(loaded.recipe_book.highlighted(&kept));
        assert!(loaded.recipe_book.settings.0[0].open);
        let mut round_trip = make_player();
        round_trip
            .try_load_with_recipes(loaded.save(), &manager)
            .unwrap();
        assert_eq!(round_trip.recipe_book.to_nbt(), loaded.recipe_book.to_nbt());
    }

    #[test]
    fn malformed_optional_recipe_book_is_diagnosed_and_keeps_the_existing_book() {
        let manager = crate::crafting::RecipeManager::new(Vec::new()).unwrap();
        let mut player = ServerPlayer::new(
            Uuid::new(1, 2),
            "recipe".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        );
        let before = player.recipe_book.to_nbt();
        let mut saved = player.save();
        saved.insert(
            "recipeBook",
            Tag::String("invalid optional packed data".into()),
        );
        player.try_load_with_recipes(saved, &manager).unwrap();
        assert_eq!(player.recipe_book.to_nbt(), before);
    }
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
    fn display_name_decoration_preserves_formatted_input_like_java() {
        use ferraloom_protocol::chat::{ClickEvent, Component, TextColor};
        let player = ServerPlayer::new(
            Uuid::new(0x0123456789abcdef, 0x0123456789abcdef),
            "Doc".into(),
            1,
            Vec3::ZERO,
            GameType::Survival,
        );
        let mut name = Component::literal("Styled");
        name.extra.push(Component::literal(" tail"));
        name.style.color = Some(TextColor::parse("red").unwrap());
        name.style.bold = Some(true);
        name.style.click_event = Some(ClickEvent::RunCommand("/old".into()));
        name.style.insertion = Some("old".into());
        let expected = Component::from_json(include_str!("decorated-name.json")).unwrap();
        assert_eq!(player.decorate_display_name_component(name), expected);
    }
    #[test]
    fn movement_sweep_cannot_tunnel_through_terrain() {
        static BEHAVIORS: Behaviors = Behaviors::EMPTY;
        let biome = Strategy::biomes(Arc::new(IdMap::new(vec![BiomeId(0)]).unwrap()));
        let section = LevelChunkSection::new(Strategy::block_states(), biome, BiomeId(0));
        let mut level = Level::new(
            Region::new(&BEHAVIORS),
            crate::level::LevelContext::fixture(0, 63),
        );
        crate::entity::tests::tags(&mut level);
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
