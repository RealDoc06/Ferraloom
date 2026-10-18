import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.densityfunction.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;

/** Oversized authored datapack templates with asymmetric base-height queries. */
public final class ShipwreckHeightGolden {
    static final class Terrain extends FlatLevelSource {
        final JsonArray queries=new JsonArray();
        Terrain(net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings settings){super(settings);}
        @Override public int getBaseHeight(int x,int z,Heightmap.Types type,LevelHeightAccessor accessor,RandomState state){queries.add(StructureGolden.ints(x,z));return 64+Math.floorMod(x*3+z*5,32);}
    }
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var biomes=registries.lookupOrThrow(Registries.BIOME);var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD);
        var generator=new Terrain(net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings.getDefault(biomes,registries.lookupOrThrow(Registries.STRUCTURE_SET),registries.lookupOrThrow(Registries.PLACED_FEATURE)));
        var out=new JsonArray();
        try(var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("shipwreck-height-world")).createAccess("test")) {
            var templates=new StructureTemplateManager(StructureGolden.RESOURCES,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
            var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,templates);
            // Override every selectable shipwreck before querying its generation.
            var tag=net.minecraft.nbt.TagParser.parseCompoundFully("{size:[35,7,11],palette:[],blocks:[],entities:[]}");
            StructureGolden.RESOURCES.listResources("structure/shipwreck",r->r.getPath().endsWith(".nbt")).keySet().forEach(id->templates.getOrCreate(Identifier.fromNamespaceAndPath(id.getNamespace(),id.getPath().substring(10,id.getPath().length()-4))).load(BuiltInRegistries.BLOCK,tag));
            for(String name:List.of("shipwreck","shipwreck_beached"))for(long seed:new long[]{0,-1,8675309})for(int[] p:new int[][]{{0,0},{-7,11}}) {
                generator.queries.asList().clear();var selected=registries.lookupOrThrow(Registries.STRUCTURE).getOrThrow(ResourceKey.create(Registries.STRUCTURE,Identifier.withDefaultNamespace(name)));var state=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings.value());
                var context=new Structure.GenerationContext(registries,generator,generator.getBiomeSource(),state.createClimateSampler(SamplerContext.builder().enableCaches().build()),state,templates,seed,new ChunkPos(p[0],p[1]),LevelHeightAccessor.create(-64,384),b->true);
                var stub=selected.value().findValidGenerationPoint(context).orElseThrow();var pieces=stub.getPiecesBuilder().build();
                var item=new JsonObject();item.addProperty("id","minecraft:"+name);item.addProperty("seed",seed);item.add("chunk",StructureGolden.ints(p));item.add("queries",generator.queries.deepCopy());item.addProperty("nbt",new StructureStart(selected.value(),new ChunkPos(p[0],p[1]),7,pieces).createTag(serialization,new ChunkPos(p[0],p[1])).toString());out.add(item);
            }
        }
        StructureGolden.write(directory.resolve("shipwreck_heights.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
