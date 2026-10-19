use super::{ComponentPredicate, ItemStackTemplate, JavaF32, JavaF64, invalid, registry};
use crate::chat::Component;
use crate::{
    Identifier, Reader, Result, Uuid, Wire, Writer,
    codec::{Either, GlobalPos, Holder, HolderSet, VarInt},
};
use ferraloom_nbt::Tag;
use ferraloom_util::collections::{JavaHash, record_hash_code};

impl Wire for JavaF32 {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        f32::decode(r).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.0.encode(w)
    }
}
impl Wire for JavaF64 {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        f64::decode(r).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.0.encode(w)
    }
}

macro_rules! record {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[doc = concat!("Port of ", stringify!($name), "#STREAM_CODEC.")]
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name { $(pub $field: $ty),* }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> { Ok(Self { $($field: r.read()?),* }) }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> { $(w.write(&self.$field)?;)* Ok(()) }
        }
    };
}
macro_rules! registry_ref {
    ($name:ident, $registry:literal) => {
        #[doc = concat!("Reference in the ", $registry, " registry (unshifted holderRegistry codec).")]
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name(pub Identifier);
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> {
                Ok(Self(r.registry(&registry($registry))?))
            }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
                w.registry(&registry($registry), &self.0)
            }
        }
    };
}
macro_rules! registry_holder {
    ($name:ident, $direct:ident, $registry:literal) => {
        #[doc = concat!("Port of ", stringify!($direct), "#STREAM_CODEC, with inline values and shifted registry IDs.")]
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(pub Holder<Box<$direct>>);
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> { Ok(Self(Holder::decode(r, &registry($registry))?)) }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> { self.0.encode(w, &registry($registry)) }
        }
    };
}
macro_rules! registry_set {
    ($name:ident, $registry:literal) => {
        #[doc = concat!("HolderSet in the ", $registry, " registry.")]
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(pub HolderSet);
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> {
                Ok(Self(HolderSet::decode(r, &registry($registry))?))
            }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
                self.0.encode(w, &registry($registry))
            }
        }
    };
}
macro_rules! wire_enum {
    ($name:ident { $first:ident = $first_id:literal $(, $variant:ident = $id:literal)* $(,)? }) => {
        #[doc = concat!("Port of ", stringify!($name), "#STREAM_CODEC (ByIdMap ZERO fallback).")]
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum $name { $first = $first_id, $($variant = $id),* }
        impl Wire for $name {
            fn decode(r: &mut Reader<'_>) -> Result<Self> {
                Ok(match r.var_int()? { $first_id => Self::$first, $($id => Self::$variant,)* _ => Self::$first })
            }
            fn encode(&self, w: &mut Writer<'_>) -> Result<()> { w.var_int(*self as i32); Ok(()) }
        }
    };
}
/// Port of Unit#STREAM_CODEC: zero bytes, unlike its persistent empty map.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Unit;
impl Wire for Unit {
    fn decode(_: &mut Reader<'_>) -> Result<Self> {
        Ok(Self)
    }
    fn encode(&self, _: &mut Writer<'_>) -> Result<()> {
        Ok(())
    }
}
/// Port of ByteBufCodecs.COMPOUND_TAG.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomData(pub Tag);
impl Wire for CustomData {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        match r.nbt(false)? {
            value @ Tag::Compound(_) => Ok(Self(value)),
            _ => Err(invalid("compound tag")),
        }
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        if !matches!(self.0, Tag::Compound(_)) {
            return Err(invalid("compound tag"));
        }
        w.nbt(&self.0)
    }
}
/// A length bounded vanilla list. Public storage is typed, with bounds enforced on both paths.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundedList<T, const MAX: usize>(pub Vec<T>);
impl<T: Wire, const MAX: usize> Wire for BoundedList<T, MAX> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(r.list(MAX, T::decode)?))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.0, MAX, |w, v| v.encode(w))
    }
}
/// Port of Filterable#streamCodec.
#[derive(Clone, Debug, PartialEq)]
pub struct Filterable<T> {
    pub raw: T,
    pub filtered: Option<T>,
}
impl<T: Wire> Wire for Filterable<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            raw: r.read()?,
            filtered: r.read()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.write(&self.raw)?;
        w.write(&self.filtered)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct BoundedString<const MAX: usize>(pub String);
impl<const MAX: usize> Wire for BoundedString<MAX> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(r.string(MAX)?.into_owned()))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(&self.0, MAX)
    }
}
registry_ref!(Enchantment, "enchantment");
registry_ref!(Attribute, "attribute");
registry_ref!(DamageType, "damage_type");
registry_ref!(MobEffect, "mob_effect");
registry_ref!(Potion, "potion");
registry_ref!(EntityType, "entity_type");
registry_ref!(BlockEntityType, "block_entity_type");
registry_ref!(VillagerType, "villager_type");
registry_ref!(VillagerProfession, "villager_profession");
registry_ref!(WolfVariant, "wolf_variant");
registry_ref!(WolfSoundVariant, "wolf_sound_variant");
registry_ref!(CowVariant, "cow_variant");
registry_ref!(CowSoundVariant, "cow_sound_variant");
registry_ref!(PigVariant, "pig_variant");
registry_ref!(PigSoundVariant, "pig_sound_variant");
registry_ref!(ChickenVariant, "chicken_variant");
registry_ref!(ChickenSoundVariant, "chicken_sound_variant");
registry_ref!(CatVariant, "cat_variant");
registry_ref!(CatSoundVariant, "cat_sound_variant");
registry_ref!(FrogVariant, "frog_variant");
registry_ref!(ZombieNautilusVariant, "zombie_nautilus_variant");
registry_ref!(DecoratedPotPattern, "decorated_pot_pattern");
registry_set!(BlockSet, "block");
registry_set!(ItemSet, "item");
registry_set!(EntityTypeSet, "entity_type");
registry_set!(DamageTypeSet, "damage_type");
registry_set!(MobEffectSet, "mob_effect");
registry_set!(BannerPatternSet, "banner_pattern");
registry_set!(ComponentTypeSet, "data_component_type");
wire_enum!(Rarity { Common = 0, Uncommon = 1, Rare = 2, Epic = 3 });
wire_enum!(DyeColor { White = 0, Orange = 1, Magenta = 2, LightBlue = 3, Yellow = 4, Lime = 5, Pink = 6, Gray = 7, LightGray = 8, Cyan = 9, Purple = 10, Blue = 11, Brown = 12, Green = 13, Red = 14, Black = 15 });
wire_enum!(EquipmentSlot { Mainhand = 0, Feet = 1, Legs = 2, Chest = 3, Head = 4, Offhand = 5, Body = 6, Saddle = 7 });
wire_enum!(EquipmentSlotGroup { Any = 0, Mainhand = 1, Offhand = 2, Hand = 3, Feet = 4, Legs = 5, Chest = 6, Head = 7, Armor = 8, Body = 9, Saddle = 10 });
impl EquipmentSlotGroup {
    /// Port of EquipmentSlotGroup#test, including animal armor but not saddles.
    pub fn test(self, slot: EquipmentSlot) -> bool {
        match self {
            Self::Any => true,
            Self::Mainhand => slot == EquipmentSlot::Mainhand,
            Self::Offhand => slot == EquipmentSlot::Offhand,
            Self::Hand => matches!(slot, EquipmentSlot::Mainhand | EquipmentSlot::Offhand),
            Self::Feet => slot == EquipmentSlot::Feet,
            Self::Legs => slot == EquipmentSlot::Legs,
            Self::Chest => slot == EquipmentSlot::Chest,
            Self::Head => slot == EquipmentSlot::Head,
            Self::Armor => matches!(
                slot,
                EquipmentSlot::Feet
                    | EquipmentSlot::Legs
                    | EquipmentSlot::Chest
                    | EquipmentSlot::Head
                    | EquipmentSlot::Body
            ),
            Self::Body => slot == EquipmentSlot::Body,
            Self::Saddle => slot == EquipmentSlot::Saddle,
        }
    }
}

wire_enum!(AttributeOperation { AddValue = 0, AddMultipliedBase = 1, AddMultipliedTotal = 2 });
wire_enum!(FireworkShape { SmallBall = 0, LargeBall = 1, Star = 2, Creeper = 3, Burst = 4 });
wire_enum!(ItemUseAnimation { None = 0, Eat = 1, Drink = 2, Block = 3, Bow = 4, Trident = 5, Crossbow = 6, Spyglass = 7, TootHorn = 8, Brush = 9, Bundle = 10, Spear = 11 });
wire_enum!(SwingAnimationType { None = 0, Whack = 1, Stab = 2 });
wire_enum!(MapPostProcessing { Lock = 0, Scale = 1 });
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerModelType {
    Slim,
    Wide,
}
impl Wire for PlayerModelType {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(if r.bool()? { Self::Slim } else { Self::Wide })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.bool(*self == Self::Slim);
        Ok(())
    }
}
wire_enum!(FoxVariant { Red = 0, Snow = 1 });
/// Port of SalmonVariant's ByIdMap fallback policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SalmonVariant {
    Small = 0,
    Medium = 1,
    Large = 2,
}
impl Wire for SalmonVariant {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let v = r.var_int()?;
        Ok(match v.clamp(0, 2) {
            0 => Self::Small,
            1 => Self::Medium,
            2 => Self::Large,
            _ => unreachable!(),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(*self as i32);
        Ok(())
    }
}
/// Port of ParrotVariant's ByIdMap fallback policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParrotVariant {
    RedBlue = 0,
    Blue = 1,
    Green = 2,
    YellowBlue = 3,
    Gray = 4,
}
impl Wire for ParrotVariant {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let v = r.var_int()?;
        Ok(match v.clamp(0, 4) {
            0 => Self::RedBlue,
            1 => Self::Blue,
            2 => Self::Green,
            3 => Self::YellowBlue,
            4 => Self::Gray,
            _ => unreachable!(),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(*self as i32);
        Ok(())
    }
}
wire_enum!(TropicalFishPattern { Kob = 0, Sunstreak = 256, Snooper = 512, Dasher = 768, Brinely = 1024, Spotty = 1280, Flopper = 1, Stripey = 257, Glitter = 513, Blockfish = 769, Betty = 1025, Clayfish = 1281 });
/// Port of MooshroomVariant's ByIdMap fallback policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MooshroomVariant {
    Red = 0,
    Brown = 1,
}
impl Wire for MooshroomVariant {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let v = r.var_int()?;
        Ok(match v.clamp(0, 1) {
            0 => Self::Red,
            1 => Self::Brown,
            _ => unreachable!(),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(*self as i32);
        Ok(())
    }
}
wire_enum!(RabbitVariant { Brown = 0, White = 1, Black = 2, WhiteSplotched = 3, Gold = 4, Salt = 5, Evil = 99 });
/// Port of HorseVariant's ByIdMap fallback policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorseVariant {
    White = 0,
    Creamy = 1,
    Chestnut = 2,
    Brown = 3,
    Black = 4,
    Gray = 5,
    DarkBrown = 6,
}
impl Wire for HorseVariant {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let v = r.var_int()?;
        Ok(match v.rem_euclid(7) {
            0 => Self::White,
            1 => Self::Creamy,
            2 => Self::Chestnut,
            3 => Self::Brown,
            4 => Self::Black,
            5 => Self::Gray,
            6 => Self::DarkBrown,
            _ => unreachable!(),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(*self as i32);
        Ok(())
    }
}
/// Port of LlamaVariant's ByIdMap fallback policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LlamaVariant {
    Creamy = 0,
    White = 1,
    Brown = 2,
    Gray = 3,
}
impl Wire for LlamaVariant {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let v = r.var_int()?;
        Ok(match v.clamp(0, 3) {
            0 => Self::Creamy,
            1 => Self::White,
            2 => Self::Brown,
            3 => Self::Gray,
            _ => unreachable!(),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(*self as i32);
        Ok(())
    }
}
wire_enum!(AxolotlVariant { Lucy = 0, Wild = 1, Gold = 2, Cyan = 3, Blue = 4 });
record!(SoundEvent { location: Identifier, fixed_range: Option<JavaF32> });
record!(TrimMaterial {
    palette_id: Identifier,
    description: Component
});
record!(TrimPattern {
    asset_id: Identifier,
    description: Component,
    decal: bool
});
record!(Instrument {
    sound_event: Sound,
    use_duration: JavaF32,
    range: JavaF32,
    durability_damage: VarInt,
    description: Component
});
record!(JukeboxSong {
    sound_event: Sound,
    description: Component,
    length_in_seconds: JavaF32,
    comparator_output: VarInt
});
record!(BannerPattern {
    asset_id: Identifier,
    translation_key: String
});
/// Port of PaintingVariant's direct codec, with boolean-prefixed optional text.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintingVariant {
    pub width: VarInt,
    pub height: VarInt,
    pub asset_id: Identifier,
    pub title: Option<Component>,
    pub author: Option<Component>,
}
impl Wire for PaintingVariant {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            width: r.read()?,
            height: r.read()?,
            asset_id: r.read()?,
            title: r.optional(Component::decode_trusted)?,
            author: r.optional(Component::decode_trusted)?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.write(&self.width)?;
        w.write(&self.height)?;
        w.write(&self.asset_id)?;
        w.optional(&self.title, |w, v| w.write(v))?;
        w.optional(&self.author, |w, v| w.write(v))
    }
}
record!(UseEffects {
    can_sprint: bool,
    interact_vibrations: bool,
    speed_multiplier: JavaF32
});
record!(FoodProperties {
    nutrition: VarInt,
    saturation: JavaF32,
    can_always_eat: bool
});
record!(Weapon {
    item_damage_per_attack: VarInt,
    disable_blocking_for_seconds: JavaF32
});
record!(UseCooldown { seconds: JavaF32, cooldown_group: Option<Identifier> });
record!(AttackRange {
    min_reach: JavaF32,
    max_reach: JavaF32,
    min_creative_reach: JavaF32,
    max_creative_reach: JavaF32,
    hitbox_margin: JavaF32,
    mob_factor: JavaF32
});

/// Port of the pure-value records' generated ObjectMethods#hashCode.
impl JavaHash for FoodProperties {
    fn java_hash_code(&self) -> i32 {
        record_hash_code([
            self.nutrition.0.java_hash_code(),
            self.saturation.java_hash_code(),
            self.can_always_eat.java_hash_code(),
        ])
    }
}
impl JavaHash for UseEffects {
    fn java_hash_code(&self) -> i32 {
        record_hash_code([
            self.can_sprint.java_hash_code(),
            self.interact_vibrations.java_hash_code(),
            self.speed_multiplier.java_hash_code(),
        ])
    }
}
impl JavaHash for AttackRange {
    fn java_hash_code(&self) -> i32 {
        record_hash_code([
            self.min_reach.java_hash_code(),
            self.max_reach.java_hash_code(),
            self.min_creative_reach.java_hash_code(),
            self.max_creative_reach.java_hash_code(),
            self.hitbox_margin.java_hash_code(),
            self.mob_factor.java_hash_code(),
        ])
    }
}
record!(Consumable { consume_seconds: JavaF32, animation: ItemUseAnimation, sound: Sound, has_consume_particles: bool, on_consume_effects: Vec<ConsumeEffect> });
record!(DeathProtection { death_effects: Vec<ConsumeEffect> });
record!(Tool { rules: Vec<ToolRule>, default_mining_speed: JavaF32, damage_per_block: VarInt, can_destroy_blocks_in_creative: bool });
record!(ToolRule { blocks: BlockSet, speed: Option<JavaF32>, correct_for_drops: Option<bool> });
record!(Equippable { slot: EquipmentSlot, equip_sound: Sound, asset_id: Option<Identifier>, camera_overlay: Option<Identifier>, allowed_entities: Option<EntityTypeSet>, dispensable: bool, swappable: bool, damage_on_hurt: bool, equip_on_interact: bool, can_be_sheared: bool, shearing_sound: Sound });
record!(BlocksAttacks { block_delay_seconds: JavaF32, disable_cooldown_scale: JavaF32, damage_reductions: Vec<DamageReduction>, item_damage: ItemDamageFunction, bypassed_by: Option<DamageTypeSet>, block_sound: Option<Sound>, disable_sound: Option<Sound> });
record!(DamageReduction { horizontal_blocking_angle: JavaF32, types: Option<DamageTypeSet>, base: JavaF32, factor: JavaF32 });
record!(ItemDamageFunction {
    threshold: JavaF32,
    base: JavaF32,
    factor: JavaF32
});
record!(PiercingWeapon { deals_knockback: bool, dismounts: bool, sound: Option<Sound>, hit_sound: Option<Sound> });
record!(KineticWeapon { contact_cooldown_ticks: VarInt, delay_ticks: VarInt, dismount_conditions: Option<KineticCondition>, knockback_conditions: Option<KineticCondition>, damage_conditions: Option<KineticCondition>, forward_movement: JavaF32, damage_multiplier: JavaF32, sound: Option<Sound>, hit_sound: Option<Sound> });
record!(KineticCondition {
    max_duration_ticks: VarInt,
    min_speed: JavaF32,
    min_relative_speed: JavaF32
});
record!(SwingAnimation {
    animation_type: SwingAnimationType,
    duration: VarInt
});
record!(CustomModelData { floats: Vec<JavaF32>, flags: Vec<bool>, strings: Vec<String>, colors: Vec<i32> });
record!(FireworkExplosion { shape: FireworkShape, colors: Vec<i32>, fade_colors: Vec<i32>, has_trail: bool, has_twinkle: bool });
record!(Fireworks { flight_duration: VarInt, explosions: BoundedList<FireworkExplosion, 256> });
record!(LodestoneTracker { target: Option<GlobalPos>, tracked: bool });
record!(ArmorTrim {
    material: TrimMaterialHolder,
    pattern: TrimPatternHolder
});
record!(BannerLayer {
    pattern: BannerPatternHolder,
    color: DyeColor
});
record!(AttributeModifier {
    id: Identifier,
    amount: JavaF64,
    operation: AttributeOperation
});
record!(AttributeEntry {
    attribute: Attribute,
    modifier: AttributeModifier,
    slot: EquipmentSlotGroup,
    display: AttributeDisplay
});
record!(MobEffectInstance {
    effect: MobEffect,
    details: MobEffectDetails
});
record!(MobEffectDetails { amplifier: VarInt, duration: VarInt, ambient: bool, show_particles: bool, show_icon: bool, hidden_effect: Option<Box<MobEffectDetails>> });
record!(PotionContents { potion: Option<Potion>, custom_color: Option<i32>, custom_effects: Vec<MobEffectInstance>, custom_name: Option<String> });
record!(StewEffect {
    effect: MobEffect,
    duration: VarInt
});
record!(WrittenBookContent { title: Filterable<BoundedString<32>>, author: String, generation: VarInt, pages: Vec<Filterable<Component>>, resolved: bool });
record!(TypedEntityData {
    entity_type: EntityType,
    tag: CustomData
});
record!(TypedBlockEntityData {
    entity_type: BlockEntityType,
    tag: CustomData
});
record!(BeeOccupant {
    entity_data: TypedEntityData,
    ticks_in_hive: VarInt,
    min_ticks_in_hive: VarInt
});
record!(PotDecorations { back: Option<ItemStackTemplate>, left: Option<ItemStackTemplate>, right: Option<ItemStackTemplate>, front: Option<ItemStackTemplate> });
record!(Compostable { layers: Either<i32, Identifier> });
record!(CookingFuel { burn_time: Either<i32, Identifier>, speed_multiplier: Either<JavaF32, Identifier> });
record!(BrewingFuel { uses: Either<i32, Identifier>, speed_multiplier: Either<JavaF32, Identifier> });
record!(MobVisibility {
    targeting_entity_types: EntityTypeSet,
    visibility: JavaF32
});
registry_holder!(Sound, SoundEvent, "sound_event");
registry_holder!(TrimMaterialHolder, TrimMaterial, "trim_material");
registry_holder!(TrimPatternHolder, TrimPattern, "trim_pattern");
registry_holder!(InstrumentHolder, Instrument, "instrument");
registry_holder!(JukeboxSongHolder, JukeboxSong, "jukebox_song");
registry_holder!(BannerPatternHolder, BannerPattern, "banner_pattern");
registry_holder!(PaintingVariantHolder, PaintingVariant, "painting_variant");

/// Port of ItemEnchantments#STREAM_CODEC. Entry order is retained for forwarding.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemEnchantments(pub Vec<(Enchantment, VarInt)>);
impl Wire for ItemEnchantments {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(
            r.list(i32::MAX as usize, |r| Ok((r.read()?, r.read()?)))?,
        ))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.0, i32::MAX as usize, |w, (k, v)| {
            w.write(k)?;
            w.write(v)
        })
    }
}
/// Port of ItemAttributeModifiers.Display's tagged stream codec.
#[derive(Clone, Debug, PartialEq)]
pub enum AttributeDisplay {
    Default,
    Hidden,
    Override(Box<Component>),
}
impl Wire for AttributeDisplay {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(match r.var_int()? {
            1 => Self::Hidden,
            2 => Self::Override(r.read()?),
            _ => Self::Default,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        match self {
            Self::Default => w.var_int(0),
            Self::Hidden => w.var_int(1),
            Self::Override(v) => {
                w.var_int(2);
                w.write(v)?;
            }
        }
        Ok(())
    }
}
/// Port of ConsumeEffect's registry-dispatched codec.
#[derive(Clone, Debug, PartialEq)]
pub enum ConsumeEffect {
    ApplyEffects {
        effects: Vec<MobEffectInstance>,
        probability: JavaF32,
    },
    RemoveEffects(MobEffectSet),
    ClearAllEffects,
    TeleportRandomly {
        diameter: JavaF32,
        directional_particles: bool,
    },
    PlaySound(Sound),
}
impl Wire for ConsumeEffect {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let key = r.registry(&registry("consume_effect_type"))?;
        Ok(match key.path() {
            "apply_effects" => Self::ApplyEffects {
                effects: r.read()?,
                probability: r.read()?,
            },
            "remove_effects" => Self::RemoveEffects(r.read()?),
            "clear_all_effects" => Self::ClearAllEffects,
            "teleport_randomly" => Self::TeleportRandomly {
                diameter: r.read()?,
                directional_particles: r.read()?,
            },
            "play_sound" => Self::PlaySound(r.read()?),
            _ => return Err(invalid("consume effect")),
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        let name = match self {
            Self::ApplyEffects { .. } => "apply_effects",
            Self::RemoveEffects(_) => "remove_effects",
            Self::ClearAllEffects => "clear_all_effects",
            Self::TeleportRandomly { .. } => "teleport_randomly",
            Self::PlaySound(_) => "play_sound",
        };
        w.registry(&registry("consume_effect_type"), &registry(name))?;
        match self {
            Self::ApplyEffects {
                effects,
                probability,
            } => {
                w.write(effects)?;
                w.write(probability)
            }
            Self::RemoveEffects(v) => w.write(v),
            Self::ClearAllEffects => Ok(()),
            Self::TeleportRandomly {
                diameter,
                directional_particles,
            } => {
                w.write(diameter)?;
                w.write(directional_particles)
            }
            Self::PlaySound(v) => w.write(v),
        }
    }
}
/// Port of BlockItemStateProperties#STREAM_CODEC.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockItemStateProperties(pub Vec<(String, String)>);
impl Wire for BlockItemStateProperties {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self(
            r.list(i32::MAX as usize, |r| Ok((r.read()?, r.read()?)))?,
        ))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.list(&self.0, i32::MAX as usize, |w, (k, v)| {
            w.write(k)?;
            w.write(v)
        })
    }
}
record!(TooltipDisplay {
    hide_tooltip: bool,
    hidden_components: ComponentTypeList
});
pub type ComponentType = super::ComponentTypeId;
record!(ComponentTypeList { entries: Vec<ComponentType> });
record!(VillagerFood { nutrition: VarInt });
record!(SkinPatch { body: Option<Identifier>, cape: Option<Identifier>, elytra: Option<Identifier>, model: Option<PlayerModelType> });
/// Port of ByteBufCodecs.GAME_PROFILE_PROPERTIES, preserving signatures and order.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileProperty {
    pub name: String,
    pub value: String,
    pub signature: Option<String>,
}
impl Wire for ProfileProperty {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            name: r.string(64)?.into_owned(),
            value: r.string(32767)?.into_owned(),
            signature: r.optional(|r| Ok(r.string(1024)?.into_owned()))?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.string(&self.name, 64)?;
        w.string(&self.value, 32767)?;
        w.optional(&self.signature, |w, v| w.string(v, 1024))
    }
}
record!(GameProfile { id: Uuid, name: BoundedString<16>, properties: BoundedList<ProfileProperty, 16> });
record!(PartialProfile { name: Option<BoundedString<16>>, id: Option<Uuid>, properties: BoundedList<ProfileProperty, 16> });
record!(ResolvableProfile { profile: Either<GameProfile, PartialProfile>, skin_patch: SkinPatch });
/// Port of SignText#STREAM_CODEC; messages have a fixed count without a list prefix.
#[derive(Clone, Debug, PartialEq)]
pub struct SignText {
    pub messages: [Component; 4],
    pub filtered_messages: Option<[Component; 4]>,
    pub color: DyeColor,
    pub has_glowing_text: bool,
}
impl Wire for SignText {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let read = |r: &mut Reader<'_>| Ok([r.read()?, r.read()?, r.read()?, r.read()?]);
        Ok(Self {
            messages: read(r)?,
            filtered_messages: r.optional(read)?,
            color: r.read()?,
            has_glowing_text: r.read()?,
        })
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        for v in &self.messages {
            w.write(v)?;
        }
        w.optional(&self.filtered_messages, |w, v| {
            for c in v {
                w.write(c)?;
            }
            Ok(())
        })?;
        w.write(&self.color)?;
        w.write(&self.has_glowing_text)
    }
}
registry_ref!(BlockTransformerHolder, "block_transformer");
record!(AdventureModePredicate { predicates: Vec<BlockPredicate> });
record!(BlockPredicate { blocks: Option<BlockSet>, properties: Option<StatePropertiesPredicate>, nbt: Option<NbtPredicate>, components: DataComponentMatchers });
record!(PropertyMatcher { name: String, matcher: Either<String, PropertyRange> });
record!(PropertyRange { min: Option<String>, max: Option<String> });
record!(DataComponentMatchers { exact: Vec<super::DataComponent>, partial: BoundedList<ComponentPredicate, 64> });
/// Port of the components whose default network codec is their persistent NBT codec.
macro_rules! persistent_wire {
 ($($ty:ty),* $(,)?) => {$(impl Wire for $ty {
  fn decode(r:&mut Reader<'_>)->Result<Self> {<Self as super::Persistent>::from_nbt(&r.nbt(false)?)}
  fn encode(&self,w:&mut Writer<'_>)->Result<()> {w.nbt(&<Self as super::Persistent>::to_nbt(self)?)}
 })*};
}
#[derive(Clone, Debug, PartialEq)]
pub struct MapDecorations(pub Vec<(String, MapDecoration)>);
#[derive(Clone, Debug, PartialEq)]
pub struct MapDecoration {
    pub decoration_type: Identifier,
    pub x: JavaF64,
    pub z: JavaF64,
    pub rotation: JavaF32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DebugStickState(pub Vec<(ferraloom_data::Block, String)>);
#[derive(Clone, Debug, PartialEq)]
pub struct Recipes(pub Vec<Identifier>);
#[derive(Clone, Debug, PartialEq)]
pub struct SeededContainerLoot {
    pub loot_table: Identifier,
    pub seed: i64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LockCode {
    pub items: Option<ItemSet>,
    pub count: IntBounds,
    pub components: DataComponentMatchers,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IntBounds {
    pub min: Option<i32>,
    pub max: Option<i32>,
}
persistent_wire!(
    MapDecorations,
    DebugStickState,
    Recipes,
    SeededContainerLoot,
    LockCode
);
record!(StatePropertiesPredicate { properties:Vec<PropertyMatcher> });
/// Port of NbtPredicate: its stream value is a compound, its persistent value is SNBT.
#[derive(Clone, Debug, PartialEq)]
pub struct NbtPredicate(pub CustomData);
impl Wire for NbtPredicate {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        CustomData::decode(r).map(Self)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.0.encode(w)
    }
}

// Shared indirection codec; needed by recursive effects and rare text payloads.
impl<T: Wire> Wire for Box<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        T::decode(r).map(Box::new)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        (**self).encode(w)
    }
}
impl Wire for ferraloom_data::Item {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.var_int()?;
        u16::try_from(id)
            .ok()
            .and_then(Self::from_protocol_id)
            .ok_or_else(|| invalid("item id"))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(self.protocol_id().into());
        Ok(())
    }
}
impl Wire for super::ComponentTypeId {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let id = r.var_int()?;
        u16::try_from(id)
            .ok()
            .and_then(Self::from_protocol_id)
            .ok_or_else(|| invalid("component type id"))
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.var_int(self.protocol_id().into());
        Ok(())
    }
}

#[cfg(test)]
mod equipment_slot_group_tests {
    use super::*;

    #[test]
    fn native_slot_groups_match_all_equipment_slots() {
        let mut cases = 0;
        for row in include_str!("../../tests/java/equipment/vectors.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            let group = [fields[0].parse::<u8>().unwrap()];
            let slot = [fields[1].parse::<u8>().unwrap()];
            let group = EquipmentSlotGroup::decode(&mut Reader::new(&group)).unwrap();
            let slot = EquipmentSlot::decode(&mut Reader::new(&slot)).unwrap();
            assert_eq!(
                group.test(slot),
                fields[2].parse::<bool>().unwrap(),
                "{row}"
            );
            cases += 1;
        }
        assert_eq!(cases, 88);
    }
}
