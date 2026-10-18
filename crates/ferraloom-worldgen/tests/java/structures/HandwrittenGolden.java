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
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;

/** Authored structure generation and bounded postProcess scenarios. */
public final class HandwrittenGolden {
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var biomes=registries.lookupOrThrow(Registries.BIOME);
        var source=new FixedBiomeSource(biomes.getOrThrow(Biomes.PLAINS));
        var generator=new FlatLevelSource(net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings.getDefault(biomes,registries.lookupOrThrow(Registries.STRUCTURE_SET),registries.lookupOrThrow(Registries.PLACED_FEATURE)));
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD);
        var out=new JsonArray();
        try(var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("handwritten-world")).createAccess("test")) {
            var templates=new StructureTemplateManager(StructureGolden.RESOURCES,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
            var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,templates);
            for(String name:List.of("buried_treasure","village_plains","trial_chambers","ancient_city"))for(long seed:new long[]{0,-1,8675309})for(int[] p:new int[][]{{0,0},{-7,11}})for(boolean accept:new boolean[]{true,false}) {
                var selected=registries.lookupOrThrow(Registries.STRUCTURE).getOrThrow(ResourceKey.create(Registries.STRUCTURE,Identifier.withDefaultNamespace(name)));
                var randomState=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings.value());
                var start=selected.value().generate(selected,Level.OVERWORLD,registries,generator,source,randomState.createClimateSampler(SamplerContext.builder().enableCaches().build()),randomState,templates,seed,new ChunkPos(p[0],p[1]),7,LevelHeightAccessor.create(-64,384),b->accept);
                var item=new JsonObject();item.addProperty("id","minecraft:"+name);item.addProperty("seed",seed);item.add("chunk",StructureGolden.ints(p));item.addProperty("accept",accept);item.addProperty("nbt",start.createTag(serialization,new ChunkPos(p[0],p[1])).toString());if(start.isValid())item.add("bounds",StructureGolden.bounds(start.getBoundingBox()));out.add(item);
            }
            StructureGolden.write(directory.resolve("structure_generation.json.gz"),out);
        }
        var cases=new JsonArray();
        for(int mode=0;mode<8;mode++)for(long seed:new long[]{0,-1,8675309}) {
            int scenario=mode;var blocks=new HashMap<BlockPos,BlockState>();
            // Candidate y=23 sits above a recognized base at22; mode0 has no base.
            for(int x=7;x<=11;x++)for(int z=7;z<=11;z++)for(int y=20;y<=26;y++) {
                var block=y<=22?(scenario==0?Blocks.DEEPSLATE:Blocks.STONE):Blocks.AIR;
                blocks.put(new BlockPos(x,y,z),block.defaultBlockState());
            }
            if(mode==1)blocks.put(new BlockPos(9,23,9),Blocks.CHEST.defaultBlockState());
            if(mode==2)blocks.put(new BlockPos(9,23,9),Blocks.GRAVEL.defaultBlockState());
            if(mode>=3) {blocks.put(new BlockPos(8,23,9),Blocks.WATER.defaultBlockState());blocks.put(new BlockPos(10,23,9),Blocks.LAVA.defaultBlockState());blocks.put(new BlockPos(9,23,8),Blocks.OAK_SLAB.defaultBlockState());blocks.put(new BlockPos(8,22,9),Blocks.AIR.defaultBlockState());}
            if(mode==6)blocks.put(new BlockPos(9,24,9),Blocks.STONE.defaultBlockState());
            if(mode==7)blocks.put(new BlockPos(9,23,9),Blocks.WATER.defaultBlockState());
            var writes=new JsonArray();var initial=new JsonArray();blocks.entrySet().stream().sorted(Comparator.comparingInt((Map.Entry<BlockPos,BlockState>e)->e.getKey().getX()).thenComparingInt(e->e.getKey().getY()).thenComparingInt(e->e.getKey().getZ())).forEach(e->{var b=new JsonObject();var p=e.getKey();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(e.getValue()));initial.add(b);});
            var world=(WorldGenLevel)java.lang.reflect.Proxy.newProxyInstance(HandwrittenGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},(proxy,method,args)->{
                return switch(method.getName()) {
                    case "getHeight" -> 26;
                    case "getMinY" -> 20;
                    case "getBlockState" -> blocks.getOrDefault((BlockPos)args[0],Blocks.AIR.defaultBlockState());
                    case "setBlock", "setBlockAndUpdate" -> {var p=((BlockPos)args[0]).immutable();var state=(BlockState)args[1];blocks.put(p,state);var b=new JsonObject();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",Block.BLOCK_STATE_REGISTRY.getId(state));b.addProperty("flags",method.getName().equals("setBlockAndUpdate")?3:(Integer)args[2]);writes.add(b);yield true;}
                    case "getBlockEntity" -> null; // no Java chest allocation in this fixture
                    default -> throw new UnsupportedOperationException(method.toString());
                };
            });
            var piece=new net.minecraft.world.level.levelgen.structure.structures.BuriedTreasurePieces.BuriedTreasurePiece(new BlockPos(9,90,9));var random=new LegacyRandomSource(seed);
            var bounds=(mode==0||mode==1)?new BoundingBox(0,20,0,15,30,15):new BoundingBox(0,24,0,15,30,15);
            piece.postProcess(world,null,null,random,bounds,new ChunkPos(0,0),BlockPos.ZERO);
            var item=new JsonObject();item.addProperty("mode",mode);item.addProperty("seed",seed);item.add("initial",initial);item.add("bounds",StructureGolden.bounds(bounds));item.add("writes",writes);item.addProperty("nbt",piece.createTag(new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,null)).toString());item.addProperty("next",random.nextInt());cases.add(item);
        }
        StructureGolden.write(directory.resolve("buried_treasure.json.gz"),cases);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
