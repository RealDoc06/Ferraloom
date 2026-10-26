import java.io.*;
import java.nio.file.*;
import java.util.*;
import java.util.stream.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.packs.PackResources;
import net.minecraft.server.packs.resources.*;
import net.minecraft.util.datafix.DataFixers;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager;
import net.minecraft.world.level.storage.*;

/** Observation-only cache/source vectors. Disposable files, no server or ticks. */
public final class TemplateManagerVectors {
    static Identifier id(String name) { return Identifier.parse("fixture:"+name); }
    static byte[] bytes(int size)throws Exception {
        var tag=TagParser.parseCompoundFully("{DataVersion:5023,size:["+size+",1,1],palette:[],blocks:[],entities:[]}");
        var out=new ByteArrayOutputStream();NbtIo.writeCompressed(tag,out);return out.toByteArray();
    }
    static final class Resources implements ResourceManager {
        final Map<Identifier,byte[]> files=new TreeMap<>();
        Resources(int size)throws Exception {for(var entry:Map.of("shared",1,"fallback",8,"resource",size).entrySet()) files.put(id("structure/"+entry.getKey()+".nbt"),bytes(entry.getValue()));}
        Resource resource(Identifier id){return new Resource(null,()->new ByteArrayInputStream(files.get(id)));}
        public Set<String> getNamespaces(){return Set.of("fixture");}
        public Optional<Resource> getResource(Identifier id){return files.containsKey(id)?Optional.of(resource(id)):Optional.empty();}
        public List<Resource> getResourceStack(Identifier id){return getResource(id).stream().toList();}
        public Map<Identifier,Resource> listResources(String directory,ResourceManager.Selector selector){var result=new TreeMap<Identifier,Resource>();for(var id:files.keySet())if(id.getPath().startsWith(directory+"/")&&selector.isIncluded(id))result.put(id,resource(id));return result;}
        public Map<Identifier,List<Resource>> listResourceStacks(String directory,ResourceManager.Selector selector){var result=new TreeMap<Identifier,List<Resource>>();listResources(directory,selector).forEach((id,r)->result.put(id,List.of(r)));return result;}
        public Stream<PackResources> listPacks(){return Stream.empty();}
    }
    static void write(Path world,String name,byte[] bytes)throws Exception {var file=world.resolve("generated/fixture/structure/"+name+".nbt");Files.createDirectories(file.getParent());Files.write(file,bytes);}
    static final List<String> rows=new ArrayList<>();
    static void record(String name,int value){rows.add(name+"\t"+value);}
    public static void main(String[] args)throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        try(var storage=LevelStorageSource.createDefault(Path.of(args[0])).createAccess("world")) {
            var world=storage.getLevelPath(LevelResource.ROOT);write(world,"shared",bytes(2));
            var resources=new Resources(7);var manager=new StructureTemplateManager(resources,storage,DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
            var first=manager.get(id("shared")).orElseThrow();record("world_wins",first.getSize().getX());
            write(world,"shared",bytes(3));var cached=manager.get(id("shared")).orElseThrow();record("cached_hit",cached.getSize().getX());record("cached_identity",first==cached?1:0);
            manager.remove(id("shared"));record("after_remove",manager.get(id("shared")).orElseThrow().getSize().getX());
            record("missing",manager.get(id("future")).isEmpty()?-1:0);write(world,"future",bytes(4));record("cached_miss",manager.get(id("future")).isEmpty()?-1:0);
            record("listed_despite_miss",manager.listTemplates().anyMatch(id("future")::equals)?1:0);manager.remove(id("future"));record("miss_after_remove",manager.get(id("future")).orElseThrow().getSize().getX());
            var created=manager.getOrCreate(id("created"));record("created_empty",created.getSize().getX());record("created_identity",created==manager.getOrCreate(id("created"))?1:0);
            record("created_not_listed",manager.listTemplates().noneMatch(id("created")::equals)?1:0);write(world,"created",bytes(5));record("created_cached",manager.get(id("created")).orElseThrow().getSize().getX());manager.remove(id("created"));record("created_after_remove",manager.get(id("created")).orElseThrow().getSize().getX());
            write(world,"fallback","invalid gzip".getBytes());record("malformed_world_falls_through",manager.get(id("fallback")).orElseThrow().getSize().getX());
            record("resource",manager.get(id("resource")).orElseThrow().getSize().getX());if(manager.get(id("reload_future")).isPresent())throw new AssertionError();write(world,"reload_future",bytes(6));manager.onResourceManagerReload(new Resources(10));record("resource_after_reload",manager.get(id("resource")).orElseThrow().getSize().getX());record("world_after_reload",manager.get(id("shared")).orElseThrow().getSize().getX());record("miss_after_reload",manager.get(id("reload_future")).orElseThrow().getSize().getX());
            var listing=manager.listTemplates().toList();record("listing_distinct",listing.size()==new HashSet<>(listing).size()?1:0);record("generated_before_resources",listing.indexOf(id("resource"))==5?1:0);
        }
        for(var row:rows)Bootstrap.realStdoutPrintln("TEMPLATE_CACHE_VECTOR\t"+row);
        System.exit(0);
    }
}
