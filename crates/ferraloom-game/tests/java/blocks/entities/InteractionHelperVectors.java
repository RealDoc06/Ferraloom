import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.util.*;
import java.util.concurrent.atomic.AtomicReference;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.core.component.DataComponentInitializers;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.*;
import net.minecraft.server.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.item.*;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.storage.loot.*;
import static org.mockito.Mockito.*;

/** Real WallHangingSignBlock support queries and Block's consumer loot overload. */
public final class InteractionHelperVectors {
  static final Gson JSON=new GsonBuilder().disableHtmlEscaping().create();
  static final BlockPos POS=new BlockPos(8,5,8);
  static HolderLookup.Provider registries;
  static <T> T missing(Class<T> type){return mock(type,call->{throw new UnsupportedOperationException(type.getSimpleName()+"."+call.getMethod().getName());});}
  static void signs(){
    var supports=new ArrayList<BlockState>();supports.add(Blocks.AIR.defaultBlockState());supports.add(Blocks.STONE.defaultBlockState());
    for(var direction:Direction.Plane.HORIZONTAL)supports.add(Blocks.OAK_WALL_HANGING_SIGN.defaultBlockState().setValue(WallHangingSignBlock.FACING,direction));
    var reader=missing(LevelReader.class);var cells=new HashMap<BlockPos,BlockState>();var reads=new ArrayList<String>();
    doAnswer(call->{var pos=(BlockPos)call.getArgument(0);reads.add(pos.toShortString());return Objects.requireNonNull(cells.get(pos));}).when(reader).getBlockState(any());
    for(var block:BuiltInRegistries.BLOCK)if(block instanceof WallHangingSignBlock hanging)for(var state:block.getStateDefinition().getPossibleStates())for(var left:supports)for(var right:supports){
      var facing=state.getValue(WallHangingSignBlock.FACING);cells.clear();reads.clear();cells.put(POS.relative(facing.getClockWise()),left);cells.put(POS.relative(facing.getCounterClockWise()),right);
      boolean result=hanging.canPlace(state,reader,POS);var row=new JsonObject();row.addProperty("kind","sign");row.addProperty("state",Block.getId(state));row.addProperty("left",Block.getId(left));row.addProperty("right",Block.getId(right));row.addProperty("result",result);row.addProperty("reads",reads.size());System.out.println(JSON.toJson(row));
    }
  }
  static final class NamedChest extends ChestBlockEntity {
    NamedChest(BlockState state){super(POS,state);}
    void name(Component name){var tag=new net.minecraft.nbt.CompoundTag();tag.put("CustomName",net.minecraft.network.chat.ComponentSerialization.CODEC.encodeStart(registries.createSerializationContext(NbtOps.INSTANCE),name).getOrThrow());loadCustomOnly(net.minecraft.world.level.storage.TagValueInput.create(net.minecraft.util.ProblemReporter.DISCARDING,registries,tag));}
  }
  static void loot(){
    var server=missing(MinecraftServer.class);var level=missing(ServerLevel.class);var reloadable=missing(ReloadableServerRegistries.Holder.class);
    doReturn(server).when(level).getServer();doReturn(FeatureFlags.VANILLA_SET).when(level).enabledFeatures();doReturn(reloadable).when(server).reloadableRegistries();doReturn(registries).when(reloadable).lookup();
    var rules=new GameRules(FeatureFlags.VANILLA_SET);rules.set(GameRules.BLOCK_DROPS,false,null);doReturn(rules).when(level).getGameRules();
    var key=ResourceKey.create(Registries.LOOT_TABLE,Identifier.withDefaultNamespace("helper_delivery"));
    var definitions=List.of(
      "{\"type\":\"block_interact\",\"pools\":[{\"rolls\":3,\"entries\":[{\"type\":\"item\",\"name\":\"stone\",\"modifier\":[{\"type\":\"set_count\",\"count\":{\"type\":\"uniform\",\"min\":1,\"max\":7}},{\"type\":\"copy_name\",\"source\":\"block_entity\"}]}]},{\"rolls\":1,\"entries\":[{\"type\":\"item\",\"name\":\"diamond\",\"modifier\":{\"type\":\"copy_name\",\"source\":\"block_entity\"}}]}]}",
      "{\"type\":\"block_interact\",\"pools\":[]}"
    );
    var modes=new ArrayList<String>(List.of("record","standard"));for(var face:Direction.values())modes.add(face.getName());
    for(var definition:definitions){
      var table=LootTable.DIRECT_CODEC.parse(registries.createSerializationContext(JsonOps.INSTANCE),JsonParser.parseString(definition)).getOrThrow();doReturn(table).when(reloadable).getLootTable(key);
      for(long seed:new long[]{0,1,42,-1,263777,Long.MIN_VALUE})for(boolean withEntity:new boolean[]{false,true})for(var mode:modes){
        var random=RandomSource.create(seed);doReturn(random).when(level).getRandom();
        var state=Blocks.CHEST.defaultBlockState();var original=new NamedChest(state);original.name(Component.literal("before"));var replacement=new NamedChest(state);replacement.name(Component.literal("replacement"));var current=new AtomicReference<ChestBlockEntity>(original);
        var deliveries=new JsonArray();
        boolean result=Block.dropFromBlockInteractLootTable(level,key,POS,state,withEntity?original:null,null,null,(world,stack)->{
          var event=new JsonObject();event.addProperty("stack",ItemStack.CODEC.encodeStart(registries.createSerializationContext(NbtOps.INSTANCE),stack).getOrThrow().toString());
          if(mode.equals("record")){
            int ordinal=deliveries.size();if(withEntity){if(ordinal==0){current.set(replacement);original.setRemoved();}original.name(Component.literal("delivery-"+ordinal));event.addProperty("name",original.getCustomName().getString());}
            event.addProperty("next",random.nextLong());
          }else if(mode.equals("standard"))Block.popResource(world,POS,stack);else Block.popResourceFromFace(world,POS,Direction.byName(mode),stack);
          deliveries.add(event);
        });
        var row=new JsonObject();row.addProperty("kind","loot");row.addProperty("definition",definition);row.addProperty("seed",seed);row.addProperty("with_entity",withEntity);row.addProperty("mode",mode);row.addProperty("result",result);row.add("deliveries",deliveries);row.addProperty("next",random.nextLong());row.addProperty("current_name",current.get().getCustomName().getString());System.out.println(JSON.toJson(row));
      }
    }
  }
  public static void main(String[] args)throws Exception{
    var out=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);var world=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);registries=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),world.listRegistries()));BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(DataComponentInitializers.PendingComponents::apply);signs();loot();
  }
}
