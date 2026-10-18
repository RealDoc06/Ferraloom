import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.attribute.*;
import net.minecraft.world.phys.Vec3;

/** Real layer/cache composition, weather maps and identity-weighted biome blending. */
public final class LayerVectors {
    static JsonElement modifier(String name,float argument) {
        var value=new JsonObject();value.addProperty("modifier",name);value.addProperty("argument",argument);return value;
    }
    static <T> EnvironmentAttributeMap map(EnvironmentAttribute<T> attribute,JsonElement value) {
        var json=new JsonObject();json.add(attribute.toString(),value);
        return EnvironmentAttributeMap.CODEC.parse(JsonOps.INSTANCE,json).getOrThrow();
    }
    static String bits(float value){return Integer.toUnsignedString(Float.floatToRawIntBits(value));}
    public static void main(String[] args) {
        var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
        var attribute=EnvironmentAttributes.FOG_END_DISTANCE;
        for(float dim:new float[]{-2000,0,10})for(float bio:new float[]{-100,0,1200})for(float multiplier:new float[]{-1,0.5f,3}) {
            var dimJson=modifier("add",dim);var bioJson=modifier("add",bio);var timeJson=modifier("multiply",multiplier);
            var biome=map(attribute,bioJson);var time=map(attribute,timeJson);
            var system=EnvironmentAttributeSystem.builder().addConstantLayer(map(attribute,dimJson))
                .addPositionalLayer(attribute,(base,pos,weights)->biome.applyModifier(attribute,base))
                .addTimeBasedLayer(attribute,(base,tick)->time.applyModifier(attribute,base)).build();
            System.out.println("layers\t"+dimJson+"\t"+bioJson+"\t"+timeJson+"\t"+bits(system.getDimensionValue(attribute))+"\t"+bits(system.getValue(attribute,Vec3.ZERO)));
        }
        for(float base:new float[]{0,6,15})for(float rain:new float[]{0,0.00001f,0.5f,1})for(float thunder:new float[]{0,0.00001f,0.5f,1})if(thunder<=rain) {
            var builder=EnvironmentAttributeSystem.builder().addConstantLayer(EnvironmentAttributeMap.builder().set(EnvironmentAttributes.SKY_LIGHT_LEVEL,base).build());
            WeatherAttributes.addBuiltinLayers(builder,new WeatherAttributes.WeatherAccess(){public float rainLevel(){return rain;}public float thunderLevel(){return thunder;}});
            System.out.println("weather\t"+base+"\t"+rain+"\t"+thunder+"\t"+bits(builder.build().getDimensionValue(EnvironmentAttributes.SKY_LIGHT_LEVEL)));
        }
        for(boolean base:new boolean[]{false,true})for(float rain:new float[]{0,0.00001f,0.5f,1})for(float thunder:new float[]{0,0.00001f,0.5f,1})if(thunder<=rain) {
            var builder=EnvironmentAttributeSystem.builder().addConstantLayer(EnvironmentAttributeMap.builder().set(EnvironmentAttributes.BEES_STAY_IN_HIVE,base).build());
            WeatherAttributes.addBuiltinLayers(builder,new WeatherAttributes.WeatherAccess(){public float rainLevel(){return rain;}public float thunderLevel(){return thunder;}});
            System.out.println("bees\t"+base+"\t"+rain+"\t"+thunder+"\t"+builder.build().getDimensionValue(EnvironmentAttributes.BEES_STAY_IN_HIVE));
        }
        for(float base:new float[]{10,1024})for(float a:new float[]{-1024,0,4,10000})for(double weight:new double[]{0.1,0.5,0.999999999}) {
            var first=map(attribute,modifier("add",a));var second=map(attribute,modifier("multiply",0.25f));var third=map(attribute,modifier("add",-100));
            var weights=new SpatialAttributeInterpolator().accumulate(weight,first).accumulate(0.3,second).accumulate(0.2,first).accumulate(0.7,third);
            var system=EnvironmentAttributeSystem.builder().addConstantLayer(EnvironmentAttributeMap.builder().set(attribute,base).build())
                .addPositionalLayer(attribute,(value,pos,interpolator)->interpolator.applyAttributeLayer(attribute,value)).build();
            System.out.println("spatial\t"+base+"\t"+a+"\t"+weight+"\t"+bits(system.getValue(attribute,Vec3.ZERO,weights)));
        }
    }
}
