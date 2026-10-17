use super::*;
use crate::{Identifier, Reader, Result, Wire, Writer, chat::Component, codec::VarInt};

macro_rules! components {
    ($($variant:ident($ty:ty) => $id:ident, $key:literal),* $(,)?) => {
        /// Port of TypedDataComponent, with a typed alternative for each vanilla component.
        #[derive(Clone, Debug, PartialEq)]
        pub enum DataComponent { $($variant($ty)),* }
        impl DataComponent {
            pub fn component_type(&self) -> ComponentTypeId { match self { $(Self::$variant(_) => ComponentTypeId::$id),* } }
            pub fn key(&self) -> Identifier { registry(self.component_type().name()) }
            pub fn from_json(key:&Identifier,value:&serde_json::Value)->Result<Self> { super::persistent::component_from_json(key,value) }
            pub fn from_nbt(key:&Identifier,value:&ferraloom_nbt::Tag)->Result<Self> { super::persistent::component_from_nbt(key,value) }
            pub fn encode_ops<O:crate::ops::EncodeOps>(&self,ops:&O)->Result<O::Value> {self.to_persistent()?.encode_ops(ops)}
            pub fn to_persistent(&self)->Result<PersistentValue> { super::persistent::component_value(self) }
            pub fn to_json(&self)->Result<serde_json::Value> { self.to_persistent()?.to_json() }
            pub fn to_nbt(&self)->Result<ferraloom_nbt::Tag> { self.to_persistent()?.to_nbt() }
            pub fn hash(&self)->Result<i32> { self.to_persistent()?.hash() }
            pub fn decode_value(key: &Identifier, r: &mut Reader<'_>) -> Result<Self> {
                if key.as_str() == "minecraft:intangible_projectile" { <Unit as Persistent>::from_nbt(&r.nbt(false)?)?; return Ok(Self::IntangibleProjectile(Unit)); }
                match key.as_str() { $(concat!("minecraft:", $key) => Ok(Self::$variant(r.read()?)),)* _ => Err(crate::Error::invalid("component", key.to_string())) }
            }
            pub fn encode_value(&self, w: &mut Writer<'_>) -> Result<()> { if let Self::IntangibleProjectile(v) = self { return w.nbt(&v.to_nbt()?); } match self { $(Self::$variant(v) => w.write(v)),* } }
        }
    };
}
components! {
    CustomData(CustomData) => CUSTOM_DATA, "custom_data",
    MaxStackSize(VarInt) => MAX_STACK_SIZE, "max_stack_size",
    MaxDamage(VarInt) => MAX_DAMAGE, "max_damage",
    Damage(VarInt) => DAMAGE, "damage",
    Unbreakable(Unit) => UNBREAKABLE, "unbreakable",
    UseEffects(UseEffects) => USE_EFFECTS, "use_effects",
    CustomName(Box<Component>) => CUSTOM_NAME, "custom_name",
    MinimumAttackCharge(f32) => MINIMUM_ATTACK_CHARGE, "minimum_attack_charge",
    DamageType(DamageType) => DAMAGE_TYPE, "damage_type",
    ItemName(Box<Component>) => ITEM_NAME, "item_name",
    ItemModel(Identifier) => ITEM_MODEL, "item_model",
    Lore(BoundedList<Component, 256>) => LORE, "lore",
    Rarity(Rarity) => RARITY, "rarity",
    Enchantments(ItemEnchantments) => ENCHANTMENTS, "enchantments",
    CanPlaceOn(AdventureModePredicate) => CAN_PLACE_ON, "can_place_on",
    CanBreak(AdventureModePredicate) => CAN_BREAK, "can_break",
    AttributeModifiers(Vec<AttributeEntry>) => ATTRIBUTE_MODIFIERS, "attribute_modifiers",
    CustomModelData(CustomModelData) => CUSTOM_MODEL_DATA, "custom_model_data",
    TooltipDisplay(TooltipDisplay) => TOOLTIP_DISPLAY, "tooltip_display",
    RepairCost(VarInt) => REPAIR_COST, "repair_cost",
    CreativeSlotLock(Unit) => CREATIVE_SLOT_LOCK, "creative_slot_lock",
    EnchantmentGlintOverride(bool) => ENCHANTMENT_GLINT_OVERRIDE, "enchantment_glint_override",
    IntangibleProjectile(Unit) => INTANGIBLE_PROJECTILE, "intangible_projectile",
    Food(FoodProperties) => FOOD, "food",
    Consumable(Consumable) => CONSUMABLE, "consumable",
    UseRemainder(ItemStackTemplate) => USE_REMAINDER, "use_remainder",
    UseCooldown(UseCooldown) => USE_COOLDOWN, "use_cooldown",
    DamageResistant(DamageTypeSet) => DAMAGE_RESISTANT, "damage_resistant",
    Tool(Tool) => TOOL, "tool",
    Weapon(Weapon) => WEAPON, "weapon",
    AttackRange(AttackRange) => ATTACK_RANGE, "attack_range",
    Enchantable(VarInt) => ENCHANTABLE, "enchantable",
    Equippable(Equippable) => EQUIPPABLE, "equippable",
    Repairable(ItemSet) => REPAIRABLE, "repairable",
    Glider(Unit) => GLIDER, "glider",
    TooltipStyle(Identifier) => TOOLTIP_STYLE, "tooltip_style",
    DeathProtection(DeathProtection) => DEATH_PROTECTION, "death_protection",
    BlocksAttacks(BlocksAttacks) => BLOCKS_ATTACKS, "blocks_attacks",
    PiercingWeapon(PiercingWeapon) => PIERCING_WEAPON, "piercing_weapon",
    KineticWeapon(KineticWeapon) => KINETIC_WEAPON, "kinetic_weapon",
    AttackAnimation(SwingAnimation) => ATTACK_ANIMATION, "attack_animation",
    InteractAnimation(SwingAnimation) => INTERACT_ANIMATION, "interact_animation",
    AdditionalTradeCost(VarInt) => ADDITIONAL_TRADE_COST, "additional_trade_cost",
    BlockTransformer(BlockTransformerHolder) => BLOCK_TRANSFORMER, "block_transformer",
    VillagerFood(VillagerFood) => VILLAGER_FOOD, "villager_food",
    StoredEnchantments(ItemEnchantments) => STORED_ENCHANTMENTS, "stored_enchantments",
    Dye(DyeColor) => DYE, "dye",
    DyedColor(i32) => DYED_COLOR, "dyed_color",
    MapId(VarInt) => MAP_ID, "map_id",
    MapDecorations(MapDecorations) => MAP_DECORATIONS, "map_decorations",
    MapPostProcessing(MapPostProcessing) => MAP_POST_PROCESSING, "map_post_processing",
    ChargedProjectiles(BoundedList<ItemStackTemplate, 1024>) => CHARGED_PROJECTILES, "charged_projectiles",
    BundleContents(Vec<ItemStackTemplate>) => BUNDLE_CONTENTS, "bundle_contents",
    PotionContents(PotionContents) => POTION_CONTENTS, "potion_contents",
    PotionDurationScale(f32) => POTION_DURATION_SCALE, "potion_duration_scale",
    SuspiciousStewEffects(Vec<StewEffect>) => SUSPICIOUS_STEW_EFFECTS, "suspicious_stew_effects",
    WritableBookContent(BoundedList<Filterable<BoundedString<1024>>, 100>) => WRITABLE_BOOK_CONTENT, "writable_book_content",
    WrittenBookContent(WrittenBookContent) => WRITTEN_BOOK_CONTENT, "written_book_content",
    Trim(ArmorTrim) => TRIM, "trim",
    DebugStickState(DebugStickState) => DEBUG_STICK_STATE, "debug_stick_state",
    EntityData(TypedEntityData) => ENTITY_DATA, "entity_data",
    BucketEntityData(CustomData) => BUCKET_ENTITY_DATA, "bucket_entity_data",
    BlockEntityData(TypedBlockEntityData) => BLOCK_ENTITY_DATA, "block_entity_data",
    Instrument(InstrumentHolder) => INSTRUMENT, "instrument",
    ProvidesTrimMaterial(TrimMaterialHolder) => PROVIDES_TRIM_MATERIAL, "provides_trim_material",
    OminousBottleAmplifier(VarInt) => OMINOUS_BOTTLE_AMPLIFIER, "ominous_bottle_amplifier",
    JukeboxPlayable(JukeboxSongHolder) => JUKEBOX_PLAYABLE, "jukebox_playable",
    ProvidesBannerPatterns(BannerPatternSet) => PROVIDES_BANNER_PATTERNS, "provides_banner_patterns",
    Recipes(Recipes) => RECIPES, "recipes",
    LodestoneTracker(LodestoneTracker) => LODESTONE_TRACKER, "lodestone_tracker",
    FireworkExplosion(FireworkExplosion) => FIREWORK_EXPLOSION, "firework_explosion",
    Fireworks(Fireworks) => FIREWORKS, "fireworks",
    Profile(ResolvableProfile) => PROFILE, "profile",
    NoteBlockSound(Identifier) => NOTE_BLOCK_SOUND, "note_block_sound",
    BannerPatterns(Vec<BannerLayer>) => BANNER_PATTERNS, "banner_patterns",
    BaseColor(DyeColor) => BASE_COLOR, "base_color",
    PotDecorations(PotDecorations) => POT_DECORATIONS, "pot_decorations",
    Container(BoundedList<Option<ItemStackTemplate>, 256>) => CONTAINER, "container",
    BlockState(BlockItemStateProperties) => BLOCK_STATE, "block_state",
    Bees(Vec<BeeOccupant>) => BEES, "bees",
    SulfurCubeContent(ItemStackTemplate) => SULFUR_CUBE_CONTENT, "sulfur_cube_content",
    Lock(LockCode) => LOCK, "lock",
    ContainerLoot(SeededContainerLoot) => CONTAINER_LOOT, "container_loot",
    BreakSound(Sound) => BREAK_SOUND, "break_sound",
    Compostable(Compostable) => COMPOSTABLE, "compostable",
    CookingFuel(CookingFuel) => COOKING_FUEL, "cooking_fuel",
    BrewingFuel(BrewingFuel) => BREWING_FUEL, "brewing_fuel",
    MobVisibility(MobVisibility) => MOB_VISIBILITY, "mob_visibility",
    VillagerVariant(VillagerType) => VILLAGER_VARIANT, "villager/variant",
    WolfVariant(WolfVariant) => WOLF_VARIANT, "wolf/variant",
    WolfSoundVariant(WolfSoundVariant) => WOLF_SOUND_VARIANT, "wolf/sound_variant",
    WolfCollar(DyeColor) => WOLF_COLLAR, "wolf/collar",
    FoxVariant(FoxVariant) => FOX_VARIANT, "fox/variant",
    SalmonSize(SalmonVariant) => SALMON_SIZE, "salmon/size",
    ParrotVariant(ParrotVariant) => PARROT_VARIANT, "parrot/variant",
    TropicalFishPattern(TropicalFishPattern) => TROPICAL_FISH_PATTERN, "tropical_fish/pattern",
    TropicalFishBaseColor(DyeColor) => TROPICAL_FISH_BASE_COLOR, "tropical_fish/base_color",
    TropicalFishPatternColor(DyeColor) => TROPICAL_FISH_PATTERN_COLOR, "tropical_fish/pattern_color",
    MooshroomVariant(MooshroomVariant) => MOOSHROOM_VARIANT, "mooshroom/variant",
    RabbitVariant(RabbitVariant) => RABBIT_VARIANT, "rabbit/variant",
    PigVariant(PigVariant) => PIG_VARIANT, "pig/variant",
    PigSoundVariant(PigSoundVariant) => PIG_SOUND_VARIANT, "pig/sound_variant",
    CowVariant(CowVariant) => COW_VARIANT, "cow/variant",
    CowSoundVariant(CowSoundVariant) => COW_SOUND_VARIANT, "cow/sound_variant",
    ChickenVariant(ChickenVariant) => CHICKEN_VARIANT, "chicken/variant",
    ChickenSoundVariant(ChickenSoundVariant) => CHICKEN_SOUND_VARIANT, "chicken/sound_variant",
    ZombieNautilusVariant(ZombieNautilusVariant) => ZOMBIE_NAUTILUS_VARIANT, "zombie_nautilus/variant",
    FrogVariant(FrogVariant) => FROG_VARIANT, "frog/variant",
    HorseVariant(HorseVariant) => HORSE_VARIANT, "horse/variant",
    PaintingVariant(PaintingVariantHolder) => PAINTING_VARIANT, "painting/variant",
    LlamaVariant(LlamaVariant) => LLAMA_VARIANT, "llama/variant",
    AxolotlVariant(AxolotlVariant) => AXOLOTL_VARIANT, "axolotl/variant",
    CatVariant(CatVariant) => CAT_VARIANT, "cat/variant",
    CatSoundVariant(CatSoundVariant) => CAT_SOUND_VARIANT, "cat/sound_variant",
    CatCollar(DyeColor) => CAT_COLLAR, "cat/collar",
    SheepColor(DyeColor) => SHEEP_COLOR, "sheep/color",
    ShulkerColor(DyeColor) => SHULKER_COLOR, "shulker/color",
    ProvidesPotteryPattern(DecoratedPotPattern) => PROVIDES_POTTERY_PATTERN, "provides_pottery_pattern",
    SignTextFront(Box<SignText>) => SIGN_TEXT_FRONT, "sign_text_front",
    SignTextBack(Box<SignText>) => SIGN_TEXT_BACK, "sign_text_back",
    Waxed(Unit) => WAXED, "waxed",
    CushionColor(DyeColor) => CUSHION_COLOR, "cushion/color",
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
                DataComponent::decode_value(&key, &mut value_reader)?
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
impl DataComponentPatch {
    /// Port of PatchedDataComponentMap#fromPatch: discard redundant sets and absent removals.
    pub fn sanitize(&mut self, prototype: &DataComponentMap) {
        self.added
            .retain(|v| prototype.get(v.component_type()) != Some(v));
        self.removed.retain(|id| prototype.get(*id).is_some());
    }
}
impl DataComponent {
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::CreativeSlotLock(_) | Self::AdditionalTradeCost(_) | Self::MapPostProcessing(_)
        )
    }
}
