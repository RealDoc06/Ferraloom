import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.util.*;
import net.minecraft.util.*;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.item.enchantment.effects.EnchantmentValueEffect;

/** Native effect codecs and methods, never a duplicate implementation. */
public final class ValueEffectVectors {
    public static void main(String[] args) throws Exception {
        var out=System.out;
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();System.setOut(out);
        var effects=new ArrayList<String>();
        effects.add("{\"type\":\"minecraft:add\",\"value\":{\"type\":\"minecraft:linear\",\"base\":1.25,\"per_level_above_first\":-0.5}}");
        effects.add("{\"type\":\"minecraft:multiply\",\"factor\":{\"type\":\"minecraft:linear\",\"base\":0.5,\"per_level_above_first\":0.125}}");
        effects.add("{\"type\":\"minecraft:set\",\"value\":{\"type\":\"minecraft:levels_squared\",\"added\":0.25}}");
        effects.add("{\"type\":\"minecraft:exponential\",\"base\":1.25,\"exponent\":{\"type\":\"minecraft:linear\",\"base\":2,\"per_level_above_first\":0.5}}");
        effects.add("{\"type\":\"minecraft:all_of\",\"effects\":[{\"type\":\"minecraft:add\",\"value\":2},{\"type\":\"minecraft:multiply\",\"factor\":3},{\"type\":\"minecraft:add\",\"value\":-1}]}");
        effects.add("{\"type\":\"minecraft:all_of\",\"effects\":[{\"type\":\"minecraft:multiply\",\"factor\":3},{\"type\":\"minecraft:add\",\"value\":2},{\"type\":\"minecraft:add\",\"value\":-1}]}");
        effects.add("{\"type\":\"minecraft:all_of\",\"effects\":[]}");
        for (float chance:new float[]{-0.1f,0,0.001f,0.2f,0.5f,0.999f,1,1.1f}) effects.add("{\"type\":\"minecraft:remove_binomial\",\"chance\":"+chance+"}");
        // The two actual Unbreaking level-value formulas, armor then non-armor.
        effects.add("{\"type\":\"minecraft:remove_binomial\",\"chance\":{\"type\":\"minecraft:fraction\",\"numerator\":{\"type\":\"minecraft:linear\",\"base\":2,\"per_level_above_first\":2},\"denominator\":{\"type\":\"minecraft:linear\",\"base\":10,\"per_level_above_first\":5}}}");
        effects.add("{\"type\":\"minecraft:remove_binomial\",\"chance\":{\"type\":\"minecraft:fraction\",\"numerator\":{\"type\":\"minecraft:linear\",\"base\":1,\"per_level_above_first\":1},\"denominator\":{\"type\":\"minecraft:linear\",\"base\":2,\"per_level_above_first\":1}}}");
        for (String json:effects) {
            var effect=EnchantmentValueEffect.CODEC.parse(JsonOps.INSTANCE,JsonParser.parseString(json)).getOrThrow();
            for (int level:new int[]{0,1,2,3,255}) for (float input:new float[]{-1,0,0.5f,1,2.5f,127.9f,128,128.1f,129,400}) for (String kind:List.of("legacy","xoroshiro")) for (long seed:new long[]{0,1,42,-1,123456789,Long.MIN_VALUE,Long.MAX_VALUE,1337}) {
                RandomSource random=kind.equals("legacy")?new LegacyRandomSource(seed):new XoroshiroRandomSource(seed);
                float result=effect.process(level,random,input);
                System.out.println(json+"\t"+level+"\t"+Integer.toUnsignedString(Float.floatToRawIntBits(input))+"\t"+kind+"\t"+seed+"\t"+Integer.toUnsignedString(Float.floatToRawIntBits(result))+"\t"+random.nextLong()+"\t"+Long.toUnsignedString(Double.doubleToRawLongBits(random.nextGaussian())));
            }
        }
    }
}
