import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.SlabType;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.*;

/** Real BlockCollisions and CollisionGetter queries around the shape epsilon. */
public final class BlockCollisionsVectors implements CollisionGetter {
    static final BlockPos POS = new BlockPos(0,64,0);
    final BlockState state;
    BlockCollisionsVectors(BlockState state) {this.state=state;}
    @Override public int getMinY(){return -64;}
    @Override public int getHeight(){return 384;}
    @Override public BlockState getBlockState(BlockPos pos){return pos.equals(POS)?state:Blocks.AIR.defaultBlockState();}
    @Override public FluidState getFluidState(BlockPos pos){return getBlockState(pos).getFluidState();}
    @Override public BlockEntity getBlockEntity(BlockPos pos){return null;}
    @Override public BlockGetter getChunkForCollisions(int x,int z){return this;}
    @Override public WorldBorder getWorldBorder(){throw new AssertionError("no entity border query");}
    @Override public List<VoxelShape> getEntityCollisions(Entity entity,AABB box){return List.of();}
    static String bits(double value){return Long.toHexString(Double.doubleToRawLongBits(value));}
    static String box(AABB b){return String.join(",",bits(b.minX),bits(b.minY),bits(b.minZ),bits(b.maxX),bits(b.maxY),bits(b.maxZ));}
    public static void main(String[] args){
        var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
        var names=List.of("thin-below","wider-below","touching-above","thin-east","thin-south");
        var boxes=List.of(
            new AABB(0.1,64.9999999,0.1,0.9,65,0.9),
            new AABB(0.1,64.9999998,0.1,0.9,65,0.9),
            new AABB(0.1,65,0.1,0.9,65.5,0.9),
            new AABB(0.9999999,64.6,0.1,1,64.9,0.9),
            new AABB(0.1,64.6,0.9999999,0.9,64.9,1));
        for(var state:List.of(Blocks.STONE.defaultBlockState(),Blocks.STONE_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.TOP),Blocks.AIR.defaultBlockState())){
            var fixture=new BlockCollisionsVectors(state);
            for(int i=0;i<boxes.size();i++){
                var query=boxes.get(i);var hits=new ArrayList<String>();
                for(var shape:fixture.getBlockCollisions(null,query))hits.add(box(shape.bounds()));
                boolean singleton=state.getCollisionShape(fixture,POS,CollisionContext.empty())==Shapes.block();
                System.out.println("block-collision-epsilon\t"+BuiltInRegistries.BLOCK.getKey(state.getBlock())+"\t"+names.get(i)+"\t"+box(query)+"\t"+singleton+"\t"+hits.size()+"\t"+fixture.noCollision(null,query)+"\t"+(hits.isEmpty()?"-":String.join(";",hits)));
            }
        }
    }
}
