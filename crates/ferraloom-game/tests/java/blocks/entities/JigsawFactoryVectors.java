import java.util.*;
import java.util.stream.Collectors;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.resources.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;
import static org.mockito.Mockito.*;

/** Native JigsawBlockEntity constructor, load, update, components and round trip. */
public final class JigsawFactoryVectors {
  static final BlockPos POS = FactoryBatchTwoVectors.POS;
  static HolderLookup.Provider context;
  static void prime(JigsawBlockEntity entity) {
    entity.setName(Identifier.parse("test:old_name"));
    entity.setTarget(Identifier.parse("test:old_target"));
    entity.setPool(ResourceKey.create(Registries.TEMPLATE_POOL, Identifier.parse("test:old_pool")));
    entity.setJoint(JigsawBlockEntity.JointType.ALIGNED);
    entity.setFinalState("old:state");
    entity.setPlacementPriority(7);
    entity.setSelectionPriority(9);
  }
  static String scalar(JigsawBlockEntity entity) {
    return entity.getName() + "," + entity.getTarget() + "," + entity.getPool().identifier()
      + "," + entity.getJoint().getSerializedName() + "," + entity.getFinalState()
      + "," + entity.getPlacementPriority() + "," + entity.getSelectionPriority();
  }
  static String units(String value) {
    return value.chars().mapToObj(c -> String.format("%04x", c)).collect(Collectors.joining(","));
  }
  public static void main(String[] args) throws Exception {
    var output = System.out;
    SharedConstants.tryDetectVersion();
    Bootstrap.bootStrap();
    System.setOut(output);
    var data = NeighborVectors.load();
    var builtins = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    context = HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(), data.listRegistries()));
    FactoryBatchTwoVectors.context = context;
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var level = mock(ServerLevel.class);
    doReturn(data).when(level).registryAccess();
    var inputs = List.of(
      "@new", "{}",
      "{name:'village/temple',target:'minecraft:door',pool:'test:not_registered',final_state:'stone[axis=x]',joint:'aligned',placement_priority:2147483647,selection_priority:-2147483648}",
      "{name:':path',target:'test:target',pool:'village/plains/streets',joint:'rollable'}",
      "{name:7,target:[],pool:{},final_state:7,joint:1,placement_priority:'bad',selection_priority:[]}",
      "{name:'BAD',target:'test::bad',pool:'bad space',joint:'ALIGNED'}",
      "{name:'',target:':',pool:'',final_state:'',joint:'invalid'}",
      "{placement_priority:1b,selection_priority:-2s}",
      "{placement_priority:2147483648L,selection_priority:9223372036854775807L}",
      "{placement_priority:-1.25f,selection_priority:-1.25d}",
      "{components:{'minecraft:custom_name':'extra','minecraft:max_stack_size':7},joint:'aligned'}",
      "{joint:'',final_state:'not a block state'}"
    );
    for (var state : Blocks.JIGSAW.getStateDefinition().getPossibleStates()) {
      for (String input : inputs) {
        var entity = new JigsawBlockEntity(POS, state);
        if (!input.equals("@new")) {
          prime(entity);
          FactoryBatchTwoVectors.load(entity, input);
        }
        entity.setLevel(level);
        var saved = entity.saveWithFullMetadata(context);
        var round = BlockEntity.loadStatic(POS, state, saved, context);
        System.out.println("jigsaw-data\t" + Block.getId(state) + "\t" + input + "\t" + saved
          + "\t" + entity.getUpdateTag(context) + "\t" + (entity.getUpdatePacket() != null)
          + "\t" + FactoryBatchTwoVectors.components(entity) + "\t" + scalar(entity)
          + "\t" + round.saveWithFullMetadata(context));
      }
      var entity = new JigsawBlockEntity(POS, state);
      prime(entity);
      String stackTag = "{id:'jigsaw',count:1,components:{'minecraft:custom_name':'placed','minecraft:max_stack_size':7,'minecraft:custom_data':{payload:13}}}";
      var stack = ItemStack.CODEC.parse(context.createSerializationContext(NbtOps.INSTANCE), TagParser.parseCompoundFully(stackTag)).getOrThrow();
      entity.applyComponentsFromItemStack(stack);
      System.out.println("jigsaw-components\t" + Block.getId(state) + "\t" + stackTag + "\t" + entity.saveWithFullMetadata(context)
        + "\t" + entity.getUpdateTag(context) + "\t" + FactoryBatchTwoVectors.components(entity) + "\t" + scalar(entity));
      var tag = new CompoundTag();
      tag.putString("final_state", new String(new char[]{'a', 0xd800, 'b', 0xdfff}));
      entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING, context, tag));
      var round = (JigsawBlockEntity)BlockEntity.loadStatic(POS, state, entity.saveWithFullMetadata(context), context);
      System.out.println("jigsaw-utf16\t" + Block.getId(state) + "\t" + units(entity.getFinalState()) + "\t" + units(round.getFinalState()));
    }
  }
}
