//! Port of Clientbound play packet bodies.
use super::*;
pub use crate::common::clientbound::*;

packet!(/// Port of ClientboundAddEntityPacket#STREAM_CODEC.
AddEntity { id: VarInt, uuid: Uuid, entity_type: EntityTypeId, x: f64, y: f64, z: f64, movement: LpVec3, x_rot: Angle, y_rot: Angle, y_head_rot: Angle, data: VarInt });
packet!(/// Port of ClientboundAddTransientBlockPacket#STREAM_CODEC.
AddTransientBlock { pos: BlockPos, block_state: BlockStateId });
packet!(/// Port of ClientboundAnimatePacket#STREAM_CODEC.
Animate { id: VarInt, action: u8 });
packet!(/// Port of ClientboundBlockChangedAckPacket#STREAM_CODEC.
BlockChangedAck { sequence: VarInt });
packet!(/// Port of ClientboundBlockDestructionPacket#STREAM_CODEC.
BlockDestruction { id: VarInt, pos: BlockPos, progress: u8 });
packet!(/// Port of ClientboundBlockEventPacket#STREAM_CODEC.
BlockEvent { pos: BlockPos, b0: u8, b1: u8, block: BlockId });
packet!(/// Port of ClientboundBlockUpdatePacket#STREAM_CODEC.
BlockUpdate { pos: BlockPos, block_state: BlockStateId });
packet!(/// Port of ClientboundBundleDelimiterPacket#STREAM_CODEC.
BundleDelimiter {  });
packet!(/// Port of ClientboundChangeDifficultyPacket#STREAM_CODEC.
ChangeDifficulty { difficulty: Difficulty, locked: bool });
packet!(/// Port of ClientboundChunkBatchFinishedPacket#STREAM_CODEC.
ChunkBatchFinished { batch_size: VarInt });
packet!(/// Port of ClientboundChunkBatchStartPacket#STREAM_CODEC.
ChunkBatchStart {  });
packet!(/// Port of ClientboundClearTitlesPacket#STREAM_CODEC.
ClearTitles { reset_times: bool });
packet!(/// Port of ClientboundContainerClosePacket#STREAM_CODEC.
ContainerClose { container_id: VarInt });
packet!(/// Port of ClientboundContainerSetDataPacket#STREAM_CODEC.
ContainerSetData { container_id: VarInt, id: i16, value: i16 });
packet!(/// Port of ClientboundCooldownPacket#STREAM_CODEC.
Cooldown { cooldown_group: Identifier, duration: VarInt });
packet!(/// Port of ClientboundCustomChatCompletionsPacket#STREAM_CODEC.
CustomChatCompletions { action: ChatCompletionAction, entries: Vec<String> });
packet!(/// Port of ClientboundDamageEventPacket#STREAM_CODEC.
DamageEvent { entity_id: VarInt, source_type: DamageTypeId, source_cause_id: OptionalVarInt, source_direct_id: OptionalVarInt, source_position: Option<Vec3> });
packet!(/// Port of ClientboundDebugSamplePacket#STREAM_CODEC.
DebugSample { sample: Vec<i64>, debug_sample_type: RemoteDebugSampleType });
packet!(/// Port of ClientboundEntityEventPacket#STREAM_CODEC.
EntityEvent { entity_id: i32, event_id: i8 });
packet!(/// Port of ClientboundEntityPositionSyncPacket#STREAM_CODEC.
EntityPositionSync { id: VarInt, position: PositionPath, y_rot: f32, x_rot: f32, on_ground: bool });
packet!(/// Port of ClientboundForgetLevelChunkPacket#STREAM_CODEC.
ForgetLevelChunk { pos: i64 });
packet!(/// Port of ClientboundGameEventPacket#STREAM_CODEC.
GameEvent { event: u8, param: f32 });
packet!(/// Port of ClientboundGameTestHighlightPosPacket#STREAM_CODEC.
GameTestHighlightPos { absolute_pos: BlockPos, relative_pos: BlockPos });
packet!(/// Port of ClientboundHurtAnimationPacket#STREAM_CODEC.
HurtAnimation { id: VarInt, yaw: f32 });
packet!(/// Port of ClientboundInitializeBorderPacket#STREAM_CODEC.
InitializeBorder { new_center_x: f64, new_center_z: f64, old_size: f64, new_size: f64, lerp_time: VarLong, new_absolute_max_size: VarInt, warning_blocks: VarInt, warning_time: VarInt });
packet!(/// Port of ClientboundLevelEventPacket#STREAM_CODEC.
LevelEvent { event_type: i32, pos: BlockPos, data: i32, global_event: bool });
packet!(/// Port of ClientboundLowDiskSpaceWarningPacket#STREAM_CODEC.
LowDiskSpaceWarning {  });
packet!(/// Port of ClientboundMountScreenOpenPacket#STREAM_CODEC.
MountScreenOpen { container_id: VarInt, inventory_columns: VarInt, entity_id: i32 });
packet!(/// Port of ClientboundMoveEntityRotPacket#STREAM_CODEC.
MoveEntityRot { entity_id: VarInt, on_ground: bool, y_rot: Angle, x_rot: Angle });
packet!(/// Port of ClientboundOpenBookPacket#STREAM_CODEC.
OpenBook { hand: InteractionHand });
packet!(/// Port of ClientboundOpenSignEditorPacket#STREAM_CODEC.
OpenSignEditor { pos: BlockPos, slot: SignTextSlot });
packet!(/// Port of ClientboundPlayerCombatEndPacket#STREAM_CODEC.
PlayerCombatEnd { duration: VarInt });
packet!(/// Port of ClientboundPlayerCombatEnterPacket#STREAM_CODEC.
PlayerCombatEnter {  });
packet!(/// Port of ClientboundPlayerInfoRemovePacket#STREAM_CODEC.
PlayerInfoRemove { profile_ids: Vec<Uuid> });
packet!(/// Port of ClientboundPlayerPositionPacket#STREAM_CODEC.
PlayerPosition { id: VarInt, change: PositionMoveRotation, relatives: i32 });
packet!(/// Port of ClientboundPlayerRotationPacket#STREAM_CODEC.
PlayerRotation { y_rot: f32, relative_y: bool, x_rot: f32, relative_x: bool });
packet!(/// Port of ClientboundProjectilePowerPacket#STREAM_CODEC.
ProjectilePower { id: VarInt, acceleration_power: f64 });
packet!(/// Port of ClientboundRecipeBookRemovePacket#STREAM_CODEC.
RecipeBookRemove { recipes: Vec<RecipeDisplayId> });
packet!(/// Port of ClientboundRemoveEntitiesPacket#STREAM_CODEC.
RemoveEntities { entity_ids: Vec<VarInt> });
packet!(/// Port of ClientboundRemoveMobEffectPacket#STREAM_CODEC.
RemoveMobEffect { entity_id: VarInt, effect: MobEffectId });
packet!(/// Port of ClientboundResetScorePacket#STREAM_CODEC.
ResetScore { owner: String, objective_name: Option<String> });
packet!(/// Port of ClientboundRotateHeadPacket#STREAM_CODEC.
RotateHead { entity_id: VarInt, y_head_rot: Angle });
packet!(/// Port of ClientboundSectionBlocksUpdatePacket#STREAM_CODEC.
SectionBlocksUpdate { section_pos: i64, changes: Vec<VarLong> });
packet!(/// Port of ClientboundSelectAdvancementsTabPacket#STREAM_CODEC.
SelectAdvancementsTab { tab: Option<Identifier> });
packet!(/// Port of ClientboundSetBorderCenterPacket#STREAM_CODEC.
SetBorderCenter { new_center_x: f64, new_center_z: f64 });
packet!(/// Port of ClientboundSetBorderLerpSizePacket#STREAM_CODEC.
SetBorderLerpSize { old_size: f64, new_size: f64, lerp_time: VarLong });
packet!(/// Port of ClientboundSetBorderSizePacket#STREAM_CODEC.
SetBorderSize { size: f64 });
packet!(/// Port of ClientboundSetBorderWarningDelayPacket#STREAM_CODEC.
SetBorderWarningDelay { warning_delay: VarInt });
packet!(/// Port of ClientboundSetBorderWarningDistancePacket#STREAM_CODEC.
SetBorderWarningDistance { warning_blocks: VarInt });
packet!(/// Port of ClientboundSetCameraPacket#STREAM_CODEC.
SetCamera { camera_id: VarInt });
packet!(/// Port of ClientboundSetChunkCacheCenterPacket#STREAM_CODEC.
SetChunkCacheCenter { x: VarInt, z: VarInt });
packet!(/// Port of ClientboundSetChunkCacheRadiusPacket#STREAM_CODEC.
SetChunkCacheRadius { radius: VarInt });
packet!(/// Port of ClientboundSetDisplayObjectivePacket#STREAM_CODEC.
SetDisplayObjective { slot: VarInt, objective_name: String });
packet!(/// Port of ClientboundSetEntityLinkPacket#STREAM_CODEC.
SetEntityLink { source_id: i32, dest_id: i32 });
packet!(/// Port of ClientboundSetEntityMotionPacket#STREAM_CODEC.
SetEntityMotion { id: VarInt, movement: LpVec3 });
packet!(/// Port of ClientboundSetExperiencePacket#STREAM_CODEC.
SetExperience { experience_progress: f32, experience_level: VarInt, total_experience: VarInt });
packet!(/// Port of ClientboundSetHealthPacket#STREAM_CODEC.
SetHealth { health: f32, food: VarInt, saturation: f32 });
packet!(/// Port of ClientboundSetHeldSlotPacket#STREAM_CODEC.
SetHeldSlot { slot: VarInt });
packet!(/// Port of ClientboundSetPassengersPacket#STREAM_CODEC.
SetPassengers { vehicle: VarInt, passengers: Vec<VarInt> });
packet!(/// Port of ClientboundSetSimulationDistancePacket#STREAM_CODEC.
SetSimulationDistance { simulation_distance: VarInt });
packet!(/// Port of ClientboundSetTitlesAnimationPacket#STREAM_CODEC.
SetTitlesAnimation { fade_in: i32, stay: i32, fade_out: i32 });
packet!(/// Port of ClientboundStartConfigurationPacket#STREAM_CODEC.
StartConfiguration {  });
packet!(/// Port of ClientboundTagQueryPacket#STREAM_CODEC.
TagQuery { transaction_id: VarInt, tag: OptionalNbt });
packet!(/// Port of ClientboundTakeItemEntityPacket#STREAM_CODEC.
TakeItemEntity { item_id: VarInt, player_id: VarInt, amount: VarInt });
packet!(/// Port of ClientboundTeleportEntityPacket#STREAM_CODEC.
TeleportEntity { id: VarInt, change: PositionMoveRotation, relatives: i32, on_ground: bool });
packet!(/// Port of ClientboundTickingStatePacket#STREAM_CODEC.
TickingState { tick_rate: f32, is_frozen: bool });
packet!(/// Port of ClientboundTickingStepPacket#STREAM_CODEC.
TickingStep { tick_steps: VarInt });
packet!(/// Port of ClientboundUpdateMobEffectPacket#STREAM_CODEC.
UpdateMobEffect { entity_id: VarInt, effect: MobEffectId, effect_amplifier: VarInt, effect_duration_ticks: VarInt, flags: u8 });

/// Port of ClientboundPlayerAbilitiesPacket.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerAbilities {
    pub invulnerable: bool,
    pub is_flying: bool,
    pub can_fly: bool,
    pub instabuild: bool,
    pub flying_speed: f32,
    pub walking_speed: f32,
}
impl Wire for PlayerAbilities {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let b = r.u8()?;
        Ok(Self {
            invulnerable: b & 1 != 0,
            is_flying: b & 2 != 0,
            can_fly: b & 4 != 0,
            instabuild: b & 8 != 0,
            flying_speed: r.read()?,
            walking_speed: r.read()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(u8::from(self.invulnerable)
            | (u8::from(self.is_flying) << 1)
            | (u8::from(self.can_fly) << 2)
            | (u8::from(self.instabuild) << 3));
        self.flying_speed.encode(w)?;
        self.walking_speed.encode(w)
    }
}

packet!(/// Port of ClientboundPlayerLookAtPacket's entity target.
LookAtEntity {id:VarInt,anchor:EntityAnchor});
packet!(/// Port of ClientboundPlayerLookAtPacket.
PlayerLookAt {from_anchor:EntityAnchor,x:f64,y:f64,z:f64,entity:Option<LookAtEntity>});

/// Port of ClientboundStopSoundPacket. Flags select independently optional fields.
#[derive(Clone, Debug, PartialEq)]
pub struct StopSound {
    pub source: Option<SoundSource>,
    pub name: Option<Identifier>,
}
impl Wire for StopSound {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let b = r.u8()?;
        Ok(Self {
            source: if b & 1 != 0 { Some(r.read()?) } else { None },
            name: if b & 2 != 0 { Some(r.read()?) } else { None },
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(u8::from(self.source.is_some()) | (u8::from(self.name.is_some()) << 1));
        if let Some(v) = &self.source {
            v.encode(w)?;
        }
        if let Some(v) = &self.name {
            v.encode(w)?;
        }
        Ok(())
    }
}

packet!(/// Port of ClientboundChunksBiomesPacket.
ChunksBiomes {chunk_biome_data:Vec<ChunkBiomeData>});
packet!(/// Port of ClientboundLevelChunkWithLightPacket.
LevelChunkWithLight {x:i32,z:i32,chunk_data:LevelChunkPacketData,light_data:LightUpdatePacketData});
packet!(/// Port of ClientboundLightUpdatePacket.
LightUpdate {x:VarInt,z:VarInt,light_data:LightUpdatePacketData});
packet!(/// Port of ClientboundMoveVehiclePacket.
MoveVehicle {moving_to:PositionAndRotation});
packet!(/// Port of ClientboundMoveEntityPacket.Pos.
MoveEntityPos {entity_id:VarInt,delta:EntityDelta});
packet!(/// Port of ClientboundMoveEntityPacket.PosRot.
MoveEntityPosRot {entity_id:VarInt,delta:EntityDelta,y_rot:Angle,x_rot:Angle});
packet!(/// Port of ClientboundMoveMinecartPacket.
MoveMinecart {entity_id:VarInt,lerp_steps:Vec<MinecartStep>});
packet!(/// Port of ClientboundDeleteChatPacket.
DeleteChat {message_signature:PackedMessageSignature});
packet!(/// Port of ClientboundLoginPacket.
Login {player_id:i32,hardcore:bool,levels:Vec<Identifier>,max_players:VarInt,chunk_radius:VarInt,simulation_distance:VarInt,reduced_debug_info:bool,show_death_screen:bool,do_limited_crafting:bool,common_player_spawn_info:CommonPlayerSpawnInfo,online_mode:bool,enforces_secure_chat:bool});
packet!(/// Port of ClientboundRespawnPacket.
Respawn {common_player_spawn_info:CommonPlayerSpawnInfo,data_to_keep:u8});
packet!(/// Port of ClientboundSetDefaultSpawnPositionPacket.
SetDefaultSpawnPosition {respawn_data:RespawnData});
packet!(/// Port of ClientboundSetTimePacket.
SetTime {game_time:i64,clock_updates:Vec<ClockUpdate>});
packet!(/// Port of ClientboundGameRuleValuesPacket.
GameRuleValues {values:Vec<GameRuleEntry>});
packet!(/// Port of ClientboundCommandSuggestionsPacket.
CommandSuggestions {id:VarInt,start:VarInt,length:VarInt,suggestions:Vec<CommandSuggestion>});
packet!(/// Port of ClientboundSetActionBarTextPacket.
SetActionBarText {text:Component});
packet!(/// Port of ClientboundSetTitleTextPacket.
SetTitleText {text:Component});
packet!(/// Port of ClientboundSetSubtitleTextPacket.
SetSubtitleText {text:Component});
packet!(/// Port of ClientboundTabListPacket.
TabList {header:Component,footer:Component});
packet!(/// Port of ClientboundSystemChatPacket.
SystemChat {content:Component,overlay:bool});
packet!(/// Port of ClientboundPlayerCombatKillPacket.
PlayerCombatKill {player_id:VarInt,message:Component});
packet!(/// Port of ClientboundServerDataPacket.
ServerData {motd:Component,icon_bytes:Option<ByteArray>});
packet!(/// Port of ClientboundBlockEntityDataPacket.
BlockEntityData {pos:BlockPos,block_entity_type:BlockEntityTypeId,tag:TrustedCompoundTag});
packet!(/// Port of ClientboundOpenScreenPacket.
OpenScreen {container_id:VarInt,menu_type:MenuId,title:Component});
packet!(/// Port of ClientboundSwingAnimationPacket.
SwingAnimationPacket {entity_id:VarInt,hand:InteractionHand,animation:SwingAnimation});
packet!(/// Port of ClientboundSoundEntityPacket.
SoundEntity {sound:SoundEventHolder,source:SoundSource,id:VarInt,volume:f32,pitch:f32,seed:i64});
packet!(/// Port of ClientboundSoundPacket.
Sound {sound:SoundEventHolder,source:SoundSource,x:i32,y:i32,z:i32,volume:f32,pitch:f32,seed:i64});
packet!(/// Port of ClientboundTestInstanceBlockStatus.
TestInstanceBlockStatus {status:Component,size:Option<Vec3i>});

/// Port of ClientboundPlayerInfoUpdatePacket. One action bitset applies to every entry.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerInfoUpdate {
    pub actions: u8,
    pub entries: Vec<PlayerInfoEntry>,
}
impl Wire for PlayerInfoUpdate {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let actions = r.u8()?;
        let entries = r.list(i32::MAX as usize, |r| {
            let profile_id = r.read()?;
            let mut values = Vec::new();
            for tag in 0..8 {
                if actions & (1 << tag) != 0 {
                    values.push(PlayerInfoAction::decode_variant(r, tag)?);
                }
            }
            Ok(PlayerInfoEntry {
                profile_id,
                actions: values,
            })
        })?;
        Ok(Self { actions, entries })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(self.actions);
        w.list(&self.entries, i32::MAX as usize, |w, entry| {
            let mut values = entry.actions.iter();
            entry.profile_id.encode(w)?;
            for tag in 0..8 {
                if self.actions & (1 << tag) != 0 {
                    let value = values.next().filter(|v| v.tag() == tag).ok_or_else(|| {
                        Error::invalid(
                            "player info actions",
                            "entry does not match packet mask/order",
                        )
                    })?;
                    value.encode_body(w)?;
                }
            }
            if values.next().is_some() {
                return Err(Error::invalid("player info actions", "extra action"));
            }
            Ok(())
        })
    }
}
packet!(/// Port of ClientboundBossEventPacket.
BossEvent {id:Uuid,operation:BossOperation});
packet!(/// Port of ClientboundContainerSetContentPacket.
ContainerSetContent {container_id:VarInt,state_id:VarInt,items:Vec<crate::component::ItemStack>,carried_item:crate::component::ItemStack});
packet!(/// Port of ClientboundContainerSetSlotPacket.
ContainerSetSlot {container_id:VarInt,state_id:VarInt,slot:i16,item_stack:crate::component::ItemStack});
packet!(/// Port of ClientboundSetCursorItemPacket.
SetCursorItem {contents:crate::component::ItemStack});
packet!(/// Port of ClientboundSetPlayerInventoryPacket.
SetPlayerInventory {slot:VarInt,contents:crate::component::ItemStack});
packet!(/// Port of ClientboundSetEntityDataPacket.
SetEntityData {id:VarInt,packed_items:crate::entity_data::EntityDataItems});
packet!(/// Port of ClientboundMerchantOffersPacket.
MerchantOffers {container_id:VarInt,offers:Vec<MerchantOffer>,villager_level:VarInt,villager_xp:VarInt,show_progress:bool,can_restock:bool});
packet!(/// Port of ClientboundRecipeBookSettingsPacket.
RecipeBookSettingsPacket {book_settings:RecipeBookSettings});

/// Port of ClientboundSetEquipmentPacket's continuation-bit equipment list.
#[derive(Clone, Debug, PartialEq)]
pub struct SetEquipment {
    pub entity: VarInt,
    pub slots: Vec<EquipmentEntry>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EquipmentEntry {
    pub slot: crate::component::EquipmentSlot,
    pub item: crate::component::ItemStack,
}
impl Wire for SetEquipment {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let entity = r.read()?;
        let mut slots = Vec::new();
        loop {
            let b = r.u8()?;
            let slot = crate::component::EquipmentSlot::from_bytes(&[b & 127])?;
            slots.push(EquipmentEntry {
                slot,
                item: r.read()?,
            });
            if b & 128 == 0 {
                break;
            }
        }
        Ok(Self { entity, slots })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        if self.slots.is_empty() {
            return Err(Error::invalid("equipment", "at least one slot is required"));
        }
        self.entity.encode(w)?;
        for (i, entry) in self.slots.iter().enumerate() {
            let tag = entry.slot.to_bytes()?[0];
            w.u8(tag | if i + 1 < self.slots.len() { 128 } else { 0 });
            entry.item.encode(w)?;
        }
        Ok(())
    }
}
packet!(/// Port of ClientboundPlaceGhostRecipePacket.
PlaceGhostRecipe {container_id:VarInt,recipe_display:RecipeDisplay});
packet!(/// Port of ClientboundRecipeBookAddPacket.
RecipeBookAdd {entries:Vec<RecipeBookEntry>,replace:bool});
packet!(/// Port of ClientboundUpdateRecipesPacket.
UpdateRecipes {item_sets:Vec<RecipePropertySet>,stonecutter_recipes:Vec<SelectableRecipe>});
packet!(/// Port of ClientboundLevelParticlesPacket.
LevelParticles {particle:crate::entity_data::Particle,override_limiter:bool,always_show:bool,x:f64,y:f64,z:f64,x_dist:f32,y_dist:f32,z_dist:f32,x_max_speed:f32,y_max_speed:f32,z_max_speed:f32,count:VarInt,randomization_type:ParticleRandomization});
packet!(/// Port of ClientboundExplodePacket.
Explode {center:Vec3,radius:f32,block_count:i32,player_knockback:Option<Vec3>,explosion_particle:crate::entity_data::Particle,explosion_sound:SoundEventHolder,block_particles:Vec<WeightedExplosionParticle>,play_sound:bool});
/// Port of ClientboundUpdateAttributesPacket's 128-attribute bound.
#[derive(Clone, Debug, PartialEq)]
pub struct UpdateAttributes {
    pub entity_id: VarInt,
    pub values: Vec<AttributeSnapshot>,
}
impl Wire for UpdateAttributes {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            entity_id: r.read()?,
            values: r.list(128, Wire::decode)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.entity_id.encode(w)?;
        w.list(&self.values, 128, |w, v| v.encode(w))
    }
}
packet!(/// Port of ClientboundMapItemDataPacket.
MapItemData {map_id:VarInt,scale:i8,locked:bool,decorations:Option<Vec<MapDecoration>>,color_patch:OptionalMapPatch});
packet!(/// Port of ClientboundTrackedWaypointPacket.
TrackedWaypoint {operation:WaypointOperation,waypoint:TrackedWaypointData});
packet!(/// Port of ClientboundCommandsPacket.
Commands {entries:Vec<CommandNode>,root_index:VarInt});
packet!(/// Port of ClientboundSetObjectivePacket.
SetObjective {objective_name:String,operation:ObjectiveOperation});
packet!(/// Port of ClientboundSetPlayerTeamPacket.
SetPlayerTeam {name:String,operation:TeamOperation});
packet!(/// Port of ClientboundSetScorePacket.
SetScore {owner:String,objective_name:String,score:VarInt,display:Option<Component>,number_format:Option<NumberFormat>});
packet!(/// Port of ClientboundDisguisedChatPacket.
DisguisedChat {message:Component,chat_type:ChatTypeBound});
packet!(/// Port of ClientboundPlayerChatPacket.
PlayerChat {global_index:VarInt,sender:Uuid,index:VarInt,signature:Option<MessageSignature>,body:SignedMessageBodyPacked,unsigned_content:Option<Component>,filter_mask:FilterMask,chat_type:ChatTypeBound});
packet!(/// Port of ClientboundUpdateAdvancementsPacket.
UpdateAdvancements {should_reset:bool,added:Vec<PositionedAdvancement>,removed:Vec<Identifier>,progress:Vec<AdvancementProgress>,show_advancements:bool});
packet!(/// Port of Stat#STREAM_CODEC followed by the statistic count.
StatValue {stat_type:VarInt,value:VarInt,count:VarInt});
packet!(/// Port of ClientboundAwardStatsPacket.
AwardStats {stats:Vec<StatValue>});
packet!(/// Port of ClientboundDebugBlockValuePacket.
DebugBlockValue {block_pos:BlockPos,update:DebugSubscriptionUpdate});
packet!(/// Port of ClientboundDebugChunkValuePacket.
DebugChunkValue {chunk_pos:i64,update:DebugSubscriptionUpdate});
packet!(/// Port of ClientboundDebugEntityValuePacket.
DebugEntityValue {entity_id:VarInt,update:DebugSubscriptionUpdate});
packet!(/// Port of ClientboundDebugEventPacket.
DebugEvent {event:DebugSubscriptionEvent});

use crate::codec::state_packets;
state_packets!(Packet, ferraloom_data::packets::play::clientbound::Packet, {
    BundleDelimiter(BundleDelimiter) => BundleDelimiter,
    AddEntity(AddEntity) => AddEntity,
    Animate(Animate) => Animate,
    AwardStats(AwardStats) => AwardStats,
    BlockChangedAck(BlockChangedAck) => BlockChangedAck,
    BlockDestruction(BlockDestruction) => BlockDestruction,
    BlockEntityData(BlockEntityData) => BlockEntityData,
    BlockEvent(BlockEvent) => BlockEvent,
    BlockUpdate(BlockUpdate) => BlockUpdate,
    BossEvent(BossEvent) => BossEvent,
    ChangeDifficulty(ChangeDifficulty) => ChangeDifficulty,
    ChunkBatchFinished(ChunkBatchFinished) => ChunkBatchFinished,
    ChunkBatchStart(ChunkBatchStart) => ChunkBatchStart,
    ChunksBiomes(ChunksBiomes) => ChunksBiomes,
    ClearTitles(ClearTitles) => ClearTitles,
    CommandSuggestions(CommandSuggestions) => CommandSuggestions,
    Commands(Commands) => Commands,
    ContainerClose(ContainerClose) => ContainerClose,
    ContainerSetContent(ContainerSetContent) => ContainerSetContent,
    ContainerSetData(ContainerSetData) => ContainerSetData,
    ContainerSetSlot(ContainerSetSlot) => ContainerSetSlot,
    CookieRequest(CookieRequest) => CookieRequest,
    Cooldown(Cooldown) => Cooldown,
    CustomChatCompletions(CustomChatCompletions) => CustomChatCompletions,
    CustomPayload(CustomPayload) => CustomPayload,
    DamageEvent(DamageEvent) => DamageEvent,
    DebugBlockValue(DebugBlockValue) => DebugBlockValue,
    DebugChunkValue(DebugChunkValue) => DebugChunkValue,
    DebugEntityValue(DebugEntityValue) => DebugEntityValue,
    DebugEvent(DebugEvent) => DebugEvent,
    DebugSample(DebugSample) => DebugSample,
    DeleteChat(DeleteChat) => DeleteChat,
    Disconnect(Disconnect) => Disconnect,
    DisguisedChat(DisguisedChat) => DisguisedChat,
    EntityEvent(EntityEvent) => EntityEvent,
    EntityPositionSync(EntityPositionSync) => EntityPositionSync,
    Explode(Explode) => Explode,
    AddTransientBlock(AddTransientBlock) => AddTransientBlock,
    ForgetLevelChunk(ForgetLevelChunk) => ForgetLevelChunk,
    GameEvent(GameEvent) => GameEvent,
    GameRuleValues(GameRuleValues) => GameRuleValues,
    GameTestHighlightPos(GameTestHighlightPos) => GameTestHighlightPos,
    MountScreenOpen(MountScreenOpen) => MountScreenOpen,
    HurtAnimation(HurtAnimation) => HurtAnimation,
    InitializeBorder(InitializeBorder) => InitializeBorder,
    KeepAlive(KeepAlive) => KeepAlive,
    LevelChunkWithLight(LevelChunkWithLight) => LevelChunkWithLight,
    LevelEvent(LevelEvent) => LevelEvent,
    LevelParticles(LevelParticles) => LevelParticles,
    LightUpdate(LightUpdate) => LightUpdate,
    Login(Login) => Login,
    LowDiskSpaceWarning(LowDiskSpaceWarning) => LowDiskSpaceWarning,
    MapItemData(MapItemData) => MapItemData,
    MerchantOffers(MerchantOffers) => MerchantOffers,
    MoveEntityPos(MoveEntityPos) => MoveEntityPos,
    MoveEntityPosRot(MoveEntityPosRot) => MoveEntityPosRot,
    MoveMinecartAlongTrack(MoveMinecart) => MoveMinecartAlongTrack,
    MoveEntityRot(MoveEntityRot) => MoveEntityRot,
    MoveVehicle(MoveVehicle) => MoveVehicle,
    OpenBook(OpenBook) => OpenBook,
    OpenScreen(OpenScreen) => OpenScreen,
    OpenSignEditor(OpenSignEditor) => OpenSignEditor,
    Ping(Ping) => Ping,
    PongResponse(PongResponse) => PongResponse,
    PlaceGhostRecipe(PlaceGhostRecipe) => PlaceGhostRecipe,
    PlayerAbilities(PlayerAbilities) => PlayerAbilities,
    PlayerChat(PlayerChat) => PlayerChat,
    PlayerCombatEnd(PlayerCombatEnd) => PlayerCombatEnd,
    PlayerCombatEnter(PlayerCombatEnter) => PlayerCombatEnter,
    PlayerCombatKill(PlayerCombatKill) => PlayerCombatKill,
    PlayerInfoRemove(PlayerInfoRemove) => PlayerInfoRemove,
    PlayerInfoUpdate(PlayerInfoUpdate) => PlayerInfoUpdate,
    PlayerLookAt(PlayerLookAt) => PlayerLookAt,
    PlayerPosition(PlayerPosition) => PlayerPosition,
    PlayerRotation(PlayerRotation) => PlayerRotation,
    RecipeBookAdd(RecipeBookAdd) => RecipeBookAdd,
    RecipeBookRemove(RecipeBookRemove) => RecipeBookRemove,
    RecipeBookSettings(RecipeBookSettingsPacket) => RecipeBookSettings,
    RemoveEntities(RemoveEntities) => RemoveEntities,
    RemoveMobEffect(RemoveMobEffect) => RemoveMobEffect,
    ResetScore(ResetScore) => ResetScore,
    ResourcePackPop(ResourcePackPop) => ResourcePackPop,
    ResourcePackPush(ResourcePackPush) => ResourcePackPush,
    PostEffects(PostEffects) => PostEffects,
    Respawn(Respawn) => Respawn,
    RotateHead(RotateHead) => RotateHead,
    SectionBlocksUpdate(SectionBlocksUpdate) => SectionBlocksUpdate,
    SelectAdvancementsTab(SelectAdvancementsTab) => SelectAdvancementsTab,
    ServerData(ServerData) => ServerData,
    SetActionBarText(SetActionBarText) => SetActionBarText,
    SetBorderCenter(SetBorderCenter) => SetBorderCenter,
    SetBorderLerpSize(SetBorderLerpSize) => SetBorderLerpSize,
    SetBorderSize(SetBorderSize) => SetBorderSize,
    SetBorderWarningDelay(SetBorderWarningDelay) => SetBorderWarningDelay,
    SetBorderWarningDistance(SetBorderWarningDistance) => SetBorderWarningDistance,
    SetCamera(SetCamera) => SetCamera,
    SetChunkCacheCenter(SetChunkCacheCenter) => SetChunkCacheCenter,
    SetChunkCacheRadius(SetChunkCacheRadius) => SetChunkCacheRadius,
    SetCursorItem(SetCursorItem) => SetCursorItem,
    SetDefaultSpawnPosition(SetDefaultSpawnPosition) => SetDefaultSpawnPosition,
    SetDisplayObjective(SetDisplayObjective) => SetDisplayObjective,
    SetEntityData(SetEntityData) => SetEntityData,
    SetEntityLink(SetEntityLink) => SetEntityLink,
    SetEntityMotion(SetEntityMotion) => SetEntityMotion,
    SetEquipment(SetEquipment) => SetEquipment,
    SetExperience(SetExperience) => SetExperience,
    SetHealth(SetHealth) => SetHealth,
    SetHeldSlot(SetHeldSlot) => SetHeldSlot,
    SetObjective(SetObjective) => SetObjective,
    SetPassengers(SetPassengers) => SetPassengers,
    SetPlayerInventory(SetPlayerInventory) => SetPlayerInventory,
    SetPlayerTeam(SetPlayerTeam) => SetPlayerTeam,
    SetScore(SetScore) => SetScore,
    SetSimulationDistance(SetSimulationDistance) => SetSimulationDistance,
    SetSubtitleText(SetSubtitleText) => SetSubtitleText,
    SetTime(SetTime) => SetTime,
    SetTitleText(SetTitleText) => SetTitleText,
    SetTitlesAnimation(SetTitlesAnimation) => SetTitlesAnimation,
    SoundEntity(SoundEntity) => SoundEntity,
    Sound(Sound) => Sound,
    StartConfiguration(StartConfiguration) => StartConfiguration,
    StopSound(StopSound) => StopSound,
    StoreCookie(StoreCookie) => StoreCookie,
    SwingAnimation(SwingAnimationPacket) => SwingAnimation,
    SystemChat(SystemChat) => SystemChat,
    TabList(TabList) => TabList,
    TagQuery(TagQuery) => TagQuery,
    TakeItemEntity(TakeItemEntity) => TakeItemEntity,
    TeleportEntity(TeleportEntity) => TeleportEntity,
    TestInstanceBlockStatus(TestInstanceBlockStatus) => TestInstanceBlockStatus,
    TickingState(TickingState) => TickingState,
    TickingStep(TickingStep) => TickingStep,
    Transfer(Transfer) => Transfer,
    UpdateAdvancements(UpdateAdvancements) => UpdateAdvancements,
    UpdateAttributes(UpdateAttributes) => UpdateAttributes,
    UpdateMobEffect(UpdateMobEffect) => UpdateMobEffect,
    UpdateRecipes(UpdateRecipes) => UpdateRecipes,
    UpdateTags(UpdateTags) => UpdateTags,
    ProjectilePower(ProjectilePower) => ProjectilePower,
    CustomReportDetails(CustomReportDetails) => CustomReportDetails,
    ServerLinks(ServerLinks) => ServerLinks,
    Waypoint(TrackedWaypoint) => Waypoint,
    ClearDialog(ClearDialog) => ClearDialog,
    ShowDialog(ShowDialog) => ShowDialog,
});
