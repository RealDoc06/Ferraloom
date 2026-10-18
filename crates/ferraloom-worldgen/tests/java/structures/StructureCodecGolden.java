import com.google.gson.*;
import com.mojang.serialization.*;
import java.nio.file.*;
import java.io.*;
import java.util.*;
import java.util.zip.GZIPInputStream;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.nbt.*;
import net.minecraft.world.level.levelgen.structure.pools.*;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;

/** Typed codec writes, including one documented inline-feature normalization gap. */
public final class StructureCodecGolden {
    static JsonObject read(Path directory,String name)throws Exception {try(var reader=new InputStreamReader(new GZIPInputStream(Files.newInputStream(directory.resolve(name+".json.gz"))),java.nio.charset.StandardCharsets.UTF_8)){return JsonParser.parseReader(reader).getAsJsonObject();}}
    static <T> void add(JsonArray out,String kind,Codec<T> codec,JsonElement input,RegistryAccess.Frozen registries,boolean exact) {
        var value=codec.parse(RegistryOps.create(JsonOps.INSTANCE,registries),input).getOrThrow();var encoded=codec.encodeStart(RegistryOps.create(NbtOps.INSTANCE,registries),value).getOrThrow();var item=new JsonObject();item.addProperty("kind",kind);item.add("input",input);item.addProperty("encoded",encoded.toString());item.addProperty("exact",exact);out.add(item);
    }
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();var seen=new HashSet<String>();
        for(var item:read(directory,"processors").getAsJsonArray("blocks")) {var input=item.getAsJsonObject().get("definition");if(seen.add(input.toString()))add(out,"processor",StructureProcessorType.SINGLE_CODEC,input,registries,true);}
        for(var field:List.of("blocks","positions"))for(var item:read(directory,"rules").getAsJsonArray(field)){var input=item.getAsJsonObject().get("definition");if(seen.add(input.toString())){if(field.equals("blocks"))add(out,"rule",RuleTest.CODEC,input,registries,true);else add(out,"position",PosRuleTest.CODEC,input,registries,true);}}
        for(var item:read(directory,"processors").getAsJsonArray("modifiers")){var input=item.getAsJsonObject().get("definition");if(seen.add(input.toString()))add(out,"modifier",net.minecraft.world.level.levelgen.structure.templatesystem.rule.blockentity.RuleBlockEntityModifier.CODEC,input,registries,true);}
        for(var entry:registries.lookupOrThrow(Registries.PROCESSOR_LIST).listElements().toList())for(var processor:entry.value().list()){var input=StructureProcessorType.SINGLE_CODEC.encodeStart(RegistryOps.create(JsonOps.INSTANCE,registries),processor).getOrThrow();if(seen.add(input.toString()))add(out,"processor",StructureProcessorType.SINGLE_CODEC,input,registries,true);}
        for(String limit:List.of("1","{type:'minecraft:uniform',min_inclusive:1,max_inclusive:7}","{type:'minecraft:biased_to_bottom',min_inclusive:1,max_inclusive:9}","{type:'minecraft:very_biased_to_bottom',min_inclusive:1,max_inclusive:9}","{type:'minecraft:clamped',source:{type:'minecraft:uniform',min_inclusive:0,max_inclusive:9},min_inclusive:1,max_inclusive:8}","{type:'minecraft:clamped_normal',mean:3.5,deviation:2,min_inclusive:1,max_inclusive:9}","{type:'minecraft:trapezoid',min:1,max:9,plateau:2}","{type:'minecraft:weighted_list',distribution:[{data:1,weight:0},{data:3,weight:2}]}"))add(out,"processor",StructureProcessorType.SINGLE_CODEC,JsonParser.parseString("{processor_type:'minecraft:capped',delegate:{processor_type:'minecraft:nop'},limit:"+limit+"}"),registries,true);
        for(var entry:registries.lookupOrThrow(Registries.TEMPLATE_POOL).listElements().toList()) {
            var id=entry.key().identifier();try(var reader=StructureGolden.RESOURCES.getResourceOrThrow(Identifier.fromNamespaceAndPath(id.getNamespace(),"worldgen/template_pool/"+id.getPath()+".json")).openAsReader()) {
                for(var element:JsonParser.parseReader(reader).getAsJsonObject().getAsJsonArray("elements")){var input=element.getAsJsonObject().get("element");if(seen.add(input.toString()))add(out,"pool",StructurePoolElement.CODEC,input,registries,true);}
            }
        }
        add(out,"pool",StructurePoolElement.CODEC,JsonParser.parseString("{element_type:'minecraft:feature_pool_element',projection:'rigid',feature:{feature:{type:'minecraft:no_op',config:{}},placement:[]}}"),registries,false);
        StructureGolden.write(directory.resolve("structure_codecs.json.gz"),out);
    }
}
