import com.google.gson.*;
import com.mojang.serialization.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.slot.SlotSources;
import net.minecraft.world.level.storage.loot.functions.LootItemFunctions;

/** Actual DynamicOps scalar codecs and the registered loot/source decode schemas. */
public final class LootInputVectors {
    static Codec<?> scalar(String type) {
        return switch(type) {
            case "bool" -> Codec.BOOL; case "int" -> Codec.INT; case "long" -> Codec.LONG;
            case "float" -> Codec.FLOAT; case "double" -> Codec.DOUBLE; case "string" -> Codec.STRING;
            default -> throw new IllegalArgumentException(type);
        };
    }
    static <V> JsonObject scalar(String format,String type,String source,DynamicOps<V> ops,V input) {
        var row=new JsonObject();row.addProperty("format",format);row.addProperty("type",type);row.addProperty("source",source);
        var result=scalar(type).parse(ops,input);
        row.addProperty("ok",result.isSuccess());
        if(result.isSuccess()) {
            var value=result.getOrThrow();
            row.addProperty("value",value instanceof Float n?String.format("%08x",Float.floatToIntBits(n)):
                value instanceof Double n?String.format("%016x",Double.doubleToLongBits(n)):value.toString());
        } else row.addProperty("error",result.error().orElseThrow().message());
        return row;
    }
    static <T> JsonObject decode(String kind,String source,Codec<T> codec,DynamicOps<Tag> nbt,DynamicOps<JsonElement> json) throws Exception {
        var row=new JsonObject();row.addProperty("kind",kind);row.addProperty("source",source);
        var result=codec.parse(nbt,TagParser.create(NbtOps.INSTANCE).parseFully(source));
        row.addProperty("ok",result.isSuccess());
        if(result.isSuccess()) {
            row.add("canonical_json",codec.encodeStart(json,result.getOrThrow()).getOrThrow());
            row.addProperty("canonical_nbt",codec.encodeStart(nbt,result.getOrThrow()).getOrThrow().toString());
        } else row.addProperty("error",result.error().orElseThrow().message());
        return row;
    }
    public static void main(String[] args) throws Exception {
        var output=new JsonObject();var primitives=new JsonArray();
        for(String type:List.of("bool","int","long","float","double","string")) {
            for(String source:List.of("0b","1b","-1b","256s","256","9007199254740993L","0.5f","1.25d","\"17\"","\"0.5\"","\"true\"","\"stone\"","{}","[1,2]","true","false"))
                primitives.add(scalar("nbt",type,source,NbtOps.INSTANCE,TagParser.create(NbtOps.INSTANCE).parseFully(source)));
            for(String source:List.of("null","true","false","0","1","-1","256","0.5","1.25","9007199254740993","\"17\"","\"0.5\"","\"true\"","\"stone\"","{}","[1,2]"))
                primitives.add(scalar("json",type,source,JsonOps.INSTANCE,JsonParser.parseString(source)));
        }
        output.add("primitives",primitives);
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var lookup=LootVectors.load();BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p->p.apply());
        var nbt=lookup.createSerializationContext(NbtOps.INSTANCE);var json=lookup.createSerializationContext(JsonOps.INSTANCE);
        var definitions=new JsonArray();
        for(String source:List.of(
            "{type:set_count,count:3b,add:1b}",
            "{type:set_count,count:3s,add:256s}",
            "{type:set_count,count:3,add:0.5d}",
            "{type:set_count,count:3,add:0b}",
            "{type:set_count,count:{type:constant,value:2.75f}}",
            "{type:set_count,count:{type:constant,value:\"17\"}}",
            "{type:set_count,count:3,add:\"true\"}",
            "{type:set_count,count:{type:add,inputs:[B;1b,2b]}}",
            "{type:set_damage,damage:0.5f,add:1b}",
            "{type:set_count,count:1,condition:{type:weather_check,raining:0.5d}}",
            "{type:filtered,item_filter:{count:{min:1b,max:3s}},on_pass:{type:set_count,count:2b,add:1b}}",
            "{type:filtered,item_filter:{count:{min:3b,max:1s}},on_pass:{type:discard}}",
            "{type:set_count,count:1,condition:{type:entity_properties,entity:this,predicate:{flags:{is_on_fire:256s}}}}",
            "{type:set_components,components:{\"minecraft:max_stack_size\":3b,\"minecraft:custom_data\":{flag:1b}}}",
            "{type:set_custom_data,tag:{n:16777217,long:9007199254740993L}}",
            "{type:toggle_tooltips,toggles:{\"minecraft:custom_name\":0.5d}}",
            "[{type:set_count,count:2b},{type:set_count,count:3s,add:1b}]",
            "{type:set_count,count:1,condition:{type:weather_check,raining:\"true\"}}",
            "{type:42b,count:1}",
            "{type:not_a_function}",
            "{type:filtered,item_filter:{components:{\"minecraft:custom_data\":{flag:1b}}},on_pass:{type:discard}}"
        )) definitions.add(decode("modifier",source,LootItemFunctions.DIRECT_CODEC,nbt,json));
        for(String source:List.of(
            "{type:empty}","[]","[B;]","[I;]","[L;]","[B;1b]",
            "{type:slot_range,slots:\"container.0\"}",
            "{type:slot_range,source:this,slots:\"armor.*\"}",
            "{type:slot_range,slots:1b}",
            "{type:slot_range,source:1b,slots:\"container.0\"}",
            "{type:limit_slots,slot_source:{type:slot_range,slots:\"container.*\"},limit:2b}",
            "{type:limit_slots,slot_source:{type:slot_range,slots:\"container.*\"},limit:2.75f}",
            "{type:limit_slots,slot_source:{type:slot_range,slots:\"container.*\"},limit:0b}",
            "{type:limit_slots,slot_source:{type:slot_range,slots:\"container.*\"},limit:\"2\"}",
            "{type:filtered,slot_source:{type:slot_range,slots:\"container.*\"},item_filter:{count:{min:1b,max:3s}}}",
            "{type:contents,slot_source:{type:slot_range,slots:\"container.*\"},component:\"minecraft:container\"}",
            "[{type:empty},{type:slot_range,slots:\"weapon.mainhand\"}]",
            "{type:not_a_source}"
        )) definitions.add(decode("source",source,SlotSources.DIRECT_CODEC,nbt,json));
        output.add("definitions",definitions);
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(output)+"\n");
        System.out.println("Captured "+primitives.size()+" native scalar and "+definitions.size()+" native loot/source codec cases");
    }
}
