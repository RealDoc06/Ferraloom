//! Port of EntityDataSerializers and SynchedEntityData.DataValue for vanilla 26.3.
mod particle;
use crate::{
    BlockPos, Error, Reader, Result, Uuid, Wire, Writer,
    chat::Component,
    codec::{GlobalPos, VarInt, VarLong},
    component::*,
};
use ferraloom_data::BlockState;
pub use particle::*;

/// A serializer's stable protocol ID, from EntityDataSerializers' registration order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum EntityDataSerializer {
    Byte = 0,
    Int = 1,
    Long = 2,
    Float = 3,
    String = 4,
    Component = 5,
    OptionalComponent = 6,
    ItemStack = 7,
    Boolean = 8,
    Rotations = 9,
    BlockPos = 10,
    OptionalBlockPos = 11,
    Direction = 12,
    OptionalLivingEntityReference = 13,
    BlockState = 14,
    OptionalBlockState = 15,
    Particle = 16,
    Particles = 17,
    VillagerData = 18,
    OptionalUnsignedInt = 19,
    Pose = 20,
    CatVariant = 21,
    CatSoundVariant = 22,
    CowVariant = 23,
    CowSoundVariant = 24,
    WolfVariant = 25,
    WolfSoundVariant = 26,
    FrogVariant = 27,
    PigVariant = 28,
    PigSoundVariant = 29,
    ChickenVariant = 30,
    ChickenSoundVariant = 31,
    ZombieNautilusVariant = 32,
    OptionalGlobalPos = 33,
    PaintingVariant = 34,
    SnifferState = 35,
    ArmadilloState = 36,
    CopperGolemState = 37,
    WeatheringCopperState = 38,
    Vector3 = 39,
    Quaternion = 40,
    ResolvableProfile = 41,
    HumanoidArm = 42,
    DyeColor = 43,
}
/// Port of EntityDataSerializer's typed values. The variant determines its serializer.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityDataValue {
    Byte(i8),
    Int(VarInt),
    Long(VarLong),
    Float(f32),
    String(String),
    Component(Component),
    OptionalComponent(Option<Component>),
    ItemStack(ItemStack),
    Boolean(bool),
    Rotations(Rotations),
    BlockPos(BlockPos),
    OptionalBlockPos(Option<BlockPos>),
    Direction(Direction),
    OptionalLivingEntityReference(Option<Uuid>),
    BlockState(BlockState),
    OptionalBlockState(Option<BlockState>),
    Particle(Particle),
    Particles(Vec<Particle>),
    VillagerData(VillagerData),
    OptionalUnsignedInt(OptionalUnsignedInt),
    Pose(Pose),
    CatVariant(CatVariant),
    CatSoundVariant(CatSoundVariant),
    CowVariant(CowVariant),
    CowSoundVariant(CowSoundVariant),
    WolfVariant(WolfVariant),
    WolfSoundVariant(WolfSoundVariant),
    FrogVariant(FrogVariant),
    PigVariant(PigVariant),
    PigSoundVariant(PigSoundVariant),
    ChickenVariant(ChickenVariant),
    ChickenSoundVariant(ChickenSoundVariant),
    ZombieNautilusVariant(ZombieNautilusVariant),
    OptionalGlobalPos(Option<GlobalPos>),
    PaintingVariant(PaintingVariantHolder),
    SnifferState(SnifferState),
    ArmadilloState(ArmadilloState),
    CopperGolemState(CopperGolemState),
    WeatheringCopperState(WeatheringCopperState),
    Vector3(Vector3),
    Quaternion(Quaternion),
    ResolvableProfile(ResolvableProfile),
    HumanoidArm(HumanoidArm),
    DyeColor(DyeColor),
}
impl EntityDataValue {
    pub fn serializer(&self) -> EntityDataSerializer {
        match self {
            Self::Byte(_) => EntityDataSerializer::Byte,
            Self::Int(_) => EntityDataSerializer::Int,
            Self::Long(_) => EntityDataSerializer::Long,
            Self::Float(_) => EntityDataSerializer::Float,
            Self::String(_) => EntityDataSerializer::String,
            Self::Component(_) => EntityDataSerializer::Component,
            Self::OptionalComponent(_) => EntityDataSerializer::OptionalComponent,
            Self::ItemStack(_) => EntityDataSerializer::ItemStack,
            Self::Boolean(_) => EntityDataSerializer::Boolean,
            Self::Rotations(_) => EntityDataSerializer::Rotations,
            Self::BlockPos(_) => EntityDataSerializer::BlockPos,
            Self::OptionalBlockPos(_) => EntityDataSerializer::OptionalBlockPos,
            Self::Direction(_) => EntityDataSerializer::Direction,
            Self::OptionalLivingEntityReference(_) => {
                EntityDataSerializer::OptionalLivingEntityReference
            }
            Self::BlockState(_) => EntityDataSerializer::BlockState,
            Self::OptionalBlockState(_) => EntityDataSerializer::OptionalBlockState,
            Self::Particle(_) => EntityDataSerializer::Particle,
            Self::Particles(_) => EntityDataSerializer::Particles,
            Self::VillagerData(_) => EntityDataSerializer::VillagerData,
            Self::OptionalUnsignedInt(_) => EntityDataSerializer::OptionalUnsignedInt,
            Self::Pose(_) => EntityDataSerializer::Pose,
            Self::CatVariant(_) => EntityDataSerializer::CatVariant,
            Self::CatSoundVariant(_) => EntityDataSerializer::CatSoundVariant,
            Self::CowVariant(_) => EntityDataSerializer::CowVariant,
            Self::CowSoundVariant(_) => EntityDataSerializer::CowSoundVariant,
            Self::WolfVariant(_) => EntityDataSerializer::WolfVariant,
            Self::WolfSoundVariant(_) => EntityDataSerializer::WolfSoundVariant,
            Self::FrogVariant(_) => EntityDataSerializer::FrogVariant,
            Self::PigVariant(_) => EntityDataSerializer::PigVariant,
            Self::PigSoundVariant(_) => EntityDataSerializer::PigSoundVariant,
            Self::ChickenVariant(_) => EntityDataSerializer::ChickenVariant,
            Self::ChickenSoundVariant(_) => EntityDataSerializer::ChickenSoundVariant,
            Self::ZombieNautilusVariant(_) => EntityDataSerializer::ZombieNautilusVariant,
            Self::OptionalGlobalPos(_) => EntityDataSerializer::OptionalGlobalPos,
            Self::PaintingVariant(_) => EntityDataSerializer::PaintingVariant,
            Self::SnifferState(_) => EntityDataSerializer::SnifferState,
            Self::ArmadilloState(_) => EntityDataSerializer::ArmadilloState,
            Self::CopperGolemState(_) => EntityDataSerializer::CopperGolemState,
            Self::WeatheringCopperState(_) => EntityDataSerializer::WeatheringCopperState,
            Self::Vector3(_) => EntityDataSerializer::Vector3,
            Self::Quaternion(_) => EntityDataSerializer::Quaternion,
            Self::ResolvableProfile(_) => EntityDataSerializer::ResolvableProfile,
            Self::HumanoidArm(_) => EntityDataSerializer::HumanoidArm,
            Self::DyeColor(_) => EntityDataSerializer::DyeColor,
        }
    }
    pub fn decode(serializer_id: i32, r: &mut Reader<'_>) -> Result<Self> {
        Ok(match serializer_id {
            0 => Self::Byte(r.read()?),
            1 => Self::Int(r.read()?),
            2 => Self::Long(r.read()?),
            3 => Self::Float(r.read()?),
            4 => Self::String(r.read()?),
            5 => Self::Component(Component::decode_trusted(r)?),
            6 => Self::OptionalComponent(r.optional(Component::decode_trusted)?),
            7 => Self::ItemStack(r.read()?),
            8 => Self::Boolean(r.read()?),
            9 => Self::Rotations(r.read()?),
            10 => Self::BlockPos(r.read()?),
            11 => Self::OptionalBlockPos(r.read()?),
            12 => Self::Direction(r.read()?),
            13 => Self::OptionalLivingEntityReference(r.read()?),
            14 => Self::BlockState(r.read()?),
            15 => Self::OptionalBlockState(match r.var_int()? {
                0 => None,
                id => Some(block_state(id)?),
            }),
            16 => Self::Particle(r.read()?),
            17 => Self::Particles(r.read()?),
            18 => Self::VillagerData(r.read()?),
            19 => Self::OptionalUnsignedInt(r.read()?),
            20 => Self::Pose(r.read()?),
            21 => Self::CatVariant(r.read()?),
            22 => Self::CatSoundVariant(r.read()?),
            23 => Self::CowVariant(r.read()?),
            24 => Self::CowSoundVariant(r.read()?),
            25 => Self::WolfVariant(r.read()?),
            26 => Self::WolfSoundVariant(r.read()?),
            27 => Self::FrogVariant(r.read()?),
            28 => Self::PigVariant(r.read()?),
            29 => Self::PigSoundVariant(r.read()?),
            30 => Self::ChickenVariant(r.read()?),
            31 => Self::ChickenSoundVariant(r.read()?),
            32 => Self::ZombieNautilusVariant(r.read()?),
            33 => Self::OptionalGlobalPos(r.read()?),
            34 => Self::PaintingVariant(r.read()?),
            35 => Self::SnifferState(r.read()?),
            36 => Self::ArmadilloState(r.read()?),
            37 => Self::CopperGolemState(r.read()?),
            38 => Self::WeatheringCopperState(r.read()?),
            39 => Self::Vector3(r.read()?),
            40 => Self::Quaternion(r.read()?),
            41 => Self::ResolvableProfile(r.read()?),
            42 => Self::HumanoidArm(r.read()?),
            43 => Self::DyeColor(r.read()?),
            _ => {
                return Err(Error::invalid(
                    "entity data serializer",
                    serializer_id.to_string(),
                ));
            }
        })
    }
    pub fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        match self {
            Self::Byte(v) => w.write(v),
            Self::Int(v) => w.write(v),
            Self::Long(v) => w.write(v),
            Self::Float(v) => w.write(v),
            Self::String(v) => w.write(v),
            Self::Component(v) => w.write(v),
            Self::OptionalComponent(v) => w.optional(v, |w, v| w.write(v)),
            Self::ItemStack(v) => w.write(v),
            Self::Boolean(v) => w.write(v),
            Self::Rotations(v) => w.write(v),
            Self::BlockPos(v) => w.write(v),
            Self::OptionalBlockPos(v) => w.write(v),
            Self::Direction(v) => w.write(v),
            Self::OptionalLivingEntityReference(v) => w.write(v),
            Self::BlockState(v) => w.write(v),
            Self::OptionalBlockState(v) => {
                w.var_int(v.map_or(0, |s| s.id().into()));
                Ok(())
            }
            Self::Particle(v) => w.write(v),
            Self::Particles(v) => w.write(v),
            Self::VillagerData(v) => w.write(v),
            Self::OptionalUnsignedInt(v) => w.write(v),
            Self::Pose(v) => w.write(v),
            Self::CatVariant(v) => w.write(v),
            Self::CatSoundVariant(v) => w.write(v),
            Self::CowVariant(v) => w.write(v),
            Self::CowSoundVariant(v) => w.write(v),
            Self::WolfVariant(v) => w.write(v),
            Self::WolfSoundVariant(v) => w.write(v),
            Self::FrogVariant(v) => w.write(v),
            Self::PigVariant(v) => w.write(v),
            Self::PigSoundVariant(v) => w.write(v),
            Self::ChickenVariant(v) => w.write(v),
            Self::ChickenSoundVariant(v) => w.write(v),
            Self::ZombieNautilusVariant(v) => w.write(v),
            Self::OptionalGlobalPos(v) => w.write(v),
            Self::PaintingVariant(v) => w.write(v),
            Self::SnifferState(v) => w.write(v),
            Self::ArmadilloState(v) => w.write(v),
            Self::CopperGolemState(v) => w.write(v),
            Self::WeatheringCopperState(v) => w.write(v),
            Self::Vector3(v) => w.write(v),
            Self::Quaternion(v) => w.write(v),
            Self::ResolvableProfile(v) => w.write(v),
            Self::HumanoidArm(v) => w.write(v),
            Self::DyeColor(v) => w.write(v),
        }
    }
}
/// Port of SynchedEntityData.DataValue. Accessor 255 is reserved as the list terminator.
#[derive(Clone, Debug, PartialEq)]
pub struct DataValue {
    pub id: u8,
    pub value: EntityDataValue,
}
/// Port of ClientboundSetEntityDataPacket's item-list format, terminated by byte 255.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityDataItems(pub Vec<DataValue>);
impl Wire for EntityDataItems {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let mut values = Vec::new();
        loop {
            let id = r.u8()?;
            if id == 255 {
                return Ok(Self(values));
            }
            let serializer_id = r.var_int()?;
            values.push(DataValue {
                id,
                value: EntityDataValue::decode(serializer_id, r)?,
            });
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        for item in &self.0 {
            if item.id == 255 {
                return Err(Error::invalid("entity data accessor", "255 is reserved"));
            }
            w.u8(item.id);
            w.var_int(item.value.serializer() as i32);
            item.value.encode(w)?;
        }
        w.u8(255);
        Ok(())
    }
}
fn block_state(id: i32) -> Result<BlockState> {
    u16::try_from(id)
        .ok()
        .and_then(BlockState::from_id)
        .ok_or_else(|| Error::invalid("block state", id.to_string()))
}
impl Wire for BlockState {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        block_state(r.var_int()?)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(self.id().into());
        Ok(())
    }
}
/// Port of OPTIONAL_UNSIGNED_INT_CODEC; zero is absent, other values wrap after subtracting one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OptionalUnsignedInt(pub Option<i32>);
impl Wire for OptionalUnsignedInt {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(match r.var_int()? {
            0 => None,
            v => Some(v.wrapping_sub(1)),
        }))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(self.0.unwrap_or(-1).wrapping_add(1));
        Ok(())
    }
}
macro_rules! vector {
    ($name:ident { $($field:ident:$ty:ty),* }) => {
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct $name { $(pub $field:$ty),* }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> { Ok(Self { $($field:r.read()?),* }) }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> { $(w.write(&self.$field)?;)* Ok(()) }
        }
    };
}
vector!(Vector3 {
    x: f32,
    y: f32,
    z: f32
});
vector!(Quaternion {
    x: f32,
    y: f32,
    z: f32,
    w: f32
});
vector!(Vec3 {
    x: f64,
    y: f64,
    z: f64
});
/// Port of Rotations, including finite-angle normalization during construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotations {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}
impl Wire for Rotations {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let normalize = |v: f32| if v.is_finite() { v % 360.0 } else { 0.0 };
        Ok(Self {
            x: normalize(r.f32()?),
            y: normalize(r.f32()?),
            z: normalize(r.f32()?),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.f32(self.x);
        w.f32(self.y);
        w.f32(self.z);
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct VillagerData {
    pub villager_type: VillagerType,
    pub profession: VillagerProfession,
    pub level: i32,
}
impl Wire for VillagerData {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            villager_type: r.read()?,
            profession: r.read()?,
            level: r.var_int()?.max(1),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.write(&self.villager_type)?;
        w.write(&self.profession)?;
        w.var_int(self.level);
        Ok(())
    }
}
macro_rules! enumeration {
    ($name:ident, $map:expr, {$($variant:ident = $id:literal),*}) => {
        #[derive(Clone, Copy, Debug, PartialEq)]
        #[repr(i32)]
        pub enum $name { $($variant = $id),* }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> { let id = ($map)(r.var_int()?); Ok(match id { $($id => Self::$variant,)* _ => unreachable!() }) }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> { w.var_int(*self as i32); Ok(()) }
        }
    };
}
enumeration!(Pose, |v:i32| if (0..18).contains(&v) { v } else { 0 }, {Standing = 0, FallFlying = 1, Sleeping = 2, Swimming = 3, SpinAttack = 4, Crouching = 5, LongJumping = 6, Dying = 7, Croaking = 8, UsingTongue = 9, Sitting = 10, Roaring = 11, Sniffing = 12, Emerging = 13, Digging = 14, Sliding = 15, Shooting = 16, Inhaling = 17});
enumeration!(Direction, |v:i32| v.rem_euclid(6), {Down = 0, Up = 1, North = 2, South = 3, West = 4, East = 5});
enumeration!(HumanoidArm, |v:i32| if (0..2).contains(&v) { v } else { 0 }, {Left = 0, Right = 1});
enumeration!(SnifferState, |v:i32| if (0..7).contains(&v) { v } else { 0 }, {Idling = 0, FeelingHappy = 1, Scenting = 2, Sniffing = 3, Searching = 4, Digging = 5, Rising = 6});
enumeration!(ArmadilloState, |v:i32| if (0..4).contains(&v) { v } else { 0 }, {Idle = 0, Rolling = 1, Scared = 2, Unrolling = 3});
enumeration!(CopperGolemState, |v:i32| if (0..5).contains(&v) { v } else { 0 }, {Idle = 0, GettingItem = 1, GettingNoItem = 2, DroppingItem = 3, DroppingNoItem = 4});
enumeration!(WeatheringCopperState, |v:i32| v.clamp(0,3), {Unaffected = 0, Exposed = 1, Weathered = 2, Oxidized = 3});
