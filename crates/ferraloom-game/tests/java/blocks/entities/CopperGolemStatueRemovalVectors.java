import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import static org.mockito.Mockito.*;
import static org.mockito.ArgumentMatchers.*;

/** Real statue removal virtual and Level comparator traversal; only the world boundary is mocked. */
public final class CopperGolemStatueRemovalVectors {
  public static void main(String[] args) {
    var output = System.out;
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    System.setOut(output);
    var world = NeighborVectors.load();
    var builtin = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var registries = HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(), world.listRegistries()));
    var pos = new BlockPos(8, 5, 8);
    var statues = new ArrayList<Block>();
    for (var block : BuiltInRegistries.BLOCK) if (block instanceof CopperGolemStatueBlock) statues.add(block);
    var replacements = new ArrayList<>(statues); replacements.add(Blocks.STONE);
    for (var old : statues) for (var replacement : replacements) {
      var receiver = ((EntityBlock)old).newBlockEntity(pos, old.defaultBlockState());
      if (!(receiver instanceof CopperGolemStatueBlockEntity)) throw new AssertionError(old);
      receiver.setComponents(DataComponentMap.builder().set(DataComponents.CUSTOM_NAME, Component.literal("kept")).build());
      boolean keep = replacement.defaultBlockState().shouldChangedStateKeepBlockEntity(old.defaultBlockState());
      if (keep) receiver.setBlockState(replacement.defaultBlockState());
      System.out.println("statue-keep\t" + Block.getId(old.defaultBlockState()) + "\t" + Block.getId(replacement.defaultBlockState()) + "\t" + keep + "\t" + receiver.saveWithFullMetadata(registries));
    }
    for (var block : BuiltInRegistries.BLOCK) {
      if (!(block instanceof CopperGolemStatueBlock)) continue;
      for (boolean moved : List.of(false, true)) {
        var trace = new ArrayList<String>();
        var level = mock(ServerLevel.class);
        doReturn(true).when(level).hasChunkAt(any(BlockPos.class));
        doReturn(Blocks.COMPARATOR.defaultBlockState()).when(level).getBlockState(any(BlockPos.class));
        doAnswer(call -> {
          trace.add("u:" + BuiltInRegistries.BLOCK.getId(call.getArgument(1)));
          return call.callRealMethod();
        }).when(level).updateNeighbourForOutputSignal(any(BlockPos.class), any(Block.class));
        doAnswer(call -> {
          BlockPos target = call.getArgument(1);
          Block source = call.getArgument(2);
          trace.add("n:" + target.getX() + "," + target.getY() + "," + target.getZ()
            + ":" + BuiltInRegistries.BLOCK.getId(source) + ":" + call.getArgument(3) + ":" + call.getArgument(4));
          return null;
        }).when(level).neighborChanged(any(), any(), any(), any(), anyBoolean());
        clearInvocations(level);
        var state = block.defaultBlockState();
        state.affectNeighborsAfterRemoval(level, pos, moved);
        if (trace.size() != 5) throw new AssertionError(trace);
        var allowed = Set.of("updateNeighbourForOutputSignal", "hasChunkAt", "getBlockState", "neighborChanged");
        for (var call : mockingDetails(level).getInvocations()) {
          if (!allowed.contains(call.getMethod().getName())) throw new AssertionError(call);
        }
        System.out.println("statue-remove\t" + Block.getId(state) + "\t" + moved + "\t" + String.join(";", trace));
      }
    }
  }
}
