//! ServerPlayer gameplay state. Connections retain handles, never another player copy.
mod drop_item;
pub use drop_item::{create_item_stack_to_drop, drop_item, drop_item_stack};
pub mod abilities;
mod ai_step;
mod contact;
pub mod cooldowns;
pub mod equipment;
mod experience;
pub mod food;
pub mod game_mode;
mod interaction;
pub(crate) mod item_tick;
mod notifications;
pub mod shoulders;
pub use notifications::{build_limit_packet, send_build_limit_message};
mod spawn;
pub use spawn::SavedPosition;
mod stuck;
mod tick;
pub use tick::{award_stat, player_do_tick_with_tick_rate, reset_stat};
pub mod travel;
pub use experience::{give_experience_levels, give_experience_points, send_experience_update};
pub use interaction::swing_and_reset_attack_strength;
pub mod inventory;
use crate::{
    entity::{Avatar, EntityData, EntityDataView, EntityRef, EntityState},
    level::{Level, LevelError},
};
use ferraloom_nbt::{CompoundTag, ListTag, Tag};
use ferraloom_protocol::component::{ItemStack, Persistent};
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
    pub food_data: food::FoodData,
    pub jump_trigger_time: i32,
    /// ServerPlayer's native dimension-transfer guard; no cross-dimension
    /// transition producer is ported yet, so it starts and remains false.
    pub changing_dimension: bool,
    pub hurt_dir: f32,
    pub(crate) spawn_extra_particles_on_fall: bool,
    starting_to_fall_position: Option<Vec3>,
    shoulder_entity_left: CompoundTag,
    shoulder_entity_right: CompoundTag,
    time_entity_sat_on_shoulder: i64,
    pub experience_level: i32,
    pub experience_progress: f32,
    pub total_experience: i32,
    /// Player#takeXpDelay is transient and decreases at Player#tick's start.
    pub take_xp_delay: i32,
    last_level_up_time: i32,
    last_sent_experience: i32,
    last_sent_health: f32,
    last_sent_food: i32,
    last_food_saturation_zero: bool,
    last_recorded_health_and_absorption: f32,
    last_recorded_food_level: i32,
    last_recorded_air_level: i32,
    last_recorded_armor: i32,
    last_recorded_experience: i32,
    last_recorded_level: i32,
    pub name: String,
    pub known_movement: Vec3,
    pub game_mode: GameType,
    pub block_breaking: game_mode::BlockBreakingState,
    pub abilities: abilities::Abilities,
    pub last_client_input: ferraloom_protocol::play::Input,
    pub text_filtering_enabled: bool,
    pub cooldowns: cooldowns::ServerItemCooldowns,
    pub(crate) last_item_in_main_hand: ItemStack,
    interaction_range_modifiers: [std::sync::Arc<super::ai::attributes::AttributeModifier>; 2],
    pub view_distance: i32,
    /// Port of ServerPlayer#chunkTrackingView: the canonical desired chunk set.
    pub chunk_tracking_view: std::collections::HashSet<ChunkPos>,
    /// Port of ChunkTrackingView.Positioned#center, absent for EMPTY.
    pub chunk_tracking_center: Option<ChunkPos>,
    /// Port of ChunkTrackingView.Positioned#viewDistance, captured with its center.
    pub chunk_tracking_distance: Option<i32>,
    /// Port of ServerPlayer#getLastSectionPos, set by ChunkMap#updatePlayerPos.
    pub chunk_tracking_section: Option<ferraloom_util::SectionPos>,
    /// Port of ServerGamePacketListenerImpl#chunkSender: canonical pending/sent
    /// chunks, quotas and acknowledgments, owned with the gameplay player.
    pub chunk_sender: crate::level::chunk_map::PlayerChunkSender,
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

    /// Mirrors the vanilla Abilities.mayBuild field.
    pub fn may_build(&self) -> bool {
        self.abilities.may_build
    }

    /// Mirrors the vanilla Abilities.instabuild field.
    pub fn instabuild(&self) -> bool {
        self.abilities.instabuild
    }

    /// Mirrors vanilla Abilities#getFlyingSpeed, independent of movement speed.
    pub fn ability_flying_speed(&self) -> f32 {
        self.abilities.flying_speed()
    }
    pub fn set_ability_flying_speed(&mut self, speed: f32) {
        self.abilities.set_flying_speed(speed);
    }
    /// Mirrors vanilla Abilities#getWalkingSpeed.
    pub fn ability_walking_speed(&self) -> f32 {
        self.abilities.walking_speed()
    }
    pub fn set_ability_walking_speed(&mut self, speed: f32) {
        self.abilities.set_walking_speed(speed);
    }
    /// Port of Player#getFlyingSpeed. Passenger movement uses the ordinary
    /// sprint-dependent speed even when the independent flying ability is set.
    pub fn flying_movement_speed(&self) -> f32 {
        if self.abilities.flying && self.vehicle.get().is_none() {
            if self.shared_flag(3) {
                self.abilities.flying_speed() * 2.0
            } else {
                self.abilities.flying_speed()
            }
        } else if self.shared_flag(3) {
            0.025999999
        } else {
            0.02
        }
    }

    /// Port of server-side Player#causeFoodExhaustion, using the independent ability.
    pub fn cause_food_exhaustion(&mut self, amount: f32) {
        if !self.abilities.invulnerable {
            self.food_data.add_exhaustion(amount);
        }
    }
    /// Port of Player#preventsBlockDrops, independent of the game mode.
    pub fn prevents_block_drops(&self) -> bool {
        self.abilities.instabuild
    }

    /// Port of GameType#updatePlayerAbilities over canonical Abilities.
    /// Runtime game-mode transitions also handle their packets and other effects.
    pub fn update_game_mode_abilities(&mut self) {
        self.abilities.update_game_mode(self.game_mode);
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
            food_data: Default::default(),
            jump_trigger_time: 0,
            changing_dimension: false,
            hurt_dir: 0.0,
            spawn_extra_particles_on_fall: false,
            starting_to_fall_position: None,
            shoulder_entity_left: CompoundTag::new(),
            shoulder_entity_right: CompoundTag::new(),
            time_entity_sat_on_shoulder: 0,
            experience_level: 0,
            experience_progress: 0.0,
            total_experience: 0,
            take_xp_delay: 0,
            last_level_up_time: 0,
            last_sent_experience: -99_999_999,
            last_sent_health: -1.0e8,
            last_sent_food: -99_999_999,
            last_food_saturation_zero: true,
            last_recorded_health_and_absorption: f32::from_bits(1),
            last_recorded_food_level: i32::MIN,
            last_recorded_air_level: i32::MIN,
            last_recorded_armor: i32::MIN,
            last_recorded_experience: i32::MIN,
            last_recorded_level: i32::MIN,
            known_movement: Vec3::ZERO,
            game_mode,
            block_breaking: Default::default(),
            abilities: {
                let mut abilities = abilities::Abilities::default();
                abilities.update_game_mode(game_mode);
                abilities
            },
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
            last_item_in_main_hand: ItemStack::default(),
            interaction_range_modifiers: interaction::range_modifiers(),
            view_distance: 10,
            chunk_tracking_view: Default::default(),
            chunk_tracking_center: None,
            chunk_tracking_distance: None,
            chunk_tracking_section: None,
            chunk_sender: Default::default(),
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
    /// Port of ServerGamePacketListenerImpl#resetPosition, before player doTick.
    pub fn reset_position(&mut self) {
        self.first_good = self.position;
        self.last_good = self.position;
    }
    /// Port of tickPlayer's listener bookkeeping after doTick and absSnapTo.
    pub fn finish_connection_tick(&mut self) {
        self.connection_ticks += 1;
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
        self.parent.parent.load_nbt_with_absorption_setter(
            tag.clone(),
            super::living::damage_state::player_internal_set_absorption_amount,
        )?;
        self.read_player_additional_save_data(&tag)
    }
    pub(crate) fn read_player_additional_save_data(
        &mut self,
        tag: &CompoundTag,
    ) -> Result<(), super::persistence::EntityLoadError> {
        // Player#readAdditionalSaveData restores the profile UUID after super.
        self.uuid = self.profile_id;
        self.food_data.load(tag);
        self.set_shoulder_entity_left(
            tag.get_compound("ShoulderEntityLeft")
                .cloned()
                .unwrap_or_default(),
        );
        self.set_shoulder_entity_right(
            tag.get_compound("ShoulderEntityRight")
                .cloned()
                .unwrap_or_default(),
        );
        self.experience_progress = tag.get_float_or("XpP", 0.0);
        self.experience_level = tag.get_int_or("XpLevel", 0);
        self.total_experience = tag.get_int_or("XpTotal", 0);
        self.set_score(tag.get_int_or("Score", 0));
        if let Some(tag) = tag.get("abilities") {
            match abilities::PackedAbilities::from_nbt(tag) {
                Ok(packed) => self.abilities.apply(packed),
                Err(error) => {
                    eprintln!("Failed to load abilities for player {}: {error}", self.uuid)
                }
            }
        }
        let walking_speed = self.abilities.walking_speed();
        self.attributes
            .get_instance(ferraloom_data::registries::attribute::Id::MOVEMENT_SPEED)
            .expect("player supplier defines MOVEMENT_SPEED")
            .set_base_value(f64::from(walking_speed));
        self.game_mode = GameType::from_id(tag.get_int_or("playerGameType", self.game_mode.id()));
        // ServerPlayer reads saved abilities before applying its loaded game mode.
        self.update_game_mode_abilities();
        let inventory = tag.get_list("Inventory").cloned().unwrap_or_default();
        let selected = tag.get_int_or("SelectedItemSlot", 0);
        self.inventory.load(&inventory);
        self.inventory.set_selected_slot(selected as usize);
        self.first_good = self.position;
        self.last_good = self.position;
        Ok(())
    }
    /// PlayerDataStorage's payload uses the same full player save chain.
    pub fn save(&self, level: &Level) -> Result<CompoundTag, LevelError> {
        self.save_without_id(level)
    }
    /// Port of Entity#saveWithoutId, Player#addAdditionalSaveData and ServerPlayer.
    pub fn save_without_id(&self, level: &Level) -> Result<CompoundTag, LevelError> {
        let mut tag = self.parent.parent.save_without_id();
        // Player#addAdditionalSaveData writes the version even for predicate NBT.
        tag.insert("DataVersion", Tag::Int(5023));
        tag.insert("recipeBook", self.recipe_book.to_nbt());
        tag.insert("XpP", Tag::Float(self.experience_progress));
        tag.insert("XpLevel", Tag::Int(self.experience_level));
        tag.insert("XpTotal", Tag::Int(self.total_experience));
        tag.insert("Score", Tag::Int(self.score()));
        tag.insert("UUID", Tag::IntArray(self.uuid.to_int_array().to_vec()));
        tag.insert(
            "Dimension",
            Tag::String(level.dimension_key()?.to_string().into()),
        );
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
        self.parent.parent.add_additional_save_data(
            &mut tag,
            level,
            super::living::damage_state::player_absorption_amount,
        )?;
        tag.insert("OnGround", Tag::Byte(i8::from(self.on_ground)));
        tag.insert(
            "Inventory",
            Tag::List(
                self.inventory
                    .save()
                    .map_err(|_| LevelError::Inventory("invalid saved inventory"))?,
            ),
        );
        tag.insert(
            "SelectedItemSlot",
            Tag::Int(self.inventory.selected_slot() as i32),
        );
        self.food_data.save(&mut tag);
        tag.remove("ShoulderEntityLeft");
        tag.remove("ShoulderEntityRight");
        if !self.shoulder_entity_left().is_empty() {
            tag.insert(
                "ShoulderEntityLeft",
                Tag::Compound(self.shoulder_entity_left().clone()),
            );
        }
        if !self.shoulder_entity_right().is_empty() {
            tag.insert(
                "ShoulderEntityRight",
                Tag::Compound(self.shoulder_entity_right().clone()),
            );
        }
        tag.insert(
            "abilities",
            self.abilities
                .pack()
                .to_nbt()
                .map_err(|_| LevelError::Inventory("invalid saved abilities"))?,
        );
        Ok(tag)
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
        // Entity#doCheckFallDamage never reads terrain while touching unloaded chunks.
        if !tick::touching_unloaded_chunk(entity, level)? {
            let pos = movement::get_on_pos(entity, 0.2_f32, level)?;
            let state = level.get_block_state(pos)?;
            movement::check_fall_damage(entity, delta.y, input.on_ground, state, pos, level)?;
        }
        level.entity_mut::<ServerPlayer>(entity)?.known_movement = delta;
        if moved_up {
            crate::entity::reset_fall_distance(entity, level)?;
        }
        level.entity_mut::<ServerPlayer>(entity)?.last_good = target;
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
        // Entity#doCheckFallDamage never reads terrain while touching unloaded chunks.
        if !tick::touching_unloaded_chunk(entity, level)? {
            let pos = movement::get_on_pos(entity, 0.2_f32, level)?;
            let state = level.get_block_state(pos)?;
            movement::check_fall_damage(entity, 0.0, input.on_ground, state, pos, level)?;
        }
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
/// Dispatch the canonical jump body used by packet-driven ground departure.
/// The ServerPlayer override synchronously awards its stat and exhaustion.
fn jump_from_ground(entity: crate::entity::EntityRef, level: &mut Level) -> Result<(), LevelError> {
    level
        .entity_behavior(entity)?
        .jump_from_ground(entity, level)
}
pub struct PlayerBehavior;
impl crate::entity::Entity for PlayerBehavior {
    /// ServerPlayer#onAttributeUpdated handles RECEIVE before its LivingEntity parent.
    fn on_attribute_updated(
        &self,
        entity: EntityRef,
        attribute: ferraloom_data::registries::attribute::Id,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if attribute == ferraloom_data::registries::attribute::Id::WAYPOINT_RECEIVE_RANGE {
            if super::living::attribute(entity, attribute, level)? > 0.0 {
                level.add_waypoint_player(entity)?;
            } else {
                level.remove_waypoint_player(entity)?;
            }
        }
        super::living::on_attribute_updated_parent(self, entity, attribute, level)
    }
    fn take(
        &self,
        collector: EntityRef,
        entity: EntityRef,
        count: i32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::living::take(collector, entity, count, level)?;
        crate::inventory::broadcast_container_menu(collector, level)?;
        crate::inventory::flush_container_packets(collector, level)
    }
    fn is_invulnerable_to(
        &self,
        entity: EntityRef,
        source: &super::DamageSource,
        level: &Level,
    ) -> Result<bool, LevelError> {
        super::living::hurt::player_is_invulnerable_to(self, entity, source, level)
    }
    fn absorption_amount(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        Ok(level.entity::<ServerPlayer>(entity)?.absorption_amount())
    }
    fn set_absorption_amount(
        &self,
        entity: EntityRef,
        amount: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level
            .entity_mut::<ServerPlayer>(entity)?
            .set_absorption_amount(amount);
        Ok(())
    }
    fn actually_hurt(
        &self,
        entity: EntityRef,
        source: &super::DamageSource,
        damage: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::living::hurt::player_actually_hurt(self, entity, source, damage, level)
    }
    fn hurt_armor(
        &self,
        entity: EntityRef,
        source: &super::DamageSource,
        damage: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        use ferraloom_protocol::component::EquipmentSlot;
        super::living::hurt::hurt_equipment(
            self,
            entity,
            source,
            damage,
            &[
                EquipmentSlot::Feet,
                EquipmentSlot::Legs,
                EquipmentSlot::Chest,
                EquipmentSlot::Head,
            ],
            level,
        )
    }
    fn hurt_helmet(
        &self,
        entity: EntityRef,
        source: &super::DamageSource,
        damage: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::living::hurt::hurt_equipment(
            self,
            entity,
            source,
            damage,
            &[ferraloom_protocol::component::EquipmentSlot::Head],
            level,
        )
    }
    fn hurt_sound(
        &self,
        _entity: EntityRef,
        source: &super::DamageSource,
        _level: &Level,
    ) -> Result<Option<ferraloom_data::registries::sound_event::Id>, LevelError> {
        Ok(Some(source.damage_type().effects.sound()))
    }
    fn death_sound(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<Option<ferraloom_data::registries::sound_event::Id>, LevelError> {
        Ok(Some(
            ferraloom_data::registries::sound_event::Id::ENTITY_PLAYER_DEATH,
        ))
    }
    fn voice_pitch(&self, _entity: EntityRef, _level: &mut Level) -> Result<f32, LevelError> {
        Ok(1.0)
    }
    fn indicate_damage(
        &self,
        entity: EntityRef,
        x: f64,
        z: f64,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::living::hurt::player_indicate_damage(entity, x, z, level)
    }
    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: super::DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        super::living::hurt::player_hurt_server(self, entity, source, damage, level)
    }
    fn on_enter_combat(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        level.entity::<ServerPlayer>(entity)?;
        level.emit(crate::level::effects::LevelEffect::PlayerPacket {
            recipients: vec![entity],
            packet: Box::new(
                ferraloom_protocol::play::clientbound::Packet::PlayerCombatEnter(
                    ferraloom_protocol::play::clientbound::PlayerCombatEnter {},
                ),
            ),
        });
        Ok(())
    }
    fn on_leave_combat(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        let player = level.entity::<ServerPlayer>(entity)?;
        let duration = player.combat_tracker().combat_duration(player.tick_count);
        level.emit(crate::level::effects::LevelEffect::PlayerPacket {
            recipients: vec![entity],
            packet: Box::new(
                ferraloom_protocol::play::clientbound::Packet::PlayerCombatEnd(
                    ferraloom_protocol::play::clientbound::PlayerCombatEnd {
                        duration: ferraloom_protocol::codec::VarInt(duration),
                    },
                ),
            ),
        });
        Ok(())
    }
    fn reset_fall_distance(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        stuck::reset_fall_distance(entity, level)
    }
    fn make_stuck_in_block(
        &self,
        entity: EntityRef,
        state: ferraloom_data::BlockState,
        multiplier: Vec3,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        stuck::make_stuck_in_block(self, entity, state, multiplier, level)
    }
    fn cause_fall_damage(
        &self,
        entity: super::EntityRef,
        level: &mut crate::level::Level,
        distance: f64,
        modifier: f32,
        source: &super::DamageSource,
    ) -> Result<bool, crate::level::LevelError> {
        super::living::fall_damage::player_cause_fall_damage(
            self, entity, level, distance, modifier, source,
        )
    }
    fn check_fall_damage(
        &self,
        entity: EntityRef,
        dy: f64,
        on_ground: bool,
        state: ferraloom_data::BlockState,
        pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::living::fall_damage::player_check_fall_damage(
            self, entity, dy, on_ground, state, pos, level,
        )
    }
    fn on_changed_block(
        &self,
        entity: EntityRef,
        _pos: BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        if !self.is_spectator(entity, level)? {
            super::living::fall_damage::on_changed_block(entity, level)?;
        }
        Ok(())
    }
    fn on_equipped_item_broken(
        &self,
        entity: EntityRef,
        broken: ferraloom_protocol::component::ItemStack,
        slot: ferraloom_protocol::component::EquipmentSlot,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        super::living::on_equipped_item_broken_parent(entity, &broken, slot, level)?;
        award_stat(entity, crate::stats::Stat::Broken(broken.item()), 1, level)
    }
    fn weapon_item<'level>(
        &self,
        entity: EntityRef,
        level: &'level Level,
    ) -> Result<Option<&'level ferraloom_protocol::component::ItemStack>, LevelError> {
        super::living::weapon_item(self, entity, level)
    }
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
    fn set_y_head_rot(
        &self,
        entity: EntityRef,
        value: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.entity_mut::<super::LivingEntity>(entity)?.y_head_rot = value;
        Ok(())
    }
    fn set_y_body_rot(
        &self,
        entity: EntityRef,
        value: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.entity_mut::<super::LivingEntity>(entity)?.y_body_rot = value;
        Ok(())
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
        if !level.entity::<ServerPlayer>(entity)?.abilities.flying {
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
        if !level.entity::<ServerPlayer>(entity)?.abilities.flying {
            crate::entity::bubble_column::on_inside_bubble_column(self, entity, drag_down, level)?;
        }
        Ok(())
    }

    fn trace_entity_refs(
        &self,
        state: &crate::entity::EntityState,
        references: &mut Vec<crate::entity::EntityRef>,
    ) {
        state
            .as_living()
            .expect("Player inherits LivingEntity")
            .trace_damage_entity_refs(references);
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
        if level.entity::<ServerPlayer>(entity)?.abilities.flying
            || self.is_spectator(entity, level)?
        {
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
    fn fall_sounds(&self) -> super::living::FallSounds {
        use ferraloom_data::registries::sound_event::Id as Sound;
        super::living::FallSounds {
            small: Sound::ENTITY_PLAYER_SMALL_FALL,
            big: Sound::ENTITY_PLAYER_BIG_FALL,
        }
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
    fn effective_gravity(&self, entity: EntityRef, level: &Level) -> Result<f64, LevelError> {
        crate::entity::living::travel::effective_gravity(self, entity, level)
    }
    fn travel(&self, entity: EntityRef, input: Vec3, level: &mut Level) -> Result<(), LevelError> {
        travel::travel(self, entity, input, level)
    }
    fn get_speed(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        Ok(crate::entity::living::attribute(
            entity,
            ferraloom_data::registries::attribute::Id::MOVEMENT_SPEED,
            level,
        )? as f32)
    }
    fn get_flying_speed(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        Ok(level
            .entity::<ServerPlayer>(entity)?
            .flying_movement_speed())
    }
    fn can_glide(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        Ok(!level.entity::<ServerPlayer>(entity)?.abilities.flying
            && crate::entity::living::gliding::can_glide(self, entity, level)?)
    }
    fn is_affected_by_fluids(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        Ok(!level.entity::<ServerPlayer>(entity)?.abilities.flying)
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
        let player = level.entity::<ServerPlayer>(entity)?;
        if player.abilities.flying || player.shared_flag(7) {
            Ok(1.0)
        } else {
            crate::entity::living::block_speed_factor(entity, level)
        }
    }
    /// Port of ServerPlayer#jumpFromGround, retaining virtual parent calls.
    fn jump_from_ground(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        crate::entity::living::jump::jump_from_ground(self, entity, level)?;
        award_stat(
            entity,
            crate::stats::Stat::Custom(crate::stats::custom::Id::JUMP),
            1,
            level,
        )?;
        let player = level.entity_mut::<ServerPlayer>(entity)?;
        let exhaustion = if player.shared_flag(3) { 0.2 } else { 0.05 };
        player.cause_food_exhaustion(exhaustion);
        Ok(())
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
        // ServerPlayer#tick: gameMode precedes warden, damage cooldown and menus.
        game_mode::tick(entity, level).map_err(LevelError::from)?;
        let player = level.entity_mut::<ServerPlayer>(entity)?;
        if player.damage_cooldown_time > 0 {
            player.damage_cooldown_time -= 1;
        }
        crate::inventory::tick_container_menu(entity, level)?;
        // Pending camera following precedes TICK.
        crate::advancements::criteria::tick(level, entity)?;
        // Pending levitation tracking precedes the native fall-start capture.
        stuck::track_start_falling_position(entity, level)?;
        // Pending vehicle-lava tracking precedes attributes.
        level
            .entity_mut::<ServerPlayer>(entity)?
            .update_interaction_range_attributes();
        crate::advancements::flush_dirty(level, entity)?;
        // Pending native post-effects synchronization follows flushDirty.
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
            level.entity::<ServerPlayer>(entity)?.abilities.flying,
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
            let saved = player
                .save(&crate::entity::tests::persistence_level())
                .unwrap();
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
        let saved = player
            .save(&crate::entity::tests::persistence_level())
            .unwrap();
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
        let mut saved = original
            .save(&crate::entity::tests::persistence_level())
            .unwrap();
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
            .try_load_with_recipes(
                loaded
                    .save(&crate::entity::tests::persistence_level())
                    .unwrap(),
                &manager,
            )
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
        let mut saved = player
            .save(&crate::entity::tests::persistence_level())
            .unwrap();
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
        let mut level = {
            let mut fixture_level = Level::new(
                Region::new(&BEHAVIORS),
                crate::level::LevelContext::fixture(0, 63),
            );
            fixture_level.install_shared(Box::default());
            fixture_level
        };
        crate::entity::tests::tags(&mut level);
        level
            .add_chunk(LevelChunk::new(ChunkPos::new(0, 0), 0..16, section))
            .unwrap();
        for y in 64..67 {
            level
                .set_block_state(BlockPos::new(4, y, 1), Block::STONE.default_state())
                .unwrap();
        }
        let entity = EntityRef::fixture(crate::level::RegionId::FIXTURE, 1);
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

#[cfg(test)]
mod ability_tests;
