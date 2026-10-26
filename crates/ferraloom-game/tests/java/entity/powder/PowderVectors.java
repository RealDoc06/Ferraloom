import java.util.*;
import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.FallingBlockEntity;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.entity.EntityInLevelCallback;
import net.minecraft.world.phys.*;
import net.minecraft.network.protocol.game.*;
import org.mockito.Answers;
import static org.mockito.Mockito.*;

/** Real FallingBlockEntity.tick, BlockGetter.clip and ConcretePowderBlock.onLand.
 * Native noPhysics/noGravity isolate the water/landing tail from collision and
 * prior block-contact effects. The controlled Level captures flags-3 writes and
 * tracking sends without running unrelated neighbor or chunk publication work. */
public final class PowderVectors {
  static String pos(BlockPos p){return p.getX()+","+p.getY()+","+p.getZ();}
  static String vec(Vec3 v){return Long.toHexString(Double.doubleToRawLongBits(v.x))+","+Long.toHexString(Double.doubleToRawLongBits(v.y))+","+Long.toHexString(Double.doubleToRawLongBits(v.z));}
  static String blocks(Map<BlockPos,BlockState> states){return states.entrySet().stream().sorted(Comparator.comparing(e->pos(e.getKey()))).map(e->pos(e.getKey())+","+Block.getId(e.getValue())).reduce((a,b)->a+";"+b).orElse("-");}
  static BlockPos p(int y){return new BlockPos(8,y,8);}
  static BlockState water(int amount){return Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,amount);}
  static Field field(Class<?> type,String name)throws Exception{var f=type.getDeclaredField(name);f.setAccessible(true);return f;}
  public static void main(String[] args)throws Exception {
    var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
    NeighborVectors.load(); // Bind the jar's actual WATER and block tags.
    Block[] powders=java.util.stream.Stream.of("white_concrete_powder","red_concrete_powder","black_concrete_powder").map(name->BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.withDefaultNamespace(name))).toArray(Block[]::new);
    for(Block powder:powders)for(int test=0;test<14;test++) {
      var world=new HashMap<BlockPos,BlockState>();var trace=new ArrayList<String>();
      Vec3 from=new Vec3(8.5,65.25,8.5), velocity=new Vec3(0,-0.125,0);boolean ground=false;
      switch(test) {
        case 0 -> world.put(p(65),water(0)); // Slow submerged, unsupported below.
        case 1 -> {from=new Vec3(8.5,68.0,8.5);velocity=new Vec3(0,-4,0);world.put(p(66),water(0));} // Crossed source, endpoint dry.
        case 2 -> {ground=true;world.put(p(64),Blocks.STONE.defaultBlockState());} // Dry landing.
        case 3 -> {ground=true;world.put(p(64),Blocks.STONE.defaultBlockState());world.put(p(66),water(0));} // Above-water onLand.
        case 4 -> {from=new Vec3(8.5,68,8.5);velocity=new Vec3(0,-4,0);world.put(p(66),water(3));} // SOURCE_ONLY misses flow.
        case 5 -> world.put(p(65),water(3)); // Current flowing WATER still solidifies.
        case 6 -> {from=new Vec3(8.5,68,8.5);velocity=new Vec3(0,-4,0);world.put(p(66),Blocks.LAVA.defaultBlockState());}
        case 7 -> {velocity=new Vec3(0,-1,0);world.put(p(65),water(0));} // No ray at exactly speed squared 1.
        case 8 -> {velocity=new Vec3(0,-Math.nextUp(1.0),0);world.put(p(65),water(0));}
        case 9 -> {from=new Vec3(8.5,68,8.5);velocity=new Vec3(0,-4,0);world.put(p(67),Blocks.STONE.defaultBlockState());world.put(p(66),water(0));} // First block occludes water.
        case 10 -> {ground=true;world.put(p(64),Blocks.WATER.defaultBlockState());} // Water below alone cannot harden.
        case 11 -> {world.put(p(65),Blocks.OAK_SLAB.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true));}
        case 12 -> {from=new Vec3(8.5,68,8.5);velocity=new Vec3(0,-4,0);world.put(p(66),Blocks.OAK_SLAB.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true));}
        case 13 -> {velocity=new Vec3(0,-0.125,0);} // Dry airborne remains active.
      }
      String initial=blocks(world);int[] clips={0};
      var chunkMap=mock(ChunkMap.class,call->{
        if(call.getMethod().getName().equals("sendToTrackingPlayers")){
          Object packet=call.getArguments()[1];
          if(packet instanceof ClientboundBlockUpdatePacket b)trace.add("block:"+pos(b.getPos())+":"+Block.getId(b.getBlockState()));
          else if(packet instanceof ClientboundAddTransientBlockPacket b)trace.add("transient:"+pos(b.getPos())+":"+Block.getId(b.getBlockState()));
          else throw new AssertionError(packet);
          return null;
        }
        return Answers.RETURNS_DEFAULTS.answer(call);
      });
      var cache=mock(ServerChunkCache.class);field(ServerChunkCache.class,"chunkMap").set(cache,chunkMap);
      ServerLevel level=mock(ServerLevel.class,call->{
        var a=call.getArguments();return switch(call.getMethod().getName()){
          case "getNextEntityId" -> 1;
          case "getBlockState" -> world.getOrDefault((BlockPos)a[0],Blocks.AIR.defaultBlockState());
          case "getFluidState" -> world.getOrDefault((BlockPos)a[0],Blocks.AIR.defaultBlockState()).getFluidState();
          case "getChunkSource" -> cache;
          case "getMinY" -> -64;
          case "getHeight" -> 384;
          case "enabledFeatures" -> FeatureFlags.DEFAULT_FLAGS;
          case "setBlockAndUpdate" -> {BlockPos at=((BlockPos)a[0]).immutable();BlockState state=(BlockState)a[1];trace.add("set:"+pos(at)+":"+Block.getId(state));world.put(at,state);yield true;}
          case "clip" -> {clips[0]++;yield call.callRealMethod();}
          case "clipWithInteractionOverride" -> call.callRealMethod();
          default -> Answers.RETURNS_DEFAULTS.answer(call);
        };
      });
      var entity=new FallingBlockEntity(EntityTypes.FALLING_BLOCK,level);
      field(FallingBlockEntity.class,"blockState").set(entity,powder.defaultBlockState());
      entity.dropItem=false;entity.noPhysics=true;entity.setNoGravity(true);entity.setPos(from);entity.setOldPosAndRot();entity.setDeltaMovement(velocity);entity.setOnGround(ground);
      entity.setLevelCallback(new EntityInLevelCallback(){public void onMove(){}public void onRemove(Entity.RemovalReason r){trace.add("remove");}});
      entity.tick();
      System.out.println("powder-tick\t"+test+"\t"+Block.getId(powder.defaultBlockState())+"\t"+vec(from)+"\t"+vec(velocity)+"\t"+ground+"\t"+initial+"\t"+vec(entity.position())+"\t"+vec(entity.getDeltaMovement())+"\t"+entity.onGround()+"\t"+entity.isRemoved()+"\t"+entity.time+"\t"+Block.getId(entity.getBlockState())+"\t"+blocks(world)+"\t"+String.join(";",trace)+"\t"+clips[0]);
    }
  }
}
