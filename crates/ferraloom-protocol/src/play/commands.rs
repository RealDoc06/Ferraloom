//! Port of ClientboundCommandsPacket. The game supplies the structural command tree.
use super::*;

/// Brigadier numeric bounds; the flags byte selects independent fixed-width endpoints.
#[derive(Clone, Debug, PartialEq)]
pub struct NumericBounds<T> {
    pub min: Option<T>,
    pub max: Option<T>,
}
pub trait NumericBound: Wire + PartialEq {
    const MIN: Self;
    const MAX: Self;
}
macro_rules! numeric_bound {($($ty:ty),*)=>{$(impl NumericBound for $ty {const MIN:Self=Self::MIN;const MAX:Self=Self::MAX;})*};}
numeric_bound!(f32, f64, i32, i64);
impl<T: NumericBound> Wire for NumericBounds<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let b = r.u8()?;
        Ok(Self {
            min: if b & 1 != 0 {
                let value = r.read()?;
                (value != T::MIN).then_some(value)
            } else {
                None
            },
            max: if b & 2 != 0 {
                let value = r.read()?;
                (value != T::MAX).then_some(value)
            } else {
                None
            },
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        let min = self.min.as_ref().filter(|v| **v != T::MIN);
        let max = self.max.as_ref().filter(|v| **v != T::MAX);
        w.u8(u8::from(min.is_some()) | (u8::from(max.is_some()) << 1));
        if let Some(v) = min {
            v.encode(w)?;
        }
        if let Some(v) = max {
            v.encode(w)?;
        }
        Ok(())
    }
}
wire_enum!(StringArgumentType {SingleWord=0,QuotablePhrase=1,GreedyPhrase=2});
#[derive(Clone, Debug, PartialEq)]
pub struct EntityArgumentFlags {
    pub single: bool,
    pub players_only: bool,
}
impl Wire for EntityArgumentFlags {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let b = r.u8()?;
        Ok(Self {
            single: b & 1 != 0,
            players_only: b & 2 != 0,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(u8::from(self.single) | (u8::from(self.players_only) << 1));
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreHolderFlags {
    pub multiple: bool,
}
impl Wire for ScoreHolderFlags {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            multiple: r.u8()? & 1 != 0,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.u8(u8::from(self.multiple));
        Ok(())
    }
}

wire_union!(CommandArgument:var_int {
    Bool=0 {},
    Float=1 {bounds:NumericBounds<f32>},
    Double=2 {bounds:NumericBounds<f64>},
    Integer=3 {bounds:NumericBounds<i32>},
    Long=4 {bounds:NumericBounds<i64>},
    String=5 {string_type:StringArgumentType},
    Entity=6 {flags:EntityArgumentFlags},
    GameProfile=7 {},
    BlockPos=8 {},
    ColumnPos=9 {},
    Vec3=10 {},
    Vec2=11 {},
    BlockState=12 {},
    BlockPredicate=13 {},
    ItemStack=14 {},
    ItemPredicate=15 {},
    TeamColor=16 {},
    HexColor=17 {},
    Component=18 {},
    Style=19 {},
    Message=20 {},
    NbtCompoundTag=21 {},
    NbtTag=22 {},
    NbtPath=23 {},
    Objective=24 {},
    ObjectiveCriteria=25 {},
    Operation=26 {},
    Particle=27 {},
    Angle=28 {},
    Rotation=29 {},
    ScoreboardSlot=30 {},
    ScoreHolder=31 {flags:ScoreHolderFlags},
    Swizzle=32 {},
    Team=33 {},
    ItemSlot=34 {},
    ItemSlots=35 {},
    ResourceLocation=36 {},
    Function=37 {},
    EntityAnchor=38 {},
    IntRange=39 {},
    FloatRange=40 {},
    Dimension=41 {},
    Gamemode=42 {},
    Time=43 {min:i32},
    ResourceOrTag=44 {registry:Identifier},
    ResourceOrTagKey=45 {registry:Identifier},
    Resource=46 {registry:Identifier},
    ResourceKey=47 {registry:Identifier},
    ResourceSelector=48 {registry:Identifier},
    TemplateMirror=49 {},
    TemplateRotation=50 {},
    Heightmap=51 {},
    LootTable=52 {},
    LootPredicate=53 {},
    LootModifier=54 {},
    ContextFloatProvider=55 {},
    ContextIntProvider=56 {},
    SlotSource=57 {},
    Dialog=58 {},
    Feature=59 {},
    SwingAnimation=60 {},
    Uuid=61 {},
});

#[derive(Clone, Debug, PartialEq)]
pub enum CommandNodeKind {
    Root,
    Literal(String),
    Argument {
        name: String,
        argument: CommandArgument,
        suggestions: Option<Identifier>,
    },
}
/// Port of ClientboundCommandsPacket.Entry, including 26.3's restricted-node bit.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandNode {
    pub executable: bool,
    pub restricted: bool,
    pub children: Vec<VarInt>,
    pub redirect: Option<VarInt>,
    pub kind: CommandNodeKind,
}
impl Wire for CommandNode {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let flags = r.u8()?;
        let children = r.read()?;
        let redirect = if flags & 8 != 0 {
            Some(r.read()?)
        } else {
            None
        };
        let kind = match flags & 3 {
            1 => CommandNodeKind::Literal(r.read()?),
            2 => CommandNodeKind::Argument {
                name: r.read()?,
                argument: r.read()?,
                suggestions: if flags & 16 != 0 {
                    Some(r.read()?)
                } else {
                    None
                },
            },
            _ => CommandNodeKind::Root,
        };
        Ok(Self {
            executable: flags & 4 != 0,
            restricted: flags & 32 != 0,
            children,
            redirect,
            kind,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        let mut flags = match &self.kind {
            CommandNodeKind::Root => 0,
            CommandNodeKind::Literal(_) => 1,
            CommandNodeKind::Argument { suggestions, .. } => {
                2 | if suggestions.is_some() { 16 } else { 0 }
            }
        };
        if self.executable {
            flags |= 4;
        }
        if self.restricted {
            flags |= 32;
        }
        if self.redirect.is_some() {
            flags |= 8;
        }
        w.u8(flags);
        self.children.encode(w)?;
        if let Some(v) = self.redirect {
            v.encode(w)?;
        }
        match &self.kind {
            CommandNodeKind::Root => {}
            CommandNodeKind::Literal(name) => name.encode(w)?,
            CommandNodeKind::Argument {
                name,
                argument,
                suggestions,
            } => {
                name.encode(w)?;
                argument.encode(w)?;
                if let Some(v) = suggestions {
                    v.encode(w)?;
                }
            }
        }
        Ok(())
    }
}
