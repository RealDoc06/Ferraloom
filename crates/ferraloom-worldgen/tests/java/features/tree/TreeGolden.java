import com.google.gson.*;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.feature.*;

/** Calls the actual configured features against a mutable, bounded test level. */
public final class TreeGolden implements InvocationHandler {
    final RegistryAccess.Frozen registries;
    final BlockPos origin;
    final String terrain;
    final BlockState ground;
    final Map<BlockPos,BlockState> blocks=new HashMap<>();
    final Map<BlockPos,BlockEntity> entities=new HashMap<>();
    final Map<ChunkPos,ProtoChunk> chunks=new HashMap<>();
    static net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager templateManager;
    final net.minecraft.server.level.ServerLevel serverLevel;
    JsonArray beforeEdge,beforeBees,beforeDestroy,beforeExternal;
    Long beforeEdgeNext,beforeBeesNext,beforeDestroyNext,beforeExternalNext;
    XoroshiroRandomSource featureSource;
    long peekNext() {
        var state=XoroshiroRandomSource.CODEC.encodeStart(com.mojang.serialization.JsonOps.INSTANCE,featureSource).getOrThrow();
        var copy=XoroshiroRandomSource.CODEC.parse(com.mojang.serialization.JsonOps.INSTANCE,state).getOrThrow();
        return new WorldgenRandom(copy).nextLong();
    }
    final RandomSource levelRandom=new LegacyRandomSource(123);
    TreeGolden(RegistryAccess.Frozen registries,BlockPos origin,String terrain,BlockState ground,ChunkGenerator generator) {
        this.registries=registries;this.origin=origin;this.terrain=terrain;this.ground=ground;
        // A concrete accessor is necessary only for PaleMossDecorator's generator lookup.
        org.mockito.stubbing.Answer<Object> forbidden = call -> {throw new UnsupportedOperationException("Unapproved server mock call: " + call.getMethod());};
        serverLevel=org.mockito.Mockito.mock(net.minecraft.server.level.ServerLevel.class, forbidden);
        var chunks=org.mockito.Mockito.mock(net.minecraft.server.level.ServerChunkCache.class, forbidden);
        org.mockito.Mockito.doReturn(chunks).when(serverLevel).getChunkSource();
        org.mockito.Mockito.doReturn(generator).when(chunks).getGenerator();
        var server=org.mockito.Mockito.mock(net.minecraft.server.MinecraftServer.class, forbidden);
        org.mockito.Mockito.doReturn(server).when(serverLevel).getServer();
        org.mockito.Mockito.doReturn(templateManager).when(server).getStructureTemplateManager();
    }
    BlockState initial(BlockPos p) {
        if(p.getY() < -64 || p.getY() >= 320)return Blocks.VOID_AIR.defaultBlockState();
        int dx=p.getX()-origin.getX(),dy=p.getY()-origin.getY(),dz=p.getZ()-origin.getZ();
        int slope=terrain.equals("slope") ? Math.floorDiv(dx,3) : 0;
        if(dy < slope)return dy==slope-1 ? ground : Blocks.STONE.defaultBlockState();
        if(terrain.equals("cave") && dy>=4 && dy<=7)return dy==7 ? ground : Blocks.STONE.defaultBlockState();
        if(terrain.equals("water") && dy<3)return Blocks.WATER.defaultBlockState();
        if(terrain.equals("blocked") && dy >= 1 && dy <= 30 && Math.abs(dx)<=12 && Math.abs(dz)<=12)return Blocks.STONE.defaultBlockState();
        if(terrain.equals("partial") && dy >= 3 && dy <= 8 && dx>=1 && dx<=3 && dz>=-2 && dz<=1)return Blocks.STONE.defaultBlockState();
        return Blocks.AIR.defaultBlockState();
    }
    BlockState block(BlockPos p) { return blocks.getOrDefault(p,initial(p)); }
    boolean set(BlockPos p,BlockState s) {
        p=p.immutable();
        if(p.getY() < -64 || p.getY() >= 320)return false;
        blocks.put(p,s);
        if(s.getBlock() instanceof EntityBlock eb) {
            BlockEntity old=entities.get(p);
            if(old==null || old.getBlockState().getBlock()!=s.getBlock())entities.put(p,eb.newBlockEntity(p,s));
        } else entities.remove(p);
        return true;
    }
    @SuppressWarnings("unchecked")
    public Object invoke(Object object,Method method,Object[] args) throws Throwable {
        switch(method.getName()) {
            case "isStateAtPosition": return ((java.util.function.Predicate<BlockState>)args[1]).test(block((BlockPos)args[0]));
            case "isFluidAtPosition": return ((java.util.function.Predicate<net.minecraft.world.level.material.FluidState>)args[1]).test(block((BlockPos)args[0]).getFluidState());
            case "getBlockState": return block((BlockPos)args[0]);
            case "getFluidState": return block((BlockPos)args[0]).getFluidState();
            case "setBlock": case "setBlockAndUpdate": return set((BlockPos)args[0],(BlockState)args[1]);
            case "removeBlock": case "destroyBlock":
                if(beforeDestroy==null){beforeDestroy=placed();beforeDestroyNext=peekNext();}
                return set((BlockPos)args[0],Blocks.AIR.defaultBlockState());
            case "getBlockEntity":
                if(args.length==1){var entity=entities.get((BlockPos)args[0]);if(entity instanceof BeehiveBlockEntity && beforeBees==null){beforeBees=placed();beforeBeesNext=peekNext();}return entity;} break;
            case "getMinY": return -64;
            case "getMaxY": return 319;
            case "getHeight":
                if(args==null || args.length==0)return 384;
                int x,z;
                if(args[1] instanceof BlockPos p) {x=p.getX();z=p.getZ();} else {x=(int)args[1];z=(int)args[2];}
                for(int y=319;y>=-64;y--)if(((Heightmap.Types)args[0]).isOpaque().test(((Heightmap.Types)args[0]).keepAfterWorldgen() ? block(new BlockPos(x,y,z)) : initial(new BlockPos(x,y,z))))return y+1;
                return -64;
            case "getSeaLevel": return 63;
            case "getSeed": return 8675309L;
            case "getLevel": if(beforeExternal==null){beforeExternal=placed();beforeExternalNext=peekNext();}return serverLevel;
            case "getRandom": if(beforeEdge==null){beforeEdge=placed();beforeEdgeNext=peekNext();}return levelRandom;
            case "registryAccess": return registries;
            case "getChunk":
                BlockPos p=args[0] instanceof BlockPos bp ? bp : new BlockPos((int)args[0]*16,0,(int)args[1]*16);
                return chunks.computeIfAbsent(new ChunkPos(p.getX() >> 4,p.getZ() >> 4),cp->new ProtoChunk(cp,UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),PalettedContainerFactory.create(registries),null));
            case "scheduleTick": case "setCurrentlyGenerating": return null;
            case "toString": return "TreeParityLevel";
        }
        if(method.isDefault())return InvocationHandler.invokeDefault(object,method,args);
        throw new UnsupportedOperationException(method.toString());
    }
    JsonArray placed() {
        var out=new JsonArray();
        blocks.entrySet().stream().filter(e->!e.getValue().equals(initial(e.getKey()))).sorted(Comparator.comparingInt((Map.Entry<BlockPos,BlockState> e)->e.getKey().getX()).thenComparingInt(e->e.getKey().getY()).thenComparingInt(e->e.getKey().getZ())).forEach(e->{var a=FeaturesGolden.values(e.getKey().getX(),e.getKey().getY(),e.getKey().getZ(),Block.getId(e.getValue()));out.add(a);});
        return out;
    }
    JsonArray blockEntities() {
        var out=new JsonArray();
        entities.entrySet().stream().sorted(Map.Entry.comparingByKey()).forEach(e->{var o=new JsonObject();o.add("position",FeaturesGolden.values(e.getKey().getX(),e.getKey().getY(),e.getKey().getZ()));o.addProperty("nbt",e.getValue().saveWithFullMetadata(registries).toString());out.add(o);});
        return out;
    }
    static void featureSizes(Path directory,RegistryAccess.Frozen registries) throws Exception {
        var out=new JsonArray();
        for(String definition:List.of(
            "{type:'two_layers_feature_size'}",
            "{type:'two_layers_feature_size',limit:0,lower_size:16,upper_size:0,min_clipped_height:0}",
            "{type:'two_layers_feature_size',limit:81,lower_size:0,upper_size:16,min_clipped_height:80}",
            "{type:'three_layers_feature_size'}",
            "{type:'three_layers_feature_size',limit:3,upper_limit:2,lower_size:2,middle_size:3,upper_size:4,min_clipped_height:5}",
            "{type:'three_layers_feature_size',limit:80,upper_limit:80,lower_size:16,middle_size:0,upper_size:8}"
        )) {
            var config=JsonParser.parseString(definition);var size=net.minecraft.world.level.levelgen.feature.featuresize.FeatureSize.CODEC.parse(FeaturesGolden.ops(registries),config).getOrThrow();
            var item=new JsonObject();item.add("config",config);var values=new JsonArray();
            for(int height:new int[]{0,1,5,16,80})for(int y=-1;y<=height+1;y++)values.add(size.getSizeAtHeight(height,y));
            item.add("sizes",values);out.add(item);
        }
        WorldgenGolden.write(directory.resolve("sizes.json.gz"),out);
        var sets=new JsonArray();
        for(int[] origin:new int[][]{{0,64,0},{-31,80,47}})for(int shape=0;shape<3;shape++) {
            var input=new JsonArray();var positions=new HashSet<BlockPos>();
            for(int y=20;y>=0;y--)for(int x=shape==0?0:-10;x<=(shape==0?1:10);x++)for(int z=shape==0?0:-10;z<=(shape==0?1:10);z++) {
                if(shape==2 && (x*x+z*z>100 || y<10 && Math.abs(x)>3))continue;
                var p=new BlockPos(origin[0]+x,origin[1]+y,origin[2]+z);positions.add(p);input.add(FeaturesGolden.values(p.getX(),p.getY(),p.getZ()));
            }
            var output=new JsonArray();for(var p:positions)output.add(FeaturesGolden.values(p.getX(),p.getY(),p.getZ()));
            var item=new JsonObject();item.add("input",input);item.add("output",output);sets.add(item);
        }
        WorldgenGolden.write(directory.resolve("position_sets.json.gz"),sets);
    }
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();
        var registries=WorldgenGolden.load();var cases=new JsonArray();
        var location=new net.minecraft.server.packs.PackLocationInfo("vanilla",net.minecraft.network.chat.Component.literal("vanilla"),net.minecraft.server.packs.repository.PackSource.BUILT_IN,Optional.empty());
        var pack=new net.minecraft.server.packs.VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        try(var resources=new net.minecraft.server.packs.resources.MultiPackResourceManager(net.minecraft.server.packs.PackType.SERVER_DATA,List.of(pack));
            var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("fixture-worlds")).createAccess("tree")) {
        templateManager=new net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager(resources,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(net.minecraft.resources.Identifier.withDefaultNamespace("overworld"));
        var generator=new NoiseBasedChunkGenerator(WorldgenGolden.source(registries,"overworld"),Holder.direct(settings));
        for(var entry:registries.lookupOrThrow(Registries.FEATURE).listElements().sorted(Comparator.comparing(e->e.key().identifier())).toList()) {
            var feature=entry.value();
            if(!(feature instanceof TreeFeature || feature instanceof FallenTreeFeature || feature instanceof HugeFungusFeature || feature instanceof AbstractHugeMushroomFeature || feature instanceof RootSystemFeature))continue;
            for(long seed:new long[]{0,1,-1,8675309})for(int[] coordinates:new int[][]{{0,64,0},{-31,80,47}})for(String terrain:feature instanceof RootSystemFeature ? List.of("flat","blocked","partial","slope","water","cave") : List.of("flat","blocked","partial","slope","water")) {
                var origin=new BlockPos(coordinates[0],coordinates[1],coordinates[2]);
                var ground=feature instanceof HugeFungusFeature f ? f.validBaseState() : Blocks.GRASS_BLOCK.defaultBlockState();
                var fixture=new TreeGolden(registries,origin,terrain,ground,generator);
                var level=(WorldGenLevel)Proxy.newProxyInstance(TreeGolden.class.getClassLoader(),new Class<?>[]{WorldGenLevel.class},fixture);
                var source=new XoroshiroRandomSource(seed);var random=new WorldgenRandom(source);fixture.featureSource=source;var item=new JsonObject();
                item.addProperty("feature",entry.key().identifier().toString());item.addProperty("seed",seed);item.add("origin",FeaturesGolden.values(coordinates));item.addProperty("terrain",terrain);item.addProperty("ground",Block.getId(ground));
                try {item.addProperty("success",feature.place(level,generator,random,origin));}
                catch(Throwable failure) {throw new RuntimeException(entry.key().identifier()+" "+seed+" "+terrain,failure);}
                item.addProperty("next",random.nextLong());item.add("blocks",fixture.placed());item.add("block_entities",fixture.blockEntities());if(fixture.beforeEdge!=null){item.add("before_edge",fixture.beforeEdge);item.addProperty("before_edge_next",fixture.beforeEdgeNext);}
                if(fixture.beforeBees!=null){item.add("before_bees",fixture.beforeBees);item.addProperty("before_bees_next",fixture.beforeBeesNext);}
                if(fixture.beforeDestroy!=null){item.add("before_destroy",fixture.beforeDestroy);item.addProperty("before_destroy_next",fixture.beforeDestroyNext);}
                if(fixture.beforeExternal!=null){item.add("before_external",fixture.beforeExternal);item.addProperty("before_external_next",fixture.beforeExternalNext);}cases.add(item);
            }
        }
        WorldgenGolden.write(Path.of(args[0]),cases);
        featureSizes(Path.of(args[0]).getParent(),registries);
        System.out.println("Captured "+cases.size()+" feature cases");
        }
    }
}
