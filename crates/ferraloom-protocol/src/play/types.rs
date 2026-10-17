//! Embedded play-state network forms shared by both directions.
use super::*;

pub use crate::common::{ByteArray, FriendlyString, OptionalNbt, OptionalVarInt, Utf8};

macro_rules! registry_ids {
    ($($name:ident),* $(,)?) => { $(
        /// Connection-local numeric registry reference; never an identifier string on the wire.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name(pub i32);
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> { r.var_int().map(Self) }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> { w.var_int(self.0); Ok(()) }
        }
    )* };
}
registry_ids!(
    BlockStateId,
    BlockId,
    BlockEntityTypeId,
    EntityTypeId,
    MobEffectId,
    DimensionTypeId,
    MenuId,
    AttributeId,
    DamageTypeId,
    MapDecorationTypeId,
    WorldClockId,
    ItemId,
    RecipeDisplayId
);

wire_enum!(InteractionHand { MainHand = 0, OffHand = 1 }, zero);
wire_enum!(Difficulty { Peaceful = 0, Easy = 1, Normal = 2, Hard = 3 }, wrapping);
wire_enum!(GameType { Survival = 0, Creative = 1, Adventure = 2, Spectator = 3 }, zero);
wire_enum!(SignTextSlot { Back = 0, Front = 1 }, zero);
wire_enum!(SoundSource { Master = 0, Music = 1, Records = 2, Weather = 3, Blocks = 4, Hostile = 5, Neutral = 6, Players = 7, Ambient = 8, Voice = 9, Ui = 10 });
wire_enum!(EntityAnchor { Feet = 0, Eyes = 1 });
wire_enum!(RecipeBookType { Crafting = 0, Furnace = 1, BlastFurnace = 2, Smoker = 3 });
wire_enum!(ClientCommandAction { PerformRespawn = 0, RequestStats = 1, RequestGameRuleValues = 2 });
wire_enum!(PlayerCommandAction { StopSleeping = 0, StartSprinting = 1, StopSprinting = 2, StartRidingJump = 3, StopRidingJump = 4, OpenInventory = 5, StartFallFlying = 6 });
wire_enum!(PlayerAction { StartDestroyBlock = 0, ChangeDestroyDirection = 1, AbortDestroyBlock = 2, StopDestroyBlock = 3, DropAllItems = 4, DropItem = 5, ReleaseUseItem = 6, SwapItemWithOffhand = 7, Stab = 8 });
wire_enum!(CommandBlockMode { Sequence = 0, Auto = 1, Redstone = 2 });
wire_enum!(TestBlockMode { Start = 0, Log = 1, Fail = 2, Accept = 3 }, zero);
wire_enum!(ChatCompletionAction { Add = 0, Remove = 1, Set = 2 }, zero);
wire_enum!(RemoteDebugSampleType { TickTime = 0 });

pub use crate::entity_data::Vec3;

packet!(/// Port of PositionAndRotation#STREAM_CODEC.
PositionAndRotation { position: Vec3, y_rot: f32, x_rot: f32 });
packet!(/// Port of PositionMoveRotation#STREAM_CODEC.
PositionMoveRotation { position: Vec3, delta_movement: Vec3, y_rot: f32, x_rot: f32 });
packet!(/// Port of PositionStep#STREAM_CODEC.
PositionStep { position: Vec3, tick_offset: VarInt });

/// Port of LpVec3. Retains quantized components exactly through decode/encode.
#[derive(Clone, Debug, PartialEq)]
pub struct LpVec3(pub Vec3);
impl Wire for LpVec3 {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let low = r.u8()?;
        if low == 0 {
            return Ok(Self(Vec3 {
                x: 0.,
                y: 0.,
                z: 0.,
            }));
        }
        let middle = r.u8()?;
        let packed = ((r.i32()? as u32 as u64) << 16) | (u64::from(middle) << 8) | u64::from(low);
        let mut scale = u64::from(low & 3);
        if low & 4 != 0 {
            scale |= (r.var_int()? as u32 as u64) << 2;
        }
        let unpack = |shift: u32| {
            (((packed >> shift) & 32767).min(32766) as f64 * 2. / 32766. - 1.) * scale as f64
        };
        Ok(Self(Vec3 {
            x: unpack(3),
            y: unpack(18),
            z: unpack(33),
        }))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        let sanitize = |v: f64| {
            if v.is_nan() {
                0.
            } else {
                v.clamp(-1.7179869183e10, 1.7179869183e10)
            }
        };
        let [x, y, z] = [self.0.x, self.0.y, self.0.z].map(sanitize);
        let length = x.abs().max(y.abs()).max(z.abs());
        if length < 3.051944088384301e-5 {
            w.u8(0);
            return Ok(());
        }
        let scale = length.ceil() as u64;
        let partial = scale > 3;
        let markers = (scale & 3) | if partial { 4 } else { 0 };
        let pack = |v: f64| ((v / scale as f64 * 0.5 + 0.5) * 32766. + 0.5).floor() as u64;
        let packed = markers | (pack(x) << 3) | (pack(y) << 18) | (pack(z) << 33);
        w.u8(packed as u8);
        w.u8((packed >> 8) as u8);
        w.i32((packed >> 16) as i32);
        if partial {
            w.var_int((scale >> 2) as i32);
        }
        Ok(())
    }
}

/// Port of PositionPath#STREAM_CODEC.
#[derive(Clone, Debug, PartialEq)]
pub enum PositionPath {
    Linear(Vec3),
    Stepped(Vec<PositionStep>),
}
impl Wire for PositionPath {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        if r.var_int()? == 1 {
            let steps: Vec<PositionStep> = r.read()?;
            if steps.is_empty() {
                return Err(Error::invalid("PositionPath", "empty steps"));
            }
            Ok(Self::Stepped(steps))
        } else {
            Ok(Self::Linear(r.read()?))
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        match self {
            Self::Linear(pos) => {
                w.var_int(0);
                pos.encode(w)
            }
            Self::Stepped(steps) => {
                if steps.is_empty() {
                    return Err(Error::invalid("PositionPath", "empty steps"));
                }
                w.var_int(1);
                steps.encode(w)
            }
        }
    }
}

/// Port of ServerboundMovePlayerPacket#packFlags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovementFlags {
    pub on_ground: bool,
    pub horizontal_collision: bool,
}
impl Wire for MovementFlags {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let b = r.u8()?;
        Ok(Self {
            on_ground: b & 1 != 0,
            horizontal_collision: b & 2 != 0,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(u8::from(self.on_ground) | (u8::from(self.horizontal_collision) << 1));
        Ok(())
    }
}

/// Port of Input#STREAM_CODEC.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Input {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub shift: bool,
    pub sprint: bool,
}
impl Wire for Input {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let b = r.u8()?;
        Ok(Self {
            forward: b & 1 != 0,
            backward: b & 2 != 0,
            left: b & 4 != 0,
            right: b & 8 != 0,
            jump: b & 16 != 0,
            shift: b & 32 != 0,
            sprint: b & 64 != 0,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(u8::from(self.forward)
            | (u8::from(self.backward) << 1)
            | (u8::from(self.left) << 2)
            | (u8::from(self.right) << 3)
            | (u8::from(self.jump) << 4)
            | (u8::from(self.shift) << 5)
            | (u8::from(self.sprint) << 6));
        Ok(())
    }
}

/// Port of GameType#OPTIONAL_STREAM_CODEC; this is a shifted VarInt, not a boolean.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionalGameType(pub Option<GameType>);
impl Wire for OptionalGameType {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.var_int()?;
        Ok(Self(if id == 0 {
            None
        } else {
            Some(match id {
                2 => GameType::Creative,
                3 => GameType::Adventure,
                4 => GameType::Spectator,
                _ => GameType::Survival,
            })
        }))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(match self.0 {
            None => 0,
            Some(GameType::Survival) => 1,
            Some(GameType::Creative) => 2,
            Some(GameType::Adventure) => 3,
            Some(GameType::Spectator) => 4,
        });
        Ok(())
    }
}
packet!(/// Port of CommonPlayerSpawnInfo.
CommonPlayerSpawnInfo {dimension_type:DimensionTypeId,dimension:Identifier,seed:i64,game_type:GameType,previous_game_type:OptionalGameType,is_debug:bool,is_flat:bool,last_death_location:Option<GlobalPos>,portal_cooldown:VarInt,sea_level:VarInt});
packet!(/// Port of LevelData.RespawnData.
RespawnData {global_pos:GlobalPos,yaw:f32,pitch:f32});
packet!(/// Port of ClockNetworkState.
ClockNetworkState {total_ticks:VarLong,partial_tick:f32,rate:f32});
packet!(/// Port of ClientboundSetTimePacket's clock map entries.
ClockUpdate {clock:WorldClockId,state:ClockNetworkState});
packet!(/// Port of gamerule key/value network entries.
GameRuleEntry {key:Identifier,value:String});
packet!(/// Port of ClientboundCommandSuggestionsPacket.Entry.
CommandSuggestion {text:String,tooltip:Option<Component> => optional_trusted_component});
pub use crate::component::{
    Sound as SoundEventHolder, SoundEvent, SwingAnimation, SwingAnimationType,
};

/// Port of BlockHitResult's offset floats, relative to the packed block position.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockHitResult {
    pub block_pos: BlockPos,
    pub direction: ferraloom_util::Direction,
    pub offset_x: f32,
    pub offset_y: f32,
    pub offset_z: f32,
    pub inside: bool,
    pub world_border_hit: bool,
}
impl Wire for BlockHitResult {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let block_pos = r.read()?;
        let id = r.var_int()?;
        let direction = ferraloom_util::Direction::VALUES[id.rem_euclid(6) as usize];
        Ok(Self {
            block_pos,
            direction,
            offset_x: r.f32()?,
            offset_y: r.f32()?,
            offset_z: r.f32()?,
            inside: r.bool()?,
            world_border_hit: r.bool()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.block_pos.encode(w)?;
        w.var_int(self.direction as i32);
        w.f32(self.offset_x);
        w.f32(self.offset_y);
        w.f32(self.offset_z);
        w.bool(self.inside);
        w.bool(self.world_border_hit);
        Ok(())
    }
}

/// Port of ByteBufCodecs.TRUSTED_COMPOUND_TAG.
#[derive(Clone, Debug, PartialEq)]
pub struct TrustedCompoundTag(pub ferraloom_nbt::CompoundTag);
impl Wire for TrustedCompoundTag {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match r.nbt(true)? {
            ferraloom_nbt::Tag::Compound(tag) => Ok(Self(tag)),
            _ => Err(Error::invalid("NBT", "expected compound")),
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.nbt(&ferraloom_nbt::Tag::Compound(self.0.clone()))
    }
}
packet!(/// Port of Vec3i#STREAM_CODEC.
Vec3i {x:VarInt,y:VarInt,z:VarInt});

wire_enum!(ParticleRandomization {Default=0,Alternative=1,AlternativeWithSpeed=2}, zero);
packet!(/// Port of ExplosionParticleInfo.
ExplosionParticleInfo {particle:crate::entity_data::Particle,scaling:f32,speed:f32});
packet!(/// Port of Weighted<ExplosionParticleInfo>.
WeightedExplosionParticle {value:ExplosionParticleInfo,weight:VarInt});
packet!(/// Port of ClientboundUpdateAttributesPacket.AttributeSnapshot.
AttributeSnapshot {attribute:AttributeId,base:f64,modifiers:Vec<AttributeModifier>});
packet!(/// Port of AttributeModifier's network form.
AttributeModifier {id:Identifier,amount:f64,operation:crate::component::AttributeOperation});

/// Port of Direction's unsigned-byte representation in player action packets.
#[derive(Clone, Debug, PartialEq)]
pub struct DirectionByte(pub ferraloom_util::Direction);
impl Wire for DirectionByte {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(ferraloom_util::Direction::from_3d_data_value(
            i32::from(r.u8()?),
        )))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(self.0 as u8);
        Ok(())
    }
}
wire_enum!(StructureUpdateType {UpdateData=0,SaveArea=1,LoadArea=2,ScanArea=3});
wire_enum!(StructureMode {Save=0,Load=1,Corner=2,Data=3});
wire_enum!(Mirror {None=0,LeftRight=1,FrontBack=2});
wire_enum!(Rotation {None=0,Clockwise90=1,Clockwise180=2,Counterclockwise90=3}, wrapping);
wire_enum!(TestInstanceStatus {Cleared=0,Running=1,Finished=2}, zero);
wire_enum!(TestInstanceAction {Init=0,Query=1,Set=2,Reset=3,Save=4,Export=5,Run=6}, zero);
packet!(/// Port of TestInstanceBlockEntity.Data.
TestInstanceData {test:Option<Identifier>,size:Vec3i,rotation:Rotation,ignore_entities:bool,status:TestInstanceStatus,error_message:Option<Component>});

macro_rules! bit_flags {
    ($name:ident {$($field:ident=$mask:literal),* $(,)?})=>{
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name {$(pub $field:bool),*}
        impl Wire for $name {
            fn decode(r:&mut Reader<'_>)->Result<Self>{let bits=r.u8()?;Ok(Self{$($field:bits&$mask!=0),*})}
            fn encode(&self,w:&mut Writer<'_>)->Result<()>{let mut bits=0;$(if self.$field{bits|=$mask;})*w.u8(bits);Ok(())}
        }
    };
}
bit_flags!(CommandBlockFlags {track_output=1,conditional=2,automatic=4});
bit_flags!(StructureBlockFlags {ignore_entities=1,show_air=2,show_bounding_box=4,strict=8});
#[derive(Clone, Debug, PartialEq)]
pub enum JigsawJoint {
    Rollable,
    Aligned,
}
impl Wire for JigsawJoint {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(if r.string(32767)?.as_ref() == "rollable" {
            Self::Rollable
        } else {
            Self::Aligned
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(
            match self {
                Self::Rollable => "rollable",
                Self::Aligned => "aligned",
            },
            32767,
        )
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StructureCoordinates<const MIN: i8, const MAX: i8> {
    pub x: i8,
    pub y: i8,
    pub z: i8,
}
impl<const MIN: i8, const MAX: i8> Wire for StructureCoordinates<MIN, MAX> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            x: r.i8()?.clamp(MIN, MAX),
            y: r.i8()?.clamp(MIN, MAX),
            z: r.i8()?.clamp(MIN, MAX),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.i8(self.x);
        w.i8(self.y);
        w.i8(self.z);
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct StructureIntegrity(pub f32);
impl Wire for StructureIntegrity {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(r.f32()?.clamp(0., 1.)))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.f32(self.0);
        Ok(())
    }
}

/// Port of ByteBufCodecs.BIT_SET. This byte-array representation differs from
/// FriendlyByteBuf's length-prefixed array of big-endian longs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ByteBitSet(pub BitSet);
impl ByteBitSet {
    pub fn encode_bits(bits: &BitSet, w: &mut Writer<'_>) -> Result<()> {
        let len = bits.bit_length().div_ceil(8);
        w.count(len, i32::MAX as usize)?;
        for i in 0..len {
            w.u8((bits.words()[i / 8] >> (i % 8 * 8)) as u8);
        }
        Ok(())
    }
}
impl Wire for ByteBitSet {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let bytes = r.byte_array(i32::MAX as usize)?;
        let mut words = vec![0; bytes.len().div_ceil(8)];
        for (i, byte) in bytes.iter().enumerate() {
            words[i / 8] |= u64::from(*byte) << (i % 8 * 8);
        }
        Ok(Self(BitSet::from_words(words)))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        Self::encode_bits(&self.0, w)
    }
}
