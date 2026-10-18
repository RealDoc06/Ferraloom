import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.context.*;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.*;
import java.util.*;

/** Native context initialization on the real Level fixture, without player mocks. */
public final class PlacementContextVectors {
    static final class Ordinary extends BlockPlaceContext {
        Ordinary(Level level,ItemStack item,BlockPos pos,Direction face) {
            super(level,null,InteractionHand.MAIN_HAND,item,new BlockHitResult(new Vec3(8.25,0.75,8.5),face,pos,false));
        }
    }
    public static void main(String[] args) {
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
        var registries=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(net.minecraft.core.registries.BuiltInRegistries.REGISTRY);
        var components=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
        net.minecraft.core.registries.BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(components).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var pos=new BlockPos(8,0,8);
        for(var block:List.of(Blocks.AIR,Blocks.STONE,Blocks.WATER,Blocks.SHORT_GRASS))for(int held=0;held<3;held++) {
            var level=new MutationFixture(registries);level.setBlock(pos,block.defaultBlockState(),816,512);
            var item=new ItemStack(held==0?Items.AIR:held==1?block.asItem():Items.DIRT);
            var context=new Ordinary(level,item,pos,Direction.UP);var clicked=context.getClickedPos();
            System.out.println("ordinary:"+net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(block)+":"+held+"\t"+context.replacingClickedOnBlock()+","+clicked.getX()+","+clicked.getY()+","+clicked.getZ()+","+context.canPlace()+","+context.getHorizontalDirection().getName()+","+context.isSecondaryUseActive()+","+context.getRotation());
        }
        for(var direction:Direction.values()) {
            var level=new MutationFixture(registries);
            var context=new DirectionalPlaceContext(level,pos,direction,ItemStack.EMPTY,Direction.UP);
            String order=String.join(",",Arrays.stream(context.getNearestLookingDirections()).map(Direction::getName).toList());
            System.out.println("directional:"+direction.getName()+"\t"+order+";"+context.getHorizontalDirection().getName()+";"+context.getRotation()+";"+context.getNearestLookingDirection().getName());
        }
    }
}
