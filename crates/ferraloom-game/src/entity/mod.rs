use ferraloom_data::EntityType;
use ferraloom_util::Vec3;

use crate::level::{Level, LevelError};

pub mod ai;
pub mod animal;
pub mod avatar;
pub use avatar::Avatar;
pub mod living;
pub use living::LivingEntity;
pub mod effect;
pub mod effects;
pub mod experience_orb;
pub mod item;
pub mod mob;
pub mod monster;
pub mod player;
pub mod pushing;
mod stuck;
pub mod variant;
pub use persistence::{EntityPersistenceError, load_entity_recursive, load_in_place};
mod spawn;
pub use spawn::{EntitySpawnReason, EntitySpawnRequest};
pub use stuck::{make_stuck_in_block, reset_fall_distance};
mod relationships;
pub use relationships::PassengerList;

/// Server-lifetime identity, independent of storage location and network ID.
/// The server allocator must never reuse a value, including after removal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntityRef {
    allocation_namespace: crate::level::RegionId,
    serial: u64,
}

impl EntityRef {
    /// Allocate from the sole birth-Region counter. The namespace identifies
    /// the allocator, not the current storage location, so transfers retain it.
    pub(crate) fn allocate(
        namespace: crate::level::RegionId,
        counter: &std::cell::Cell<u64>,
    ) -> Self {
        let serial = counter
            .get()
            .checked_add(1)
            .expect("entity reference space exhausted");
        counter.set(serial);
        Self {
            allocation_namespace: namespace,
            serial,
        }
    }
    pub const fn allocation_namespace(self) -> crate::level::RegionId {
        self.allocation_namespace
    }
    pub const fn serial(self) -> u64 {
        self.serial
    }

    /// Explicit identities for model fixtures or a known serial in a live
    /// fixture's namespace. Production callers must use Level#next_entity_ref.
    #[cfg(test)]
    pub const fn fixture(namespace: crate::level::RegionId, serial: u64) -> Self {
        Self {
            allocation_namespace: namespace,
            serial,
        }
    }
    pub fn teleport_relative(
        self,
        delta: ferraloom_util::Vec3,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        teleport::teleport_relative(self, delta, level)
    }

    pub fn count_player_passengers(self, level: &Level) -> Result<i32, LevelError> {
        count_player_passengers(self, level)
    }

    pub fn move_by(
        &mut self,
        level: &mut Level,
        mover: movement::MoverType,
        delta: ferraloom_util::Vec3,
    ) -> Result<(), LevelError> {
        let behavior = level.entity_behavior(*self)?;
        behavior.move_by(*self, level, mover, delta)
    }

    /// The mutable receiver is a call handle; canonical state stays in `level`.
    pub fn tick(&mut self, level: &mut Level) -> Result<(), LevelError> {
        let behavior = level.entity_behavior(*self)?;
        behavior.tick(*self, level)
    }

    pub fn hurt_server(
        &mut self,
        level: &mut Level,
        source: DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        let behavior = level.entity_behavior(*self)?;
        behavior.hurt_server(*self, level, source, damage)
    }
}

mod base;
pub mod bubble_column;
pub mod collision;
pub mod collision_context;
pub mod damage;
pub use damage::DamageSource;
pub mod dimensions;
pub mod fluid;
pub mod inside_block;
mod lifecycle;
pub(crate) use lifecycle::start_riding_loaded_shared;
pub mod movement;
mod names;
pub mod sounds;
pub mod teleport;
pub mod trace;
pub use lifecycle::{
    base_tick, common_tick, count_player_passengers, eject_passengers, living_ride_tick,
    living_ride_tick_with_tick_rate, position_rider, ride_tick, ride_tick_with_tick_rate,
    start_riding, stop_riding,
};
pub mod persistence;
pub mod reference;
pub use reference::EntityReference;
pub mod synched_data;
pub mod tracking;
pub use base::{EntityData, EntityDimensions, Movement, RemovalReason};

/// Port of Mob's entity references. Goals/navigation are added here as ported.
#[derive(Debug)]
pub struct Mob {
    pub parent: LivingEntity,
    pub target: Option<EntityRef>,
    pub can_pick_up_loot: bool,
    pub persistence_required: bool,
    pub drop_chances: mob::DropChances,
    pub leash_data: Option<mob::LeashData>,
    pub home_radius: i32,
    pub home_position: ferraloom_util::BlockPos,
    pub loot_table: Option<ferraloom_protocol::Identifier>,
    pub loot_table_seed: i64,
}

/// Port of PathfinderMob. Navigation belongs to Mob in vanilla.
#[derive(Debug)]
pub struct PathfinderMob {
    pub parent: Mob,
}

/// Port of AgeableMob, which is between Animal and PathfinderMob in 26.3.
#[derive(Debug)]
pub struct AgeableMob {
    pub parent: PathfinderMob,
    pub age: i32,
    pub forced_age: i32,
}

/// Concrete stored subclasses. Parent accessors never copy canonical state.
#[derive(Debug)]
pub enum EntityState {
    Cow(Box<animal::Cow>),
    Player(Box<player::ServerPlayer>),
    PrimedTnt(Box<item::PrimedTnt>),
    Item(Box<item::ItemEntity>),
    FallingBlock(Box<item::FallingBlockEntity>),
    ExperienceOrb(Box<experience_orb::ExperienceOrb>),
}

impl EntityState {
    pub fn entity_type(&self) -> EntityType {
        match self {
            Self::Cow(_) => EntityType::COW,
            Self::Player(_) => EntityType::PLAYER,
            Self::PrimedTnt(_) => EntityType::TNT,
            Self::Item(_) => EntityType::ITEM,
            Self::FallingBlock(_) => EntityType::FALLING_BLOCK,
            Self::ExperienceOrb(_) => EntityType::EXPERIENCE_ORB,
        }
    }

    pub fn as_mob(&self) -> Option<&Mob> {
        match self {
            Self::Cow(cow) => Some(cow.as_mob()),
            _ => None,
        }
    }

    pub fn as_mob_mut(&mut self) -> Option<&mut Mob> {
        match self {
            Self::Cow(cow) => Some(cow.as_mob_mut()),
            _ => None,
        }
    }

    pub fn as_living(&self) -> Option<&LivingEntity> {
        match self {
            Self::Player(p) => Some(&p.parent),
            _ => self.as_mob().map(|mob| &mob.parent),
        }
    }
    pub fn as_living_mut(&mut self) -> Option<&mut LivingEntity> {
        match self {
            Self::Player(p) => Some(&mut p.parent),
            _ => self.as_mob_mut().map(|mob| &mut mob.parent),
        }
    }
    pub fn as_entity(&self) -> &EntityData {
        match self {
            Self::Cow(cow) => &cow.as_living().parent,
            Self::Player(p) => &p.parent.parent,
            Self::PrimedTnt(tnt) => &tnt.parent,
            Self::Item(item) => &item.parent,
            Self::FallingBlock(block) => &block.parent,
            Self::ExperienceOrb(orb) => &orb.parent,
        }
    }
    pub fn as_entity_mut(&mut self) -> &mut EntityData {
        match self {
            Self::Cow(cow) => &mut cow.as_living_mut().parent,
            Self::Player(p) => &mut p.parent.parent,
            Self::PrimedTnt(tnt) => &mut tnt.parent,
            Self::Item(item) => &mut item.parent,
            Self::FallingBlock(block) => &mut block.parent,
            Self::ExperienceOrb(orb) => &mut orb.parent,
        }
    }
}

/// Typed projections are borrowed views, not casts or runtime downcasts.
pub trait EntityDataView: Sized {
    fn view(state: &EntityState) -> Option<&Self>;
    fn view_mut(state: &mut EntityState) -> Option<&mut Self>;
}

impl EntityDataView for animal::Cow {
    fn view(state: &EntityState) -> Option<&Self> {
        match state {
            EntityState::Cow(cow) => Some(cow),
            _ => None,
        }
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        match state {
            EntityState::Cow(cow) => Some(cow),
            _ => None,
        }
    }
}

impl EntityDataView for EntityData {
    fn view(state: &EntityState) -> Option<&Self> {
        Some(state.as_entity())
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        Some(state.as_entity_mut())
    }
}
impl EntityDataView for LivingEntity {
    fn view(state: &EntityState) -> Option<&Self> {
        state.as_living()
    }
    fn view_mut(state: &mut EntityState) -> Option<&mut Self> {
        state.as_living_mut()
    }
}
/// Typed virtual effects whose complete implementation belongs to a later slice.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EntityMethod {
    PlayerTouch,
    PlayShoulderEntityAmbientSound,
    RespawnEntityOnShoulder,
    CheckAutoSpinAttack,
    RunLocationChangedEffects,
    BroadcastEntityEvent,
    BroadcastDamageEvent,
    EnchantmentAttributeModifiers,
    StopLocationBasedEffects,
    ModifyArmorEffectiveness,
    JumpInLiquid,
    EnchantmentDamageImmunity,
    EnchantmentDamageProtection,
    ApplyItemBlocking,
    StopSleeping,
    CheckTotemDeathProtection,
    Die,
    LoadSleepingPosition,
    LoadBrain,
    CowVariantBehavior,
    FinalizeSpawn,
}
impl EntityMethod {
    pub const fn name(self) -> &'static str {
        match self {
            Self::PlayerTouch => "playerTouch",
            Self::PlayShoulderEntityAmbientSound => "playShoulderEntityAmbientSound",
            Self::RespawnEntityOnShoulder => "respawnEntityOnShoulder",
            Self::CheckAutoSpinAttack => "checkAutoSpinAttack",
            Self::RunLocationChangedEffects => "EnchantmentHelper#runLocationChangedEffects",
            Self::BroadcastEntityEvent => "ServerLevel#broadcastEntityEvent",
            Self::BroadcastDamageEvent => "ServerLevel#broadcastDamageEvent",
            Self::EnchantmentAttributeModifiers => "EnchantmentHelper#forEachModifier",
            Self::StopLocationBasedEffects => "EnchantmentHelper#stopLocationBasedEffects",
            Self::ModifyArmorEffectiveness => "EnchantmentHelper#modifyArmorEffectiveness",
            Self::JumpInLiquid => "jumpInLiquid",
            Self::EnchantmentDamageImmunity => "EnchantmentHelper#isImmuneToDamage",
            Self::EnchantmentDamageProtection => "EnchantmentHelper#getDamageProtection",
            Self::ApplyItemBlocking => "LivingEntity#applyItemBlocking",
            Self::StopSleeping => "LivingEntity#stopSleeping",
            Self::CheckTotemDeathProtection => "LivingEntity#checkTotemDeathProtection",
            Self::LoadSleepingPosition => "LivingEntity#readAdditionalSaveData(sleeping_pos)",
            Self::LoadBrain => "LivingEntity#makeBrain",
            Self::CowVariantBehavior => "Cow#getSoundSet",
            Self::FinalizeSpawn => "Mob#finalizeSpawn",
            Self::Die => "LivingEntity#die",
        }
    }
}

/// Shared embedded Entity parent references for the host lifetime tracer.
pub fn trace_base_entity_refs(base: &EntityData, references: &mut Vec<EntityRef>) {
    references.extend(base.vehicle.get());
    references.extend(base.passengers.iter().copied());
}

/// Vanilla virtual methods, separated from canonical embedded-parent state.
///
/// Dispatch objects are stateless. Never hold a state borrow across a world call.
pub trait Entity: Sync {
    /// Port of Player#awardStat's virtual entry. ServerPlayer owns the single
    /// canonical counter and scoreboard producer.
    fn award_stat(
        &self,
        entity: EntityRef,
        stat: crate::stats::Stat,
        amount: i32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        player::award_stat(entity, stat, amount, level)
    }
    /// Port of Entity#gameEvent(Holder), retaining the derived overload dispatch.
    fn game_event(
        &self,
        entity: EntityRef,
        event: ferraloom_data::registries::game_event::Id,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        self.game_event_with_source(entity, event, Some(entity), level)
    }
    /// Port of Entity#gameEvent(Holder, Entity).
    fn game_event_with_source(
        &self,
        entity: EntityRef,
        event: ferraloom_data::registries::game_event::Id,
        source: Option<EntityRef>,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let position = level.entity::<EntityData>(entity)?.position;
        level.game_event(event, position, source, None)
    }
    /// Port of LivingEntity#onEnterCombat's empty parent callback.
    fn on_enter_combat(&self, _entity: EntityRef, _level: &mut Level) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of LivingEntity#onLeaveCombat's empty parent callback.
    fn on_leave_combat(&self, _entity: EntityRef, _level: &mut Level) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Entity#resetFallDistance; ServerPlayer adds its fall criterion.
    fn reset_fall_distance(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        stuck::base_reset_fall_distance(entity, level)
    }
    /// Port of Entity#makeStuckInBlock, retaining this derived reset dispatcher.
    fn make_stuck_in_block(
        &self,
        entity: EntityRef,
        state: ferraloom_data::BlockState,
        multiplier: Vec3,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        stuck::base_make_stuck_in_block(self, entity, state, multiplier, level)
    }
    /// Port of Entity#getName, with Player and ItemEntity virtual overrides.
    fn name(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_protocol::chat::Component, LevelError> {
        names::base_name(self, entity, level)
    }
    fn type_name(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_protocol::chat::Component, LevelError> {
        names::type_name(entity, level)
    }
    fn display_name(
        &self,
        entity: EntityRef,
        level: &Level,
        scoreboard: &crate::scoreboard::Scoreboard,
    ) -> Result<ferraloom_protocol::chat::Component, LevelError> {
        names::display_name(self, entity, level, scoreboard)
    }
    /// Port of Entity#couldAcceptPassenger. Marker and OminousItemSpawner
    /// override this independently of removal state.
    fn could_accept_passenger(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    /// Port of Entity#playerTouch; concrete pickup entities override this hook.
    /// Port of LivingEntity#take, overridden by ServerPlayer for menu sync.
    fn take(
        &self,
        collector: EntityRef,
        entity: EntityRef,
        count: i32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        living::take(collector, entity, count, level)
    }
    fn player_touch(
        &self,
        _entity: EntityRef,
        _player: EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Host lifetime tracing is required for every dispatcher. Declare all held
    /// EntityRef fields here; UUID lookup references do not retain old states.
    fn trace_entity_refs(&self, state: &EntityState, references: &mut Vec<EntityRef>);
    /// Port of Entity#getPickResult. Vanilla returns null; empty ItemStack is a
    /// distinct successful result and must not erase that distinction.
    fn get_pick_result(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<Option<ferraloom_protocol::component::ItemStack>, LevelError> {
        Ok(None)
    }
    fn block_pos_below_that_affects_my_movement(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_util::BlockPos, LevelError> {
        movement::get_on_pos(entity, 0.500_001_f32, level)
    }
    /// Port of Entity#getKnownMovement; ServerPlayer supplies its client movement.
    fn known_movement(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_util::Vec3, LevelError> {
        teleport::base_known_movement(self, entity, level)
    }
    /// Port of Entity#teleportRelative. The player override uses listener state.
    fn teleport_relative(
        &self,
        entity: EntityRef,
        delta: ferraloom_util::Vec3,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        teleport::base_teleport_relative(entity, delta, level)
    }
    fn is_alive(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        Ok(!level.entity::<EntityData>(entity)?.removed)
    }
    /// Port of LivingEntity#getItemBySlot; None identifies a nonliving entity.
    fn item_by_slot<'level>(
        &self,
        entity: EntityRef,
        slot: ferraloom_protocol::component::EquipmentSlot,
        level: &'level Level,
    ) -> Result<Option<&'level ferraloom_protocol::component::ItemStack>, LevelError> {
        Ok(level
            .entity_state(entity)?
            .as_living()
            .map(|living| living.equipment.get(slot)))
    }
    /// Port of Entity#getSlot, with LivingEntity's equipment implementation.
    fn get_slot(
        &self,
        entity: EntityRef,
        slot: i32,
        level: &Level,
    ) -> Result<Option<crate::item::slot::SlotAccess>, LevelError> {
        crate::item::slot::living_access(entity, slot, level)
    }
    /// Port of LivingEntity#canUseSlot's parent body.
    fn can_use_slot(
        &self,
        entity: EntityRef,
        _slot: ferraloom_protocol::component::EquipmentSlot,
        level: &Level,
    ) -> Result<bool, LevelError> {
        level.entity::<LivingEntity>(entity)?;
        Ok(true)
    }
    /// Port of LivingEntity#setItemSlot. Old and new object receivers are rooted
    /// through the ordered onEquipItem callback, including PlayerEquipment.
    fn set_item_slot(
        &self,
        entity: EntityRef,
        slot: ferraloom_protocol::component::EquipmentSlot,
        value: crate::inventory::StackValue,
        operation: &mut crate::inventory::StackOperation<'_>,
    ) -> Result<(), LevelError> {
        crate::item::slot::set_equipment_value(entity, slot, value, operation)
    }
    fn on_equip_item(
        &self,
        entity: EntityRef,
        slot: ferraloom_protocol::component::EquipmentSlot,
        old: crate::inventory::StackRef,
        new: crate::inventory::StackRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        living::on_equip_item_parent(self, entity, slot, old, new, level)
    }
    /// Port of LivingEntity#doesEmitEquipEvent; Player restricts it to armor.
    fn does_emit_equip_event(&self, _slot: ferraloom_protocol::component::EquipmentSlot) -> bool {
        true
    }
    /// Port of Entity#getWeaponItem; LivingEntity returns its live main hand.
    fn weapon_item<'level>(
        &self,
        _entity: EntityRef,
        _level: &'level Level,
    ) -> Result<Option<&'level ferraloom_protocol::component::ItemStack>, LevelError> {
        Ok(None)
    }
    /// Port of LivingEntity#isBaby; ageable entities override this read.
    fn is_baby(&self, _entity: EntityRef, _level: &Level) -> Result<bool, LevelError> {
        Ok(false)
    }
    /// Port of Entity#fireImmune; ItemEntity additionally queries its stack.
    fn fire_immune(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        Ok(level
            .entity::<EntityData>(entity)?
            .entity_type
            .data()
            .fire_immune)
    }
    fn on_pos(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_util::BlockPos, LevelError> {
        movement::get_on_pos(entity, 1.0e-5_f32, level)
    }
    fn is_spectator(&self, _entity: EntityRef, _level: &Level) -> Result<bool, LevelError> {
        Ok(false)
    }
    /// Port of Entity#dampensVibrations; ItemEntity overrides its effective stack query.
    fn dampens_vibrations(&self, _entity: EntityRef, _level: &Level) -> Result<bool, LevelError> {
        Ok(false)
    }
    /// Port of Entity#isSteppingCarefully; Cat and Ocelot override this read.
    fn is_stepping_carefully(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        Ok(level.entity::<EntityData>(entity)?.shared_flag(1))
    }
    /// Port of Entity#isSwimming, dispatched through Player's override.
    fn is_swimming(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        Ok(level.entity::<EntityData>(entity)?.is_swimming())
    }
    fn is_pushable(&self, _entity: EntityRef, _level: &Level) -> Result<bool, LevelError> {
        Ok(false)
    }
    /// Port of Entity#push(Entity); LivingEntity adds its sleeping guard.
    fn push(
        &self,
        entity: EntityRef,
        other: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        pushing::push(self, entity, other, level)
    }
    fn push_impulse(
        &self,
        entity: EntityRef,
        impulse: ferraloom_util::Vec3,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level
            .entity_mut::<EntityData>(entity)?
            .push_impulse(impulse);
        Ok(())
    }
    /// Port of LivingEntity#pushEntities. The player override reads the caller's
    /// canonical tick manager, rather than keeping a second freeze flag.
    fn push_entities(
        &self,
        entity: EntityRef,
        level: &mut Level,
        _tick_rate: &crate::level::tick_rate::TickRateManager,
    ) -> Result<(), LevelError> {
        if level.entity_state(entity)?.as_living().is_some() {
            pushing::living_push_entities(self, entity, level)?;
        }
        Ok(())
    }
    /// Port of LivingEntity#doPush. Dispatch on the other entity as vanilla does.
    fn do_push(
        &self,
        entity: EntityRef,
        other: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level.entity_behavior(other)?.push(other, entity, level)
    }
    fn controlling_passenger(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<Option<EntityRef>, LevelError> {
        Ok(None)
    }
    fn sound_source(&self) -> ferraloom_protocol::play::SoundSource {
        ferraloom_protocol::play::SoundSource::Neutral
    }
    fn swim_sound(&self) -> ferraloom_data::registries::sound_event::Id {
        ferraloom_data::registries::sound_event::Id::ENTITY_GENERIC_SWIM
    }
    /// Port of LivingEntity#calculateFallDamage's virtual calculation.
    fn calculate_fall_damage(
        &self,
        entity: EntityRef,
        distance: f64,
        modifier: f32,
        level: &Level,
    ) -> Result<i32, LevelError> {
        living::fall_damage::calculate_fall_damage(entity, distance, modifier, level)
    }
    fn play_block_fall_sound(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        living::fall_damage::play_block_fall_sound(self, entity, level)
    }
    /// Port of LivingEntity#swing, with subtype dispatch retained by callers.
    fn swing(
        &self,
        entity: EntityRef,
        hand: ferraloom_protocol::play::InteractionHand,
        animation: ferraloom_protocol::component::SwingAnimation,
        send_to_self: bool,
        level: &mut Level,
    ) -> Result<bool, LevelError> {
        living::swing::swing(entity, hand, animation, send_to_self, level)
    }
    /// Port of LivingEntity#getFallSounds's inherited selection.
    fn fall_sounds(&self) -> living::FallSounds {
        use ferraloom_data::registries::sound_event::Id as Sound;
        living::FallSounds {
            small: Sound::ENTITY_GENERIC_SMALL_FALL,
            big: Sound::ENTITY_GENERIC_BIG_FALL,
        }
    }
    fn swim_splash_sound(&self) -> ferraloom_data::registries::sound_event::Id {
        ferraloom_data::registries::sound_event::Id::ENTITY_GENERIC_SPLASH
    }
    fn swim_high_speed_splash_sound(&self) -> ferraloom_data::registries::sound_event::Id {
        ferraloom_data::registries::sound_event::Id::ENTITY_GENERIC_SPLASH
    }
    fn play_sound(
        &self,
        entity: EntityRef,
        sound: ferraloom_data::registries::sound_event::Id,
        volume: f32,
        pitch: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        sounds::play_sound(self, entity, sound, volume, pitch, false, level)
    }
    fn play_fire_extinguished_sound(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        sounds::play_fire_extinguished_sound(self, entity, level)
    }
    fn play_step_sound(
        &self,
        entity: EntityRef,
        _pos: ferraloom_util::BlockPos,
        state: ferraloom_data::BlockState,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        sounds::play_step_sound(self, entity, state, level)
    }
    fn do_water_splash_effect(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        sounds::water_splash_effect(self, entity, level)
    }
    fn fire_immune_ticks(&self) -> i32 {
        0
    }
    fn apply_effects_from_blocks(
        &self,
        entity: EntityRef,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        let movements = inside_block::collect_movements(entity, level)?;
        inside_block::apply_effects(self, entity, &movements, level)
    }
    /// Port of Entity#applyEffectsFromBlocks(Vec3,Vec3), without draining or
    /// replacing movementThisTick/finalMovementsThisTick.
    fn apply_effects_from_blocks_between(
        &self,
        entity: EntityRef,
        from: ferraloom_util::Vec3,
        to: ferraloom_util::Vec3,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        inside_block::apply_effects(
            self,
            entity,
            &[Movement {
                from,
                to,
                axis_dependent_original_movement: None,
            }],
            level,
        )
    }
    /// Port of Entity#getPistonPushReaction. Ported subclasses inherit PUSH_PULL.
    fn piston_push_reaction(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<ferraloom_data::PushReaction, LevelError> {
        Ok(ferraloom_data::PushReaction::PushPull)
    }
    /// Port of Entity#isIgnoringBlockTriggers. Spectator filtering is separate.
    fn is_ignoring_block_triggers(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn can_freeze(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        living::freezing::entity_can_freeze(entity, level)
    }
    fn should_play_lava_hurt_sound(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    fn on_inside_block(
        &self,
        _entity: EntityRef,
        _state: ferraloom_data::BlockState,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn move_by(
        &self,
        entity: EntityRef,
        level: &mut Level,
        mover: movement::MoverType,
        delta: ferraloom_util::Vec3,
    ) -> Result<(), LevelError> {
        movement::move_entity(self, entity, mover, delta, level)
    }
    fn max_up_step(&self, _entity: EntityRef, _level: &Level) -> Result<f32, LevelError> {
        Ok(0.0)
    }
    fn maybe_back_off_from_edge(
        &self,
        _entity: EntityRef,
        _level: &Level,
        delta: ferraloom_util::Vec3,
        _mover: movement::MoverType,
    ) -> Result<ferraloom_util::Vec3, LevelError> {
        Ok(delta)
    }
    fn is_local_instance_authoritative(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(!matches!(
            level.entity_state(entity)?,
            EntityState::Player(_)
        ))
    }
    /// Port of LivingEntity#canGlide; Player supplies its abilities guard.
    fn can_glide(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        living::gliding::can_glide(self, entity, level)
    }
    /// Port of Entity#canSimulateMovement's server AUTHORITATIVE_SIDE branch.
    /// Player overrides the AUTHORITATIVE_SIDE_AND_SERVER result.
    fn can_simulate_movement(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        self.is_local_instance_authoritative(entity, level)
    }
    fn on_above_bubble_column(
        &self,
        entity: EntityRef,
        drag_down: bool,
        pos: ferraloom_util::BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        bubble_column::on_above_bubble_column(self, entity, drag_down, pos, level)
    }
    fn on_inside_bubble_column(
        &self,
        entity: EntityRef,
        drag_down: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        bubble_column::on_inside_bubble_column(self, entity, drag_down, level)
    }
    fn is_horizontal_collision_minor(
        &self,
        _entity: EntityRef,
        _level: &Level,
        _movement: ferraloom_util::Vec3,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn movement_emission(&self) -> movement::MovementEmission {
        movement::MovementEmission::All
    }
    fn bounciness(&self, _entity: EntityRef, _level: &Level) -> Result<f64, LevelError> {
        Ok(0.0)
    }
    fn default_gravity(&self, _entity: EntityRef, _level: &Level) -> Result<f64, LevelError> {
        Ok(0.0)
    }
    /// Full native readAdditionalSaveData chain on the same canonical receiver.
    fn read_additional_save_data(
        &self,
        entity: EntityRef,
        tag: &ferraloom_nbt::CompoundTag,
        level: &mut Level,
    ) -> Result<(), EntityPersistenceError> {
        persistence::live::read_additional_save_data(self, entity, tag, level)
    }
    /// Port of Entity#notifyLeasheeRemoved's inherited empty body.
    fn notify_leashee_removed(
        &self,
        _entity: EntityRef,
        _leashee: EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Entity#postDataManipulated. All currently ported families inherit
    /// this empty body; Villager's future override must run after UUID restore.
    fn post_data_manipulated(
        &self,
        _entity: EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn set_y_head_rot(
        &self,
        _entity: EntityRef,
        _value: f32,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn set_y_body_rot(
        &self,
        _entity: EntityRef,
        _value: f32,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Entity#repositionEntityAfterLoad's inherited true result.
    fn reposition_entity_after_load(&self) -> bool {
        true
    }
    fn y_head_rot(&self, _entity: EntityRef, _level: &Level) -> Result<f32, LevelError> {
        Ok(0.0)
    }
    fn effective_gravity(&self, entity: EntityRef, level: &Level) -> Result<f64, LevelError> {
        base_effective_gravity(self, entity, level)
    }
    /// Port of LivingEntity#getSpeed; Player overrides its attribute read.
    /// Port of LivingEntity#travel; Player preserves its explicit parent call.
    fn travel(&self, entity: EntityRef, input: Vec3, level: &mut Level) -> Result<(), LevelError> {
        living::travel::travel(self, entity, input, level)
    }
    fn get_speed(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        Ok(level.entity::<LivingEntity>(entity)?.speed)
    }
    fn get_flying_speed(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        if let Some(passenger) = self.controlling_passenger(entity, level)?
            && level.entity::<EntityData>(passenger)?.entity_type == EntityType::PLAYER
        {
            Ok(self.get_speed(entity, level)? * 0.1)
        } else {
            Ok(0.02)
        }
    }
    /// Port of LivingEntity#isAffectedByFluids; Player checks its flying ability.
    fn is_affected_by_fluids(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    /// Port of LivingEntity#canStandOnFluid's inherited default.
    fn can_stand_on_fluid(
        &self,
        _entity: EntityRef,
        _fluid: ferraloom_data::FluidState,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    /// Port of LivingEntity#getWaterSlowDown's inherited default.
    fn water_slow_down(&self) -> f32 {
        0.8
    }
    /// Port of Entity#isFree: fluid scanning runs only after collision succeeds.
    fn is_free(
        &self,
        entity: EntityRef,
        movement: Vec3,
        level: &Level,
    ) -> Result<bool, LevelError> {
        let bounds = level
            .entity::<EntityData>(entity)?
            .bounding_box
            .move_vec(movement);
        Ok(level.no_collision(Some(entity), bounds, false)?
            && !level.contains_any_liquid(bounds)?)
    }
    /// Port of Entity#omnidirectionalAirMover's inherited default.
    fn omnidirectional_air_mover(&self) -> bool {
        false
    }
    fn air_drag(&self, _entity: EntityRef, _level: &Level) -> Result<f32, LevelError> {
        Ok(0.98)
    }
    fn block_speed_factor(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        movement::block_speed_factor(entity, level)
    }
    fn block_jump_factor(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        movement::block_jump_factor(entity, level)
    }
    fn jump_power(
        &self,
        entity: EntityRef,
        multiplier: f32,
        level: &Level,
    ) -> Result<f32, LevelError> {
        living::jump::jump_power(self, entity, multiplier, level)
    }
    /// LivingEntity parent body; ServerPlayer's stat/exhaustion override follows.
    fn jump_from_ground(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        living::jump::jump_from_ground(self, entity, level)
    }
    fn jump_in_liquid(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        living::jump::jump_in_liquid(entity, level)
    }
    /// Port of Entity#checkFallDamage; living subclasses select their parent body.
    #[allow(clippy::too_many_arguments)]
    fn check_fall_damage(
        &self,
        entity: EntityRef,
        dy: f64,
        on_ground: bool,
        state: ferraloom_data::BlockState,
        pos: ferraloom_util::BlockPos,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        movement::base_check_fall_damage(self, entity, dy, on_ground, state, pos, level)
    }
    fn on_changed_block(
        &self,
        _entity: EntityRef,
        _pos: ferraloom_util::BlockPos,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn ignite_for_ticks(
        &self,
        entity: EntityRef,
        ticks: i32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level
            .entity_mut::<EntityData>(entity)?
            .ignite_for_ticks(ticks);
        Ok(())
    }

    /// Port of LivingEntity#getLiquidCollisionShape's inherited empty result.
    fn liquid_collision_shape(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<ferraloom_util::VoxelShape, LevelError> {
        Ok(ferraloom_util::shapes::empty())
    }

    /// Port of Entity#causeFallDamage's immunity and passenger propagation.
    fn cause_fall_damage(
        &self,
        entity: EntityRef,
        level: &mut Level,
        fall_distance: f64,
        damage_modifier: f32,
        source: &DamageSource,
    ) -> Result<bool, LevelError> {
        movement::base_cause_fall_damage(entity, level, fall_distance, damage_modifier, source)
    }

    /// Port of LivingEntity#onAttributeUpdated with the derived dispatcher.
    fn on_attribute_updated(
        &self,
        entity: EntityRef,
        attribute: ferraloom_data::registries::attribute::Id,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        living::on_attribute_updated_parent(self, entity, attribute, level)
    }
    /// Port of LivingEntity#onEquippedItemBroken, overridden by ServerPlayer.
    fn on_equipped_item_broken(
        &self,
        entity: EntityRef,
        broken: ferraloom_protocol::component::ItemStack,
        slot: ferraloom_protocol::component::EquipmentSlot,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        living::on_equipped_item_broken_parent(entity, &broken, slot, level)
    }

    fn can_be_collided_with(
        &self,
        _entity: EntityRef,
        _other: Option<EntityRef>,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(false)
    }
    fn dimensions(
        &self,
        entity: EntityRef,
        _pose: ferraloom_protocol::entity_data::Pose,
        level: &Level,
    ) -> Result<EntityDimensions, LevelError> {
        Ok(EntityDimensions::for_type(
            level.entity_state(entity)?.entity_type(),
        ))
    }
    fn default_dimensions(
        &self,
        entity: EntityRef,
        _pose: ferraloom_protocol::entity_data::Pose,
        level: &Level,
    ) -> Result<EntityDimensions, LevelError> {
        Ok(EntityDimensions::for_type(
            level.entity_state(entity)?.entity_type(),
        ))
    }
    fn passenger_riding_position(
        &self,
        entity: EntityRef,
        passenger: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_util::Vec3, LevelError> {
        let (position, dimensions) = {
            let base = level.entity::<EntityData>(entity)?;
            (base.position, base.dimensions.clone())
        };
        Ok(position
            + self.passenger_attachment_point(entity, passenger, &dimensions, 1.0, level)?)
    }
    fn passenger_attachment_point(
        &self,
        entity: EntityRef,
        passenger: EntityRef,
        dimensions: &EntityDimensions,
        _scale: f32,
        level: &Level,
    ) -> Result<ferraloom_util::Vec3, LevelError> {
        let base = level.entity::<EntityData>(entity)?;
        let index = base
            .passengers
            .iter()
            .position(|id| *id == passenger)
            .map_or(-1, |index| index as i32);
        Ok(dimensions.attachments.get_clamped(
            dimensions::EntityAttachment::Passenger,
            index,
            base.yaw,
        ))
    }
    fn vehicle_attachment_point(
        &self,
        entity: EntityRef,
        _vehicle: EntityRef,
        level: &Level,
    ) -> Result<ferraloom_util::Vec3, LevelError> {
        let base = level.entity::<EntityData>(entity)?;
        Ok(base
            .dimensions
            .attachments
            .get(dimensions::EntityAttachment::Vehicle, 0, base.yaw))
    }

    fn fluid_interaction_box(
        &self,
        entity: EntityRef,
        level: &Level,
    ) -> Result<Option<ferraloom_util::Aabb>, LevelError> {
        Ok(Some(
            level
                .entity::<EntityData>(entity)?
                .bounding_box
                .deflate(0.001, 0.001, 0.001),
        ))
    }
    fn is_pushed_by_fluid(&self, entity: EntityRef, level: &Level) -> Result<bool, LevelError> {
        Ok(match level.entity_state(entity)? {
            EntityState::Player(player) => !player.abilities.flying,
            _ => true,
        })
    }
    /// Port of Entity#checkDespawn's base hook, before simulation eligibility.
    fn check_despawn(&self, _entity: EntityRef, _level: &mut Level) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Entity#updateDataBeforeSync's base virtual method.
    fn update_data_before_sync(
        &self,
        _entity: EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Entity#broadcastToPlayer's base predicate.
    fn broadcast_to_player(
        &self,
        _entity: EntityRef,
        _player: EntityRef,
        _level: &Level,
    ) -> Result<bool, LevelError> {
        Ok(true)
    }
    /// Port of Entity#startSeenByPlayer's base hook.
    fn start_seen_by_player(
        &self,
        _entity: EntityRef,
        _player: EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Entity#stopSeenByPlayer's base hook.
    fn stop_seen_by_player(
        &self,
        _entity: EntityRef,
        _player: EntityRef,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    /// Port of Entity#tick; ServerLevel owns commonTick separately.
    fn tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        base_tick(self, entity, level)
    }

    /// Entity phase context borrows the server's canonical manager. Dispatchers
    /// that do not read tick-rate state retain their existing tick override.
    fn tick_with_tick_rate(
        &self,
        entity: EntityRef,
        level: &mut Level,
        _tick_rate: &crate::level::tick_rate::TickRateManager,
    ) -> Result<(), LevelError> {
        self.tick(entity, level)
    }

    fn ride_tick(&self, entity: EntityRef, level: &mut Level) -> Result<(), LevelError> {
        ride_tick(self, entity, level)
    }

    fn ride_tick_with_tick_rate(
        &self,
        entity: EntityRef,
        level: &mut Level,
        _tick_rate: &crate::level::tick_rate::TickRateManager,
    ) -> Result<(), LevelError> {
        self.ride_tick(entity, level)
    }

    fn is_invulnerable_to(
        &self,
        entity: EntityRef,
        source: &DamageSource,
        level: &Level,
    ) -> Result<bool, LevelError> {
        living::hurt::is_invulnerable_to_base(self, entity, source, level)
    }
    fn absorption_amount(&self, entity: EntityRef, level: &Level) -> Result<f32, LevelError> {
        Ok(level.entity::<LivingEntity>(entity)?.absorption_amount())
    }
    fn set_absorption_amount(
        &self,
        entity: EntityRef,
        amount: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        level
            .entity_mut::<LivingEntity>(entity)?
            .set_absorption_amount(amount);
        Ok(())
    }
    fn actually_hurt(
        &self,
        entity: EntityRef,
        source: &DamageSource,
        damage: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        living::hurt::actually_hurt(self, entity, source, damage, level)
    }
    /// LivingEntity's armor and helmet parent callbacks are empty.
    fn hurt_armor(
        &self,
        _entity: EntityRef,
        _source: &DamageSource,
        _damage: f32,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn hurt_helmet(
        &self,
        _entity: EntityRef,
        _source: &DamageSource,
        _damage: f32,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn apply_item_blocking(
        &self,
        entity: EntityRef,
        _source: &DamageSource,
        damage: f32,
        level: &mut Level,
    ) -> Result<f32, LevelError> {
        living::hurt::apply_item_blocking(entity, damage, level)
    }
    #[allow(clippy::too_many_arguments)]
    fn knockback(
        &self,
        entity: EntityRef,
        power: f64,
        x: f64,
        z: f64,
        _source: &DamageSource,
        _damage: f32,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        living::hurt::knockback(entity, power, x, z, level)
    }
    fn indicate_damage(
        &self,
        _entity: EntityRef,
        _x: f64,
        _z: f64,
        _level: &mut Level,
    ) -> Result<(), LevelError> {
        Ok(())
    }
    fn deal_default_knockback(
        &self,
        entity: EntityRef,
        source: &DamageSource,
        damage: f32,
        blocked: bool,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        living::hurt::default_knockback(self, entity, source, damage, blocked, level)
    }
    fn hurt_sound(
        &self,
        _entity: EntityRef,
        _source: &DamageSource,
        _level: &Level,
    ) -> Result<Option<ferraloom_data::registries::sound_event::Id>, LevelError> {
        Ok(Some(
            ferraloom_data::registries::sound_event::Id::ENTITY_GENERIC_HURT,
        ))
    }
    fn death_sound(
        &self,
        _entity: EntityRef,
        _level: &Level,
    ) -> Result<Option<ferraloom_data::registries::sound_event::Id>, LevelError> {
        Ok(Some(
            ferraloom_data::registries::sound_event::Id::ENTITY_GENERIC_DEATH,
        ))
    }
    fn sound_volume(&self) -> f32 {
        1.0
    }
    fn voice_pitch(&self, entity: EntityRef, level: &mut Level) -> Result<f32, LevelError> {
        living::hurt::voice_pitch(self, entity, level)
    }
    fn die(
        &self,
        entity: EntityRef,
        _source: &DamageSource,
        level: &mut Level,
    ) -> Result<(), LevelError> {
        Err(LevelError::UnsupportedEntityMethod(
            level.entity_state(entity)?.entity_type(),
            EntityMethod::Die,
        ))
    }
    fn hurt_server(
        &self,
        entity: EntityRef,
        level: &mut Level,
        source: DamageSource,
        damage: f32,
    ) -> Result<bool, LevelError> {
        if level.entity_state(entity)?.as_living().is_some() {
            living::hurt::hurt_server(self, entity, source, damage, level)
        } else {
            Ok(false)
        }
    }
}

/// Entity#getEffectiveGravity's parent body, for LivingEntity's explicit super call.
pub(crate) fn base_effective_gravity<E: Entity + ?Sized>(
    behavior: &E,
    entity: EntityRef,
    level: &Level,
) -> Result<f64, LevelError> {
    if *level
        .entity::<EntityData>(entity)?
        .entity_data
        .get(synched_data::NO_GRAVITY)
    {
        Ok(0.0)
    } else {
        behavior.default_gravity(entity, level)
    }
}

/// Isolated block-dispatch fixtures retain their historical controlled damage
/// body; production always enters living::hurt and its virtual callbacks.
#[cfg(test)]
pub(crate) fn hurt_unarmored(
    entity: EntityRef,
    level: &mut Level,
    source: DamageSource,
    damage: f32,
) -> Result<bool, LevelError> {
    let living = level
        .entity_state_mut(entity)?
        .as_living_mut()
        .ok_or(LevelError::WrongEntityType(entity))?;
    if living.parent.removed || living.health() <= 0.0 {
        return Ok(false);
    }
    let damage = if damage < 0.0 { 0.0 } else { damage };
    let damage = if damage.is_finite() { damage } else { f32::MAX };
    let applied = if living.damage_cooldown_time > 10
        && !source.is_tag(damage::DamageTypeTag::BypassesCooldown)
    {
        if damage <= living.last_hurt {
            return Ok(false);
        }
        damage - living.last_hurt
    } else {
        living.damage_cooldown_time = 20;
        living.hurt_duration = 10;
        living.hurt_time = living.hurt_duration;
        damage
    };
    living.last_hurt = damage;
    living.set_health(living.health() - applied);
    Ok(true)
}

/// Built-in stateless dispatchers. Explicit behavior tables still override these.
pub fn vanilla_behavior(kind: EntityType) -> Option<&'static dyn Entity> {
    match kind {
        EntityType::COW => Some(&animal::CowBehavior),
        EntityType::PLAYER => Some(&player::PlayerBehavior),
        EntityType::TNT => Some(&item::PrimedTntBehavior),
        EntityType::ITEM => Some(&item::ItemEntityBehavior),
        EntityType::FALLING_BLOCK => Some(&item::FallingBlockBehavior),
        EntityType::EXPERIENCE_ORB => Some(&experience_orb::ExperienceOrbBehavior),
        _ => None,
    }
}

#[cfg(test)]
mod predicate_facts;
#[cfg(test)]
pub(crate) mod tests;
