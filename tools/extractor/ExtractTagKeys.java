import com.google.gson.*;
import java.lang.reflect.Modifier;
import java.nio.file.*;
import java.util.*;
import java.util.jar.JarFile;
import net.minecraft.tags.TagKey;

/** Extracts typed tag-key names and their registry identities, never tag contents. */
public final class ExtractTagKeys {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        var registries=new TreeMap<String,JsonObject>();
        try(var jar=new JarFile(args[0])) {
            for(var entry:Collections.list(jar.entries())) {
                String path=entry.getName();
                if(!path.startsWith("net/minecraft/tags/") || !path.endsWith("Tags.class") || path.contains("$"))continue;
                Class<?> type=Class.forName(path.substring(0,path.length()-6).replace('/','.'));
                for(var field:type.getDeclaredFields()) {
                    if(!Modifier.isStatic(field.getModifiers()) || field.getType()!=TagKey.class)continue;
                    field.setAccessible(true);var key=(TagKey<?>)field.get(null);String registry=key.registry().identifier().toString();
                    var group=registries.computeIfAbsent(registry,id->{var object=new JsonObject();object.addProperty("class",type.getSimpleName());object.add("keys",new JsonObject());return object;});
                    group.getAsJsonObject("keys").addProperty(field.getName(),key.location().toString());
                }
            }
            // Include built-in datapack tags that have no Java constant too.
            for(var entry:Collections.list(jar.entries())) {
                String path=entry.getName();if(!path.startsWith("data/minecraft/tags/") || !path.endsWith(".json"))continue;
                String relative=path.substring("data/minecraft/tags/".length(),path.length()-5);
                String registry=registries.keySet().stream().filter(id->relative.startsWith(id.substring("minecraft:".length())+"/")).max(Comparator.comparingInt(String::length)).orElseThrow(()->new AssertionError("unmapped tag registry "+relative));
                String name="minecraft:"+relative.substring(registry.length()-"minecraft:".length()+1);
                var keys=registries.get(registry).getAsJsonObject("keys");
                boolean present=keys.entrySet().stream().anyMatch(e->e.getValue().getAsString().equals(name));
                if(!present)keys.addProperty(name.substring("minecraft:".length()).replaceAll("[^a-zA-Z0-9]","_").toUpperCase(Locale.ROOT),name);
            }
        }
        var root=new JsonObject();registries.forEach((name,group)->root.add(name,group));
        Files.writeString(Path.of(args[1]),new GsonBuilder().setPrettyPrinting().create().toJson(root)+"\n");
    }
}
