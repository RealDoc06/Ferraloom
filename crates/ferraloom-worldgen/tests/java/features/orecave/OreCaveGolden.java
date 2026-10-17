import com.google.gson.*;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import java.util.function.Predicate;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.Identifier;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.feature.*;

/** Calls the real configured Feature.place against controlled ProtoChunks.
 * No generated/decompiled Mojang code is included in this driver. */
public final class OreCaveGolden {
    static net.minecraft.server.level.ServerLevel serverLevel;
    static final Set<String> TYPES=Set.of("ore","scattered_ore","geode","large_dripstone","speleothem","speleothem_cluster","sculk_patch","multiface_growth","underwater_magma","vegetation_patch","waterlogged_vegetation_patch","monster_room","fossil","spring_feature","lake","disk","replace_single_block");
    static final LevelHeightAccessor HEIGHTS=LevelHeightAccessor.create(-64,128);
    static final Comparator<BlockPos> ORDER=Comparator.<BlockPos>comparingInt(p->p.getX()).thenComparingInt(BlockPos::getY).thenComparingInt(BlockPos::getZ);
    static JsonArray point(BlockPos p) {return WorldgenGolden.array(p.getX(),p.getY(),p.getZ());}
    static final class Level implements InvocationHandler {
        final RegistryAccess.Frozen registries;
        final PalettedContainerFactory factory;
        final Map<ChunkPos,ProtoChunk> chunks=new HashMap<>();
        final Map<BlockPos,BlockEntity> entities=new TreeMap<>(ORDER);
        final Map<BlockPos,BlockState> writes=new TreeMap<>(ORDER);
        final List<JsonArray> ticks=new ArrayList<>();
        final Set<String> tickKeys=new HashSet<>();
        final WorldGenLevel proxy;
        final BlockPos origin;
        final String terrain;
        final long seed;
        final RandomSource levelRandom=new LegacyRandomSource(123);
        Level(RegistryAccess.Frozen registries,BlockPos origin,String terrain,long seed) {
            this.registries=registries;this.factory=PalettedContainerFactory.create(registries);this.origin=origin;this.terrain=terrain;this.seed=seed;
            proxy=(WorldGenLevel)Proxy.newProxyInstance(getClass().getClassLoader(),new Class<?>[]{WorldGenLevel.class},this);
        }
        BlockState initial(BlockPos p) {
            if(p.getY()<-64 || p.getY()>=64)return Blocks.VOID_AIR.defaultBlockState();
            int y=p.getY()-origin.getY(),x=p.getX()-origin.getX(),z=p.getZ()-origin.getZ();
            if(terrain.startsWith("spring:"))return y==0&&z==0&&x>=0&&x<=3?Blocks.AIR.defaultBlockState():BuiltInRegistries.BLOCK.getValue(Identifier.parse(terrain.substring(7))).defaultBlockState();
            Block block=switch(terrain) {
                case "nether" -> Blocks.NETHERRACK;
                case "deepslate" -> Blocks.DEEPSLATE;
                case "surface" -> y>0?Blocks.AIR:y>=-3?Blocks.DIRT:Blocks.STONE;
                case "water" -> y>=0&&y<=7?Blocks.WATER:Blocks.STONE;
                case "cave" -> y>=0&&y<=7?Blocks.CAVE_AIR:Blocks.STONE;
                case "pockets" -> ((Math.floorMod(x,7)==0&&Math.floorMod(z,5)<=1&&y>=-5&&y<=3)?Blocks.AIR:Blocks.STONE);
                case "room" -> y>=0&&y<=1&&z==0&&x>=0&&x<=7?Blocks.AIR:Blocks.STONE;
                default -> Blocks.STONE;
            };
            return block.defaultBlockState();
        }
        ProtoChunk chunk(ChunkPos pos) {
            return chunks.computeIfAbsent(pos,key->{
                var chunk=new ProtoChunk(key,UpgradeData.EMPTY,HEIGHTS,factory,null);
                for(int y=-64;y<64;y++)for(int z=0;z<16;z++)for(int x=0;x<16;x++) {
                    var p=new BlockPos(key.getMinBlockX()+x,y,key.getMinBlockZ()+z);
                    chunk.getSection(chunk.getSectionIndex(y)).setBlockState(x,y&15,z,initial(p),false);
                }
                Heightmap.primeHeightmaps(chunk,Set.of(Heightmap.Types.OCEAN_FLOOR_WG,Heightmap.Types.WORLD_SURFACE_WG));
                return chunk;
            });
        }
        BlockState block(BlockPos p) {return chunk(ChunkPos.containing(p)).getBlockState(p);}
        boolean set(BlockPos p,BlockState state) {
            p=p.immutable();chunk(ChunkPos.containing(p)).setBlockState(p,state,0);writes.put(p,state);
            if(state.getBlock() instanceof EntityBlock e)entities.put(p,e.newBlockEntity(p,state));else entities.remove(p);
            return true;
        }
        @SuppressWarnings("unchecked")
        public Object invoke(Object object,Method method,Object[] a) throws Throwable {
            a=a==null?new Object[0]:a;
            switch(method.getName()) {
                case "getBlockState":return block((BlockPos)a[0]);
                case "getFluidState":return block((BlockPos)a[0]).getFluidState();
                case "setBlock":return set((BlockPos)a[0],(BlockState)a[1]);
                case "getChunk": {
                    if(a[0] instanceof BlockPos p)return chunk(ChunkPos.containing(p));
                    return chunk(new ChunkPos((Integer)a[0],(Integer)a[1]));
                }
                case "hasChunk":case "hasChunkAt":case "ensureCanWrite":return true;
                case "getHeight":if(a.length==0)return 128;return chunk(new ChunkPos((Integer)a[1]>>4,(Integer)a[2]>>4)).getHeight((Heightmap.Types)a[0],(Integer)a[1]&15,(Integer)a[2]&15)+1;
                case "getMinY":return -64;
                case "getMaxY":return 63;
                case "getSeed":return seed;
                case "getSeaLevel":return 63;
                case "getRandom":return levelRandom;
                case "isStateAtPosition":return ((Predicate<BlockState>)a[1]).test(block((BlockPos)a[0]));
                case "isFluidAtPosition":return ((Predicate<net.minecraft.world.level.material.FluidState>)a[1]).test(block((BlockPos)a[0]).getFluidState());
                case "registryAccess":return registries;
                case "getBiome":return registries.lookupOrThrow(Registries.BIOME).get(Identifier.withDefaultNamespace("plains")).orElseThrow();
                case "getBlockEntity":return entities.get((BlockPos)a[0]);
                case "scheduleTick": {
                    BlockPos p=((BlockPos)a[0]).immutable();Object kind=a[1];
                    String id=kind instanceof Block b?BuiltInRegistries.BLOCK.getKey(b).toString():BuiltInRegistries.FLUID.getKey((net.minecraft.world.level.material.Fluid)kind).toString();
                    String category=kind instanceof Block?"block":"fluid";
                    if(tickKeys.add(category+p+id)) {var tick=point(p);tick.add(category);tick.add(id);tick.add(a.length>3?((net.minecraft.world.ticks.TickPriority)a[3]).getValue():0);ticks.add(tick);}return null;
                }
                case "levelEvent":case "gameEvent":case "playSound":case "addParticle":case "blockUpdated":case "neighborShapeChanged":return null;
                case "isClientSide":return false;
                case "getLevel":return serverLevel;
                case "toString":return "ControlledOreCaveLevel";
            }
            if(method.isDefault())return InvocationHandler.invokeDefault(object,method,a);
            throw new UnsupportedOperationException(method.toString());
        }
        JsonArray blocks() {
            var result=new TreeMap<BlockPos,Integer>(ORDER);
            // Include direct section writes, which bypass Level.setBlock.
            for(var chunk:chunks.values())for(int y=-64;y<64;y++)for(int z=0;z<16;z++)for(int x=0;x<16;x++) {
                var p=new BlockPos(chunk.getPos().getMinBlockX()+x,y,chunk.getPos().getMinBlockZ()+z);var state=chunk.getBlockState(p);
                if(state!=initial(p))result.put(p,Block.getId(state));
            }
            var out=new JsonArray();result.forEach((p,s)->{var v=point(p);v.add(s);out.add(v);});return out;
        }
        JsonArray blockEntities() {
            var out=new JsonArray();entities.forEach((p,be)->{var v=new JsonArray();v.add(point(p));v.add(be.saveWithFullMetadata(registries).toString());out.add(v);});return out;
        }
    }
    static void collect(JsonElement value,String path,Map<String,JsonObject> features) {
        if(value.isJsonObject()) {
            var o=value.getAsJsonObject();
            if(o.has("type")&&TYPES.contains(o.get("type").getAsString().replace("minecraft:","")))features.put(path,o);
            o.entrySet().forEach(e->collect(e.getValue(),path+"/"+e.getKey(),features));
        }else if(value.isJsonArray())for(int i=0;i<value.getAsJsonArray().size();i++)collect(value.getAsJsonArray().get(i),path+"/"+i,features);
    }
    public static void main(String[] args)throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var registries=WorldgenGolden.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.registries().<HolderLookup.RegistryLookup<?>>map(e->e.value()),registries.registries().<HolderLookup.RegistryLookup<?>>map(e->e.value())));var ops=net.minecraft.resources.RegistryOps.create(com.mojang.serialization.JsonOps.INSTANCE,lookup);
        var location=new net.minecraft.server.packs.PackLocationInfo("vanilla",net.minecraft.network.chat.Component.literal("vanilla"),net.minecraft.server.packs.repository.PackSource.BUILT_IN,Optional.empty());
        var pack=new net.minecraft.server.packs.VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        var resources=new net.minecraft.server.packs.resources.MultiPackResourceManager(net.minecraft.server.packs.PackType.SERVER_DATA,List.of(pack));
        var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("fixture-worlds")).createAccess("orecave");
        var manager=new net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager(resources,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
        org.mockito.stubbing.Answer<Object> forbidden=call->{throw new UnsupportedOperationException("Unapproved server mock call: "+call.getMethod());};
        serverLevel=org.mockito.Mockito.mock(net.minecraft.server.level.ServerLevel.class,forbidden);
        var server=org.mockito.Mockito.mock(net.minecraft.server.MinecraftServer.class,forbidden);
        org.mockito.Mockito.doReturn(server).when(serverLevel).getServer();
        org.mockito.Mockito.doReturn(manager).when(server).getStructureTemplateManager();
        var features=new TreeMap<String,JsonObject>();
        try(var jar=new java.util.jar.JarFile(args[0])) {
            for(var entry:Collections.list(jar.entries()))if(entry.getName().startsWith("data/minecraft/worldgen/feature/")&&entry.getName().endsWith(".json")) {
                var value=JsonParser.parseReader(new java.io.InputStreamReader(jar.getInputStream(entry)));
                collect(value,entry.getName().replace("data/minecraft/worldgen/feature/","").replace(".json",""),features);
            }
        }
        features.put("fixture/replace_single_block",JsonParser.parseString("{type:'minecraft:replace_single_block',targets:[{target:{predicate_type:'minecraft:random_block_match',block:'minecraft:stone',probability:0.5},state:'minecraft:diamond_ore'}]}").getAsJsonObject());
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("overworld"));
        var generator=new NoiseBasedChunkGenerator(WorldgenGolden.source(registries,"overworld"),Holder.direct(settings));
        var out=new JsonArray();int count=0;
        String[] terrains={"stone","nether","deepslate","surface","water","cave","pockets","room"};
        for(var entry:features.entrySet()) {
            var feature=Feature.DIRECT_CODEC.parse(ops,entry.getValue()).getOrThrow();
            var scenarios=new ArrayList<String>(List.of(terrains));
            if(entry.getValue().get("type").getAsString().equals("minecraft:spring_feature")) {
                var valid=entry.getValue().get("valid_blocks");scenarios.add("spring:"+(valid.isJsonArray()?valid.getAsJsonArray().get(0):valid).getAsString());
            }
            for(int i=0;i<scenarios.size();i++)for(long seed:new long[]{0,-1,8675309}) {
                var origin=switch(i%3){case 0->new BlockPos(0,0,0);case 1->new BlockPos(15,-4,15);default->new BlockPos(-17,7,-1);};
                var level=new Level(registries,origin,scenarios.get(i),seed);var random=new WorldgenRandom(new XoroshiroRandomSource(seed));
                var item=new JsonObject();item.addProperty("feature",entry.getKey());item.add("config",entry.getValue());item.addProperty("terrain",scenarios.get(i));item.addProperty("seed",seed);item.add("origin",point(origin));
                try {item.addProperty("result",feature.place(level.proxy,generator,random,origin));}
                catch(UnsupportedOperationException e) {throw new IllegalStateException(entry.getKey()+" "+scenarios.get(i),e);}
                item.add("blocks",level.blocks());item.add("block_entities",level.blockEntities());var ticks=new JsonArray();level.ticks.forEach(ticks::add);item.add("ticks",ticks);item.addProperty("next",random.nextLong());out.add(item);
            }
            System.err.println(++count+" "+entry.getKey());
        }
        WorldgenGolden.write(Path.of(args[1]),out);
        storage.close();resources.close();
        System.err.println(out.size()+" cases");
    }
}
