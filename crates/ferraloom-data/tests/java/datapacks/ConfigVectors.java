import com.google.gson.*;
import com.mojang.serialization.Dynamic;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.dedicated.DedicatedServerProperties;
import net.minecraft.world.flag.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.storage.LevelStorageSource;

/** Actual startup config decode, registry feature order and property splitting. */
public final class ConfigVectors {
    static final Gson JSON=new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
    static JsonObject describe(WorldDataConfiguration config) {
        var out=new JsonObject();out.add("enabled",JSON.toJsonTree(config.dataPacks().getEnabled()));
        out.add("disabled",JSON.toJsonTree(config.dataPacks().getDisabled()));
        out.add("features",JSON.toJsonTree(FeatureFlags.REGISTRY.toNames(config.enabledFeatures()).stream().map(Object::toString).toList()));
        out.addProperty("encoded",WorldDataConfiguration.CODEC.encodeStart(NbtOps.INSTANCE,config).getOrThrow().toString());
        return out;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var decoded=new JsonArray();
        for(String input:List.of("{}","{DataPacks:{Enabled:[vanilla],Disabled:[]}}",
            "{DataPacks:{Enabled:[],Disabled:[]}}","{DataPacks:{Enabled:[a,a,b],Disabled:[c,c]}}",
            "{DataPacks:{Enabled:[a]}}","{DataPacks:{Enabled:[1],Disabled:[]}}","{DataPacks:42}",
            "{enabled_features:[]}","{enabled_features:[vanilla]}","{enabled_features:[trade_rebalance]}",
            "{enabled_features:[vanilla,minecart_improvements,redstone_experiments,trade_rebalance]}",
            "{enabled_features:[unknown]}","{enabled_features:[trade_rebalance,unknown]}","{enabled_features:[1]}",
            "{enabled_features:1}","{enabled_features:[I;]}","{enabled_features:[B;]}","{enabled_features:[L;]}",
            "{DataPacks:{Enabled:[I;],Disabled:[B;]}}","{enabled_features:[\"BAD\"]}",
            "{DataPacks:{Enabled:[foo],Disabled:[]},enabled_features:[unknown]}",
            "{DataPacks:{Enabled:[a],Disabled:[1]},enabled_features:[trade_rebalance]}")) {
            var config=LevelStorageSource.readDataConfig(new Dynamic<Tag>(NbtOps.INSTANCE,TagParser.parseCompoundFully(input)));
            var row=describe(config);row.addProperty("input",input);decoded.add(row);
        }
        var properties=new JsonArray();var method=DedicatedServerProperties.class.getDeclaredMethod("getDatapackConfig",String.class,String.class);method.setAccessible(true);
        for(var pair:List.of(List.of("vanilla",""),List.of("",""),List.of(" a , b, ,c,",",x,,"),
            List.of("vanilla,file/a.zip","file/b"),List.of("\u00a0a\u0085,\t b\n", " x "),List.of("a\u001c", "\u2003b\u2003"))) {
            var config=(DataPackConfig)method.invoke(null,pair.get(0),pair.get(1));
            var row=new JsonObject();row.add("input",JSON.toJsonTree(pair));row.add("enabled",JSON.toJsonTree(config.getEnabled()));row.add("disabled",JSON.toJsonTree(config.getDisabled()));properties.add(row);
        }
        var flags=new JsonArray();var names=List.of("vanilla","trade_rebalance","redstone_experiments","minecart_improvements");
        for(int bits=0;bits<16;bits++) {
            var input=new ArrayList<String>();for(int i=0;i<4;i++)if((bits&(1<<i))!=0)input.add(names.get(i));
            var set=FeatureFlags.REGISTRY.fromNames(input.stream().map(net.minecraft.resources.Identifier::withDefaultNamespace).toList());
            var row=describe(new WorldDataConfiguration(DataPackConfig.DEFAULT,set));row.add("input",JSON.toJsonTree(input));flags.add(row);
        }
        var root=new JsonObject();root.add("decode",decoded);root.add("properties",properties);root.add("flags",flags);
        Files.writeString(Path.of(args[0]),JSON.toJson(root)+"\n");
        System.out.println("Captured config="+decoded.size()+", properties="+properties.size()+", feature sets="+flags.size());
    }
}
