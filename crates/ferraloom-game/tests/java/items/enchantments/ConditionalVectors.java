import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.item.*;
import net.minecraft.world.item.enchantment.*;
import net.minecraft.world.item.enchantment.effects.EnchantmentValueEffect;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.storage.loot.*;
import net.minecraft.world.level.storage.loot.parameters.*;

/** Native wrapper codecs, context validation, conditions and short-circuit RNG. */
public final class ConditionalVectors {
    public static void main(String[] args) throws Exception {
        var out=System.out;
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();System.setOut(out);
        var world=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),world.registries())).freeze();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var ops=registries.createSerializationContext(JsonOps.INSTANCE);
        var constructor=LootContext.class.getDeclaredConstructor(LootParams.class,net.minecraft.util.RandomSource.class,HolderGetter.Provider.class);
        constructor.setAccessible(true);
        var match="{\"type\":\"minecraft:match_tool\",\"predicate\":{\"items\":\"minecraft:stone\"}}";
        var chance="{\"type\":\"minecraft:random_chance\",\"chance\":0.5}";
        var requirements=List.of("",match,chance,"{\"type\":\"minecraft:all_of\",\"terms\":["+match+","+chance+"]}","{\"type\":\"minecraft:all_of\",\"terms\":["+chance+","+match+"]}","{\"type\":\"minecraft:entity_properties\",\"entity\":\"this\",\"predicate\":{}}");
        for (String kind:List.of("conditional","targeted","equipment","equipment-invalid")) for (String requirement:requirements) {
            var json=JsonParser.parseString("{\"effect\":{\"type\":\"minecraft:add\",\"value\":1}} ").getAsJsonObject();
            if (!requirement.isEmpty()) json.add("requirements",JsonParser.parseString(requirement));
            if (!kind.equals("conditional")) {
                json.addProperty("enchanted",kind.equals("equipment-invalid")?"damaging_entity":"attacker");
                if (kind.equals("targeted")) json.addProperty("affected","damaging_entity");
            }
            var decoded=kind.equals("conditional")?ConditionalEffect.codec(EnchantmentValueEffect.CODEC).parse(ops,json):
                kind.equals("targeted")?TargetedConditionalEffect.codec(EnchantmentValueEffect.CODEC).parse(ops,json):
                TargetedConditionalEffect.equipmentDropsCodec(EnchantmentValueEffect.CODEC).parse(ops,json);
            if (decoded.result().isEmpty()) { System.out.println("codec\t"+kind+"\t"+json+"\tfalse"); continue; }
            var effect=decoded.result().get();
            boolean valid=Validatable.listValidatorForContext(LootContextParamSets.ENCHANTED_ITEM).apply(List.of((Validatable)effect)).result().isPresent();
            System.out.println("codec\t"+kind+"\t"+json+"\t"+valid);
            if (!valid) continue;
            for (var item:List.of(Items.STONE,Items.DIRT)) for (int count:new int[]{0,1}) for (long seed:new long[]{0,1,42,1234}) {
                var stack=new ItemStack(item,count);
                var params=new LootParams.Builder(null).withParameter(LootContextParams.TOOL,stack).withParameter(LootContextParams.ENCHANTMENT_LEVEL,3).create(LootContextParamSets.ENCHANTED_ITEM);
                var random=new LegacyRandomSource(seed);
                var context=(LootContext)constructor.newInstance(params,random,null);
                boolean matches=effect instanceof ConditionalEffect<?> simple?simple.matches(context):((TargetedConditionalEffect<?>)effect).matches(context);
                System.out.println("match\t"+kind+"\t"+json+"\t"+BuiltInRegistries.ITEM.getId(item)+"\t"+count+"\t"+seed+"\t"+matches+"\t"+random.nextLong());
            }
        }
    }
}
