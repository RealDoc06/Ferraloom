import com.google.gson.*;
import java.lang.reflect.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.properties.*;

/** Immutable constructor parameters only. No level queries or sampled behavior. */
public final class ExtractRedstoneProperties {
    static Object parameter(Object object,Class<?> type) throws ReflectiveOperationException {
        for(Class<?> c=object.getClass();c!=null;c=c.getSuperclass()) for(var field:c.getDeclaredFields()) {
            if(!Modifier.isStatic(field.getModifiers())&&field.getType()==type){field.setAccessible(true);return field.get(object);}
        }
        return null;
    }
    static int integer(Object object,Class<?> owner,String field) throws ReflectiveOperationException {
        var f=owner.getDeclaredField(field);f.setAccessible(true);return f.getInt(object);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var entries=new JsonArray();
        for(var block:BuiltInRegistries.BLOCK){
            var out=new JsonObject();out.addProperty("id",BuiltInRegistries.BLOCK.getId(block));
            var type=(BlockSetType)parameter(block,BlockSetType.class);
            if(type==null){out.add("set",JsonNull.INSTANCE);}else{
                var set=new JsonObject();set.addProperty("can_open_by_hand",type.canOpenByHand());set.addProperty("arrows_press_button",type.canButtonBeActivatedByArrows());set.addProperty("mobs_press_plate",type.pressurePlateSensitivity()==BlockSetType.PressurePlateSensitivity.MOBS);
                set.addProperty("door_open",BuiltInRegistries.SOUND_EVENT.getId(type.doorOpen()));set.addProperty("door_close",BuiltInRegistries.SOUND_EVENT.getId(type.doorClose()));
                set.addProperty("trapdoor_open",BuiltInRegistries.SOUND_EVENT.getId(type.trapdoorOpen()));set.addProperty("trapdoor_close",BuiltInRegistries.SOUND_EVENT.getId(type.trapdoorClose()));
                set.addProperty("button_on",BuiltInRegistries.SOUND_EVENT.getId(type.buttonClickOn()));set.addProperty("button_off",BuiltInRegistries.SOUND_EVENT.getId(type.buttonClickOff()));
                set.addProperty("plate_on",BuiltInRegistries.SOUND_EVENT.getId(type.pressurePlateClickOn()));set.addProperty("plate_off",BuiltInRegistries.SOUND_EVENT.getId(type.pressurePlateClickOff()));out.add("set",set);
            }
            out.addProperty("button_ticks",block instanceof ButtonBlock?integer(block,ButtonBlock.class,"ticksToStayPressed"):0);
            out.addProperty("max_weight",block instanceof WeightedPressurePlateBlock?integer(block,WeightedPressurePlateBlock.class,"maxWeight"):0);
            var wood=(WoodType)parameter(block,WoodType.class);
            if(block instanceof FenceGateBlock){out.addProperty("gate_open",BuiltInRegistries.SOUND_EVENT.getId(wood.fenceGateOpen()));out.addProperty("gate_close",BuiltInRegistries.SOUND_EVENT.getId(wood.fenceGateClose()));}
            entries.add(out);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().create().toJson(entries)+"\n");
    }
}
