//! Port of Clientbound play packet bodies.
use super::*;

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
pub struct PlayerAbilities {pub invulnerable:bool,pub is_flying:bool,pub can_fly:bool,pub instabuild:bool,pub flying_speed:f32,pub walking_speed:f32}
impl Wire for PlayerAbilities {
    fn decode(r:&mut Reader<'_>)->Result<Self>{let b=r.u8()?;Ok(Self{invulnerable:b&1!=0,is_flying:b&2!=0,can_fly:b&4!=0,instabuild:b&8!=0,flying_speed:r.read()?,walking_speed:r.read()?})}
    fn encode(&self,w:&mut Writer<'_>)->Result<()>{w.u8(u8::from(self.invulnerable)|(u8::from(self.is_flying)<<1)|(u8::from(self.can_fly)<<2)|(u8::from(self.instabuild)<<3));self.flying_speed.encode(w)?;self.walking_speed.encode(w)}
}

packet!(/// Port of ClientboundPlayerLookAtPacket's entity target.
LookAtEntity {id:VarInt,anchor:EntityAnchor});
packet!(/// Port of ClientboundPlayerLookAtPacket.
PlayerLookAt {from_anchor:EntityAnchor,x:f64,y:f64,z:f64,entity:Option<LookAtEntity>});

/// Port of ClientboundStopSoundPacket. Flags select independently optional fields.
#[derive(Clone, Debug, PartialEq)]
pub struct StopSound {pub source:Option<SoundSource>,pub name:Option<Identifier>}
impl Wire for StopSound {
    fn decode(r:&mut Reader<'_>)->Result<Self>{let b=r.u8()?;Ok(Self{source:if b&1!=0{Some(r.read()?)}else{None},name:if b&2!=0{Some(r.read()?)}else{None}})}
    fn encode(&self,w:&mut Writer<'_>)->Result<()>{w.u8(u8::from(self.source.is_some())|(u8::from(self.name.is_some())<<1));if let Some(v)=&self.source{v.encode(w)?;}if let Some(v)=&self.name{v.encode(w)?;}Ok(())}
}
