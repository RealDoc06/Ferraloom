import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.VoxelShape;

/** Verifies CollisionGetter stops before later required collision reads. */
public final class CollisionVectors implements CollisionGetter {
  final BlockState obstacle;
  CollisionVectors(BlockState obstacle) {this.obstacle=obstacle;}
  @Override public int getMinY() {return -64;}
  @Override public int getHeight() {return 384;}
  @Override public BlockState getBlockState(BlockPos pos) {
    if(pos.getZ()>=16) throw new IllegalStateException("required-read");
    return pos.equals(new BlockPos(0,64,0))?obstacle:Blocks.AIR.defaultBlockState();
  }
  @Override public FluidState getFluidState(BlockPos pos) {return getBlockState(pos).getFluidState();}
  @Override public BlockEntity getBlockEntity(BlockPos pos) {return null;}
  @Override public BlockGetter getChunkForCollisions(int x,int z) {return this;}
  @Override public WorldBorder getWorldBorder() {throw new AssertionError();}
  @Override public List<VoxelShape> getEntityCollisions(Entity source,AABB box) {return List.of();}
  public static void run() {
    for(Block block:List.of(Blocks.STONE,Blocks.AIR,Blocks.WATER)) for(boolean liquids:List.of(false,true)) {
      var fixture=new CollisionVectors(block.defaultBlockState());String result;
      try {result=Boolean.toString(fixture.noCollision(null,new AABB(0.1,64.1,0.1,0.9,64.9,16.2),liquids));}
      catch(IllegalStateException missing) {result=missing.getMessage();}
      System.out.println("no-collision-order\t"+BuiltInRegistries.BLOCK.getKey(block).toString()+"\t"+liquids+"\t"+result);
    }
  }
}
