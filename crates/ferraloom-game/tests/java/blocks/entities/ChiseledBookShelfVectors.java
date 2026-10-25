import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;

/** Native fixed-list loading and CONTAINER components, without item mutations. */
public final class ChiseledBookShelfVectors {
  static final BlockPos POS = new BlockPos(8, 64, 8);
  static HolderLookup.Provider context;
  static String components(ChiseledBookShelfBlockEntity entity) {
    return DataComponentMap.CODEC.encodeStart(context.createSerializationContext(NbtOps.INSTANCE), entity.collectComponents()).getOrThrow().toString();
  }
  public static void main(String[] args) throws Exception {
    var output = System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
    var world = NeighborVectors.load(); var builtin = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    context = HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(), world.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var block = (ChiseledBookShelfBlock)Blocks.CHISELED_BOOKSHELF;
    for (var state : block.getStateDefinition().getPossibleStates()) {
      var entity = (ChiseledBookShelfBlockEntity)block.newBlockEntity(POS, state);
      System.out.println("shelf-default\t" + Block.getId(state) + "\t" + entity.saveWithFullMetadata(context) + "\t" + entity.getContainerSize() + "\t" + entity.getMaxStackSize() + "\t" + entity.getUpdateTag(context) + "\t" + (entity.getUpdatePacket() != null) + "\t" + components(entity) + "\t" + (block.getTicker(null, state, BlockEntityTypes.CHISELED_BOOKSHELF) != null));
    }
    var inputs = List.of(
      "{}", "{Items:[]}", "{Items:[{Slot:0b,id:'book',count:1}]}",
      "{Items:[{Slot:0b,id:'stone',count:64},{Slot:5b,id:'enchanted_book',count:3,components:{'minecraft:custom_name':'stored'}}]}",
      "{Items:[{Slot:0b,id:'book',count:0},{Slot:1b,id:'book',count:100}]}",
      "{Items:[{Slot:2b,id:'book',count:1},{Slot:2b,id:'writable_book',count:2},{Slot:6b,id:'book',count:1},{Slot:-1b,id:'book',count:1}]}",
      "{Items:[{id:'book',count:1}]}", "{Items:[1,2,3]}", "{Items:'bad'}",
      "{last_interacted_slot:-1}", "{last_interacted_slot:0}", "{last_interacted_slot:5}", "{last_interacted_slot:6}",
      "{last_interacted_slot:-2147483648}", "{last_interacted_slot:2147483647}", "{last_interacted_slot:'bad'}",
      "{last_interacted_slot:2.9d}", "{last_interacted_slot:4294967301L}",
      "{CustomName:'ignored',lock:{},LootTable:'chests/simple_dungeon',last_interacted_slot:4,Items:[{Slot:4b,id:'book',count:7}]}",
      "{components:{'minecraft:custom_name':'shelf','minecraft:container':[{slot:1,item:{id:'diamond',count:2}}]},Items:[{Slot:0b,id:'book',count:1}],last_interacted_slot:3}"
    );
    for (var facing : Direction.Plane.HORIZONTAL) {
      var state = block.defaultBlockState().setValue(ChiseledBookShelfBlock.FACING, facing);
      for (var input : inputs) {
        var entity = (ChiseledBookShelfBlockEntity)block.newBlockEntity(POS, state);
        entity.getItems().set(5, new ItemStack(Items.DIAMOND, 3));
        entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING, context, TagParser.parseCompoundFully(input)));
        var saved = entity.saveWithFullMetadata(context); var again = BlockEntity.loadStatic(POS, state, saved, context);
        System.out.println("shelf-load\t" + Block.getId(state) + "\t" + input + "\t" + saved + "\t" + entity.getLastInteractedSlot() + "\t" + components(entity) + "\t" + again.saveWithFullMetadata(context));
      }
      for (var contents : List.of("[]", "[{slot:0,item:{id:'book',count:99}},{slot:5,item:{id:'writable_book',count:2}},{slot:6,item:{id:'diamond',count:3}}]", "[{slot:3,item:{id:'stone',count:7}}]")) {
        var entity = (ChiseledBookShelfBlockEntity)block.newBlockEntity(POS, state);
        entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING, context, TagParser.parseCompoundFully("{last_interacted_slot:3,Items:[{Slot:1b,id:'diamond',count:4}]}")));
        var itemTag = TagParser.parseCompoundFully("{id:'chiseled_bookshelf',count:1,components:{'minecraft:custom_name':'placed','minecraft:container':" + contents + "}}");
        entity.applyComponentsFromItemStack(ItemStack.CODEC.parse(context.createSerializationContext(NbtOps.INSTANCE), itemTag).getOrThrow());
        var stripped = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, context); entity.saveCustomOnly(stripped); entity.removeComponentsFromTag(stripped);
        System.out.println("shelf-components\t" + Block.getId(state) + "\t" + itemTag + "\t" + entity.saveWithFullMetadata(context) + "\t" + components(entity) + "\t" + Block.getId(entity.getBlockState()) + "\t" + stripped.buildResult());
      }
    }
  }
}
