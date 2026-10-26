import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.zip.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.resources.Identifier;
import net.minecraft.server.*;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.*;
import net.minecraft.server.packs.resources.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.*;
import net.minecraft.world.level.validation.DirectoryValidator;

/** Exact real repository bootstrap and resource stacks over shared text fixtures. */
public final class RepositoryVectors {
    static final Gson JSON=new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
    static List<String> strings(JsonElement value){var out=new ArrayList<String>();for(var entry:value.getAsJsonArray())out.add(entry.getAsString());return out;}
    static JsonArray features(net.minecraft.world.flag.FeatureFlagSet flags){return JSON.toJsonTree(FeatureFlags.REGISTRY.toNames(flags).stream().map(Object::toString).toList()).getAsJsonArray();}
    static String text(Resource resource)throws Exception{try(var input=resource.open()){return new String(input.readAllBytes(),StandardCharsets.UTF_8);}}
    public static void main(String[] args)throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var input=JsonParser.parseString(Files.readString(Path.of(args[0]))).getAsJsonObject();var folder=Files.createTempDirectory("ferraloom-repository-");
        for(var value:input.getAsJsonArray("packs")) {
            var pack=value.getAsJsonObject();var path=folder.resolve(pack.get("name").getAsString());var kind=pack.get("kind").getAsString();
            if(kind.equals("symlink")){Files.createSymbolicLink(path,folder.resolve(pack.get("target").getAsString()));continue;}
            if(kind.equals("ignored")){Files.writeString(path,"ignored");continue;}
            if(kind.equals("zip")) {
                try(var zip=new ZipOutputStream(Files.newOutputStream(path))) {
                    zip.putNextEntry(new ZipEntry("pack.mcmeta"));zip.write(JSON.toJson(pack.get("metadata")).getBytes(StandardCharsets.UTF_8));zip.closeEntry();
                    for(var entry:pack.getAsJsonObject("files").entrySet()){zip.putNextEntry(new ZipEntry(entry.getKey()));zip.write(entry.getValue().getAsString().getBytes(StandardCharsets.UTF_8));zip.closeEntry();}
                }
            } else {
                Files.createDirectories(path);Files.writeString(path.resolve("pack.mcmeta"),JSON.toJson(pack.get("metadata")));
                for(var entry:pack.getAsJsonObject("files").entrySet()){var file=path.resolve(entry.getKey());Files.createDirectories(file.getParent());Files.writeString(file,entry.getValue().getAsString());}
                if(pack.has("nested_link"))Files.createSymbolicLink(path.resolve("linked"),folder.resolve(pack.get("nested_link").getAsString()));
            }
        }
        var output=new JsonArray();var metadataField=Pack.class.getDeclaredField("metadata");metadataField.setAccessible(true);
        for(var value:input.getAsJsonArray("cases")) {
            var test=value.getAsJsonObject();var repository=ServerPacksSource.createPackRepository(folder,new DirectoryValidator(path->false));
            var initial=new WorldDataConfiguration(new DataPackConfig(strings(test.get("enabled")),strings(test.get("disabled"))),FeatureFlags.REGISTRY.fromNames(strings(test.get("features")).stream().map(Identifier::parse).toList()));
            var config=MinecraftServer.configurePackRepository(repository,initial,test.get("init").getAsBoolean(),test.get("safe").getAsBoolean());
            var row=new JsonObject();row.addProperty("name",test.get("name").getAsString());var available=new JsonArray();
            for(var pack:repository.getAvailablePacks()) {
                var entry=new JsonObject();entry.addProperty("id",pack.getId());entry.addProperty("source",pack.getPackSource()==PackSource.FEATURE?"Feature":pack.getPackSource()==PackSource.WORLD?"World":"BuiltIn");
                entry.addProperty("required",pack.isRequired());entry.addProperty("fixed",pack.isFixedPosition());entry.addProperty("position",pack.getDefaultPosition().name());entry.addProperty("compatibility",pack.getCompatibility().name());
                entry.add("features",features(pack.getRequestedFeatures()));entry.add("description",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,pack.getDescription()).getOrThrow());
                entry.add("overlays",JSON.toJsonTree(((Pack.Metadata)metadataField.get(pack)).overlays()));available.add(entry);
            }
            row.add("available",available);row.add("selected",JSON.toJsonTree(repository.getSelectedIds()));row.add("features",features(config.enabledFeatures()));
            row.addProperty("encoded",WorldDataConfiguration.CODEC.encodeStart(NbtOps.INSTANCE,config).getOrThrow().toString());
            try(var resources=new MultiPackResourceManager(PackType.SERVER_DATA,repository.openAllSelected())) {
                var id=Identifier.parse("test:function/ordered.mcfunction");var resource=resources.getResource(id);if(resource.isPresent())row.add("resource",JSON.toJsonTree(List.of(resource.get().sourcePackId(),text(resource.get()))));else row.add("resource",JsonNull.INSTANCE);
                var stack=new JsonArray();for(var item:resources.getResourceStack(id))stack.add(JSON.toJsonTree(List.of(item.sourcePackId(),text(item))));row.add("stack",stack);
                var listed=new JsonArray();for(var entry:resources.listResources("function",key->true).entrySet())listed.add(JSON.toJsonTree(List.of(entry.getKey().toString(),entry.getValue().sourcePackId(),text(entry.getValue()))));row.add("listed",listed);
            }
            output.add(row);
        }
        try(var paths=Files.walk(folder)){for(var path:paths.sorted(Comparator.reverseOrder()).toList())Files.delete(path);}
        Files.writeString(Path.of(args[1]),JSON.toJson(output)+"\n");System.out.println("Captured "+output.size()+" real repository bootstrap snapshots");
    }
}
