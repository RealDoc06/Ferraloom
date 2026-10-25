import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.enchantment.EnchantmentEffectComponents;

/** Native value-component payload codecs and their validatedListCodec contexts. */
public final class ValueComponentVectors {
    public static void main(String[] args) throws Exception {
        var out=System.out;
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();System.setOut(out);
        var world=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),world.registries())).freeze();
        var ops=registries.createSerializationContext(JsonOps.INSTANCE);
        var components=List.of("damage_protection","damage","smash_damage_per_fallen_block","knockback","armor_effectiveness","item_damage","ammo_use","projectile_piercing","block_experience","repair_with_xp","projectile_spread","projectile_count","trident_return_acceleration","fishing_time_reduction","fishing_luck_bonus","mob_experience","crossbow_charge_time","trident_spin_attack_strength");
        var requirements=List.of("null","{\"type\":\"minecraft:match_tool\",\"predicate\":{\"items\":\"minecraft:stone\"}}","{\"type\":\"minecraft:entity_properties\",\"entity\":\"this\",\"predicate\":{}}","{\"type\":\"minecraft:damage_source_properties\",\"predicate\":{}}","{\"type\":\"minecraft:location_check\",\"predicate\":{}}","{\"type\":\"minecraft:enchantment_active_check\",\"active\":true}","{\"type\":\"minecraft:random_chance\",\"chance\":0.5}","\"fixture:missing\"");
        var inputs=new ArrayList<JsonElement>();
        inputs.add(JsonParser.parseString("[]"));
        inputs.add(JsonParser.parseString("{\"type\":\"minecraft:add\",\"value\":2}"));
        inputs.add(JsonParser.parseString("[{\"effect\":{\"type\":\"minecraft:add\",\"value\":2}}]"));
        inputs.add(JsonParser.parseString("[{\"effect\":{\"type\":\"minecraft:add\",\"value\":2}},{\"effect\":{\"type\":\"minecraft:multiply\",\"factor\":3}}]"));
        for(String requirement:requirements) {
            var input=JsonParser.parseString("[{\"effect\":{\"type\":\"minecraft:add\",\"value\":2}}]").getAsJsonArray();
            input.get(0).getAsJsonObject().add("requirements",JsonParser.parseString(requirement));inputs.add(input);
        }
        inputs.add(JsonParser.parseString("[{}]"));inputs.add(JsonParser.parseString("[{\"effect\":5}]"));
        for(String component:components) for(var input:inputs) {
            var json=new JsonObject();json.add("minecraft:"+component,input);
            var decoded=EnchantmentEffectComponents.CODEC.parse(ops,json);
            int length=-1;
            if(decoded.result().isPresent()) {
                var type=BuiltInRegistries.ENCHANTMENT_EFFECT_COMPONENT_TYPE.getValue(Identifier.withDefaultNamespace(component));
                var value=decoded.getOrThrow().get(type);
                if(value instanceof List<?> values) length=values.size();
            }
            System.out.println("component\tminecraft:"+component+"\t"+input+"\t"+decoded.result().isPresent()+"\t"+length);
        }
    }
}
