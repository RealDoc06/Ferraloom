import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.metadata.pack.PackMetadataSection;
import net.minecraft.server.packs.repository.*;
import net.minecraft.world.flag.FeatureFlags;

/** Read real PathPackResources through Pack#readMetaAndCreate, including fallback. */
public final class MetadataVectors {
    static String pack(String fields) {return "{\"pack\":{\"description\":\"test\","+fields+"}}";}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var inputs=new ArrayList<String>();
        for(String fields:List.of("\"min_format\":121,\"max_format\":121", "\"pack_format\":81", "\"pack_format\":121",
            "\"pack_format\":-1", "\"min_format\":121", "\"max_format\":121", "\"min_format\":122,\"max_format\":121",
            "\"min_format\":121.9,\"max_format\":121.1", "\"min_format\":[121],\"max_format\":[121]",
            "\"min_format\":[121,1],\"max_format\":121", "\"min_format\":[121,0,5],\"max_format\":121",
            "\"min_format\":[121,0,-1],\"max_format\":121", "\"min_format\":[121,0,\"bad\"],\"max_format\":121",
            "\"min_format\":[],\"max_format\":121", "\"min_format\":-1,\"max_format\":121",
            "\"min_format\":121,\"max_format\":121,\"supported_formats\":[121,121]",
            "\"min_format\":121,\"max_format\":121,\"pack_format\":122", "\"pack_format\":15,\"supported_formats\":[15,81]",
            "\"pack_format\":15,\"supported_formats\":{\"min_inclusive\":15,\"max_inclusive\":81}",
            "\"supported_formats\":[15,81]", "\"pack_format\":14,\"supported_formats\":[14,81]",
            "\"pack_format\":15,\"min_format\":15,\"max_format\":121,\"supported_formats\":[15,81]",
            "\"pack_format\":15,\"min_format\":15,\"max_format\":121,\"supported_formats\":[15,121]", "\"min_format\":null,\"max_format\":121")) inputs.add(pack(fields));
        inputs.add("{}");inputs.add("{\"pack\":{}}");inputs.add("{\"pack\":{\"description\":42,\"min_format\":121,\"max_format\":121}}");
        inputs.add("{\"pack\":{\"description\":{\"translate\":\"dataPack.vanilla.description\",\"color\":\"yellow\"},\"min_format\":121,\"max_format\":121}}");
        String base=pack("\"min_format\":121,\"max_format\":121");
        for(String section:List.of("\"features\":{\"enabled\":[]}","\"features\":{\"enabled\":[\"trade_rebalance\"]}",
            "\"features\":{\"enabled\":[\"unknown\"]}","\"features\":{}", "\"features\":{\"enabled\":[1]}",
            "\"overlays\":{}", "\"overlays\":{\"entries\":[]}",
            "\"overlays\":{\"entries\":[{\"directory\":\"a\",\"min_format\":121,\"max_format\":121},{\"directory\":\"b\",\"min_format\":122,\"max_format\":122}]}",
            "\"overlays\":{\"entries\":[{\"directory\":\"ignored\"}]}",
            "\"overlays\":{\"entries\":[{\"directory\":\"../x\",\"min_format\":121,\"max_format\":121}]}",
            "\"overlays\":{\"entries\":[{\"directory\":\"..\",\"min_format\":121,\"max_format\":121}]}",
            "\"overlays\":{\"entries\":[{\"directory\":\"old\",\"formats\":[80,81]},{\"directory\":\"new\",\"min_format\":121,\"max_format\":121}]}")) inputs.add(base.substring(0,base.length()-1)+","+section+"}");
        var directory=Files.createTempDirectory("ferraloom-metadata-");var output=new JsonArray();
        var metadataField=Pack.class.getDeclaredField("metadata");metadataField.setAccessible(true);
        var info=new PackLocationInfo("file/test",Component.literal("test"),PackSource.WORLD,Optional.empty());
        for(String input:inputs) {
            Files.writeString(directory.resolve("pack.mcmeta"),input);
            var row=new JsonObject();row.add("input",JsonParser.parseString(input));
            var loaded=Pack.readMetaAndCreate(info,new PathPackResources.PathResourcesSupplier(directory),PackType.SERVER_DATA,new PackSelectionConfig(false,Pack.Position.TOP,false));
            row.addProperty("loaded",loaded!=null);
            if(loaded!=null) {
                var metadata=(Pack.Metadata)metadataField.get(loaded);
                row.addProperty("compatibility",loaded.getCompatibility().name());
                row.add("description",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,loaded.getDescription()).getOrThrow());
                row.add("features",new Gson().toJsonTree(FeatureFlags.REGISTRY.toNames(loaded.getRequestedFeatures()).stream().map(Object::toString).toList()));
                row.add("overlays",new Gson().toJsonTree(metadata.overlays()));
                try(var resources=loaded.openMetadata()) {
                    try {var range=resources.getMetadataSection(PackMetadataSection.SERVER_TYPE).supportedFormats();row.add("range",new Gson().toJsonTree(List.of(List.of(range.minInclusive().major(),range.minInclusive().minor()),List.of(range.maxInclusive().major(),range.maxInclusive().minor()))));}
                    catch(Exception e){row.addProperty("fallback",true);}
                }
            }
            output.add(row);
        }
        Files.delete(directory.resolve("pack.mcmeta"));Files.delete(directory);
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(output)+"\n");
        System.out.println("Captured "+output.size()+" native pack metadata cases");
    }
}
