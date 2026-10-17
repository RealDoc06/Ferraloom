//! Embedded play-state network forms shared by both directions.
use super::*;

pub use crate::common::{ByteArray, OptionalNbt, OptionalVarInt, Utf8};

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

wire_enum!(InteractionHand { MainHand = 0, OffHand = 1 });
wire_enum!(Difficulty { Peaceful = 0, Easy = 1, Normal = 2, Hard = 3 });
wire_enum!(GameType { Survival = 0, Creative = 1, Adventure = 2, Spectator = 3 });
wire_enum!(SignTextSlot { Back = 0, Front = 1 });
wire_enum!(SoundSource { Master = 0, Music = 1, Records = 2, Weather = 3, Blocks = 4, Hostile = 5, Neutral = 6, Players = 7, Ambient = 8, Voice = 9, Ui = 10 });
wire_enum!(EntityAnchor { Feet = 0, Eyes = 1 });
wire_enum!(RecipeBookType { Crafting = 0, Furnace = 1, BlastFurnace = 2, Smoker = 3 });
wire_enum!(ClientCommandAction { PerformRespawn = 0, RequestStats = 1, RequestGameRuleValues = 2 });
wire_enum!(PlayerCommandAction { StopSleeping = 0, StartSprinting = 1, StopSprinting = 2, StartRidingJump = 3, StopRidingJump = 4, OpenInventory = 5, StartFallFlying = 6 });
wire_enum!(PlayerAction { StartDestroyBlock = 0, ChangeDestroyDirection = 1, AbortDestroyBlock = 2, StopDestroyBlock = 3, DropAllItems = 4, DropItem = 5, ReleaseUseItem = 6, SwapItemWithOffhand = 7, Stab = 8 });
wire_enum!(CommandBlockMode { Sequence = 0, Auto = 1, Redstone = 2 });
wire_enum!(TestBlockMode { Start = 0, Log = 1, Fail = 2, Accept = 3 });
wire_enum!(ChatCompletionAction { Add = 0, Remove = 1, Set = 2 });
wire_enum!(RemoteDebugSampleType { TickTime = 0 });

packet!(/// Port of Vec3#STREAM_CODEC.
Vec3 { x: f64, y: f64, z: f64 });
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
