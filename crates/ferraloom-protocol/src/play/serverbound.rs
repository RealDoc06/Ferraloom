//! Port of Serverbound play packet bodies.
use super::*;
pub use crate::common::serverbound::*;

packet!(/// Port of ServerboundAcceptTeleportationPacket#STREAM_CODEC.
AcceptTeleportation { id: VarInt, x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32 });
packet!(/// Port of ServerboundAttackPacket#STREAM_CODEC.
Attack { entity_id: VarInt });
packet!(/// Port of ServerboundBlockEntityTagQueryPacket#STREAM_CODEC.
BlockEntityTagQuery { transaction_id: VarInt, pos: BlockPos });
packet!(/// Port of ServerboundChangeDifficultyPacket#STREAM_CODEC.
ChangeDifficulty { difficulty: Difficulty });
packet!(/// Port of ServerboundChangeGameModePacket#STREAM_CODEC.
ChangeGameMode { mode: GameType });
packet!(/// Port of ServerboundChatAckPacket#STREAM_CODEC.
ChatAck { offset: VarInt });
packet!(/// Port of ServerboundChatCommandPacket#STREAM_CODEC.
ChatCommand { command: String });
packet!(/// Port of ServerboundChunkBatchReceivedPacket#STREAM_CODEC.
ChunkBatchReceived { desired_chunks_per_tick: f32 });
packet!(/// Port of ServerboundClientCommandPacket#STREAM_CODEC.
ClientCommand { action: ClientCommandAction });
packet!(/// Port of ServerboundClientTickEndPacket#STREAM_CODEC.
ClientTickEnd {  });
packet!(/// Port of ServerboundCommandSuggestionPacket#STREAM_CODEC.
CommandSuggestion { id: VarInt, command: Utf8<32500> });
packet!(/// Port of ServerboundConfigurationAcknowledgedPacket#STREAM_CODEC.
ConfigurationAcknowledged {  });
packet!(/// Port of ServerboundContainerButtonClickPacket#STREAM_CODEC.
ContainerButtonClick { container_id: VarInt, button_id: VarInt });
packet!(/// Port of ServerboundContainerClosePacket#STREAM_CODEC.
ContainerClose { container_id: VarInt });
packet!(/// Port of ServerboundContainerSlotStateChangedPacket#STREAM_CODEC.
ContainerSlotStateChanged { slot_id: VarInt, container_id: VarInt, new_state: bool });
packet!(/// Port of ServerboundEntityTagQueryPacket#STREAM_CODEC.
EntityTagQuery { transaction_id: VarInt, entity_id: VarInt });
packet!(/// Port of ServerboundInteractPacket#STREAM_CODEC.
Interact { entity_id: VarInt, hand: InteractionHand, location: LpVec3, using_secondary_action: bool });
packet!(/// Port of ServerboundJigsawGeneratePacket#STREAM_CODEC.
JigsawGenerate { pos: BlockPos, levels: VarInt, keep_jigsaws: bool });
packet!(/// Port of ServerboundLockDifficultyPacket#STREAM_CODEC.
LockDifficulty { locked: bool });
packet!(/// Port of ServerboundMovePlayerPosPacket#STREAM_CODEC.
MovePlayerPos { x: f64, y: f64, z: f64, flags: MovementFlags });
packet!(/// Port of ServerboundMovePlayerPosRotPacket#STREAM_CODEC.
MovePlayerPosRot { x: f64, y: f64, z: f64, y_rot: f32, x_rot: f32, flags: MovementFlags });
packet!(/// Port of ServerboundMovePlayerRotPacket#STREAM_CODEC.
MovePlayerRot { y_rot: f32, x_rot: f32, flags: MovementFlags });
packet!(/// Port of ServerboundMovePlayerStatusOnlyPacket#STREAM_CODEC.
MovePlayerStatusOnly { flags: MovementFlags });
packet!(/// Port of ServerboundMoveVehiclePacket#STREAM_CODEC.
MoveVehicle { moving_to: PositionAndRotation, on_ground: bool });
packet!(/// Port of ServerboundPaddleBoatPacket#STREAM_CODEC.
PaddleBoat { left: bool, right: bool });
packet!(/// Port of ServerboundPickItemFromBlockPacket#STREAM_CODEC.
PickItemFromBlock { pos: BlockPos, include_data: bool });
packet!(/// Port of ServerboundPickItemFromEntityPacket#STREAM_CODEC.
PickItemFromEntity { id: VarInt, include_data: bool });
packet!(/// Port of ServerboundPlaceRecipePacket#STREAM_CODEC.
PlaceRecipe { container_id: VarInt, recipe: RecipeDisplayId, use_max_items: bool });
packet!(/// Port of ServerboundPlayerCommandPacket#STREAM_CODEC.
PlayerCommand { id: VarInt, action: PlayerCommandAction, data: VarInt });
packet!(/// Port of ServerboundPlayerInputPacket#STREAM_CODEC.
PlayerInput { input: Input });
packet!(/// Port of ServerboundPlayerLoadedPacket#STREAM_CODEC.
PlayerLoaded {  });
packet!(/// Port of ServerboundPunchPacket#STREAM_CODEC.
Punch {  });
packet!(/// Port of ServerboundRecipeBookChangeSettingsPacket#STREAM_CODEC.
RecipeBookChangeSettings { book_type: RecipeBookType, is_open: bool, is_filtering: bool });
packet!(/// Port of ServerboundRecipeBookSeenRecipePacket#STREAM_CODEC.
RecipeBookSeenRecipe { recipe: RecipeDisplayId });
packet!(/// Port of ServerboundRenameItemPacket#STREAM_CODEC.
RenameItem { name: String });
packet!(/// Port of ServerboundSelectBundleItemPacket#STREAM_CODEC.
SelectBundleItem { slot_id: VarInt, selected_item_index: VarInt });
packet!(/// Port of ServerboundSelectTradePacket#STREAM_CODEC.
SelectTrade { item: VarInt });
packet!(/// Port of ServerboundSetBeaconPacket#STREAM_CODEC.
SetBeacon { primary: Option<MobEffectId>, secondary: Option<MobEffectId> });
packet!(/// Port of ServerboundSetCarriedItemPacket#STREAM_CODEC.
SetCarriedItem { slot: i16 });
packet!(/// Port of ServerboundSetCommandMinecartPacket#STREAM_CODEC.
SetCommandMinecart { entity: VarInt, command: String, track_output: bool });
packet!(/// Port of ServerboundSetTestBlockPacket#STREAM_CODEC.
SetTestBlock { position: BlockPos, mode: TestBlockMode, message: String });
packet!(/// Port of ServerboundSpectatorActionPacket#STREAM_CODEC.
SpectatorAction { spectate_entity_id: OptionalVarInt });
packet!(/// Port of ServerboundTeleportToEntityPacket#STREAM_CODEC.
TeleportToEntity { uuid: Uuid });
packet!(/// Port of ServerboundUseItemPacket#STREAM_CODEC.
UseItem { hand: InteractionHand, sequence: VarInt, y_rot: f32, x_rot: f32 });

/// Port of ServerboundPlayerAbilitiesPacket.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerAbilities {
    pub is_flying: bool,
}
impl Wire for PlayerAbilities {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            is_flying: r.u8()? & 2 != 0,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(if self.is_flying { 2 } else { 0 });
        Ok(())
    }
}

/// Port of ServerboundSeenAdvancementsPacket. The action selects whether a tab follows.
#[derive(Clone, Debug, PartialEq)]
pub enum SeenAdvancements {
    OpenedTab(Identifier),
    ClosedScreen,
}
impl Wire for SeenAdvancements {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match r.var_int()? {
            0 => Ok(Self::OpenedTab(r.read()?)),
            1 => Ok(Self::ClosedScreen),
            id => Err(Error::invalid("advancement action", id.to_string())),
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        match self {
            Self::OpenedTab(tab) => {
                w.var_int(0);
                tab.encode(w)
            }
            Self::ClosedScreen => {
                w.var_int(1);
                Ok(())
            }
        }
    }
}

/// Port of ServerboundEditBookPacket, including the page and title limits.
#[derive(Clone, Debug, PartialEq)]
pub struct EditBook {
    pub slot: VarInt,
    pub pages: Vec<Utf8<1024>>,
    pub title: Option<Utf8<32>>,
}
impl Wire for EditBook {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            slot: r.read()?,
            pages: r.list(100, Wire::decode)?,
            title: r.read()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.slot.encode(w)?;
        w.list(&self.pages, 100, |w, v| v.encode(w))?;
        self.title.encode(w)
    }
}

/// Port of ServerboundSignUpdatePacket. Four strings have no list length prefix.
#[derive(Clone, Debug, PartialEq)]
pub struct SignUpdate {
    pub pos: BlockPos,
    pub lines: [Utf8<384>; 4],
    pub slot: SignTextSlot,
}
impl Wire for SignUpdate {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            pos: r.read()?,
            lines: [r.read()?, r.read()?, r.read()?, r.read()?],
            slot: r.read()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.pos.encode(w)?;
        for line in &self.lines {
            line.encode(w)?;
        }
        self.slot.encode(w)
    }
}
packet!(/// Port of ServerboundChatCommandSignedPacket.
ChatCommandSigned {command:String,timestamp:i64,salt:i64,argument_signatures:ArgumentSignatures,last_seen_messages:LastSeenMessagesUpdate});
packet!(/// Port of ServerboundChatPacket.
Chat {message:Utf8<256>,timestamp:i64,salt:i64,signature:Option<MessageSignature>,last_seen_messages:LastSeenMessagesUpdate});
packet!(/// Port of ServerboundChatSessionUpdatePacket.
ChatSessionUpdate {chat_session:RemoteChatSessionData});
packet!(/// Port of ServerboundSetGameRulePacket.
SetGameRule {entries:Vec<GameRuleEntry>});
packet!(/// Port of ServerboundUseItemOnPacket.
UseItemOn {hand:InteractionHand,hit_result:BlockHitResult,sequence:VarInt});
