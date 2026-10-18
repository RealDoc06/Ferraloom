//! Typed nested predicates used by lock and adventure-mode component codecs.
use super::persistent::{
    entry, field, field_nbt, object, persistent_record, set_from_json, set_value,
};
use super::*;
use crate::{Identifier, Reader, Result, Wire, Writer, chat::Component, codec::HolderSet};
use ferraloom_nbt::Tag;
use serde_json::Value as Json;

/// Port of DamagePredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct DamagePredicate {
    pub durability: IntBounds,
    pub damage: IntBounds,
}
persistent_record!(DamagePredicate {durability=>"durability"[Some(IntBounds::default())],damage=>"damage"[Some(IntBounds::default())]});
/// Port of EnchantmentPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct EnchantmentPredicate {
    pub enchantments: Option<HolderSet>,
    pub levels: IntBounds,
}
persistent_record!(EnchantmentPredicate {enchantments=>"enchantments"[Some(None)],levels=>"levels"[Some(IntBounds::default())]});
/// Port of PotionsPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct PotionsPredicate {
    pub potions: Option<HolderSet>,
    pub effects: Option<CollectionPredicate<MobEffectsPredicate>>,
}
persistent_record!(PotionsPredicate {potions=>"potions"[Some(None)],effects=>"effects"[Some(None)]});
/// Port of ContainerPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct ContainerPredicate {
    pub items: Option<CollectionPredicate<LockCode>>,
}
persistent_record!(ContainerPredicate {items=>"items"[Some(None)]});
/// Port of FireworkPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct FireworkPredicate {
    pub shape: Option<FireworkShape>,
    pub has_twinkle: Option<bool>,
    pub has_trail: Option<bool>,
}
persistent_record!(FireworkPredicate {shape=>"shape"[Some(None)],has_twinkle=>"has_twinkle"[Some(None)],has_trail=>"has_trail"[Some(None)]});
/// Port of FireworksPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct FireworksPredicate {
    pub explosions: Option<CollectionPredicate<FireworkPredicate>>,
    pub flight_duration: IntBounds,
}
persistent_record!(FireworksPredicate {explosions=>"explosions"[Some(None)],flight_duration=>"flight_duration"[Some(IntBounds::default())]});
/// Port of WritableBookPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct WritableBookPredicate {
    pub pages: Option<CollectionPredicate<String>>,
}
persistent_record!(WritableBookPredicate {pages=>"pages"[Some(None)]});
/// Port of WrittenBookPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct WrittenBookPredicate {
    pub pages: Option<CollectionPredicate<Component>>,
    pub author: Option<String>,
    pub title: Option<String>,
    pub generation: IntBounds,
    pub resolved: Option<bool>,
}
persistent_record!(WrittenBookPredicate {pages=>"pages"[Some(None)],author=>"author"[Some(None)],title=>"title"[Some(None)],generation=>"generation"[Some(IntBounds::default())],resolved=>"resolved"[Some(None)]});
/// Port of AttributeModifiersPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct AttributeModifiersPredicate {
    pub modifiers: Option<CollectionPredicate<AttributePredicate>>,
}
persistent_record!(AttributeModifiersPredicate {modifiers=>"modifiers"[Some(None)]});
/// Port of AttributePredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct AttributePredicate {
    pub attribute: Option<HolderSet>,
    pub id: Option<Identifier>,
    pub amount: DoubleBounds,
    pub operation: Option<AttributeOperation>,
    pub slot: Option<EquipmentSlotGroup>,
}
persistent_record!(AttributePredicate {attribute=>"attribute"[Some(None)],id=>"id"[Some(None)],amount=>"amount"[Some(DoubleBounds::default())],operation=>"operation"[Some(None)],slot=>"slot"[Some(None)]});
/// Port of TrimPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct TrimPredicate {
    pub material: Option<HolderSet>,
    pub pattern: Option<HolderSet>,
}
persistent_record!(TrimPredicate {material=>"material"[Some(None)],pattern=>"pattern"[Some(None)]});
/// Port of JukeboxPlayablePredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct JukeboxPlayablePredicate {
    pub song: Option<HolderSet>,
}
persistent_record!(JukeboxPlayablePredicate {song=>"song"[Some(None)]});
/// Port of EffectPredicate#CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectPredicate {
    pub amplifier: IntBounds,
    pub duration: IntBounds,
    pub ambient: Option<bool>,
    pub visible: Option<bool>,
}
persistent_record!(EffectPredicate {amplifier=>"amplifier"[Some(IntBounds::default())],duration=>"duration"[Some(IntBounds::default())],ambient=>"ambient"[Some(None)],visible=>"visible"[Some(None)]});

impl Persistent for HolderSet {
    fn from_nbt(v: &Tag) -> Result<Self> {
        if let Tag::String(value) = v {
            let value = value.to_string_lossy();
            return if let Some(tag) = value.strip_prefix('#') {
                Ok(Self::Named(Identifier::parse(tag)?))
            } else {
                Ok(Self::Direct(vec![Identifier::parse(&value)?]))
            };
        }
        Ok(Self::Direct(Vec::<Identifier>::from_nbt(v)?))
    }
    fn from_json(v: &Json) -> Result<Self> {
        set_from_json(v)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        set_value(self)
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DoubleBounds {
    pub min: Option<f64>,
    pub max: Option<f64>,
}
// Double.compare/equals canonicalize NaNs and distinguish signed zero.
fn double_bits(value: f64) -> u64 {
    if value.is_nan() {
        f64::NAN.to_bits()
    } else {
        value.to_bits()
    }
}
impl Persistent for DoubleBounds {
    fn from_nbt(v: &Tag) -> Result<Self> {
        let result = if matches!(
            v,
            Tag::Byte(_)
                | Tag::Short(_)
                | Tag::Int(_)
                | Tag::Long(_)
                | Tag::Float(_)
                | Tag::Double(_)
        ) {
            let value = f64::from_nbt(v)?;
            Self {
                min: Some(value),
                max: Some(value),
            }
        } else {
            if v.as_compound().is_none() {
                return Err(invalid("double bounds"));
            }
            Self {
                min: field_nbt(v, "min", Some(None))?,
                max: field_nbt(v, "max", Some(None))?,
            }
        };
        result.to_persistent()?;
        Ok(result)
    }
    fn from_json(v: &Json) -> Result<Self> {
        let result = if v.is_number() {
            let n = f64::from_json(v)?;
            Self {
                min: Some(n),
                max: Some(n),
            }
        } else {
            object(v)?;
            Self {
                min: field(v, "min", Some(None))?,
                max: field(v, "max", Some(None))?,
            }
        };
        result.to_persistent()?;
        Ok(result)
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        if let (Some(min), Some(max)) = (self.min, self.max)
            && f64::from_bits(double_bits(min))
                .total_cmp(&f64::from_bits(double_bits(max)))
                .is_gt()
        {
            return Err(invalid("swapped bounds"));
        }
        if let (Some(min), Some(max)) = (self.min, self.max)
            && double_bits(min) == double_bits(max)
        {
            return min.to_persistent();
        }
        let mut v = Vec::new();
        entry(&mut v, "min", &self.min, Some(None))?;
        entry(&mut v, "max", &self.max, Some(None))?;
        Ok(PersistentValue::Map(v))
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct CollectionPredicate<P> {
    pub contains: Option<Vec<P>>,
    pub count: Option<Vec<CollectionCount<P>>>,
    pub size: Option<IntBounds>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CollectionCount<P> {
    pub test: P,
    pub count: IntBounds,
}
impl<P: Persistent + PartialEq> Persistent for CollectionCount<P> {
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self {
            test: field(v, "test", None)?,
            count: field(v, "count", None)?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self {
            test: field_nbt(v, "test", None)?,
            count: field_nbt(v, "count", None)?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(vec![
            ("test".into(), self.test.to_persistent()?),
            ("count".into(), self.count.to_persistent()?),
        ]))
    }
}
impl<P: Persistent + PartialEq> Persistent for CollectionPredicate<P> {
    fn from_json(v: &Json) -> Result<Self> {
        object(v)?;
        Ok(Self {
            contains: field(v, "contains", Some(None))?,
            count: field(v, "count", Some(None))?,
            size: field(v, "size", Some(None))?,
        })
    }
    fn from_nbt(v: &Tag) -> Result<Self> {
        v.as_compound()
            .ok_or_else(|| invalid("collection predicate"))?;
        Ok(Self {
            contains: field_nbt(v, "contains", Some(None))?,
            count: field_nbt(v, "count", Some(None))?,
            size: field_nbt(v, "size", Some(None))?,
        })
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        let mut v = Vec::new();
        entry(&mut v, "contains", &self.contains, Some(None))?;
        entry(&mut v, "count", &self.count, Some(None))?;
        entry(&mut v, "size", &self.size, Some(None))?;
        Ok(PersistentValue::Map(v))
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct MobEffectsPredicate(pub Vec<(MobEffect, EffectPredicate)>);
impl Persistent for MobEffectsPredicate {
    fn from_nbt(v: &Tag) -> Result<Self> {
        Ok(Self(
            v.as_compound()
                .ok_or_else(|| invalid("effect predicate map"))?
                .iter()
                .map(|(key, value)| {
                    Ok((
                        MobEffect(registry(&key.to_string_lossy())),
                        EffectPredicate::from_nbt(value)?,
                    ))
                })
                .collect::<Result<_>>()?,
        ))
    }
    fn from_json(v: &Json) -> Result<Self> {
        Ok(Self(
            object(v)?
                .iter()
                .map(|(key, value)| {
                    Ok((MobEffect(registry(key)), EffectPredicate::from_json(value)?))
                })
                .collect::<Result<_>>()?,
        ))
    }
    fn to_persistent(&self) -> Result<PersistentValue> {
        Ok(PersistentValue::Map(
            self.0
                .iter()
                .map(|(key, value)| Ok((key.0.to_string(), value.to_persistent()?)))
                .collect::<Result<_>>()?,
        ))
    }
}
/// Port of DataComponentPredicate.Single, including all concrete predicate codecs and AnyValue.
#[derive(Clone, Debug, PartialEq)]
pub enum ComponentPredicate {
    AnyValue(ComponentTypeId),
    Damage(DamagePredicate),
    Enchantments(Vec<EnchantmentPredicate>),
    StoredEnchantments(Vec<EnchantmentPredicate>),
    Potions(PotionsPredicate),
    CustomData(NbtPredicate),
    Container(ContainerPredicate),
    BundleContents(ContainerPredicate),
    FireworkExplosion(FireworkPredicate),
    Fireworks(FireworksPredicate),
    WritableBook(WritableBookPredicate),
    WrittenBook(WrittenBookPredicate),
    AttributeModifiers(AttributeModifiersPredicate),
    Trim(TrimPredicate),
    JukeboxPlayable(JukeboxPlayablePredicate),
    VillagerType(HolderSet),
}
impl ComponentPredicate {
    pub fn key(&self) -> Identifier {
        registry(match self {
            Self::AnyValue(id) => id.name(),
            Self::Damage(_) => "minecraft:damage",
            Self::Enchantments(_) => "minecraft:enchantments",
            Self::StoredEnchantments(_) => "minecraft:stored_enchantments",
            Self::Potions(_) => "minecraft:potion_contents",
            Self::CustomData(_) => "minecraft:custom_data",
            Self::Container(_) => "minecraft:container",
            Self::BundleContents(_) => "minecraft:bundle_contents",
            Self::FireworkExplosion(_) => "minecraft:firework_explosion",
            Self::Fireworks(_) => "minecraft:fireworks",
            Self::WritableBook(_) => "minecraft:writable_book_content",
            Self::WrittenBook(_) => "minecraft:written_book_content",
            Self::AttributeModifiers(_) => "minecraft:attribute_modifiers",
            Self::Trim(_) => "minecraft:trim",
            Self::JukeboxPlayable(_) => "minecraft:jukebox_playable",
            Self::VillagerType(_) => "minecraft:villager/variant",
        })
    }
    pub fn from_key(key: &Identifier, value: &Json) -> Result<Self> {
        Ok(match key.as_str() {
            "minecraft:damage" => Self::Damage(<DamagePredicate as Persistent>::from_json(value)?),
            "minecraft:enchantments" => {
                Self::Enchantments(<Vec<EnchantmentPredicate> as Persistent>::from_json(value)?)
            }
            "minecraft:stored_enchantments" => Self::StoredEnchantments(
                <Vec<EnchantmentPredicate> as Persistent>::from_json(value)?,
            ),
            "minecraft:potion_contents" => {
                Self::Potions(<PotionsPredicate as Persistent>::from_json(value)?)
            }
            "minecraft:custom_data" => {
                Self::CustomData(<NbtPredicate as Persistent>::from_json(value)?)
            }
            "minecraft:container" => {
                Self::Container(<ContainerPredicate as Persistent>::from_json(value)?)
            }
            "minecraft:bundle_contents" => {
                Self::BundleContents(<ContainerPredicate as Persistent>::from_json(value)?)
            }
            "minecraft:firework_explosion" => {
                Self::FireworkExplosion(<FireworkPredicate as Persistent>::from_json(value)?)
            }
            "minecraft:fireworks" => {
                Self::Fireworks(<FireworksPredicate as Persistent>::from_json(value)?)
            }
            "minecraft:writable_book_content" => {
                Self::WritableBook(<WritableBookPredicate as Persistent>::from_json(value)?)
            }
            "minecraft:written_book_content" => {
                Self::WrittenBook(<WrittenBookPredicate as Persistent>::from_json(value)?)
            }
            "minecraft:attribute_modifiers" => Self::AttributeModifiers(
                <AttributeModifiersPredicate as Persistent>::from_json(value)?,
            ),
            "minecraft:trim" => Self::Trim(<TrimPredicate as Persistent>::from_json(value)?),
            "minecraft:jukebox_playable" => {
                Self::JukeboxPlayable(<JukeboxPlayablePredicate as Persistent>::from_json(value)?)
            }
            "minecraft:villager/variant" => {
                Self::VillagerType(<HolderSet as Persistent>::from_json(value)?)
            }
            _ => {
                Unit::from_json(value)?;
                Self::AnyValue(
                    ComponentTypeId::from_name(key.as_str())
                        .ok_or_else(|| invalid("component predicate type"))?,
                )
            }
        })
    }
    pub fn from_key_nbt(key: &Identifier, value: &Tag) -> Result<Self> {
        Ok(match key.as_str() {
            "minecraft:damage" => Self::Damage(<DamagePredicate as Persistent>::from_nbt(value)?),
            "minecraft:enchantments" => {
                Self::Enchantments(<Vec<EnchantmentPredicate> as Persistent>::from_nbt(value)?)
            }
            "minecraft:stored_enchantments" => Self::StoredEnchantments(
                <Vec<EnchantmentPredicate> as Persistent>::from_nbt(value)?,
            ),
            "minecraft:potion_contents" => {
                Self::Potions(<PotionsPredicate as Persistent>::from_nbt(value)?)
            }
            "minecraft:custom_data" => {
                Self::CustomData(<NbtPredicate as Persistent>::from_nbt(value)?)
            }
            "minecraft:container" => {
                Self::Container(<ContainerPredicate as Persistent>::from_nbt(value)?)
            }
            "minecraft:bundle_contents" => {
                Self::BundleContents(<ContainerPredicate as Persistent>::from_nbt(value)?)
            }
            "minecraft:firework_explosion" => {
                Self::FireworkExplosion(<FireworkPredicate as Persistent>::from_nbt(value)?)
            }
            "minecraft:fireworks" => {
                Self::Fireworks(<FireworksPredicate as Persistent>::from_nbt(value)?)
            }
            "minecraft:writable_book_content" => {
                Self::WritableBook(<WritableBookPredicate as Persistent>::from_nbt(value)?)
            }
            "minecraft:written_book_content" => {
                Self::WrittenBook(<WrittenBookPredicate as Persistent>::from_nbt(value)?)
            }
            "minecraft:attribute_modifiers" => Self::AttributeModifiers(
                <AttributeModifiersPredicate as Persistent>::from_nbt(value)?,
            ),
            "minecraft:trim" => Self::Trim(<TrimPredicate as Persistent>::from_nbt(value)?),
            "minecraft:jukebox_playable" => {
                Self::JukeboxPlayable(<JukeboxPlayablePredicate as Persistent>::from_nbt(value)?)
            }
            "minecraft:villager/variant" => {
                Self::VillagerType(<HolderSet as Persistent>::from_nbt(value)?)
            }
            _ => {
                Unit::from_nbt(value)?;
                Self::AnyValue(
                    ComponentTypeId::from_name(key.as_str())
                        .ok_or_else(|| invalid("component predicate type"))?,
                )
            }
        })
    }
    pub fn to_persistent(&self) -> Result<PersistentValue> {
        match self {
            Self::AnyValue(_) => Unit.to_persistent(),
            Self::Damage(v) => v.to_persistent(),
            Self::Enchantments(v) => v.to_persistent(),
            Self::StoredEnchantments(v) => v.to_persistent(),
            Self::Potions(v) => v.to_persistent(),
            Self::CustomData(v) => v.to_persistent(),
            Self::Container(v) => v.to_persistent(),
            Self::BundleContents(v) => v.to_persistent(),
            Self::FireworkExplosion(v) => v.to_persistent(),
            Self::Fireworks(v) => v.to_persistent(),
            Self::WritableBook(v) => v.to_persistent(),
            Self::WrittenBook(v) => v.to_persistent(),
            Self::AttributeModifiers(v) => v.to_persistent(),
            Self::Trim(v) => v.to_persistent(),
            Self::JukeboxPlayable(v) => v.to_persistent(),
            Self::VillagerType(v) => v.to_persistent(),
        }
    }
}
impl Wire for ComponentPredicate {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        if r.bool()? {
            let key = r.registry(&registry("data_component_predicate_type"))?;
            let tag = r.nbt(false)?;
            Self::from_key_nbt(&key, &tag)
        } else {
            let id = r.read()?;
            Unit::from_nbt(&r.nbt(false)?)?;
            Ok(Self::AnyValue(id))
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        if let Self::AnyValue(id) = self {
            w.bool(false);
            w.write(id)?;
        } else {
            w.bool(true);
            w.registry(&registry("data_component_predicate_type"), &self.key())?;
        }
        w.nbt(&self.to_persistent()?.to_nbt()?)
    }
}
