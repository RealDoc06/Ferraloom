import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.densityfunction.*;
import net.minecraft.world.level.levelgen.structure.*;
import net.minecraft.world.level.levelgen.structure.pieces.*;
import net.minecraft.world.level.levelgen.structure.templatesystem.*;

/** Real stronghold retry generation, imposed crossing and pending-child order. */
public final class StrongholdGolden {
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var biomes=registries.lookupOrThrow(Registries.BIOME);var source=new FixedBiomeSource(biomes.getOrThrow(Biomes.PLAINS));
        var flat=new FlatLevelSource(net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings.getDefault(biomes,registries.lookupOrThrow(Registries.STRUCTURE_SET),registries.lookupOrThrow(Registries.PLACED_FEATURE)));
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD);
        var out=new JsonArray();
        try(var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("stronghold-world")).createAccess("test")) {
            var templates=new StructureTemplateManager(StructureGolden.RESOURCES,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
            var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,templates);
            for(String name:List.of("stronghold"))for(long sample=0;sample<64;sample++)for(int[] p:new int[][]{{0,0},{-7,11}}) {
                long seed=sample*341873128712L+132897987541L;
                var randomState=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings.value());
                var chunk=new ChunkPos(p[0],p[1]);
                var context=new Structure.GenerationContext(registries,flat,source,randomState.createClimateSampler(SamplerContext.builder().enableCaches().build()),randomState,templates,seed,chunk,LevelHeightAccessor.create(-64,384),b->true);
                var structure=registries.lookupOrThrow(Registries.STRUCTURE).getValue(Identifier.withDefaultNamespace(name));
                var stub=structure.findValidGenerationPoint(context).orElseThrow();var pieces=stub.getPiecesBuilder().build();var tag=pieces.save(serialization);
                var item=new JsonObject();item.addProperty("id","minecraft:"+name);item.addProperty("seed",seed);item.add("chunk",StructureGolden.ints(p));item.add("position",StructureGolden.ints(stub.position().getX(),stub.position().getY(),stub.position().getZ()));item.addProperty("nbt",tag.toString());item.addProperty("next",context.random().nextInt());item.addProperty("loaded",PiecesContainer.load((net.minecraft.nbt.ListTag)tag,serialization).save(serialization).toString());out.add(item);
            }
        }
        StructureGolden.write(directory.resolve("stronghold_pieces.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
