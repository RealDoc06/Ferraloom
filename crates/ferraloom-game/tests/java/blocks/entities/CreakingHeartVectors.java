import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Real constructor/factory/load/save, transient fields, updates and ticker gate. */
public final class CreakingHeartVectors {
  static final BlockPos POS = new BlockPos(8, 64, 8);
  static final List<String> FIELDS = List.of("ticksExisted", "ticker", "emitter", "emitterTarget", "outputSignal");
  static Field field(String name) throws Exception { var field = CreakingHeartBlockEntity.class.getDeclaredField(name); field.setAccessible(true); return field; }
  static void seed(CreakingHeartBlockEntity entity) throws Exception {
    entity.setCreakingInfo(new UUID(9, 10));
    field("ticksExisted").setLong(entity, 73); field("ticker").setInt(entity, -7);
    field("emitter").setInt(entity, 57); field("emitterTarget").set(entity, new Vec3(1.25, 2.5, -3.75)); field("outputSignal").setInt(entity, 9);
  }
  static String transientFields(CreakingHeartBlockEntity entity) throws Exception {
    var values = new ArrayList<String>(); for (var name : FIELDS) values.add(String.valueOf(field(name).get(entity))); return String.join(";", values);
  }
  public static void main(String[] args) throws Exception {
    var output = System.out; SharedConstants.tryDetectVersion(); Bootstrap.bootStrap(); System.setOut(output);
    var world = NeighborVectors.load(); var builtin = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context = HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(), world.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var level = mock(Level.class); doReturn(false).when(level).isClientSide(); doReturn(world).when(level).registryAccess();
    var effects = new ArrayList<String>();
    doAnswer(call -> { effects.add("dirty"); return null; }).when(level).blockEntityChanged(POS);
    doAnswer(call -> { effects.add("analog"); return null; }).when(level).updateNeighbourForOutputSignal(POS, Blocks.CREAKING_HEART);
    var inputs = List.of(
      "{}", "{creaking:[I;1,2,3,4]}", "{creaking:[I;0,0,0,0]}",
      "{creaking:[I;-2147483648,2147483647,-1,0]}", "{creaking:[1,2,3,4]}",
      "{creaking:[B;1b,2b,3b,4b]}", "{creaking:[L;1L,2L,3L,4L]}",
      "{creaking:[1.9d,2.1d,-3.9d,4d]}", "{creaking:[I;1,2,3]}", "{creaking:[I;1,2,3,4,5]}",
      "{creaking:'00000000-0000-0001-0000-000000000002'}", "{creaking:{}}", "{creaking:7}", "{creaking:['bad','bad','bad','bad']}",
      "{ticksExisted:999L,ticker:999,emitter:999,emitterTarget:[0d,0d,0d],outputSignal:999}",
      "{creaking:[I;1,2,3,4],components:{'minecraft:custom_name':'heart','minecraft:max_stack_size':7}}"
    );
    for (var state : Blocks.CREAKING_HEART.getStateDefinition().getPossibleStates()) {
      var fresh = (CreakingHeartBlockEntity)((EntityBlock)Blocks.CREAKING_HEART).newBlockEntity(POS, state);
      fresh.setLevel(level); // Native update-packet creation requires an attached level.
      var ticker = ((CreakingHeartBlock)Blocks.CREAKING_HEART).getTicker(level, state, BlockEntityTypes.CREAKING_HEART);
      System.out.println("heart-default\t" + Block.getId(state) + "\t" + fresh.saveWithFullMetadata(context) + "\t" + transientFields(fresh) + "\t" + (ticker != null) + "\t" + fresh.getUpdateTag(context) + "\t" + (fresh.getUpdatePacket() != null));
      for (var input : inputs) for (boolean attached : new boolean[]{false, true}) {
        var entity = new CreakingHeartBlockEntity(POS, state); seed(entity);
        if (attached) entity.setLevel(level); effects.clear();
        entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING, context, TagParser.parseCompoundFully(input)));
        var saved = entity.saveWithFullMetadata(context); var again = BlockEntity.loadStatic(POS, state, saved, context);
        if (!attached) entity.setLevel(level); // Attach only after testing unattached loading.
        System.out.println("heart-load\t" + Block.getId(state) + "\t" + attached + "\t" + input + "\t" + saved + "\t" + transientFields(entity) + "\t" + entity.getUpdateTag(context) + "\t" + (entity.getUpdatePacket() != null) + "\t" + DataComponentMap.CODEC.encodeStart(context.createSerializationContext(NbtOps.INSTANCE), entity.collectComponents()).getOrThrow() + "\t" + again.saveWithFullMetadata(context) + "\t" + String.join(",", effects));
      }
    }
  }
}
