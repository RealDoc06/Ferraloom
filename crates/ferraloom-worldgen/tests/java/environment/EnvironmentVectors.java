import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.attribute.*;

/** Actual map codec, modifier composition and final native sanitization. */
public final class EnvironmentVectors {
    static <T> void emit(EnvironmentAttribute<T> attribute,T base,JsonElement value) {
        var input=new JsonObject();input.add(attribute.toString(),value);
        var decoded=EnvironmentAttributeMap.CODEC.parse(JsonOps.INSTANCE,input);
        String encoded="error";
        if(decoded.isSuccess()) {
            var system=EnvironmentAttributeSystem.builder().addConstantLayer(EnvironmentAttributeMap.builder().set(attribute,base).build()).addConstantLayer(decoded.getOrThrow()).build();
            T result=system.getDimensionValue(attribute);
            encoded=result instanceof Float f?Integer.toUnsignedString(Float.floatToRawIntBits(f)):result.toString();
        }
        System.out.println(attribute+"\t"+base+"\t"+value+"\t"+encoded);
    }
    static JsonElement modifier(String operation,JsonElement argument) {
        var value=new JsonObject();value.addProperty("modifier",operation);value.add("argument",argument);return value;
    }
    public static void main(String[]args) {
        var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);
        for(boolean base:new boolean[]{false,true})for(boolean arg:new boolean[]{false,true})for(String op:new String[]{"override","and","nand","or","nor","xor","xnor"})emit(EnvironmentAttributes.FAST_LAVA,base,modifier(op,new JsonPrimitive(arg)));
        for(float base:new float[]{-0.0f,3.0f,15.0f})for(float arg:new float[]{-0.0f,2.25f,20.0f})for(String op:new String[]{"override","add","subtract","multiply","minimum","maximum","alpha_blend"})emit(EnvironmentAttributes.SKY_LIGHT_LEVEL,base,modifier(op,new JsonPrimitive(arg)));
        for(String op:new String[]{"override","and","invalid"})emit(EnvironmentAttributes.FAST_LAVA,false,modifier(op,new JsonPrimitive(1)));
        for(float alpha:new float[]{-0.1f,0.0f,0.3125f,1.0f,1.1f}) {
            var arg=new JsonObject();arg.addProperty("value",4.0);arg.addProperty("alpha",alpha);
            emit(EnvironmentAttributes.SKY_LIGHT_LEVEL,15.0f,modifier("alpha_blend",arg));
        }
    }
}
