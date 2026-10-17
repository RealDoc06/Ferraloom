import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.*;
import net.minecraft.network.codec.*;
import net.minecraft.network.syncher.*;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.HashOps;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.*;
import net.minecraft.world.item.enchantment.*;

/** Generates independent codec fixtures against the official, unobfuscated 26.3 jar. */
public final class ComponentFixtures {
    static RegistryAccess.Frozen access;
    static HolderLookup.Provider lookup;
    static JsonObject fixtures = new JsonObject();
    static JsonArray components = new JsonArray();
    static final Gson GSON = new GsonBuilder().setPrettyPrinting().create();
    static <T> String bytes(StreamCodec<? super RegistryFriendlyByteBuf,T> codec, T value) {
        var buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
        try {
            codec.encode(buf,value);
            byte[] bytes = new byte[buf.readableBytes()]; buf.readBytes(bytes);
            return HexFormat.of().formatHex(bytes);
        } finally { buf.release(); }
    }
    static <T> void component(String name, TypedDataComponent<T> entry) {
        var out = new JsonObject(); out.addProperty("name",name);
        out.addProperty("type", BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(entry.type()).toString());
        out.addProperty("bytes", bytes(entry.type().streamCodec(),entry.value()));
        if (!entry.type().isTransient()) {
            var persistent = entry.encodeValue(lookup.createSerializationContext(JsonOps.INSTANCE)).result();
            if (persistent.isPresent()) {
                out.add("value",persistent.get());
                out.addProperty("hash",entry.encodeValue(lookup.createSerializationContext(HashOps.CRC32C_INSTANCE)).getOrThrow().asInt());
            }
        }
        components.add(out);
    }
    static void stack(String name, ItemStack value) {
        fixtures.addProperty(name,bytes(ItemStack.OPTIONAL_STREAM_CODEC,value));
        fixtures.addProperty(name+"_untrusted",bytes(ItemStack.OPTIONAL_UNTRUSTED_STREAM_CODEC,value));
        HashedPatchMap.HashGenerator hasher = c -> c.encodeValue(lookup.createSerializationContext(HashOps.CRC32C_INSTANCE)).getOrThrow().asInt();
        fixtures.addProperty(name+"_hashed",bytes(HashedStack.STREAM_CODEC,HashedStack.create(value,hasher)));
    }
    @SuppressWarnings({"unchecked", "rawtypes"})
    static void sample(String key,String json) {
        DataComponentType type = BuiltInRegistries.DATA_COMPONENT_TYPE.getValue(net.minecraft.resources.Identifier.parse(key));
        Object value = type.codecOrThrow().parse(lookup.createSerializationContext(JsonOps.INSTANCE),JsonParser.parseString(json)).getOrThrow();
        component("sample",new TypedDataComponent(type,value));
    }
    static <T> String first(net.minecraft.resources.ResourceKey<? extends Registry<? extends T>> key) {
        return lookup.lookupOrThrow(key).listElementIds().map(k -> k.identifier().toString()).sorted().findFirst().orElseThrow();
    }
    @SuppressWarnings({"unchecked", "rawtypes"})
    static void metadata(List<SynchedEntityData.DataValue<?>> values,String name,Object value) throws Exception {
        EntityDataSerializer serializer=(EntityDataSerializer)EntityDataSerializers.class.getField(name).get(null);
        values.add(new SynchedEntityData.DataValue(values.size(),serializer,value));
    }
    static <T> Holder.Reference<T> holder(net.minecraft.resources.ResourceKey<? extends Registry<? extends T>> key) {
        return lookup.lookupOrThrow(key).listElements().sorted(Comparator.comparing(e -> e.key().identifier().toString())).findFirst().orElseThrow();
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        lookup = net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p -> p.apply());
        access = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var all = new ArrayList<Registry<?>>();
        access.registries().forEach(r -> all.add(r.value()));
        lookup.listRegistries().forEach(r -> {
            if (access.lookup(r.key()).isPresent() || !Set.of("enchantment","damage_type","trim_material","trim_pattern","jukebox_song","instrument","banner_pattern","wolf_variant","wolf_sound_variant","pig_variant","pig_sound_variant","cow_variant","cow_sound_variant","chicken_variant","chicken_sound_variant","cat_variant","cat_sound_variant","frog_variant","zombie_nautilus_variant","painting_variant","decorated_pot_pattern","block_transformer").contains(r.key().identifier().getPath())) return;
            var registry = new MappedRegistry(r.key(),com.mojang.serialization.Lifecycle.stable());
            r.listElements().sorted(Comparator.comparing(e -> e.key().identifier().toString())).forEach(e -> Registry.register(registry,e.key().identifier(),e.value()));
            all.add(registry.freeze());
        });
        access = new RegistryAccess.ImmutableRegistryAccess(all).freeze();
        var registries = new JsonObject();
        access.registries().forEach(r -> {
            var names = new JsonArray(); for(var value:r.value()) names.add(((Registry)r.value()).getKey(value).toString());
            registries.add(r.key().identifier().toString(),names);
        });
        fixtures.add("registries",registries);
        var seen = new HashSet<DataComponentType<?>>();
        for (var item : BuiltInRegistries.ITEM) for (var c : item.components()) {
            if(seen.add(c.type())) component(BuiltInRegistries.ITEM.getKey(item).toString(),c);
        }
        var custom = new CompoundTag(); custom.putInt("answer",42); custom.putString("unicode","hello 🌍");
        component("custom_data",new TypedDataComponent<>(DataComponents.CUSTOM_DATA,CustomData.of(custom)));
        var sword = new ItemStack(Items.DIAMOND_SWORD,1);
        sword.set(DataComponents.DAMAGE,7); sword.set(DataComponents.CUSTOM_DATA,CustomData.of(custom));
        sword.remove(DataComponents.ITEM_NAME);
        var enchants = new ItemEnchantments.Mutable(ItemEnchantments.EMPTY);
        enchants.set(lookup.lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(Enchantments.SHARPNESS),3);
        sword.set(DataComponents.ENCHANTMENTS,enchants.toImmutable());
        stack("sword",sword); stack("empty",ItemStack.EMPTY); stack("stone",new ItemStack(Items.STONE,64));
        component("enchantments",new TypedDataComponent<>(DataComponents.ENCHANTMENTS,enchants.toImmutable()));
        var nested = new ItemStack(Items.CHEST);
        nested.set(DataComponents.CONTAINER,ItemContainerContents.fromItems(List.of(sword,ItemStack.EMPTY,new ItemStack(Items.APPLE,2))));
        stack("nested",nested);
        var profile = ResolvableProfile.createResolved(new com.mojang.authlib.GameProfile(UUID.fromString("01234567-89ab-cdef-fedc-ba9876543210"),"Doc"));
        component("profile",new TypedDataComponent<>(DataComponents.PROFILE,profile));
        sample("unbreakable","{}");
        sample("custom_name","{\"text\":\"Doc\",\"bold\":true,\"color\":\"gold\"}");
        sample("can_place_on","{\"blocks\":\"minecraft:stone\",\"state\":{\"axis\":{\"min\":\"x\",\"max\":\"z\"}}}");
        sample("can_break","{\"blocks\":\"#minecraft:mineable/pickaxe\"}");
        sample("custom_model_data","{\"floats\":[0.5,-1.0],\"flags\":[true,false],\"strings\":[\"alpha\"],\"colors\":[16711680]}");
        sample("intangible_projectile","{}");
        sample("tooltip_style","\"minecraft:test\"");
        sample("dyed_color","123456");
        sample("map_id","42");
        sample("written_book_content","{\"title\":{\"raw\":\"Title\",\"filtered\":\"Clean\"},\"author\":\"Doc\",\"generation\":2,\"pages\":[{\"raw\":{\"text\":\"A\"},\"filtered\":{\"text\":\"B\",\"italic\":true}}],\"resolved\":true}");
        sample("trim","{\"material\":\"minecraft:quartz\",\"pattern\":\"minecraft:sentry\"}");
        sample("block_entity_data","{\"id\":\"minecraft:chest\",\"value\":42}");
        sample("lodestone_tracker","{\"target\":{\"dimension\":\"minecraft:overworld\",\"pos\":[1,64,-2]},\"tracked\":false}");
        sample("firework_explosion","{\"shape\":\"star\",\"colors\":[16711680,255],\"fade_colors\":[65280],\"has_trail\":true,\"has_twinkle\":true}");
        sample("note_block_sound","\"minecraft:test\"");
        sample("base_color","\"blue\"");
        sample("sulfur_cube_content","{\"id\":\"minecraft:stone\",\"count\":2}");
        sample("lock","{\"count\":{\"min\":1,\"max\":4},\"items\":[\"minecraft:stone\",\"minecraft:apple\"]}");
        sample("container_loot","{\"loot_table\":\"minecraft:chests/simple_dungeon\",\"seed\":123456789}");
        sample("wolf/collar","\"red\"");
        sample("fox/variant","\"snow\"");
        sample("salmon/size","\"large\"");
        sample("parrot/variant","\"blue\"");
        sample("tropical_fish/pattern","\"clayfish\"");
        sample("tropical_fish/base_color","\"green\"");
        sample("tropical_fish/pattern_color","\"yellow\"");
        sample("mooshroom/variant","\"brown\"");
        sample("rabbit/variant","\"evil\"");
        sample("horse/variant","\"dark_brown\"");
        sample("painting/variant",GSON.toJson(first(Registries.PAINTING_VARIANT)));
        sample("llama/variant","\"gray\"");
        sample("axolotl/variant","\"blue\"");
        sample("cat/collar","\"orange\"");
        sample("sheep/color","\"black\"");
        sample("shulker/color","\"purple\"");
        sample("waxed","{}");
        sample("villager/variant",GSON.toJson(first(Registries.VILLAGER_TYPE)));
        sample("wolf/variant",GSON.toJson(first(Registries.WOLF_VARIANT)));
        sample("wolf/sound_variant",GSON.toJson(first(Registries.WOLF_SOUND_VARIANT)));
        sample("pig/variant",GSON.toJson(first(Registries.PIG_VARIANT)));
        sample("pig/sound_variant",GSON.toJson(first(Registries.PIG_SOUND_VARIANT)));
        sample("cow/variant",GSON.toJson(first(Registries.COW_VARIANT)));
        sample("cow/sound_variant",GSON.toJson(first(Registries.COW_SOUND_VARIANT)));
        sample("chicken/sound_variant",GSON.toJson(first(Registries.CHICKEN_SOUND_VARIANT)));
        sample("zombie_nautilus/variant",GSON.toJson(first(Registries.ZOMBIE_NAUTILUS_VARIANT)));
        sample("frog/variant",GSON.toJson(first(Registries.FROG_VARIANT)));
        sample("cat/variant",GSON.toJson(first(Registries.CAT_VARIANT)));
        sample("cat/sound_variant",GSON.toJson(first(Registries.CAT_SOUND_VARIANT)));
        component("transient",new TypedDataComponent<>(DataComponents.CREATIVE_SLOT_LOCK,net.minecraft.util.Unit.INSTANCE));
        component("transient",new TypedDataComponent<>(DataComponents.ADDITIONAL_TRADE_COST,12));
        component("transient",new TypedDataComponent<>(DataComponents.MAP_POST_PROCESSING,MapPostProcessing.SCALE));
        sample("consumable","{\"on_consume_effects\":[{\"type\":\"minecraft:teleport_randomly\",\"diameter\":24.0,\"directional_particles\":false},{\"type\":\"minecraft:remove_effects\",\"effects\":[\"minecraft:poison\"]},{\"type\":\"minecraft:play_sound\",\"sound\":\"minecraft:item.shield.block\"}]}");
        component("inline_painting",new TypedDataComponent<>(DataComponents.PAINTING_VARIANT,Holder.direct(new net.minecraft.world.entity.decoration.painting.PaintingVariant(2,1,net.minecraft.resources.Identifier.parse("minecraft:test"),Optional.of(net.minecraft.network.chat.Component.literal("Title")),Optional.empty()))));
        fixtures.add("components",components);
        var serializers = new JsonObject();
        for (var field:EntityDataSerializers.class.getFields()) if(field.getType()==EntityDataSerializer.class) serializers.addProperty(field.getName(),EntityDataSerializers.getSerializedId((EntityDataSerializer<?>)field.get(null)));
        fixtures.add("serializers",serializers);
        var values = List.<SynchedEntityData.DataValue<?>>of(
            new SynchedEntityData.DataValue<>(0,EntityDataSerializers.BYTE,(byte)-1),
            new SynchedEntityData.DataValue<>(1,EntityDataSerializers.INT,300),
            new SynchedEntityData.DataValue<>(2,EntityDataSerializers.LONG,-1L),
            new SynchedEntityData.DataValue<>(3,EntityDataSerializers.FLOAT,1.25f),
            new SynchedEntityData.DataValue<>(4,EntityDataSerializers.STRING,"hello"),
            new SynchedEntityData.DataValue<>(5,EntityDataSerializers.BOOLEAN,true),
            new SynchedEntityData.DataValue<>(6,EntityDataSerializers.OPTIONAL_UNSIGNED_INT,OptionalInt.of(300)),
            new SynchedEntityData.DataValue<>(7,EntityDataSerializers.OPTIONAL_GLOBAL_POS,Optional.of(GlobalPos.of(net.minecraft.world.level.Level.OVERWORLD,new BlockPos(1,64,-2)))),
            new SynchedEntityData.DataValue<>(8,EntityDataSerializers.ITEM_STACK,sword));
        fixtures.addProperty("entity_data",bytes(net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket.STREAM_CODEC,new net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket(123,values)));
        var allMetadata = new ArrayList<SynchedEntityData.DataValue<?>>();
        metadata(allMetadata,"BYTE",(byte)-20);
        metadata(allMetadata,"INT",-300);
        metadata(allMetadata,"LONG",12345678901234L);
        metadata(allMetadata,"FLOAT",1.25f);
        metadata(allMetadata,"STRING","hi 🌍");
        metadata(allMetadata,"COMPONENT",net.minecraft.network.chat.Component.literal("hello").withStyle(net.minecraft.ChatFormatting.BOLD));
        metadata(allMetadata,"OPTIONAL_COMPONENT",Optional.of(net.minecraft.network.chat.Component.literal("optional")));
        metadata(allMetadata,"ITEM_STACK",sword);
        metadata(allMetadata,"BOOLEAN",true);
        metadata(allMetadata,"ROTATIONS",new Rotations(400,-370,Float.NaN));
        metadata(allMetadata,"BLOCK_POS",new BlockPos(10,64,-20));
        metadata(allMetadata,"OPTIONAL_BLOCK_POS",Optional.of(new BlockPos(-1,0,1)));
        metadata(allMetadata,"DIRECTION",Direction.WEST);
        metadata(allMetadata,"OPTIONAL_LIVING_ENTITY_REFERENCE",Optional.of(net.minecraft.world.entity.EntityReference.of(UUID.fromString("01234567-89ab-cdef-fedc-ba9876543210"))));
        metadata(allMetadata,"BLOCK_STATE",net.minecraft.world.level.block.Blocks.STONE.defaultBlockState());
        metadata(allMetadata,"OPTIONAL_BLOCK_STATE",Optional.of(net.minecraft.world.level.block.Blocks.STONE.defaultBlockState()));
        metadata(allMetadata,"PARTICLE",net.minecraft.core.particles.ParticleTypes.FLAME);
        metadata(allMetadata,"PARTICLES",List.of(net.minecraft.core.particles.ParticleTypes.FLAME,net.minecraft.core.particles.ParticleTypes.CLOUD));
        metadata(allMetadata,"VILLAGER_DATA",new net.minecraft.world.entity.npc.villager.VillagerData(holder(Registries.VILLAGER_TYPE),holder(Registries.VILLAGER_PROFESSION),4));
        metadata(allMetadata,"OPTIONAL_UNSIGNED_INT",OptionalInt.empty());
        metadata(allMetadata,"POSE",net.minecraft.world.entity.Pose.SWIMMING);
        metadata(allMetadata,"CAT_VARIANT",holder(Registries.CAT_VARIANT));
        metadata(allMetadata,"CAT_SOUND_VARIANT",holder(Registries.CAT_SOUND_VARIANT));
        metadata(allMetadata,"COW_VARIANT",holder(Registries.COW_VARIANT));
        metadata(allMetadata,"COW_SOUND_VARIANT",holder(Registries.COW_SOUND_VARIANT));
        metadata(allMetadata,"WOLF_VARIANT",holder(Registries.WOLF_VARIANT));
        metadata(allMetadata,"WOLF_SOUND_VARIANT",holder(Registries.WOLF_SOUND_VARIANT));
        metadata(allMetadata,"FROG_VARIANT",holder(Registries.FROG_VARIANT));
        metadata(allMetadata,"PIG_VARIANT",holder(Registries.PIG_VARIANT));
        metadata(allMetadata,"PIG_SOUND_VARIANT",holder(Registries.PIG_SOUND_VARIANT));
        metadata(allMetadata,"CHICKEN_VARIANT",holder(Registries.CHICKEN_VARIANT));
        metadata(allMetadata,"CHICKEN_SOUND_VARIANT",holder(Registries.CHICKEN_SOUND_VARIANT));
        metadata(allMetadata,"ZOMBIE_NAUTILUS_VARIANT",holder(Registries.ZOMBIE_NAUTILUS_VARIANT));
        metadata(allMetadata,"OPTIONAL_GLOBAL_POS",Optional.empty());
        metadata(allMetadata,"PAINTING_VARIANT",holder(Registries.PAINTING_VARIANT));
        metadata(allMetadata,"SNIFFER_STATE",net.minecraft.world.entity.animal.sniffer.Sniffer.State.DIGGING);
        metadata(allMetadata,"ARMADILLO_STATE",net.minecraft.world.entity.animal.armadillo.Armadillo.ArmadilloState.ROLLING);
        metadata(allMetadata,"COPPER_GOLEM_STATE",net.minecraft.world.entity.animal.golem.CopperGolemState.DROPPING_ITEM);
        metadata(allMetadata,"WEATHERING_COPPER_STATE",net.minecraft.world.level.block.WeatheringCopper.WeatherState.OXIDIZED);
        metadata(allMetadata,"VECTOR3",new org.joml.Vector3f(1,2,3));
        metadata(allMetadata,"QUATERNION",new org.joml.Quaternionf(0,0,0,1));
        metadata(allMetadata,"RESOLVABLE_PROFILE",profile);
        metadata(allMetadata,"HUMANOID_ARM",net.minecraft.world.entity.HumanoidArm.LEFT);
        metadata(allMetadata,"DYE_COLOR",DyeColor.CYAN);
        fixtures.addProperty("all_entity_data",bytes(net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket.STREAM_CODEC,new net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket(42,allMetadata)));
        var hashOps = HashOps.CRC32C_INSTANCE;
        var hashes = new JsonArray();
        for(var value:List.of(hashOps.empty(),hashOps.createBoolean(false),hashOps.createBoolean(true),hashOps.createByte((byte)-5),hashOps.createShort((short)1000),hashOps.createInt(-300),hashOps.createLong(12345678901234L),hashOps.createFloat(1.25f),hashOps.createDouble(-1.75),hashOps.createString("hello 🌍"),hashOps.createByteList(java.nio.ByteBuffer.wrap(new byte[]{-1,2,3})),hashOps.createIntList(java.util.stream.IntStream.of(-1,2,300)),hashOps.createLongList(java.util.stream.LongStream.of(-1,2,300)),hashOps.createList(java.util.stream.Stream.of(hashOps.createInt(1),hashOps.createInt(2))),hashOps.createMap(java.util.stream.Stream.of(com.mojang.datafixers.util.Pair.of(hashOps.createString("x"),hashOps.createFloat(1.5f)),com.mojang.datafixers.util.Pair.of(hashOps.createString("y"),hashOps.createBoolean(true)))))) hashes.add(value.asInt());
        fixtures.add("hash_ops",hashes);
        Files.writeString(Path.of(args[0]),GSON.toJson(fixtures)+"\n");
    }
}
