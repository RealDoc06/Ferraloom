import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.boss.enderdragon.EndCrystal;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.*;
import net.minecraft.world.level.levelgen.*;
import net.minecraft.world.level.levelgen.feature.*;
import net.minecraft.world.level.levelgen.placement.*;
import org.mockito.Mockito;

/** Calls vanilla Feature.place, recording ordered writes and final block entities. */
public final class NetherEndGolden {
    static final Set<String> TYPES=Set.of("delta_feature","stepped_column_cluster","netherrack_replace_blobs","chorus_plant","end_gateway","end_island","end_platform","end_podium","end_spike","void_start_platform","bonus_chest","template","no_op","random_boolean_selector","random_selector","simple_random_selector","weighted_random_selector","sequence");
    static final Set<String> COMPOSITES=Set.of("random_boolean_selector","random_selector","simple_random_selector","weighted_random_selector","sequence");
    static final int[][] ORIGINS={{8,64,8},{-17,64,31},{42,64,0},{8,80,8},{8,-63,8}};
    static RegistryAccess.Frozen registries;
    static ServerLevel entityLevel;
    static JsonArray xyz(BlockPos p){return FeaturesGolden.values(p.getX(),p.getY(),p.getZ());}
    static BlockState initial(BlockPos p,String ground,int mode){
        if(p.getY() < -64 || p.getY()>319)return Blocks.VOID_AIR.defaultBlockState();
        if(p.getY()==-64)return Blocks.BEDROCK.defaultBlockState();
        int floor=mode==0?63:60+Math.floorMod(p.getX()*31+p.getZ()*17,7);
        if(p.getY()<=floor)return BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(ground)).defaultBlockState();
        return Blocks.AIR.defaultBlockState();
    }
    static final class ControlledLevel implements InvocationHandler {
        final Map<BlockPos,BlockState> blocks=new HashMap<>();
        final Map<BlockPos,BlockEntity> blockEntities=new HashMap<>();
        final Map<ChunkPos,ProtoChunk> chunks=new HashMap<>();
        final JsonArray writes=new JsonArray(),entities=new JsonArray();
        final WorldGenLevel level;
        final String ground;final int mode;
        ControlledLevel(String ground,int mode){this.ground=ground;this.mode=mode;level=(WorldGenLevel)Proxy.newProxyInstance(getClass().getClassLoader(),new Class<?>[]{WorldGenLevel.class},this);}
        BlockState block(BlockPos p){return blocks.getOrDefault(p,initial(p,ground,mode));}
        boolean set(BlockPos p,BlockState state,int flags){
            p=p.immutable();
            var row=new JsonArray();row.add(p.getX());row.add(p.getY());row.add(p.getZ());row.add(Block.getId(state));row.add(flags);writes.add(row);
            if(p.getY() < -64 || p.getY()>319)return false;
            blocks.put(p,state);
            var chunk=chunks.get(ChunkPos.containing(p));if(chunk!=null)chunk.setBlockState(p,state,flags);
            if(state.getBlock() instanceof EntityBlock entityBlock)blockEntities.put(p,entityBlock.newBlockEntity(p,state));else blockEntities.remove(p);
            return true;
        }
        ProtoChunk chunk(int x,int z){
            return chunks.computeIfAbsent(new ChunkPos(x,z),key->{
                var chunk=new ProtoChunk(key,UpgradeData.EMPTY,LevelHeightAccessor.create(-64,384),PalettedContainerFactory.create(registries),null);
                for(var p:BlockPos.betweenClosed(x*16,-64,z*16,x*16+15,66,z*16+15))chunk.setBlockState(p,block(p),0);
                return chunk;
            });
        }
        public Object invoke(Object proxy,Method method,Object[] args)throws Throwable{
            return switch(method.getName()){
                case "getChunk" -> args[0] instanceof BlockPos p ? chunk(p.getX()>>4,p.getZ()>>4) : chunk((int)args[0],(int)args[1]);
                case "getBlockState" -> block((BlockPos)args[0]);
                case "getFluidState" -> block((BlockPos)args[0]).getFluidState();
                case "isEmptyBlock" -> block((BlockPos)args[0]).isAir();
                case "setBlock" -> set((BlockPos)args[0],(BlockState)args[1],(int)args[2]);
                case "setBlockAndUpdate" -> set((BlockPos)args[0],(BlockState)args[1],3);
                case "destroyBlock" -> {var p=(BlockPos)args[0];if(block(p).isAir())yield false;yield set(p,Blocks.AIR.defaultBlockState(),3);}
                case "getBlockEntity" -> blockEntities.get((BlockPos)args[0]);
                case "getMinY" -> -64;
                case "getMaxY" -> 319;
                case "getHeight" -> {
                    if(args==null||args.length==0)yield 384;
                    int x=(int)args[1],z=(int)args[2],y=319;
                    while(y>=-64 && !((Heightmap.Types)args[0]).isOpaque().test(block(new BlockPos(x,y,z))))y--;
                    yield y+1;
                }
                case "getHeightmapPos" -> {var p=(BlockPos)args[1];int y=(int)invoke(proxy,WorldGenLevel.class.getMethod("getHeight",Heightmap.Types.class,int.class,int.class),new Object[]{args[0],p.getX(),p.getZ()});yield p.atY(y);}
                case "getSeaLevel" -> 63;
                case "getSeed" -> 8675309L;
                case "getLevel" -> entityLevel;
                case "registryAccess" -> registries;
                case "isClientSide" -> false;
                case "getRandom" -> new LegacyRandomSource(0);
                case "getBiome" -> registries.lookupOrThrow(Registries.BIOME).getOrThrow(net.minecraft.resources.ResourceKey.create(Registries.BIOME,Identifier.withDefaultNamespace("plains")));
                case "getBlockTicks","getFluidTicks" -> throw new UnsupportedOperationException(method.toString());
                case "playSound","levelEvent","gameEvent","scheduleTick","blockUpdated","updateNeighborsAt","neighborShapeChanged","markAboveForPostProcessing","setCurrentlyGenerating" -> null;
                case "ensureCanWrite","hasChunk","hasChunksAt","isAreaLoaded","isStateAtPosition","isFluidAtPosition" -> {
                    if(method.getName().equals("isStateAtPosition"))yield ((java.util.function.Predicate<BlockState>)args[1]).test(block((BlockPos)args[0]));
                    if(method.getName().equals("isFluidAtPosition"))yield ((java.util.function.Predicate<net.minecraft.world.level.material.FluidState>)args[1]).test(block((BlockPos)args[0]).getFluidState());
                    yield true;
                }
                case "addFreshEntity" -> {
                    var crystal=(EndCrystal)args[0];var e=new JsonObject();e.addProperty("id","minecraft:end_crystal");
                    e.add("position",new com.google.gson.Gson().toJsonTree(new double[]{crystal.getX(),crystal.getY(),crystal.getZ()}));
                    e.addProperty("yaw",crystal.getYRot());e.addProperty("invulnerable",crystal.isPermanentlyInvulnerable());
                    e.add("beam",crystal.getBeamTarget()==null?JsonNull.INSTANCE:xyz(crystal.getBeamTarget()));entities.add(e);yield true;
                }
                case "toString" -> "NetherEndParityLevel";
                default -> {if(method.isDefault())yield InvocationHandler.invokeDefault(proxy,method,args);throw new UnsupportedOperationException(method.toString());}
            };
        }
        JsonArray savedBlockEntities(){
            var result=new JsonArray();
            for(var e:blockEntities.entrySet().stream().sorted(Comparator.comparing((Map.Entry<BlockPos,BlockEntity> e)->e.getKey().getX()).thenComparing(e->e.getKey().getY()).thenComparing(e->e.getKey().getZ())).toList()){
                var entry=new JsonObject();entry.add("position",xyz(e.getKey()));entry.addProperty("nbt",e.getValue().saveWithoutMetadata(registries).toString());result.add(entry);
            }
            return result;
        }
    }
    static void run(JsonArray cases,String name,JsonElement config,Feature feature,NoiseBasedChunkGenerator generator,String ground,int mode,int[] origin,long seed,boolean crossFamily)throws Exception{
        var controlled=new ControlledLevel(ground,mode);var random=new WorldgenRandom(new XoroshiroRandomSource(seed));
        var item=new JsonObject();item.addProperty("name",name);item.add("config",config);item.addProperty("ground",ground);item.addProperty("mode",mode);item.addProperty("seed",seed);item.add("origin",FeaturesGolden.values(origin));
        if(crossFamily)item.addProperty("pending","cross-family children");
        try {
            boolean result=feature.place(controlled.level,generator,random,new BlockPos(origin[0],origin[1],origin[2]));
            item.addProperty("result",result);item.addProperty("next",random.nextLong());item.add("writes",controlled.writes);item.add("block_entities",controlled.savedBlockEntities());item.add("entities",controlled.entities);
        }catch(UnsupportedOperationException ex){item.addProperty("vanilla_pending",ex.getMessage());}
        cases.add(item);
    }
    public static void main(String[] args)throws Exception{
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();registries=WorldgenGolden.load();
        entityLevel=Mockito.mock(ServerLevel.class);Mockito.when(entityLevel.getNextEntityId()).thenReturn(1);Mockito.when(entityLevel.registryAccess()).thenReturn(registries);Mockito.when(entityLevel.enabledFeatures()).thenReturn(net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS);
        var server=Mockito.mock(net.minecraft.server.MinecraftServer.class);
        var templates=Mockito.mock(net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplateManager.class);
        Mockito.when(entityLevel.getServer()).thenReturn(server);Mockito.when(server.getStructureTemplateManager()).thenReturn(templates);
        Mockito.when(templates.getOrCreate(Mockito.any())).thenAnswer(call->{
            Identifier id=call.getArgument(0);var template=new net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate();
            if(id.getNamespace().equals("ferraloom")){
                var tag=net.minecraft.nbt.TagParser.parseCompoundFully(Files.readString(Path.of(args[1])));
                if(id.getPath().equals("empty"))tag.put("blocks",new net.minecraft.nbt.ListTag());
                template.load(BuiltInRegistries.BLOCK,tag);return template;
            }
            try(var stream=NetherEndGolden.class.getResourceAsStream("/data/"+id.getNamespace()+"/structure/"+id.getPath()+".nbt")){
                if(stream!=null)template.load(BuiltInRegistries.BLOCK,net.minecraft.nbt.NbtIo.readCompressed(stream,net.minecraft.nbt.NbtAccounter.unlimitedHeap()));
            }
            return template;
        });
        var settings=registries.lookupOrThrow(Registries.NOISE_SETTINGS).getValue(Identifier.withDefaultNamespace("overworld"));
        var generator=new NoiseBasedChunkGenerator(WorldgenGolden.source(registries,"overworld"),Holder.direct(settings));
        var cases=new JsonArray();var ops=FeaturesGolden.ops(registries);
        for(var entry:registries.lookupOrThrow(Registries.FEATURE).listElements().sorted(Comparator.comparing(e->e.key().identifier())).toList()){
            var config=Feature.DIRECT_CODEC.encodeStart(ops,entry.value()).getOrThrow();
            String type=config.getAsJsonObject().get("type").getAsString().replace("minecraft:","");if(!TYPES.contains(type))continue;
            String name=entry.key().identifier().toString();boolean cross=COMPOSITES.contains(type)&&!name.contains("basalt_columns");
            String ground=type.equals("chorus_plant")?"end_stone":"netherrack";
            for(long seed:new long[]{0,1,-1,8675309})for(int[] origin:ORIGINS)for(int mode:new int[]{0,1})run(cases,name,config,entry.value(),generator,ground,mode,origin,seed,cross);
        }
        var empty="{feature:{type:'no_op'},placement:[{type:'random_chance',chance:0.0}]}";
        var island="{feature:{type:'end_island'},placement:[{type:'offset',x:1,y:0,z:-1}]}";
        var yes="{feature:{type:'no_op'},placement:[]}";
        var configs=List.of(
            "{type:'random_boolean_selector',feature_true:"+island+",feature_false:"+empty+"}",
            "{type:'random_selector',features:[{feature:"+empty+",chance:0.25},{feature:"+island+",chance:0.65}],default:"+yes+"}",
            "{type:'simple_random_selector',features:["+empty+","+island+","+yes+"]}",
            "{type:'weighted_random_selector',features:[{data:"+empty+",weight:0},{data:"+island+",weight:2},{data:"+yes+",weight:3}]}",
            "{type:'weighted_random_selector',features:[]}",
            "{type:'weighted_random_selector',features:[{data:"+island+",weight:0}]}",
            "{type:'sequence',features:["+island+","+empty+","+island+"]}",
            "{type:'sequence',features:["+island+","+yes+","+island+"]}",
            "{type:'no_op'}",
            "{type:'stepped_column_cluster',block:{id:'basalt'},can_replace:{type:'matching_block_tag',tag:'air'},continue_through:{type:'matching_blocks',blocks:'basalt'},cannot_place_on:'#cannot_place_basalt_pillar_on',cluster_reach:5,column_count:50,column_reach:2,height:{type:'uniform',min_inclusive:5,max_inclusive:10}}",
            "{type:'end_spike',spikes:[{centerX:8,centerZ:8,radius:3,height:79,guarded:true}],crystal_invulnerable:true,crystal_beam_target:[1,99,2]}"
        );
        for(int i=0;i<configs.size();i++){
            var config=JsonParser.parseString(configs.get(i));var feature=Feature.DIRECT_CODEC.parse(ops,config).getOrThrow();
            for(long seed:new long[]{0,1,-1,8675309})for(int[] origin:ORIGINS)for(int mode:new int[]{0,1})run(cases,"custom:"+i,config,feature,generator,"netherrack",mode,origin,seed,false);
        }
        for(String name:List.of("empty","solid")){
            var config=JsonParser.parseString("{type:'template',templates:[{data:{id:'ferraloom:"+name+"'},weight:2},{data:{id:'ferraloom:"+name+"',rotations:['clockwise_90','180']},weight:3}]}");
            var feature=Feature.DIRECT_CODEC.parse(ops,config).getOrThrow();
            for(long seed:new long[]{0,1,-1,8675309})for(int[] origin:ORIGINS)for(int mode:new int[]{0,1})run(cases,"custom:template_"+name,config,feature,generator,"netherrack",mode,origin,seed,false);
        }
        WorldgenGolden.write(Path.of(args[0]).resolve("nether_end.json.gz"),cases);
        System.out.println("Captured "+cases.size()+" vanilla cases");
    }
}
