import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.item.enchantment.effects.EnchantmentAttributeEffect;

/** Native EnchantmentAttributeEffect codec and getModifier, for every slot. */
public final class AttributeEffectVectors {
    public static void main(String[] args) throws Exception {
        var out=System.out;
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();System.setOut(out);
        var world=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),world.registries())).freeze();
        var ops=registries.createSerializationContext(JsonOps.INSTANCE);
        var amounts=List.of("0.1","-2.5","{\"type\":\"minecraft:linear\",\"base\":0.1,\"per_level_above_first\":0.1}","{\"type\":\"minecraft:levels_squared\",\"added\":0.1}","{\"type\":\"minecraft:fraction\",\"numerator\":3,\"denominator\":7}");
        for(String attribute:List.of("movement_speed","attack_damage")) for(String operation:List.of("add_value","add_multiplied_base","add_multiplied_total")) for(String amount:amounts) {
            var json=JsonParser.parseString("{\"id\":\"fixture:modifier/path\",\"attribute\":\"minecraft:"+attribute+"\",\"amount\":"+amount+",\"operation\":\""+operation+"\"}");
            var effect=EnchantmentAttributeEffect.MAP_CODEC.codec().parse(ops,json).getOrThrow();
            for(int level:new int[]{Integer.MIN_VALUE,-1,0,1,3,255,Integer.MAX_VALUE}) for(var slot:EquipmentSlot.values()) {
                var modifier=effect.getModifier(level,slot);
                System.out.println("modifier\t"+json+"\t"+level+"\t"+slot.getSerializedName()+"\t"+modifier.id()+"\t"+Double.doubleToRawLongBits(modifier.amount())+"\t"+modifier.operation().getSerializedName());
            }
        }
        String base="{\"id\":\"fixture:modifier\",\"attribute\":\"minecraft:movement_speed\",\"amount\":1,\"operation\":\"add_value\"}";
        for(String field:List.of("id","attribute","amount","operation")) for(String input:List.of("null","0","\"fixture:missing\"","[]")) {
            var json=JsonParser.parseString(base).getAsJsonObject();json.add(field,JsonParser.parseString(input));
            System.out.println("codec\t"+json+"\t"+EnchantmentAttributeEffect.MAP_CODEC.codec().parse(ops,json).result().isPresent());
        }
    }
}
