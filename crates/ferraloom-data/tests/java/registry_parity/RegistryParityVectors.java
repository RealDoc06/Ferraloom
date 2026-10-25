import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import java.util.stream.Stream;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.particles.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.resources.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.item.JukeboxSong;

/** Actual 26.3 registry iteration, option codecs, world-registry codecs and
 * TagLoader bindings. The optional profile uses vanilla's own pack repository. */
public final class RegistryParityVectors {
    private static final Gson JSON = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().serializeNulls().create();
    private static void write(Path output,String name,JsonElement value)throws Exception{
        Files.writeString(output.resolve(name+".json"),JSON.toJson(value)+"\n");
    }
    private static void sounds(Path output)throws Exception{
        var entries=new JsonArray();
        for(var sound:BuiltInRegistries.SOUND_EVENT){
            var entry=new JsonObject();entry.addProperty("name",BuiltInRegistries.SOUND_EVENT.getKey(sound).toString());entry.addProperty("id",BuiltInRegistries.SOUND_EVENT.getId(sound));
            entry.add("fixed_range_bits",sound.fixedRange().<JsonElement>map(value->new JsonPrimitive(Integer.toUnsignedLong(Float.floatToRawIntBits(value)))).orElse(JsonNull.INSTANCE));
            entries.add(entry);
        }
        write(output,"sounds",entries);
    }
    private static void particles(Path output)throws Exception{
        var registries=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var ops=registries.createSerializationContext(JsonOps.INSTANCE);
        var options=JsonParser.parseString("""
            {"water_blocks":3,"burst_impulse_base":0.25,"power":0.75,
             "color":3368601,"scale":1.25,"from_color":3368601,"to_color":6719692,
             "roll":0.25,"block_state":"minecraft:stone","item":{"id":"minecraft:stone","count":2},
             "destination":{"type":"minecraft:block","pos":[1,2,3]},"arrival_in_ticks":5,
             "target":[1.5,2.5,3.5],"duration":7,"delay":2}
            """);
        var entries=new JsonArray();
        for(var type:BuiltInRegistries.PARTICLE_TYPE){
            var particle=(ParticleOptions)type.codec().codec().parse(ops,options).getOrThrow();
            var entry=new JsonObject();entry.addProperty("name",BuiltInRegistries.PARTICLE_TYPE.getKey(type).toString());entry.addProperty("id",BuiltInRegistries.PARTICLE_TYPE.getId(type));
            // Observed only. This server-only port intentionally does not model
            // the client limiter flag, which has no caller in the server jar.
            entry.addProperty("override_limiter",type.getOverrideLimiter());
            entry.addProperty("codec_kind",particle.getClass().getSimpleName());
            var buffer=new RegistryFriendlyByteBuf(Unpooled.buffer(),registries);
            try{
                ParticleTypes.STREAM_CODEC.encode(buffer,particle);
                byte[] bytes=new byte[buffer.readableBytes()];buffer.readBytes(bytes);
                entry.addProperty("wire",HexFormat.of().formatHex(bytes));
            }finally{buffer.release();}
            entries.add(entry);
        }
        write(output,"particles",entries);
    }
    private static List<PackResources> packs(boolean trade){
        var repository=ServerPacksSource.createVanillaTrustedRepository();repository.reload();
        repository.setSelected(trade?List.of("vanilla","trade_rebalance"):List.of("vanilla"));
        if(trade&&repository.getSelectedPacks().stream().noneMatch(pack->pack.getId().equals("trade_rebalance")))throw new AssertionError("Optional trade pack not selected");
        return repository.openAllSelected();
    }
    private static RegistryAccess.Frozen load(ResourceManager resources){
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        TagLoader.loadTagsForExistingRegistries(resources,builtins).forEach(Registry.PendingTags::apply);
        var world=RegistryDataLoader.load(resources,builtins.listRegistries().toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
        return new RegistryAccess.ImmutableRegistryAccess(Stream.concat(builtins.registries(),world.registries())).freeze();
    }
    private static <T> JsonObject registryTags(Registry<T> registry){
        var tags=new JsonObject();
        registry.getTags().sorted(Comparator.comparing(tag->tag.key().location())).forEach(tag->{
            var values=new JsonArray();tag.stream().forEach(holder->values.add(registry.getKey(holder.value()).toString()));
            tags.add(tag.key().location().toString(),values);
        });
        return tags;
    }
    private static JsonObject tags(RegistryAccess registries){
        var groups=new JsonObject();
        registries.registries().sorted(Comparator.comparing(entry->entry.key().identifier())).forEach(entry->{
            var tags=registryTags(entry.value());if(!tags.isEmpty())groups.add(entry.key().identifier().toString(),tags);
        });
        return groups;
    }
    private static void jukebox(Path output,RegistryAccess registries)throws Exception{
        var registry=registries.lookupOrThrow(Registries.JUKEBOX_SONG);
        var entries=new JsonArray();
        for(var song:registry){
            var entry=new JsonObject();entry.addProperty("name",registry.getKey(song).toString());entry.addProperty("id",registry.getId(song));
            entry.add("entry",JukeboxSong.DIRECT_CODEC.encodeStart(registries.createSerializationContext(JsonOps.INSTANCE),song).getOrThrow());entries.add(entry);
        }
        write(output,"jukebox",entries);
    }
    public static void main(String[] args)throws Exception{
        var output=Path.of(args[0]);Files.createDirectories(output);
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();sounds(output);particles(output);
        var profiles=new JsonObject();
        for(boolean trade:new boolean[]{false,true}){
            try(var resources=new MultiPackResourceManager(PackType.SERVER_DATA,packs(trade))){
                var registries=load(resources);
                if(!trade)jukebox(output,registries);
                var profile=tags(registries);profiles.add(trade?"trade_rebalance":"vanilla",profile);
                int count=profile.entrySet().stream().mapToInt(entry->entry.getValue().getAsJsonObject().size()).sum();
                System.out.println("NATIVE tags "+(trade?"trade_rebalance":"vanilla")+" "+count);
            }
        }
        write(output,"tags",profiles);
        System.out.println("NATIVE sounds "+BuiltInRegistries.SOUND_EVENT.size()+", particles "+BuiltInRegistries.PARTICLE_TYPE.size());
    }
}
