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

/** Actual portal setup selection, floor projection and persisted processor settings. */
public final class RuinedPortalGolden {
    static void capture(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var biomes=registries.lookupOrThrow(Registries.BIOME);
        var flat=new FlatLevelSource(net.minecraft.world.level.levelgen.flat.FlatLevelGeneratorSettings.getDefault(biomes,registries.lookupOrThrow(Registries.STRUCTURE_SET),registries.lookupOrThrow(Registries.PLACED_FEATURE)));
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getOrThrow(NoiseGeneratorSettings.OVERWORLD);
        var out=new JsonArray();
        try(var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("portal-world")).createAccess("test")) {
            var templates=new StructureTemplateManager(StructureGolden.RESOURCES,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
            var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,templates);
            for(String name:List.of("ruined_portal","ruined_portal_desert","ruined_portal_jungle","ruined_portal_mountain","ruined_portal_nether","ruined_portal_ocean","ruined_portal_swamp"))for(long sample=0;sample<64;sample++) {
                long seed=sample*341873128712L+132897987541L;
                var source=new FixedBiomeSource(biomes.getOrThrow(sample%2==0?Biomes.PLAINS:Biomes.SNOWY_PLAINS));
                var randomState=RandomState.create(registries.lookupOrThrow(Registries.NOISE),seed,settings.value());
                var chunk=new ChunkPos(sample%3==0?-7:0,sample%3==0?11:0);
                var context=new Structure.GenerationContext(registries,flat,source,randomState.createClimateSampler(SamplerContext.builder().enableCaches().build()),randomState,templates,seed,chunk,LevelHeightAccessor.create(-64,384),b->true);
                var structure=registries.lookupOrThrow(Registries.STRUCTURE).getValue(Identifier.withDefaultNamespace(name));
                var stub=structure.findValidGenerationPoint(context).orElseThrow();
                var pieces=stub.getPiecesBuilder().build();
                var piece=(net.minecraft.world.level.levelgen.structure.structures.RuinedPortalPiece)pieces.pieces().getFirst();
                var tag=piece.createTag(serialization);
                var item=new JsonObject();item.addProperty("id","minecraft:"+name);item.addProperty("seed",seed);item.add("chunk",StructureGolden.ints(chunk.x(),chunk.z()));item.addProperty("biome",sample%2==0?"minecraft:plains":"minecraft:snowy_plains");item.add("position",StructureGolden.ints(stub.position().getX(),stub.position().getY(),stub.position().getZ()));item.addProperty("nbt",tag.toString());item.addProperty("next",context.random().nextInt());
                var loaded=new net.minecraft.world.level.levelgen.structure.structures.RuinedPortalPiece(serialization,tag);item.addProperty("loaded",loaded.createTag(serialization).toString());
                var processors=new JsonArray();var field=TemplateStructurePiece.class.getDeclaredField("placeSettings");field.setAccessible(true);var placement=(StructurePlaceSettings)field.get(piece);for(var processor:placement.getProcessors())processors.add(StructureProcessorType.SINGLE_CODEC.encodeStart(RegistryOps.create(net.minecraft.nbt.NbtOps.INSTANCE,registries),processor).getOrThrow().toString());item.add("processors",processors);out.add(item);
            }
        }
        StructureGolden.write(directory.resolve("ruined_portal_pieces.json.gz"),out);
        terrain(registries,directory);
    }

    static int ground(int mode,int x,int z) {return mode==0?63:60+Math.floorMod(x*3+z*5,8);}
    static net.minecraft.world.level.block.state.BlockState original(int mode,BlockPos p) {
        int y=ground(mode,p.getX(),p.getZ());
        if(p.getY()>y)return net.minecraft.world.level.block.Blocks.AIR.defaultBlockState();
        if(mode==2&&p.getY()==y) return switch(Math.floorMod(p.getX()*7+p.getZ()*11,5)) {
            case 0 -> net.minecraft.world.level.block.Blocks.LAVA.defaultBlockState();
            case 1 -> net.minecraft.world.level.block.Blocks.WATER.defaultBlockState();
            case 2 -> net.minecraft.world.level.block.Blocks.OBSIDIAN.defaultBlockState();
            case 3 -> net.minecraft.world.level.block.Blocks.BEDROCK.defaultBlockState();
            default -> net.minecraft.world.level.block.Blocks.NETHERRACK.defaultBlockState();
        };
        return net.minecraft.world.level.block.Blocks.STONE.defaultBlockState();
    }
    static void terrain(RegistryAccess.Frozen registries,Path directory)throws Exception {
        var out=new JsonArray();
        try(var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("portal-terrain")).createAccess("test")) {
            var templates=new StructureTemplateManager(StructureGolden.RESOURCES,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
            var location=Identifier.withDefaultNamespace("ruined_portal/portal_1");var template=templates.getOrCreate(location);
            var pivot=new BlockPos(template.getSize().getX()/2,0,template.getSize().getZ()/2);
            var type=net.minecraft.world.level.levelgen.structure.structures.RuinedPortalPiece.class;
            var spread=type.getDeclaredMethod("spreadNetherrack",net.minecraft.util.RandomSource.class,LevelAccessor.class);spread.setAccessible(true);
            var drips=type.getDeclaredMethod("addNetherrackDripColumnsBelowPortal",net.minecraft.util.RandomSource.class,LevelAccessor.class);drips.setAccessible(true);
            var leaves=type.getDeclaredMethod("maybeAddLeavesAbove",net.minecraft.util.RandomSource.class,LevelAccessor.class,BlockPos.class);leaves.setAccessible(true);
            var serialization=new StructurePieceSerializationContext(StructureGolden.RESOURCES,registries,templates);
            for(var placement:net.minecraft.world.level.levelgen.structure.structures.RuinedPortalPiece.VerticalPlacement.values())for(boolean cold:new boolean[]{false,true})for(boolean grown:new boolean[]{false,true})for(int mode=0;mode<3;mode++)for(long seed:new long[]{0,8675309}) {
                var properties=new net.minecraft.world.level.levelgen.structure.structures.RuinedPortalPiece.Properties(cold,0.2F,false,grown,false,false);
                var piece=new net.minecraft.world.level.levelgen.structure.structures.RuinedPortalPiece(registries,templates,new BlockPos(0,63,0),placement,properties,location,template,net.minecraft.world.level.block.Rotation.NONE,net.minecraft.world.level.block.Mirror.NONE,pivot);
                var changed=new HashMap<BlockPos,net.minecraft.world.level.block.state.BlockState>();var writes=new JsonArray();int terrain=mode;
                var world=(LevelAccessor)java.lang.reflect.Proxy.newProxyInstance(RuinedPortalGolden.class.getClassLoader(),new Class<?>[]{LevelAccessor.class},(proxy,method,args)->switch(method.getName()) {
                    case "getBlockState" -> changed.getOrDefault((BlockPos)args[0],original(terrain,(BlockPos)args[0]));
                    case "getHeight" -> {int x=(Integer)args[1],z=(Integer)args[2],y=ground(terrain,x,z);yield ((Heightmap.Types)args[0]).isOpaque().test(original(terrain,new BlockPos(x,y,z)))?y+1:y;}
                    case "setBlockAndUpdate" -> {var p=((BlockPos)args[0]).immutable();var state=(net.minecraft.world.level.block.state.BlockState)args[1];changed.put(p,state);var b=new JsonObject();b.add("pos",StructureGolden.ints(p.getX(),p.getY(),p.getZ()));b.addProperty("state",net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.getId(state));writes.add(b);yield true;}
                    default -> throw new UnsupportedOperationException(method.toString());
                });
                var random=new LegacyRandomSource(seed);spread.invoke(piece,random,world);drips.invoke(piece,random,world);
                if(grown) for(var p:BlockPos.betweenClosedStream(piece.getBoundingBox()).map(BlockPos::immutable).toList())leaves.invoke(piece,random,world,p);
                var item=new JsonObject();item.addProperty("nbt",piece.createTag(serialization).toString());item.addProperty("mode",mode);item.addProperty("seed",seed);item.add("writes",writes);item.addProperty("next",random.nextInt());out.add(item);
            }
        }
        StructureGolden.write(directory.resolve("ruined_portal_terrain.json.gz"),out);
    }
    public static void main(String[]args)throws Exception {net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();capture(StructureGolden.load(),Path.of(args[0]));}
}
