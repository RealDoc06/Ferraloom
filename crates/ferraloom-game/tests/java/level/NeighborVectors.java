import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.packs.*;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.*;
import net.minecraft.tags.TagLoader;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.redstone.*;

/** Real CollectingNeighborUpdater callbacks on an in-memory concrete Level.
 * Unused abstract Level services throw rather than pretending to be a server. */
public final class NeighborVectors {
    static RegistryAccess.Frozen load() {
        var location=new PackLocationInfo("vanilla",Component.literal("vanilla"),PackSource.BUILT_IN,Optional.empty());
        var pack=new VanillaPackResourcesBuilder().pushJarResources().exposeNamespace("minecraft").build(location).fullResources();
        var manager=new MultiPackResourceManager(PackType.SERVER_DATA,List.of(pack));
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        TagLoader.loadTagsForExistingRegistries(manager,builtins).forEach(Registry.PendingTags::apply);
        return RegistryDataLoader.load(manager,builtins.registries().<HolderLookup.RegistryLookup<?>>map(e->e.value()).toList(),RegistryDataLoader.WORLD_REGISTRIES,Runnable::run).join();
    }
    // Run once to generate the compile-time fixture; no allocation bypass or
    // reflection into Minecraft objects is used in the oracle itself.
    public static void main(String[] args) throws Exception {
        if(args.length>0 && args[0].equals("generate")) { generate(Path.of(args[1])); return; }
        var output=System.out; var errors=System.err; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output); System.setErr(errors);
        var registries=load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        Class.forName("NeighborFixture").getMethod("run", RegistryAccess.class).invoke(null,registries);
        var mutation=Class.forName("MutationFixture").getMethod("run",RegistryAccess.class);mutation.setAccessible(true);mutation.invoke(null,registries);
    }
    static String type(Class<?> c) { return c.getCanonicalName(); }
    static void generate(Path path) throws Exception {
        var out=new StringBuilder("import net.minecraft.core.*; import net.minecraft.world.level.*; import net.minecraft.world.level.block.*; import net.minecraft.world.level.block.state.*; import net.minecraft.world.level.redstone.*; import net.minecraft.world.flag.*; import java.util.*;\nclass NeighborFixture extends Level {\n");
        out.append("NeighborFixture(RegistryAccess r,int max){super(null,Level.OVERWORLD,r,r.lookupOrThrow(net.minecraft.core.registries.Registries.DIMENSION_TYPE).getOrThrow(net.minecraft.world.level.dimension.BuiltinDimensionTypes.OVERWORLD),false,false,0,max);}\n");
        var seen=new HashSet<String>();
        for(Class<?> cls=Level.class;cls!=null;cls=cls.getSuperclass()) for(var m:cls.getDeclaredMethods()) if(Modifier.isAbstract(m.getModifiers())) stub(out,m,seen);
        for(var m:Level.class.getMethods()) if(Modifier.isAbstract(m.getModifiers())) stub(out,m,seen);
        out.append("""
            final Map<BlockPos,BlockState> blocks=new HashMap<>();
            final List<String> trace=new ArrayList<>();
            boolean nested;
            boolean experiments;
            @Override public BlockState getBlockState(BlockPos p){
              trace.add(p.toShortString());
              if(nested && p.equals(new BlockPos(-1,0,0))){
                neighborUpdater.neighborChanged(new BlockPos(10,0,0),Blocks.STONE,null);
                neighborUpdater.neighborChanged(new BlockPos(11,0,0),Blocks.STONE,null);
              }
              return Blocks.AIR.defaultBlockState();
            }
            @Override public FeatureFlagSet enabledFeatures(){return experiments?FeatureFlagSet.of(FeatureFlags.VANILLA,FeatureFlags.REDSTONE_EXPERIMENTS):FeatureFlags.DEFAULT_FLAGS;}
            static void scenario(RegistryAccess r,String name,int max,boolean nested,Direction skip){
              var f=new NeighborFixture(r,max);f.nested=nested;var block=Blocks.STONE;
              for(Direction d:Direction.values())f.blocks.put(BlockPos.ZERO.relative(d),block.defaultBlockState());
              f.blocks.put(new BlockPos(10,0,0),block.defaultBlockState());f.blocks.put(new BlockPos(11,0,0),block.defaultBlockState());
              f.neighborUpdater.updateNeighborsAtExceptFromFacing(BlockPos.ZERO,block,skip,null);
              System.out.println(name+"\\t"+String.join(";",f.trace));
            }
            public static void run(RegistryAccess r) throws Exception {
              scenario(r,"direction-order",100,false,null);
              scenario(r,"skip-west",100,false,Direction.WEST);
              scenario(r,"nested-layer",100,true,null);
              scenario(r,"chain-limit",2,true,null);
              scenario(r,"zero-limit",0,true,null);
              var f=new NeighborFixture(r,100);f.randValue=0;
              var positions=new ArrayList<String>();
              for(int i=0;i<4;i++)positions.add(f.getBlockRandomPos(-16,-64,32,15).toShortString());
              System.out.println("random-positions\\t"+String.join(";",positions));
              // Slot oracle uses real Level#random and getBlockRandomPos. The
              // tickChunk body has no section samples in an all-air chunk.
              for(long seed:new long[]{0,1,12345})for(int speed:new int[]{0,1,3,48}){
                var slots=new NeighborFixture(r,100);slots.random.setSeed(seed);slots.randValue=0;
                var precipitation=new ArrayList<String>();
                for(int tick=0;tick<30;tick++)for(int i=0;i<speed;i++)if(slots.random.nextInt(48)==0)precipitation.add(slots.getBlockRandomPos(0,0,0,15).toShortString());
                System.out.println("precipitation-slots:"+seed+":"+speed+"\\t"+String.join(";",precipitation)+"|"+slots.random.nextInt()+"|"+slots.getBlockRandomPos(0,0,0,15).toShortString());
              }
              long strikeSeed=0;
              while(net.minecraft.util.RandomSource.create(strikeSeed).nextInt(100000)!=0)strikeSeed++;
              for(long seed:new long[]{0,strikeSeed})for(int weather=0;weather<3;weather++){
                var slots=new NeighborFixture(r,100);slots.random.setSeed(seed);slots.randValue=0;
                boolean strike=weather==2&&slots.random.nextInt(100000)==0;
                if(strike)slots.getBlockRandomPos(0,0,0,15);
                System.out.println("thunder-slots:"+seed+":"+weather+"\\t"+strike+"|"+slots.random.nextInt()+"|"+slots.getBlockRandomPos(0,0,0,15).toShortString());
              }
              System.out.println("valid-chunk-bound\\t"+net.minecraft.world.level.chunk.status.ChunkPyramid.MAX_CHUNK_COORDINATE_VALUE);
              for(var block:List.of(Blocks.STONE,Blocks.AIR,Blocks.WALL_TORCH,Blocks.CHEST))for(boolean includeData:new boolean[]{false,true}){
                var stack=block.defaultBlockState().getCloneItemStack(null,BlockPos.ZERO,includeData);
                System.out.println("clone-item:"+net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(block)+":"+includeData+"\\t"+net.minecraft.core.registries.BuiltInRegistries.ITEM.getKey(stack.getItem())+":"+stack.getCount());
              }
              for(int i=0;i<48;i++){
                var orientation=Orientation.fromIndex(i);var fronts=new ArrayList<String>();var ups=new ArrayList<String>();
                for(var direction:Direction.values()){fronts.add(""+orientation.withFront(direction).getIndex());ups.add(""+orientation.withUp(direction).getIndex());}
                System.out.println("orientation:"+i+"\\t"+orientation.getUp().get3DDataValue()+":"+orientation.getFront().get3DDataValue()+":"+orientation.getSideBias().ordinal()+"|"+String.join(",",fronts)+"|"+String.join(",",ups));
              }
              for(boolean experiments:new boolean[]{false,true})for(Direction front:new Direction[]{null,Direction.WEST,Direction.UP}){
                var oriented=new NeighborFixture(r,100);oriented.experiments=experiments;oriented.random.setSeed(0);
                var orientation=ExperimentalRedstoneUtils.initialOrientation(oriented,front,null);
                System.out.println("initial-orientation:"+experiments+":"+(front==null?-1:front.get3DDataValue())+"\\t"+(orientation==null?-1:orientation.getIndex())+":"+oriented.random.nextInt());
              }

              for(var block:List.of(Blocks.WATER,Blocks.LAVA))for(int liquidLevel=0;liquidLevel<16;liquidLevel++){
                var state=block.defaultBlockState().setValue(net.minecraft.world.level.block.LiquidBlock.LEVEL,liquidLevel);var fluid=state.getFluidState();
                System.out.println("fluid-legacy:"+net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(block)+":"+liquidLevel+"\\t"+Block.getId(fluid.createLegacyBlock()));
              }
              var wall=net.minecraft.world.level.material.FlowingFluid.class.getDeclaredMethod("canPassThroughWall",Direction.class,net.minecraft.world.level.BlockGetter.class,BlockPos.class,BlockState.class,BlockPos.class,BlockState.class);wall.setAccessible(true);
              // Moving piston's dynamic shape requires the real level's block
              // entity query, which this minimal Level explicitly rejects.
              var wallStates=List.of(Blocks.STONE.defaultBlockState(),Blocks.WATER.defaultBlockState(),Blocks.MOVING_PISTON.defaultBlockState());
              for(var wallPositions:List.of(new int[]{2,0},new int[]{0,2},new int[]{1,1},new int[]{0,1})){
                String result;
                try{result=""+wall.invoke(null,Direction.EAST,f,new BlockPos(wallPositions[0],0,0),wallStates.get(wallPositions[0]),new BlockPos(wallPositions[1],0,0),wallStates.get(wallPositions[1]));}
                catch(java.lang.reflect.InvocationTargetException error){if(!(error.getCause() instanceof UnsupportedOperationException))throw error;result="unsupported";}
                System.out.println("dynamic-fluid-wall:"+wallPositions[0]+":"+wallPositions[1]+"\t"+result);
              }
              var states=List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState(),Blocks.STONE_SLAB.defaultBlockState(),Blocks.STONE_SLAB.defaultBlockState().setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.SLAB_TYPE,net.minecraft.world.level.block.state.properties.SlabType.TOP),Blocks.WATER.defaultBlockState(),Blocks.SHORT_GRASS.defaultBlockState());
              for(var first:states)for(var second:states)for(var direction:Direction.values()){
                boolean pass=(Boolean)wall.invoke(null,direction,f,BlockPos.ZERO,first,BlockPos.ZERO.relative(direction),second);
                System.out.println("fluid-wall:"+Block.getId(first)+":"+Block.getId(second)+":"+direction.get3DDataValue()+"\\t"+pass);
              }
              for(var tag:List.of(net.minecraft.tags.FluidTags.WATER,net.minecraft.tags.FluidTags.LAVA,net.minecraft.tags.FluidTags.BUBBLE_COLUMN_CAN_OCCUPY)){
                var names=new ArrayList<String>();for(var fluid:net.minecraft.core.registries.BuiltInRegistries.FLUID)if(fluid.is(tag))names.add(net.minecraft.core.registries.BuiltInRegistries.FLUID.getKey(fluid).toString());
                System.out.println("fluid-tag:"+tag.location()+"\\t"+String.join(",",names));
              }
              var sample=List.of(Blocks.AIR,Blocks.CAVE_AIR,Blocks.VOID_AIR,Blocks.STONE,Blocks.DIRT,Blocks.SAND,Blocks.GRAVEL,Blocks.WATER,Blocks.LAVA,Blocks.OBSIDIAN,Blocks.COBBLESTONE,Blocks.BASALT,Blocks.SOUL_SOIL,Blocks.BLUE_ICE,Blocks.SHORT_GRASS,Blocks.DANDELION,Blocks.BUSH,Blocks.FIRE,Blocks.SOUL_FIRE,Blocks.MAGMA_BLOCK);
              for(var tag:List.of(net.minecraft.tags.BlockTags.SUPPORTS_VEGETATION,net.minecraft.tags.BlockTags.FIRE,net.minecraft.tags.BlockTags.WASHED_AWAY_BY_FLUIDS,net.minecraft.tags.BlockTags.BLOCKS_MOTION_IN_HEIGHTMAP,net.minecraft.tags.BlockTags.BLOCKS_MOTION_IN_HEIGHTMAP_NO_LEAVES,net.minecraft.tags.BlockTags.ENABLES_BUBBLE_COLUMN_DRAG_DOWN,net.minecraft.tags.BlockTags.ENABLES_BUBBLE_COLUMN_PUSH_UP)){
                var names=new ArrayList<String>();for(var block:sample)if(block.defaultBlockState().is(tag))names.add(net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(block).toString());
                System.out.println("tag:"+tag.location()+"\\t"+String.join(",",names));
              }

            }
            }
            """);
        out.append("""
            final class MutationFixture extends NeighborFixture {
              final Map<ChunkPos,net.minecraft.world.level.chunk.LevelChunk> chunks=new HashMap<>();
              final Source source=new Source();
              net.minecraft.server.level.FullChunkStatus fullStatus=net.minecraft.server.level.FullChunkStatus.BLOCK_TICKING;
              net.minecraft.server.level.ChunkHolder holder;
              MutationFixture(RegistryAccess r){super(r,1000);}
              final class Source extends net.minecraft.world.level.chunk.ChunkSource {
                final net.minecraft.world.level.lighting.LevelLightEngine lights=new net.minecraft.world.level.lighting.LevelLightEngine(this,false,false);
                @Override public net.minecraft.world.level.BlockGetter getLevel(){return MutationFixture.this;}
                @Override public net.minecraft.world.level.chunk.ChunkAccess getChunk(int x,int z,net.minecraft.world.level.chunk.status.ChunkStatus status,boolean load){
                  var p=new ChunkPos(x,z);return load?chunks.computeIfAbsent(p,k->new net.minecraft.world.level.chunk.LevelChunk(MutationFixture.this,k){
                    @Override public net.minecraft.server.level.FullChunkStatus getFullStatus(){return MutationFixture.this.fullStatus;}
                  }):chunks.get(p);
                }
                @Override public void tick(java.util.function.BooleanSupplier time,boolean tick){}
                @Override public String gatherStats(){return "fixture";}
                @Override public int getLoadedChunksCount(){return chunks.size();}
                @Override public net.minecraft.world.level.lighting.LevelLightEngine getLightEngine(){return lights;}
                @Override public void onSectionEmptinessChanged(int x,int y,int z,boolean empty){trace.add("section:"+empty);}
              }
              @Override public net.minecraft.world.level.chunk.ChunkSource getChunkSource(){return source;}
              @Override public BlockState getBlockState(BlockPos p){return getChunkAt(p).getBlockState(p);}
              @Override public void sendBlockUpdated(BlockPos p,BlockState a,BlockState b,int flags){trace.add("send");if(holder!=null)holder.blockChanged(p);}
              @Override public void updatePOIOnBlockStateChange(BlockPos p,BlockState a,BlockState b){trace.add("poi");}
              @Override public void updateNeighborsAt(BlockPos p,Block b){trace.add("neighbors");neighborUpdater.updateNeighborsAtExceptFromFacing(p,b,null,null);}
              @Override public void neighborChanged(BlockState state,BlockPos p,Block b,Orientation o,boolean moved){trace.add("analog:"+p.toShortString());}
              @Override public void scheduleTick(BlockPos p,Block b,int delay){trace.add("schedule:"+delay);}
              @Override public void scheduleTick(BlockPos p,net.minecraft.world.level.material.Fluid f,int delay){trace.add("fluid:"+delay);}

              public static void run(RegistryAccess r) throws Exception {
                for(var status:List.of(net.minecraft.server.level.FullChunkStatus.FULL,net.minecraft.server.level.FullChunkStatus.BLOCK_TICKING))for(boolean ready:new boolean[]{false,true}){
                  var live=new MutationFixture(r);live.fullStatus=status;var updatePos=new BlockPos(8,0,8);
                  var chunk=live.getChunkAt(updatePos);
                  live.holder=new net.minecraft.server.level.ChunkHolder(chunk.getPos(),31,live,live.source.lights,(p,old,l,set)->{},(p,b)->List.of());
                  if(ready){
                    var future=net.minecraft.server.level.ChunkHolder.class.getDeclaredField("tickingChunkFuture");future.setAccessible(true);
                    future.set(live.holder,java.util.concurrent.CompletableFuture.completedFuture(net.minecraft.server.level.ChunkResult.of(chunk)));
                  }
                  live.setBlock(updatePos,Blocks.STONE.defaultBlockState(),18,512);
                  System.out.println("client-update:"+status+":"+ready+"\\t"+live.trace.contains("send")+":"+live.holder.hasChangesToBroadcast());
                }
                var contextClass=Class.forName("net.minecraft.world.level.material.FlowingFluid$SpreadContext");
                var contextConstructor=contextClass.getDeclaredConstructor(net.minecraft.world.level.material.FlowingFluid.class,net.minecraft.world.level.BlockGetter.class,BlockPos.class);contextConstructor.setAccessible(true);
                var slope=net.minecraft.world.level.material.FlowingFluid.class.getDeclaredMethod("getSlopeDistance",net.minecraft.world.level.LevelReader.class,BlockPos.class,int.class,Direction.class,BlockState.class,contextClass);slope.setAccessible(true);
                for(int holeDistance:new int[]{2,3,5,6}){
                  var pool=new MutationFixture(r);var origin=new BlockPos(15,0,8);
                  for(int x=7;x<=24;x++)for(int z=0;z<=16;z++)pool.getChunkAt(new BlockPos(x,-1,z)).setBlockState(new BlockPos(x,-1,z),Blocks.STONE.defaultBlockState(),816);
                  var hole=origin.east(holeDistance).below();pool.getChunkAt(hole).setBlockState(hole,Blocks.AIR.defaultBlockState(),816);
                  var context=contextConstructor.newInstance(net.minecraft.world.level.material.Fluids.WATER,pool,origin);
                  int distance=(Integer)slope.invoke(net.minecraft.world.level.material.Fluids.WATER,pool,origin.east(),1,Direction.WEST,Blocks.AIR.defaultBlockState(),context);
                  System.out.println("water-slope:"+holeDistance+"\\t"+distance);
                }
                var analog=new MutationFixture(r);var center=new BlockPos(8,0,8);
                for(var entry:Map.of(center.north(),Blocks.COMPARATOR,center.east(),Blocks.STONE,center.east(2),Blocks.COMPARATOR,center.south(),Blocks.GLASS,center.south(2),Blocks.COMPARATOR,center.west(),Blocks.STONE_SLAB,center.west(2),Blocks.COMPARATOR).entrySet())analog.getChunkAt(entry.getKey()).setBlockState(entry.getKey(),entry.getValue().defaultBlockState(),816);
                analog.trace.clear();analog.updateNeighbourForOutputSignal(center,Blocks.CAKE);
                System.out.println("analog-neighbors\\t"+String.join(";",analog.trace));
                var waterlogged=new MutationFixture(r);var pos=new BlockPos(8,0,8);
                for(var block:List.of(Blocks.AIR,Blocks.STONE,Blocks.WATER,Blocks.SHORT_GRASS))for(int held=0;held<3;held++){
                  var placementLevel=new MutationFixture(r);var state=block.defaultBlockState();placementLevel.setBlock(pos,state,816,512);
                  var item=held==0?net.minecraft.world.item.Items.AIR:held==1?block.asItem():net.minecraft.world.item.Items.DIRT;
                  var stack=new net.minecraft.world.item.ItemStack(item);
                  var context=new net.minecraft.world.item.context.DirectionalPlaceContext(placementLevel,pos,Direction.DOWN,stack,Direction.UP);
                  System.out.println("directional-replacement:"+net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(block)+":"+held+"\\t"+state.canBeReplaced(context));
                }
                for(var type:net.minecraft.world.level.block.state.properties.SlabType.values())for(var liquid:List.of(net.minecraft.world.level.material.Fluids.WATER.getSource(false),net.minecraft.world.level.material.Fluids.FLOWING_WATER.getFlowing(7,false),net.minecraft.world.level.material.Fluids.LAVA.getSource(false))){
                  var slabLevel=new MutationFixture(r);var slab=Blocks.STONE_SLAB.defaultBlockState().setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.SLAB_TYPE,type);
                  slabLevel.setBlock(pos,slab,816,512);
                  var behavior=(net.minecraft.world.level.block.LiquidBlockContainer)Blocks.STONE_SLAB;
                  boolean can=behavior.canPlaceLiquid(null,slabLevel,pos,slab,liquid.getType());
                  boolean placed=behavior.placeLiquid(slabLevel,pos,slab,liquid);
                  boolean wet=slabLevel.getBlockState(pos).getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.WATERLOGGED);
                  var bucket=((net.minecraft.world.level.block.BucketPickup)Blocks.STONE_SLAB).pickupBlock(null,slabLevel,pos,slabLevel.getBlockState(pos));
                  System.out.println("slab-waterlogged:"+type.getSerializedName()+":"+net.minecraft.core.registries.BuiltInRegistries.FLUID.getKey(liquid.getType())+"\\t"+can+":"+placed+":"+wet+":"+net.minecraft.core.registries.BuiltInRegistries.ITEM.getKey(bucket.getItem()));
                }
                waterlogged.setBlock(pos,Blocks.IRON_CHAIN.defaultBlockState(),528,512);waterlogged.trace.clear();
                var fluid=net.minecraft.world.level.material.Fluids.WATER.getSource(false);
                boolean placed=((net.minecraft.world.level.block.LiquidBlockContainer)Blocks.IRON_CHAIN).placeLiquid(waterlogged,pos,waterlogged.getBlockState(pos),fluid);
                boolean wet=waterlogged.getBlockState(pos).getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.WATERLOGGED);
                var item=((net.minecraft.world.level.block.BucketPickup)Blocks.IRON_CHAIN).pickupBlock(null,waterlogged,pos,waterlogged.getBlockState(pos));
                System.out.println("simple-waterlogged\\t"+placed+":"+wet+":"+net.minecraft.core.registries.BuiltInRegistries.ITEM.getKey(item.getItem())+":"+String.join(",",waterlogged.trace));
                for(int flags:new int[]{0,1,2,3,4,8,16,32,64,128,256,512,816}){
                  var f=new MutationFixture(r);
                  boolean result=f.setBlock(new BlockPos(8,0,8),Blocks.SAND.defaultBlockState(),flags,512);
                  var c=f.getChunk(0,0);
                  var heights=new ArrayList<String>();
                  for(var map:List.of(net.minecraft.world.level.levelgen.Heightmap.Types.MOTION_BLOCKING,net.minecraft.world.level.levelgen.Heightmap.Types.MOTION_BLOCKING_NO_LEAVES,net.minecraft.world.level.levelgen.Heightmap.Types.OCEAN_FLOOR,net.minecraft.world.level.levelgen.Heightmap.Types.WORLD_SURFACE))heights.add(""+c.getHeight(map,8,8));
                  System.out.println("setblock-"+flags+"\\t"+result+":"+String.join(",",f.trace)+":"+String.join(",",heights));
                }
              }
            }
            """);
        Files.writeString(path,out);
    }
    static void stub(StringBuilder out,Method m,Set<String> seen){
        var params=m.getParameterTypes(); var key=m.getName()+Arrays.toString(params);
        if(!seen.add(key)||m.getName().equals("enabledFeatures"))return;
        out.append("@Override public ").append(type(m.getReturnType())).append(' ').append(m.getName()).append('(');
        for(int i=0;i<params.length;i++){if(i>0)out.append(',');out.append(type(params[i])).append(" p").append(i);}
        out.append("){throw new UnsupportedOperationException(\"").append(m.getName()).append("\");}\n");
    }
}
