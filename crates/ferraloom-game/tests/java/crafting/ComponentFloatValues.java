import com.google.gson.*;
import com.mojang.serialization.DynamicOps;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.*;
import net.minecraft.network.codec.*;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.util.HashOps;
import net.minecraft.world.entity.EquipmentSlotGroup;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.food.FoodProperties;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.storage.loot.providers.number.floats.ResolvableFloat;
import net.minecraft.world.level.storage.loot.providers.number.ints.ResolvableInt;

/** Native component record equality/hashCode, stack equality and raw codec observations. */
public final class ComponentFloatValues {
    static final Gson JSON=new GsonBuilder().serializeNulls().disableHtmlEscaping().create();
    static RegistryAccess.Frozen access;
    static HolderLookup.Provider lookup;
    // Isolate EncoderCache entries: record NaN equality must not mask the raw codec bits.
    static <T> RegistryOps<T> freshOps(DynamicOps<T> delegate) {
        return RegistryOps.create(delegate,new RegistryOps.RegistryInfoLookup() {
            public <E> Optional<HolderGetter<E>> lookup(ResourceKey<? extends Registry<? extends E>> key) {
                return ComponentFloatValues.lookup.lookup(key).map(value->value);
            }
        });
    }
    static TypedDataComponent<?> floatValue(String type,int bits) {
        float value=Float.intBitsToFloat(bits);
        return switch(type) {
            case "food" -> new TypedDataComponent<>(DataComponents.FOOD,new FoodProperties(3,value,false));
            case "use_effects" -> new TypedDataComponent<>(DataComponents.USE_EFFECTS,new UseEffects(false,true,value));
            case "attack_range" -> new TypedDataComponent<>(DataComponents.ATTACK_RANGE,new AttackRange(0,3,0,5,0.3f,value));
            case "tool" -> new TypedDataComponent<>(DataComponents.TOOL,new Tool(List.of(),value,1,true));
            case "tool_rule" -> new TypedDataComponent<>(DataComponents.TOOL,new Tool(List.of(new Tool.Rule(HolderSet.direct(Blocks.STONE.builtInRegistryHolder()),Optional.of(value),Optional.empty())),1,1,true));
            case "consumable" -> new TypedDataComponent<>(DataComponents.CONSUMABLE,new Consumable(value,ItemUseAnimation.EAT,SoundEvents.GENERIC_EAT,true,List.of()));
            case "custom_model_data" -> new TypedDataComponent<>(DataComponents.CUSTOM_MODEL_DATA,new CustomModelData(List.of(value),List.of(true,false),List.of("probe"),List.of(0x123456)));
            case "cooking_fuel" -> new TypedDataComponent<>(DataComponents.COOKING_FUEL,new CookingFuel(new ResolvableInt.Constant(200),new ResolvableFloat.Constant(value)));
            case "minimum_attack_charge" -> new TypedDataComponent<>(DataComponents.MINIMUM_ATTACK_CHARGE,value);
            case "potion_duration_scale" -> new TypedDataComponent<>(DataComponents.POTION_DURATION_SCALE,value);
            default -> throw new IllegalArgumentException(type);
        };
    }
    static TypedDataComponent<?> doubleValue(long bits) {
        return new TypedDataComponent<>(DataComponents.ATTRIBUTE_MODIFIERS,new ItemAttributeModifiers(List.of(new ItemAttributeModifiers.Entry(
            Attributes.MAX_HEALTH,new AttributeModifier(Identifier.parse("minecraft:probe"),Double.longBitsToDouble(bits),AttributeModifier.Operation.ADD_VALUE),EquipmentSlotGroup.ANY))));
    }
    static <T> String wire(TypedDataComponent<T> value) {
        var buf=new RegistryFriendlyByteBuf(Unpooled.buffer(),access);
        try {
            value.type().streamCodec().encode(buf,value.value());
            byte[] bytes=new byte[buf.readableBytes()];buf.readBytes(bytes);
            return HexFormat.of().formatHex(bytes);
        } finally {buf.release();}
    }
    static <T> ItemStack stack(TypedDataComponent<T> value,int count) {
        var result=new ItemStack(Items.STONE,count);result.set(value.type(),value.value());return result;
    }
    static JsonObject group(String type,List<TypedDataComponent<?>> values,List<String> bits) {
        var group=new JsonObject();group.addProperty("type",type);
        group.addProperty("component",BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(values.getFirst().type()).toString());
        var rows=new JsonArray();
        for(int i=0;i<values.size();i++) {
            var value=values.get(i);var row=new JsonObject();row.addProperty("bits",bits.get(i));row.addProperty("wire",wire(value));
            row.addProperty("record_hash",Set.of("food","use_effects","attack_range").contains(type) ? value.value().hashCode() : null);
            var hash=value.encodeValue(freshOps(HashOps.CRC32C_INSTANCE)).result();
            row.addProperty("hash",hash.map(h->h.asInt()).orElse(null));
            var nbt=value.encodeValue(freshOps(net.minecraft.nbt.NbtOps.INSTANCE)).result();
            if(nbt.isPresent()) {
                var buf=Unpooled.buffer();
                try {ByteBufCodecs.TAG.encode(buf,nbt.get());byte[] data=new byte[buf.readableBytes()];buf.readBytes(data);row.addProperty("nbt",HexFormat.of().formatHex(data));}
                finally {buf.release();}
            } else row.add("nbt",JsonNull.INSTANCE);
            var equality=new StringBuilder();var stacks=new StringBuilder();
            for(var other:values) {
                equality.append(value.value().equals(other.value()) ? '1':'0');
                stacks.append(ItemStack.isSameItemSameComponents(stack(value,1),stack(other,7)) ? '1':'0');
            }
            row.addProperty("equal",equality.toString());row.addProperty("stacks",stacks.toString());rows.add(row);
        }
        group.add("rows",rows);return group;
    }
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();
        lookup=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p->p.apply());
        access=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var groups=new JsonArray();
        for(String type:List.of("food","use_effects","attack_range","tool","tool_rule","consumable","custom_model_data","cooking_fuel","minimum_attack_charge","potion_duration_scale")) {
            var values=new ArrayList<TypedDataComponent<?>>();var bits=new ArrayList<String>();
            for(int input:FloatValues.FLOATS) {values.add(floatValue(type,input));bits.add(Integer.toHexString(input));}
            groups.add(group(type,values,bits));
        }
        var values=new ArrayList<TypedDataComponent<?>>();var bits=new ArrayList<String>();
        for(long input:FloatValues.DOUBLES) {values.add(doubleValue(input));bits.add(Long.toHexString(input));}
        groups.add(group("attribute_modifiers",values,bits));
        var bounds=new JsonArray();
        for(long input:FloatValues.DOUBLES) {
            var value=net.minecraft.advancements.predicates.MinMaxBounds.Doubles.exactly(Double.longBitsToDouble(input));
            var row=new JsonObject();row.addProperty("bits",Long.toHexString(input));var equality=new StringBuilder();
            for(long other:FloatValues.DOUBLES) equality.append(value.equals(net.minecraft.advancements.predicates.MinMaxBounds.Doubles.exactly(Double.longBitsToDouble(other))) ? '1':'0');
            row.addProperty("equal",equality.toString());bounds.add(row);
        }
        var chat=new ArrayList<TypedDataComponent<?>>();var chatBits=new ArrayList<String>();
        for(int input:FloatValues.FLOATS) {
            chat.add(new TypedDataComponent<>(DataComponents.CUSTOM_NAME,net.minecraft.network.chat.Component.translatable("probe.number",Float.intBitsToFloat(input))));
            chatBits.add("float:"+Integer.toHexString(input));
        }
        for(long input:FloatValues.DOUBLES) {
            chat.add(new TypedDataComponent<>(DataComponents.CUSTOM_NAME,net.minecraft.network.chat.Component.translatable("probe.number",Double.longBitsToDouble(input))));
            chatBits.add("double:"+Long.toHexString(input));
        }
        var out=new JsonObject();out.add("groups",groups);out.add("bounds",bounds);out.add("chat",group("chat_translation",chat,chatBits));
        Files.writeString(Path.of(args[0]),JSON.toJson(out)+"\n");
        System.out.println("Component float vectors: 198 encodings, 54 record hashes, 3,564 component and 3,564 stack comparisons, 324 bounds comparisons");
        System.out.println("Translation vectors: 36 encodings, 1,296 component and 1,296 stack comparisons");
    }
}
