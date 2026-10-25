import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.lang.reflect.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.enchantment.*;
import net.minecraft.world.item.enchantment.effects.EnchantmentValueEffect;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.storage.loot.*;
import net.minecraft.world.level.storage.loot.parameters.*;

/** Invoke native Enchantment.applyEffects with real conditions and one stream. */
public final class EffectListVectors {
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
        var actionType=Class.forName("net.minecraft.world.item.enchantment.Enchantment$GenericAction");
        var apply=Enchantment.class.getDeclaredMethod("applyEffects",List.class,LootContext.class,actionType);apply.setAccessible(true);
        String add="{\"effect\":{\"type\":\"minecraft:add\",\"value\":1}}";
        String multiply="{\"effect\":{\"type\":\"minecraft:multiply\",\"factor\":2}}";
        String chance="{\"type\":\"minecraft:random_chance\",\"chance\":0.5}";
        String match="{\"type\":\"minecraft:match_tool\",\"predicate\":{\"items\":\"minecraft:stone\"}}";
        String binomial="{\"effect\":{\"type\":\"minecraft:remove_binomial\",\"chance\":0.5}}";
        var matchSet=JsonParser.parseString("{\"effect\":{\"type\":\"minecraft:set\",\"value\":0.5}}").getAsJsonObject();matchSet.add("requirements",JsonParser.parseString(match));
        var chanceSet=JsonParser.parseString("{\"effect\":{\"type\":\"minecraft:set\",\"value\":7}}").getAsJsonObject();chanceSet.add("requirements",JsonParser.parseString(chance));
        var chanceBinomial=JsonParser.parseString(binomial).getAsJsonObject();chanceBinomial.add("requirements",JsonParser.parseString(chance));
        var lists=List.of("[]","["+add+","+multiply+"]","["+multiply+","+add+"]","["+chanceBinomial+","+add+"]","["+chanceSet+","+binomial+"]","["+matchSet+","+binomial+"]","["+chanceBinomial+","+binomial+"]");
        for(String json:lists) {
            var value=JsonParser.parseString(json);
            var effects=ConditionalEffect.codec(EnchantmentValueEffect.CODEC).listOf().parse(ops,value).getOrThrow();
            for(var item:List.of(Items.STONE,Items.DIRT)) for(int count:new int[]{0,1}) for(int level:new int[]{1,3}) for(float input:new float[]{0,0.5f,7,129}) for(long seed:new long[]{0,1,42,1234}) {
                var stack=new ItemStack(item,count);
                var params=new LootParams.Builder(null).withParameter(LootContextParams.TOOL,stack).withParameter(LootContextParams.ENCHANTMENT_LEVEL,level).create(LootContextParamSets.ENCHANTED_ITEM);
                var random=new LegacyRandomSource(seed);
                var context=(LootContext)constructor.newInstance(params,random,null);
                float[] current={input};
                var action=Proxy.newProxyInstance(actionType.getClassLoader(),new Class<?>[]{actionType},(proxy,method,arguments)->{current[0]=((EnchantmentValueEffect)arguments[0]).process(level,random,current[0]);return null;});
                apply.invoke(null,effects,context,action);
                System.out.println("list\t"+json+"\t"+BuiltInRegistries.ITEM.getId(item)+"\t"+count+"\t"+level+"\t"+Float.floatToRawIntBits(input)+"\t"+seed+"\t"+Float.floatToRawIntBits(current[0])+"\t"+random.nextLong());
            }
        }
    }
}
