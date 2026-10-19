import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;

/** Native placement on actual chunks, including every concrete registry variant,
 * all clicked faces and directions, and source/flowing/waterlogged replacements. */
public final class OrientationVectors extends MutationFixture {
    static final BlockPos POS = new BlockPos(8,64,8);
    static final Set<Class<?>> OWNERS = Set.of(RotatedPillarBlock.class,
        InfestedRotatedPillarBlock.class,ChainBlock.class,GlazedTerracottaBlock.class,
        AnvilBlock.class,HeavyCoreBlock.class,WaterloggedTransparentBlock.class,
        LoomBlock.class,StonecutterBlock.class);
    OrientationVectors(RegistryAccess registries) { super(registries); }
    void raw(BlockPos pos,BlockState state) { getChunkAt(pos).setBlockState(pos,state,816); }
    static boolean supported(Block block) throws Exception {
        return OWNERS.contains(block.getClass().getMethod("getStateForPlacement",BlockPlaceContext.class).getDeclaringClass());
    }
    static boolean waterlogged(Block block) {
        return block instanceof ChainBlock || block instanceof HeavyCoreBlock || block instanceof WaterloggedTransparentBlock;
    }
    static void run() throws Exception {
        var registries=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var lookup=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var level=new OrientationVectors(registries);
        var replaced=new ArrayList<BlockState>(List.of(Blocks.AIR.defaultBlockState(),Blocks.STONE.defaultBlockState()));
        for(int amount:new int[]{0,1,7,8,15})replaced.add(Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,amount));
        for(int amount:new int[]{0,8})replaced.add(Blocks.LAVA.defaultBlockState().setValue(LiquidBlock.LEVEL,amount));
        replaced.add(Blocks.IRON_CHAIN.defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true));
        replaced.add(Blocks.COPPER_GRATE.weathering().unaffected().defaultBlockState().setValue(BlockStateProperties.WATERLOGGED,true));
        int count=0;
        for(var block:BuiltInRegistries.BLOCK)if(supported(block)){
            count++;
            for(var direction:Direction.values())for(var face:Direction.values())for(var old:replaced){
                level.raw(POS,old);
                var context=new DirectionalPlaceContext(level,POS,direction,ItemStack.EMPTY,face);
                var result=block.getStateForPlacement(context);
                System.out.println("BUILDING\tplace\t"+BuiltInRegistries.BLOCK.getId(block)+"\t"+direction.get3DDataValue()+"\t"+face.get3DDataValue()+"\t"+Block.getId(old)+"\t"+(result==null?-1:Block.getId(result)));
            }
            if(waterlogged(block)||block instanceof AnvilBlock){
                for(var state:block.getStateDefinition().getPossibleStates())for(var direction:Direction.values())for(var neighbor:replaced.subList(0,4)){
                    var at=POS.relative(direction);level.raw(POS,state);level.raw(at,neighbor);level.trace.clear();
                    var result=state.updateShape(level,level,POS,direction,at,neighbor,level.random);
                    var ticks=level.trace.stream().filter(s->s.startsWith("schedule:")||s.startsWith("fluid:")).toList();
                    System.out.println("BUILDING\tupdate\t"+Block.getId(state)+"\t"+direction.get3DDataValue()+"\t"+Block.getId(neighbor)+"\t"+Block.getId(result)+"\t"+String.join(",",ticks));
                }
            }
        }
        if(count!=101)throw new AssertionError("Expected 101 registered blocks, got "+count);
    }
}

final class OrientationBootstrap {
    public static void main(String[] args) throws Exception {
        var output=System.out;var errors=System.err;
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
        OrientationVectors.run();
    }
}
