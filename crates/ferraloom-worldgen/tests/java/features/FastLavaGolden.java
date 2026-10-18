import com.google.gson.*;
import java.lang.reflect.Proxy;
import java.nio.file.Path;
import java.nio.file.Files;
import java.nio.charset.StandardCharsets;
import java.util.zip.GZIPOutputStream;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.attribute.EnvironmentAttributeSystem;
import net.minecraft.world.attribute.EnvironmentAttributes;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.material.Fluids;

/** Dimension constants use the same static environment layer as WorldGenRegion. */
public final class FastLavaGolden {
    static void capture(RegistryAccess.Frozen registries, JsonArray cases, String name, JsonObject config) {
        var item=new JsonObject(); item.addProperty("name",name); item.add("config",config.deepCopy());
        var parsed=DimensionType.DIRECT_CODEC.parse(FeaturesGolden.ops(registries),config).result();
        item.addProperty("success",parsed.isPresent());
        parsed.ifPresent(dimension -> {
            var environment=EnvironmentAttributeSystem.builder().addConstantLayer(dimension.attributes()).build();
            item.addProperty("fast_lava",environment.getDimensionValue(EnvironmentAttributes.FAST_LAVA));
            var reader=(LevelReader)Proxy.newProxyInstance(LevelReader.class.getClassLoader(),new Class<?>[]{LevelReader.class},(proxy,method,args) -> {
                if(method.getName().equals("environmentAttributes"))return environment;
                throw new AssertionError(method.toString());
            });
            item.addProperty("lava_delay",Fluids.LAVA.getTickDelay(reader));
        });
        cases.add(item);
    }
    static void generate(RegistryAccess.Frozen registries, Path out) throws Exception {
        var cases=new JsonArray(); JsonObject overworld=null;
        for(var holder:registries.lookupOrThrow(Registries.DIMENSION_TYPE).listElements().toList()) {
            var config=DimensionType.DIRECT_CODEC.encodeStart(FeaturesGolden.ops(registries),holder.value()).getOrThrow().getAsJsonObject();
            var name=holder.key().identifier().toString(); capture(registries,cases,name,config);
            if(name.equals("minecraft:overworld"))overworld=config;
        }
        if(overworld==null)throw new AssertionError("missing overworld");
        var absent=overworld.deepCopy();absent.remove("attributes");capture(registries,cases,"missing_attributes",absent);
        for(String text:new String[]{"{}","null","false","[]"}) {
            var config=overworld.deepCopy();config.add("attributes",JsonParser.parseString(text));capture(registries,cases,"attributes_"+text,config);
        }
        for(String modifier:new String[]{"override","and","nand","or","nor","xor","xnor"})for(boolean argument:new boolean[]{false,true}) {
            var entry=new JsonObject();entry.addProperty("modifier",modifier);entry.addProperty("argument",argument);
            var config=overworld.deepCopy();var attributes=new JsonObject();attributes.add("minecraft:gameplay/fast_lava",entry);config.add("attributes",attributes);
            capture(registries,cases,modifier+"_"+argument,config);
        }
        for(String text:new String[]{"true","false","null","0","\"true\"","[]","{}","{modifier:'override'}","{argument:true}","{modifier:'override',argument:0}","{modifier:'add',argument:true}","{modifier:'minecraft:or',argument:true}","{modifier:'OR',argument:true}"}) {
            var config=overworld.deepCopy();var attributes=new JsonObject();attributes.add("minecraft:gameplay/fast_lava",JsonParser.parseString(text));config.add("attributes",attributes);
            capture(registries,cases,"entry_"+text,config);
        }
        var alias=overworld.deepCopy();var attributes=new JsonObject();attributes.addProperty("gameplay/fast_lava",true);alias.add("attributes",attributes);capture(registries,cases,"short_key",alias);
        attributes.addProperty("minecraft:gameplay/fast_lava",false);capture(registries,cases,"duplicate_key",alias.deepCopy());
        // Explicit null entries are codec inputs, so the golden must retain them.
        try(var stream=new GZIPOutputStream(Files.newOutputStream(out.resolve("fast_lava.json.gz")))) {
            stream.write(new GsonBuilder().serializeNulls().create().toJson(cases).getBytes(StandardCharsets.UTF_8));
        }
    }
}
