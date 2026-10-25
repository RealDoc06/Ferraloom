import java.nio.file.*;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.level.block.state.properties.*;
import net.minecraft.world.phys.shapes.*;
import static org.mockito.Mockito.*;

/** Executes the actual native scan, cached collision shapes and positional context. */
public final class SmokeyVectors {
    static final BlockPos POS=new BlockPos(8,80,8);
    static final List<String> rows=new ArrayList<>();
    static void capture(String name,List<BlockState> column,boolean probe) {
        var reads=new ArrayList<String>();
        var level=mock(Level.class);
        final var initial=column;
        doAnswer(call->{
            BlockPos at=call.getArgument(0);
            int depth=POS.getY()-at.getY();reads.add(Integer.toString(depth));
            if(at.getX()!=POS.getX()||at.getZ()!=POS.getZ()||depth<1||depth>6)throw new AssertionError(at);
            return initial.get(depth-1);
        }).when(level).getBlockState(any());
        var ids=column.stream().map(state->Integer.toString(Block.getId(state))).toList();
        if(probe) {
            var state=mock(BlockState.class);
            doAnswer(call->{
                BlockPos at=call.getArgument(1);
                if(!at.equals(POS)||call.getArgument(2)!=CollisionContext.empty())throw new AssertionError("wrong shape context");
                reads.add("shape@0:empty");return Shapes.block();
            }).when(state).getCollisionShape(any(BlockGetter.class),any(BlockPos.class),any(CollisionContext.class));
            column=new ArrayList<>(column);column.set(0,state);
            final var actual=column;
            doAnswer(call->{BlockPos at=call.getArgument(0);int depth=POS.getY()-at.getY();reads.add(Integer.toString(depth));return actual.get(depth-1);}).when(level).getBlockState(any());
        }
        boolean result=CampfireBlock.isSmokeyPos(level,POS);
        rows.add(name+"\t"+String.join(",",ids)+"\t"+result+"\t"+String.join(",",reads));
    }
    static List<BlockState> empty() {return new ArrayList<>(Collections.nCopies(6,Blocks.AIR.defaultBlockState()));}
    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();
        for(var campfire:List.of(Blocks.CAMPFIRE,Blocks.SOUL_CAMPFIRE))for(boolean lit:List.of(false,true))for(int depth=1;depth<=6;depth++) {
            var column=empty();column.set(depth-1,campfire.defaultBlockState().setValue(CampfireBlock.LIT,lit));
            capture("campfire-"+campfire.getName().getString()+"-"+lit+"-"+depth,column,false);
        }
        capture("air",empty(),false);
        for(var obstacle:List.of(Blocks.STONE.defaultBlockState(),Blocks.GLASS.defaultBlockState(),Blocks.OAK_FENCE.defaultBlockState(),Blocks.IRON_BARS.defaultBlockState(),Blocks.OAK_SLAB.defaultBlockState(),Blocks.OAK_SLAB.defaultBlockState().setValue(SlabBlock.TYPE,SlabType.TOP),Blocks.OAK_TRAPDOOR.defaultBlockState(),Blocks.OAK_TRAPDOOR.defaultBlockState().setValue(TrapDoorBlock.OPEN,true),Blocks.LADDER.defaultBlockState(),Blocks.TORCH.defaultBlockState(),Blocks.GRASS_BLOCK.defaultBlockState(),Blocks.REDSTONE_TORCH.defaultBlockState(),Blocks.CAMPFIRE.defaultBlockState().setValue(CampfireBlock.LIT,false)))for(int depth:new int[]{2,3,5,6}) {
            var column=empty();column.set(0,obstacle);column.set(depth-1,Blocks.CAMPFIRE.defaultBlockState());
            capture("obstacle-"+Block.getId(obstacle)+"-"+depth,column,false);
        }
        for(int depth=1;depth<=5;depth++) {
            var column=empty();column.set(depth-1,Blocks.STONE.defaultBlockState());column.set(depth,Blocks.SOUL_CAMPFIRE.defaultBlockState());
            capture("blocked-extra-"+depth,column,false);
        }
        var column=empty();column.set(0,Blocks.SCAFFOLDING.defaultBlockState());column.set(1,Blocks.CAMPFIRE.defaultBlockState());
        capture("original-position-empty-context",column,true);
        for(boolean smokey:List.of(false,true)) {
            var reads=new ArrayList<String>();
            var level=mock(net.minecraft.server.level.ServerLevel.class);
            doReturn(true).when(level).isLoaded(any());
            doAnswer(call->{BlockPos at=call.getArgument(0);int depth=POS.getY()-at.getY();reads.add(Integer.toString(depth));return depth==0?Blocks.GRASS_BLOCK.defaultBlockState():Blocks.AIR.defaultBlockState();}).when(level).getBlockState(any());
            String input="{\"smokey\":"+smokey+",\"block\":{\"blocks\":\"minecraft:grass_block\"}}";
            var predicate=net.minecraft.advancements.predicates.LocationPredicate.CODEC.parse(AdvancementVectors.ops,com.google.gson.JsonParser.parseString(input)).getOrThrow();
            boolean result=predicate.matches(level,8.5,80,8.5);
            rows.add("location-"+smokey+"\t"+input+"\t"+result+"\t"+String.join(",",reads));
        }
        Files.write(Path.of(args[0]),rows);System.out.println(rows.size()+" native smokey observations");
    }
}
