use super::*;
use crate::{Identifier, Reader, Result, Wire, Writer, chat::Component, codec::VarInt};

macro_rules! components {
    ($($variant:ident($ty:ty) => $key:literal),* $(,)?) => {
        /// Port of TypedDataComponent, with a typed alternative for each vanilla component.
        #[derive(Clone, Debug, PartialEq)]
        pub enum DataComponent { $($variant($ty)),* }
        impl DataComponent {
            pub fn component_type(&self) -> ComponentTypeId { match self { $(Self::$variant(_) => ComponentTypeId::from_name(concat!("minecraft:", $key)).expect("registered component")),* } }
            pub fn key(&self) -> Identifier { registry(self.component_type().name()) }
            pub fn from_json(key:&Identifier,value:&serde_json::Value)->Result<Self> { super::persistent::component_from_json(key,value) }
            pub fn from_nbt(key:&Identifier,value:&ferraloom_nbt::Tag)->Result<Self> { super::persistent::component_from_nbt(key,value) }
            pub fn to_persistent(&self)->Result<PersistentValue> { super::persistent::component_value(self) }
            pub fn to_json(&self)->Result<serde_json::Value> { Ok(self.to_persistent()?.to_json()) }
            pub fn to_nbt(&self)->Result<ferraloom_nbt::Tag> { Ok(self.to_persistent()?.to_nbt()) }
            pub fn hash(&self)->Result<i32> { Ok(self.to_persistent()?.hash()) }
            pub fn decode_value(key: &Identifier, r: &mut Reader<'_>) -> Result<Self> {
                if key.as_str() == "minecraft:intangible_projectile" { <Unit as Persistent>::from_nbt(&r.nbt(false)?)?; return Ok(Self::IntangibleProjectile(Unit)); }
                match key.as_str() { $(concat!("minecraft:", $key) => Ok(Self::$variant(r.read()?)),)* _ => Err(crate::Error::invalid("component", key.to_string())) }
            }
            pub fn encode_value(&self, w: &mut Writer<'_>) -> Result<()> { if let Self::IntangibleProjectile(v) = self { return w.nbt(&v.to_nbt()?); } match self { $(Self::$variant(v) => w.write(v)),* } }
        }
    };
}
components! {
    CustomData(CustomData) => "custom_data",
    MaxStackSize(VarInt) => "max_stack_size",
    MaxDamage(VarInt) => "max_damage",
    Damage(VarInt) => "damage",
    Unbreakable(Unit) => "unbreakable",
    UseEffects(UseEffects) => "use_effects",
    CustomName(Component) => "custom_name",
    MinimumAttackCharge(f32) => "minimum_attack_charge",
    DamageType(DamageType) => "damage_type",
    ItemName(Component) => "item_name",
    ItemModel(Identifier) => "item_model",
    Lore(BoundedList<Component, 256>) => "lore",
    Rarity(Rarity) => "rarity",
    Enchantments(ItemEnchantments) => "enchantments",
    CanPlaceOn(AdventureModePredicate) => "can_place_on",
    CanBreak(AdventureModePredicate) => "can_break",
    AttributeModifiers(Vec<AttributeEntry>) => "attribute_modifiers",
    CustomModelData(CustomModelData) => "custom_model_data",
    TooltipDisplay(TooltipDisplay) => "tooltip_display",
    RepairCost(VarInt) => "repair_cost",
    CreativeSlotLock(Unit) => "creative_slot_lock",
    EnchantmentGlintOverride(bool) => "enchantment_glint_override",
    IntangibleProjectile(Unit) => "intangible_projectile",
    Food(FoodProperties) => "food",
    Consumable(Consumable) => "consumable",
    UseRemainder(ItemStackTemplate) => "use_remainder",
    UseCooldown(UseCooldown) => "use_cooldown",
    DamageResistant(DamageTypeSet) => "damage_resistant",
    Tool(Tool) => "tool",
    Weapon(Weapon) => "weapon",
    AttackRange(AttackRange) => "attack_range",
    Enchantable(VarInt) => "enchantable",
    Equippable(Equippable) => "equippable",
    Repairable(ItemSet) => "repairable",
    Glider(Unit) => "glider",
    TooltipStyle(Identifier) => "tooltip_style",
    DeathProtection(DeathProtection) => "death_protection",
    BlocksAttacks(BlocksAttacks) => "blocks_attacks",
    PiercingWeapon(PiercingWeapon) => "piercing_weapon",
    KineticWeapon(KineticWeapon) => "kinetic_weapon",
    AttackAnimation(SwingAnimation) => "attack_animation",
    InteractAnimation(SwingAnimation) => "interact_animation",
    AdditionalTradeCost(VarInt) => "additional_trade_cost",
    BlockTransformer(BlockTransformerHolder) => "block_transformer",
    VillagerFood(VillagerFood) => "villager_food",
    StoredEnchantments(ItemEnchantments) => "stored_enchantments",
    Dye(DyeColor) => "dye",
    DyedColor(i32) => "dyed_color",
    MapId(VarInt) => "map_id",
    MapDecorations(MapDecorations) => "map_decorations",
    MapPostProcessing(MapPostProcessing) => "map_post_processing",
    ChargedProjectiles(BoundedList<ItemStackTemplate, 1024>) => "charged_projectiles",
    BundleContents(Vec<ItemStackTemplate>) => "bundle_contents",
    PotionContents(PotionContents) => "potion_contents",
    PotionDurationScale(f32) => "potion_duration_scale",
    SuspiciousStewEffects(Vec<StewEffect>) => "suspicious_stew_effects",
    WritableBookContent(BoundedList<Filterable<BoundedString<1024>>, 100>) => "writable_book_content",
    WrittenBookContent(WrittenBookContent) => "written_book_content",
    Trim(ArmorTrim) => "trim",
    DebugStickState(DebugStickState) => "debug_stick_state",
    EntityData(TypedEntityData) => "entity_data",
    BucketEntityData(CustomData) => "bucket_entity_data",
    BlockEntityData(TypedBlockEntityData) => "block_entity_data",
    Instrument(InstrumentHolder) => "instrument",
    ProvidesTrimMaterial(TrimMaterialHolder) => "provides_trim_material",
    OminousBottleAmplifier(VarInt) => "ominous_bottle_amplifier",
    JukeboxPlayable(JukeboxSongHolder) => "jukebox_playable",
    ProvidesBannerPatterns(BannerPatternSet) => "provides_banner_patterns",
    Recipes(Recipes) => "recipes",
    LodestoneTracker(LodestoneTracker) => "lodestone_tracker",
    FireworkExplosion(FireworkExplosion) => "firework_explosion",
    Fireworks(Fireworks) => "fireworks",
    Profile(ResolvableProfile) => "profile",
    NoteBlockSound(Identifier) => "note_block_sound",
    BannerPatterns(Vec<BannerLayer>) => "banner_patterns",
    BaseColor(DyeColor) => "base_color",
    PotDecorations(PotDecorations) => "pot_decorations",
    Container(BoundedList<Option<ItemStackTemplate>, 256>) => "container",
    BlockState(BlockItemStateProperties) => "block_state",
    Bees(Vec<BeeOccupant>) => "bees",
    SulfurCubeContent(ItemStackTemplate) => "sulfur_cube_content",
    Lock(LockCode) => "lock",
    ContainerLoot(SeededContainerLoot) => "container_loot",
    BreakSound(Sound) => "break_sound",
    Compostable(Compostable) => "compostable",
    CookingFuel(CookingFuel) => "cooking_fuel",
    BrewingFuel(BrewingFuel) => "brewing_fuel",
    MobVisibility(MobVisibility) => "mob_visibility",
    VillagerVariant(VillagerType) => "villager/variant",
    WolfVariant(WolfVariant) => "wolf/variant",
    WolfSoundVariant(WolfSoundVariant) => "wolf/sound_variant",
    WolfCollar(DyeColor) => "wolf/collar",
    FoxVariant(FoxVariant) => "fox/variant",
    SalmonSize(SalmonVariant) => "salmon/size",
    ParrotVariant(ParrotVariant) => "parrot/variant",
    TropicalFishPattern(TropicalFishPattern) => "tropical_fish/pattern",
    TropicalFishBaseColor(DyeColor) => "tropical_fish/base_color",
    TropicalFishPatternColor(DyeColor) => "tropical_fish/pattern_color",
    MooshroomVariant(MooshroomVariant) => "mooshroom/variant",
    RabbitVariant(RabbitVariant) => "rabbit/variant",
    PigVariant(PigVariant) => "pig/variant",
    PigSoundVariant(PigSoundVariant) => "pig/sound_variant",
    CowVariant(CowVariant) => "cow/variant",
    CowSoundVariant(CowSoundVariant) => "cow/sound_variant",
    ChickenVariant(ChickenVariant) => "chicken/variant",
    ChickenSoundVariant(ChickenSoundVariant) => "chicken/sound_variant",
    ZombieNautilusVariant(ZombieNautilusVariant) => "zombie_nautilus/variant",
    FrogVariant(FrogVariant) => "frog/variant",
    HorseVariant(HorseVariant) => "horse/variant",
    PaintingVariant(PaintingVariantHolder) => "painting/variant",
    LlamaVariant(LlamaVariant) => "llama/variant",
    AxolotlVariant(AxolotlVariant) => "axolotl/variant",
    CatVariant(CatVariant) => "cat/variant",
    CatSoundVariant(CatSoundVariant) => "cat/sound_variant",
    CatCollar(DyeColor) => "cat/collar",
    SheepColor(DyeColor) => "sheep/color",
    ShulkerColor(DyeColor) => "shulker/color",
    ProvidesPotteryPattern(DecoratedPotPattern) => "provides_pottery_pattern",
    SignTextFront(SignText) => "sign_text_front",
    SignTextBack(SignText) => "sign_text_back",
    Waxed(Unit) => "waxed",
    CushionColor(DyeColor) => "cushion/color",
}

/// Port of DataComponentPatch. Order is preserved within additions and removals.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataComponentPatch {
    pub added: Vec<DataComponent>,
    pub removed: Vec<ComponentTypeId>,
}
impl DataComponentPatch {
    pub fn decode_delimited(r: &mut Reader<'_>) -> Result<Self> {
        Self::decode_with(r, true)
    }
    pub fn encode_delimited(&self, w: &mut Writer<'_>) -> Result<()> {
        self.encode_with(w, true)
    }
    fn decode_with(r: &mut Reader<'_>, delimited: bool) -> Result<Self> {
        let positive = r.count(i32::MAX as usize)?;
        let negative = r.count(i32::MAX as usize)?;
        let mut patch = Self::default();
        for _ in 0..positive {
            let component_type: ComponentTypeId = r.read()?;
            let key = registry(component_type.name());
            let value = if delimited {
                let bytes = r.byte_array(i32::MAX as usize)?;
                let mut value_reader = Reader::with_context(bytes, r.context()?);
                let value = DataComponent::decode_value(&key, &mut value_reader)?;
                value_reader.finish()?;
                value
            } else {
                DataComponent::decode_value(&key, r)?
            };
            patch.set(value);
        }
        for _ in 0..negative {
            patch.remove(r.read()?);
        }
        Ok(patch)
    }
    fn encode_with(&self, w: &mut Writer<'_>, delimited: bool) -> Result<()> {
        w.count(self.added.len(), i32::MAX as usize)?;
        w.count(self.removed.len(), i32::MAX as usize)?;
        for value in &self.added {
            w.write(&value.component_type())?;
            if delimited {
                let mut value_writer = Writer::with_context(w.context()?);
                value.encode_value(&mut value_writer)?;
                w.byte_array(value_writer.as_slice(), i32::MAX as usize)?;
            } else {
                value.encode_value(w)?;
            }
        }
        for key in &self.removed {
            w.write(key)?;
        }
        Ok(())
    }
    pub fn set(&mut self, value: DataComponent) {
        let key = value.component_type();
        self.removed.retain(|v| v != &key);
        if let Some(old) = self.added.iter_mut().find(|v| v.component_type() == key) {
            *old = value;
        } else {
            self.added.push(value);
        }
    }
    pub fn remove(&mut self, key: ComponentTypeId) {
        self.added.retain(|v| v.component_type() != key);
        if !self.removed.contains(&key) {
            self.removed.push(key);
        }
    }
}
impl Wire for DataComponentPatch {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Self::decode_with(r, false)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        self.encode_with(w, false)
    }
}
impl Wire for DataComponent {
    fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let component_type: ComponentTypeId = r.read()?;
        let key = registry(component_type.name());
        Self::decode_value(&key, r)
    }
    fn encode(&self, w: &mut Writer<'_>) -> Result<()> {
        w.write(&self.component_type())?;
        self.encode_value(w)
    }
}
