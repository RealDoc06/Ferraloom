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
            out.add("value",entry.encodeValue(lookup.createSerializationContext(JsonOps.INSTANCE)).getOrThrow());
            out.addProperty("hash",entry.encodeValue(lookup.createSerializationContext(HashOps.CRC32C_INSTANCE)).getOrThrow().asInt());
        }
        components.add(out);
    }
    static void stack(String name, ItemStack value) {
        fixtures.addProperty(name,bytes(ItemStack.OPTIONAL_STREAM_CODEC,value));
        fixtures.addProperty(name+"_untrusted",bytes(ItemStack.OPTIONAL_UNTRUSTED_STREAM_CODEC,value));
        HashedPatchMap.HashGenerator hasher = c -> c.encodeValue(lookup.createSerializationContext(HashOps.CRC32C_INSTANCE)).getOrThrow().asInt();
        fixtures.addProperty(name+"_hashed",bytes(HashedStack.STREAM_CODEC,HashedStack.create(value,hasher)));
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
        Files.writeString(Path.of(args[0]),GSON.toJson(fixtures)+"\n");
    }
}
