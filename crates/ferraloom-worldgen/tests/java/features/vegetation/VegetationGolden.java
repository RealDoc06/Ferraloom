import com.google.gson.*;
import java.lang.reflect.Proxy;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.*;
import net.minecraft.world.level.biome.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.feature.*;
import net.minecraft.world.level.lighting.*;
import net.minecraft.world.ticks.*;

/** Real feature and block implementations on an exclusive, unlit proto-level. */
public final class VegetationGolden {
    static net.minecraft.server.level.ServerLevel serverLevel;
    static final Set<String> TYPES=Set.of("bamboo","block_column","block_pile","blue_ice","coral_claw","coral_tree","iceberg","freeze_top_layer","vines","random_neighbor_spread","projected_random_patchy_square","overlay","single_block_pillar","simple_block","spike","fill_layer","block_blob");
    static final Comparator<BlockPos> ORDER=Comparator.<BlockPos>comparingInt(p->p.getX()).thenComparingInt(BlockPos::getY).thenComparingInt(BlockPos::getZ);
    static JsonArray pos(BlockPos p) {return WorldgenGolden.array(p.getX(),p.getY(),p.getZ());}
    static final class Controlled {
        final RegistryAccess.Frozen registries;
        final String terrain;
        final Map<BlockPos,BlockState> blocks=new TreeMap<>(ORDER);
        final ProtoChunkTicks<Block> blockTicks=new ProtoChunkTicks<>();
        final ProtoChunkTicks<net.minecraft.world.level.material.Fluid> fluidTicks=new ProtoChunkTicks<>();
        long subTick;
        final RandomSource levelRandom=new XoroshiroRandomSource(7331);
        final LevelLightEngine lights;
        final WorldGenLevel level;
        Controlled(RegistryAccess.Frozen r,String terrain) {
            this.registries=r;this.terrain=terrain;
            LightChunkGetter chunks=new LightChunkGetter() {
                public LightChunk getChunkForLighting(int x,int z) {return null;}
                public BlockGetter getLevel() {return EmptyBlockGetter.INSTANCE;}
            };
            lights=new LevelLightEngine(chunks,true,true);
            level=(WorldGenLevel)Proxy.newProxyInstance(getClass().getClassLoader(),new Class<?>[]{WorldGenLevel.class},(object,method,args)-> {
                if(method.isDefault())return java.lang.reflect.InvocationHandler.invokeDefault(object,method,args);
                return switch(method.getName()) {
                    case "getBlockState" -> block((BlockPos)args[0]);
                    case "getFluidState" -> block((BlockPos)args[0]).getFluidState();
                    case "setBlock" -> {blocks.put(((BlockPos)args[0]).immutable(),(BlockState)args[1]);yield true;}
                    case "getMinY" -> -64;
                    case "getHeight" -> args==null||args.length==0 ? 384 : height((Heightmap.Types)args[0],(int)args[1],(int)args[2]);
                    case "getSeaLevel" -> 63;
                    case "dimensionType" -> registries.lookupOrThrow(Registries.DIMENSION_TYPE).getValue(Identifier.withDefaultNamespace("overworld"));
                    case "getBiomeManager" -> new BiomeManager((x,y,z)->biome(),BiomeManager.obfuscateSeed(8675309L));
                    case "getLevelData" -> new net.minecraft.world.level.storage.LevelData() {
                        public RespawnData getRespawnData() {return RespawnData.DEFAULT;}
                        public long getGameTime() {return 0;}
                        public boolean isHardcore() {return false;}
                        public net.minecraft.world.Difficulty getDifficulty() {return net.minecraft.world.Difficulty.NORMAL;}
                        public boolean isDifficultyLocked() {return false;}
                    };
                    case "getSeed" -> 8675309L;
                    case "getLevel" -> serverLevel;
                    case "nextSubTickCount" -> subTick++;
                    case "registryAccess" -> registries;
                    case "getBiome" -> registries.lookupOrThrow(Registries.BIOME).getOrThrow(ResourceKey.create(Registries.BIOME,Identifier.withDefaultNamespace(terrain.equals("snow")?"snowy_plains":"plains")));
                    case "getLightEngine" -> lights;
                    case "getRawBrightness", "getMaxLocalRawBrightness" -> 15;
                    case "getSkyDarken" -> 0;
                    case "getRandom" -> levelRandom;
                    case "getBlockTicks" -> ticks(blockTicks);
                    case "getFluidTicks" -> ticks(fluidTicks);
                    case "getBlockEntity" -> null;
                    case "isClientSide" -> false;
                    case "ensureCanWrite" -> true;
                    case "toString" -> "UnlitControlledFeatureLevel";
                    default -> throw new UnsupportedOperationException(method.toString());
                };
            });
        }
        static <T> LevelTickAccess<T> ticks(ProtoChunkTicks<T> ticks) {
            return new LevelTickAccess<>() {
                public void schedule(ScheduledTick<T> tick) { ticks.schedule(tick); }
                public boolean hasScheduledTick(BlockPos p,T type) {return ticks.hasScheduledTick(p,type);}
                public boolean willTickThisTick(BlockPos p,T type) {return false;}
                public int count() {return ticks.count();}
            };
        }
        Holder<Biome> biome() {return registries.lookupOrThrow(Registries.BIOME).getOrThrow(ResourceKey.create(Registries.BIOME,Identifier.withDefaultNamespace(terrain.equals("snow")?"snowy_plains":"plains")));}
        int ground(int x,int z) {return terrain.equals("slope")?61+Math.floorMod(x+2*z,5):terrain.equals("water")?57:63;}
        BlockState initial(BlockPos p) {
            int y=p.getY(),x=p.getX(),z=p.getZ(),h=ground(x,z);
            if(y < -64 || y >= 320)return Blocks.VOID_AIR.defaultBlockState();
            if(y<h)return Blocks.STONE.defaultBlockState();
            if(y==h)return (terrain.equals("snow")?Blocks.SNOW_BLOCK:terrain.equals("water")?Blocks.SAND:Blocks.GRASS_BLOCK).defaultBlockState();
            if(terrain.equals("water") && y<=62)return (Math.floorMod(x,7)==0 && Math.floorMod(z,7)==0?Blocks.PACKED_ICE:Blocks.WATER).defaultBlockState();
            return Blocks.AIR.defaultBlockState();
        }
        BlockState block(BlockPos p) {return blocks.getOrDefault(p,initial(p));}
        int height(Heightmap.Types type,int x,int z) {
            for(int y=319;y>=-64;y--) {var s=block(new BlockPos(x,y,z));if(type.isOpaque().test(s))return y+1;}
            return -64;
        }
        JsonArray output() {
            var out=new JsonArray();
            blocks.forEach((p,s)->{var a=pos(p);a.add(Block.getId(s));out.add(a);});return out;
        }
        JsonArray ticks() {
            var out=new JsonArray();
            for(var tick:blockTicks.pack(0)) {var a=pos(tick.pos());a.add(BuiltInRegistries.BLOCK.getId(tick.type()));a.add(tick.priority().getValue());out.add(a);}
            return out;
        }
    }
    static RegistryOps<JsonElement> ops(RegistryAccess.Frozen r) {
        var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.registries().<HolderLookup.RegistryLookup<?>>map(e->e.value()),r.registries().<HolderLookup.RegistryLookup<?>>map(e->e.value())));
        return RegistryOps.create(com.mojang.serialization.JsonOps.INSTANCE,lookup);
    }
    static JsonElement config(RegistryAccess.Frozen r,Feature f) {return Feature.DIRECT_CODEC.encodeStart(ops(r),f).getOrThrow();}
    static void capture(JsonArray cases, RegistryAccess.Frozen r,ChunkGenerator generator,String name,Feature feature,JsonElement config) {
        for(String terrain:List.of("land","water","slope","snow"))for(long seed:new long[]{0,1,-1})for(int[] xyz:new int[][]{{0,64,0},{15,62,15},{-1,70,-1},{0,62,1}}) {
            var controlled=new Controlled(r,terrain);
            var random=new WorldgenRandom(new XoroshiroRandomSource(seed));
            var item=new JsonObject();item.addProperty("name",name);item.add("config",config);item.addProperty("terrain",terrain);item.addProperty("seed",seed);item.add("origin",WorldgenGolden.point(xyz));
            try {item.addProperty("success",feature.place(controlled.level,generator,random,new BlockPos(xyz[0],xyz[1],xyz[2])));}
            catch(UnsupportedOperationException e) {item.addProperty("vanilla_pending",e.getMessage());}
            item.add("blocks",controlled.output());item.add("ticks",controlled.ticks());
            var fluid=new JsonArray();for(var tick:controlled.fluidTicks.pack(0)){var a=pos(tick.pos());a.add(BuiltInRegistries.FLUID.getId(tick.type()));a.add(tick.priority().getValue());fluid.add(a);}item.add("fluid_ticks",fluid);
            item.addProperty("next",random.nextLong());item.addProperty("level_next",controlled.levelRandom.nextLong());cases.add(item);
        }
    }
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();
        var r=WorldgenGolden.load();var cases=new JsonArray();
        var settings=r.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("overworld"));
        var generator=new NoiseBasedChunkGenerator(WorldgenGolden.source(r,"overworld"),Holder.direct(settings));
        var location=new net.minecraft.server.packs.PackLocationInfo("vanilla",net.minecraft.network.chat.Component.literal("vanilla"),net.minecraft.server.packs.repository.PackSource.BUILT_IN,Optional.empty());
        var pack=new net.minecraft.server.packs.VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        try(var resources=new net.minecraft.server.packs.resources.MultiPackResourceManager(net.minecraft.server.packs.PackType.SERVER_DATA,List.of(pack));
            var storage=net.minecraft.world.level.storage.LevelStorageSource.createDefault(Path.of("fixture-worlds")).createAccess("vegetation")) {
        var manager=new net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager(resources,storage,net.minecraft.util.datafix.DataFixers.getDataFixer(),BuiltInRegistries.BLOCK);
        org.mockito.stubbing.Answer<Object> forbidden=call->{throw new UnsupportedOperationException("Unapproved server mock call: "+call.getMethod());};
        serverLevel=org.mockito.Mockito.mock(net.minecraft.server.level.ServerLevel.class,forbidden);
        var server=org.mockito.Mockito.mock(net.minecraft.server.MinecraftServer.class,forbidden);
        org.mockito.Mockito.doReturn(server).when(serverLevel).getServer();
        org.mockito.Mockito.doReturn(manager).when(server).getStructureTemplateManager();
        for(var holder:r.lookupOrThrow(Registries.FEATURE).listElements().sorted(Comparator.comparing(h->h.key().identifier())).toList()) {
            var f=holder.value();var c=config(r,f);if(!TYPES.contains(c.getAsJsonObject().get("type").getAsString().replace("minecraft:","")))continue;
            System.err.println(holder.getRegisteredName());capture(cases,r,generator,holder.getRegisteredName(),f,c);
        }
        for(String text:List.of(
            "{type:'fill_layer',height:129,state:{id:'gold_block'}}",
            "{type:'projected_random_patchy_square',block:{id:'moss_block'},project_through:{type:'matching_blocks',blocks:'air'},size:{type:'uniform',min_inclusive:1,max_inclusive:4},max_projection_height:8}",
            "{type:'single_block_pillar',block:{id:'bone_block'},can_replace:{type:'matching_blocks',blocks:'air'},direction:'up',chance_to_continue:0.7,cap_feature:{feature:{type:'fill_layer',height:132,state:{id:'gold_block'}},placement:[]}}",
            "{type:'coral_claw',feature:{feature:{type:'block_column',layers:[{height:1,provider:{id:'brain_coral_block'}}],direction:'up',allowed_placement:{type:'matching_blocks',blocks:['air','water']},prioritize_tip:false},placement:[]}}",
            "{type:'coral_tree',feature:{feature:{type:'block_column',layers:[{height:1,provider:{id:'fire_coral_block'}}],direction:'up',allowed_placement:{type:'matching_blocks',blocks:['air','water']},prioritize_tip:false},placement:[]}}",
            "{type:'overlay',features:[{feature:{type:'fill_layer',height:129,state:{id:'gold_block'}},placement:[]},{feature:{type:'fill_layer',height:130,state:{id:'diamond_block'}},placement:[]}]}",
            "{type:'simple_block',to_place:{id:'stone'},schedule_tick:true}",
            "{type:'projected_random_patchy_square',block:{type:'rule_based',rules:[]},project_through:{type:'matching_blocks',blocks:'air'},size:1,max_projection_height:0}"
        )) {
            var c=JsonParser.parseString(text);var f=Feature.DIRECT_CODEC.parse(ops(r),c).getOrThrow();capture(cases,r,generator,"synthetic:"+c.getAsJsonObject().get("type").getAsString(),f,c);
        }
        WorldgenGolden.write(Path.of(args[0]),cases);
        UnlitLightGolden.generate(r,Path.of(args[0]).resolveSibling("unlit_light.json.gz"));
        System.err.println("captured "+cases.size()+" cases");
        }
    }
}
