import java.lang.reflect.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.*;
import net.minecraft.nbt.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.util.RandomSource;
import net.minecraft.resources.ResourceKey;
import net.minecraft.resources.Identifier;
import net.minecraft.world.RandomizableContainer;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.*;
import static org.mockito.Mockito.*;

/** Native data/codec boundaries; no replacement implementation of these classes. */
public final class FactoryBatchTwoVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  static HolderLookup.Provider context;
  static Field field(Object object,String name)throws Exception {
    for(Class<?> type=object.getClass();type!=null;type=type.getSuperclass())try{var f=type.getDeclaredField(name);f.setAccessible(true);return f;}catch(NoSuchFieldException ignored){}
    throw new NoSuchFieldException(name);
  }
  static Object get(Object object,String name)throws Exception{return field(object,name).get(object);}
  static void set(Object object,String name,Object value)throws Exception{field(object,name).set(object,value);}
  static void load(BlockEntity entity,String input)throws Exception{entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,context,TagParser.parseCompoundFully(input)));}
  static String components(BlockEntity entity){return DataComponentMap.CODEC.encodeStart(context.createSerializationContext(NbtOps.INSTANCE),entity.collectComponents()).getOrThrow().toString();}
  static void prime(BlockEntity entity)throws Exception {
    if(entity instanceof CampfireBlockEntity campfire){campfire.getItems().set(0,new ItemStack(Items.DIAMOND,3));System.arraycopy(new int[]{91,92,93,94},0,(int[])get(entity,"cookingProgress"),0,4);System.arraycopy(new int[]{201,202,203,204},0,(int[])get(entity,"cookingTime"),0,4);}
    if(entity instanceof BellBlockEntity){set(entity,"lastRingTimestamp",91L);set(entity,"ticks",7);set(entity,"shaking",true);set(entity,"clickDirection",Direction.WEST);set(entity,"nearbyEntities",List.of());set(entity,"resonating",true);set(entity,"resonationTicks",8);}
    if(entity instanceof LecternBlockEntity){set(entity,"book",new ItemStack(Items.DIAMOND,3));set(entity,"page",7);set(entity,"pageCount",9);}
    if(entity instanceof DecoratedPotBlockEntity){set(entity,"wobbleStartedAtTick",77L);set(entity,"lastWobbleStyle",DecoratedPotBlockEntity.WobbleStyle.NEGATIVE);set(entity,"item",new ItemStack(Items.DIAMOND,3));set(entity,"decorations",PotDecorations.allBrick(context.lookupOrThrow(Registries.ITEM)));}
  }
  static String scalar(BlockEntity entity)throws Exception {
    if(entity instanceof CampfireBlockEntity)return Arrays.toString((int[])get(entity,"cookingProgress")).replace(" ","")+";"+Arrays.toString((int[])get(entity,"cookingTime")).replace(" ","");
    if(entity instanceof BellBlockEntity)return get(entity,"lastRingTimestamp")+","+get(entity,"ticks")+","+get(entity,"shaking")+","+get(entity,"clickDirection")+","+(get(entity,"nearbyEntities")==null?"null":((List<?>)get(entity,"nearbyEntities")).size())+","+get(entity,"resonating")+","+get(entity,"resonationTicks");
    if(entity instanceof LecternBlockEntity lectern)return lectern.getPage()+","+get(entity,"pageCount")+","+lectern.hasBook()+","+lectern.getRedstoneSignal();
    var pot=(DecoratedPotBlockEntity)entity;return pot.wobbleStartedAtTick+","+pot.lastWobbleStyle+","+pot.getLootTableSeed();
  }
  @SuppressWarnings({"rawtypes","unchecked"})
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var registries=NeighborVectors.load();var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtin.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var level=mock(ServerLevel.class);doReturn(false).when(level).isClientSide();doReturn(registries).when(level).registryAccess();
    var states=new ArrayList<BlockState>();
    for(var block:List.of(Blocks.CAMPFIRE,Blocks.SOUL_CAMPFIRE))for(boolean lit:List.of(false,true))states.add(block.defaultBlockState().setValue(CampfireBlock.LIT,lit));
    states.add(Blocks.BELL.defaultBlockState());states.add(Blocks.LECTERN.defaultBlockState());states.add(Blocks.DECORATED_POT.defaultBlockState());
    for(var state:states){
      var block=(EntityBlock)state.getBlock();var inputs=new ArrayList<String>(List.of("@new","{}","{CustomName:'ignored',components:{'minecraft:custom_name':'extra','minecraft:max_stack_size':7}}"));
      if(block instanceof CampfireBlock)inputs.addAll(List.of(
        "{Items:[{Slot:0b,id:'stone',count:0},{Slot:1b,id:'beef',count:100},{Slot:3b,id:'potato',count:2},{Slot:4b,id:'diamond',count:3}],CookingTimes:[I;1,2,3,4,5],CookingTotalTimes:[I;6,7]}",
        "{CookingTimes:[I;],CookingTotalTimes:[I;]}",
        "{CookingTimes:[I;-4],CookingTotalTimes:[I;-5,0,2147483647,-2147483648]}",
        "{CookingTimes:[1,2],CookingTotalTimes:'wrong'}",
        "{Items:[{Slot:0b,id:'diamond',count:1},{Slot:0b,id:'stone',count:3},{Slot:-1b,id:'dirt',count:2},{Slot:2b,id:'test:unknown',count:1}]}"));
      if(block instanceof BellBlock)inputs.add("{ticks:44,shaking:0b,lastRingTimestamp:999L,resonating:0b,clickDirection:'north'}");
      if(block instanceof LecternBlock)inputs.addAll(List.of(
        "{Book:{id:'written_book',count:1,components:{'minecraft:written_book_content':{title:'native',author:'a',pages:['first','second','third']}}},Page:99}",
        "{Book:{id:'writable_book',count:1,components:{'minecraft:writable_book_content':{pages:['one','two']}}},Page:-4}",
        "{Book:{id:'stone',count:0},Page:5}","{Book:{id:'written_book',count:100},Page:-5}",
        "{Book:{id:'written_book',count:1,components:{'minecraft:written_book_content':{title:'done',author:'a',pages:[{text:'literal',bold:true}],resolved:true}}},Page:1}",
        "{Book:{id:'writable_book',count:2},Page:4294967301L}","{Book:{id:'test:unknown',count:1},Page:1}","{Book:'wrong',Page:'wrong'}"));
      if(block instanceof DecoratedPotBlock)inputs.addAll(List.of(
        "{sherds:{back:'brick',left:{id:'archer_pottery_sherd',count:2},front:{id:'stone',components:{'minecraft:custom_name':'side'}}},item:{id:'diamond',count:7}}",
        "{sherds:{back:'test:unknown',front:'brick'}}",
        "{sherds:{back:{id:'brick',count:0},front:'brick'}}",
        "{sherds:{back:{id:'brick',count:100},front:'brick'}}",
        "{sherds:[\"brick\"]}","{item:{id:'stone',count:0}}","{item:{id:'stone',count:100}}",
        "{LootTable:'chests/simple_dungeon',LootTableSeed:-123L,item:{id:'diamond',count:4},sherds:{front:'brick'}}",
        "{LootTable:'invalid identifier',LootTableSeed:21L,item:{id:'diamond',count:3}}",
        "{LootTable:'chests/simple_dungeon',LootTableSeed:'wrong'}",
        "{components:{'minecraft:pot_decorations':{back:'test:unknown',front:'brick'}}}",
        "{components:{'minecraft:pot_decorations':'wrong'}}"));
      for(String input:inputs){
        var entity=block.newBlockEntity(POS,state);if(!input.equals("@new")){prime(entity);load(entity,input);}
        var saved=entity.saveWithFullMetadata(context);var round=BlockEntity.loadStatic(POS,state,saved,context);entity.setLevel(level);
        boolean ticker=block.getTicker(level,state,(BlockEntityType)entity.getType())!=null;
        System.out.println("factory2-data\t"+Block.getId(state)+"\t"+input+"\t"+saved+"\t"+entity.getUpdateTag(context)+"\t"+(entity.getUpdatePacket()!=null)+"\t"+components(entity)+"\t"+scalar(entity)+"\t"+ticker+"\t"+round.saveWithFullMetadata(context));
      }
      for(String contents:List.of("[]","[{slot:0,item:{id:'stone',count:7}},{slot:3,item:{id:'beef',count:2}},{slot:4,item:{id:'diamond',count:3}}]")){
        var entity=block.newBlockEntity(POS,state);prime(entity);load(entity,"{LootTable:'chests/simple_dungeon',LootTableSeed:5L,CookingTimes:[I;1,2,3,4],CookingTotalTimes:[I;11,12,13,14]}");
        String itemInput="{id:'"+BuiltInRegistries.ITEM.getKey(state.getBlock().asItem())+"',count:1,components:{'minecraft:custom_name':'placed','minecraft:container':"+contents+",'minecraft:pot_decorations':{front:'brick'}}}";
        var item=ItemStack.CODEC.parse(context.createSerializationContext(NbtOps.INSTANCE),TagParser.parseCompoundFully(itemInput)).getOrThrow();entity.applyComponentsFromItemStack(item);
        var stripped=TagValueOutput.createWithContext(ProblemReporter.DISCARDING,context);entity.saveCustomOnly(stripped);entity.removeComponentsFromTag(stripped);
        System.out.println("factory2-components\t"+Block.getId(state)+"\t"+itemInput+"\t"+entity.saveWithFullMetadata(context)+"\t"+components(entity)+"\t"+stripped.buildResult()+"\t"+scalar(entity));
      }
      if(block instanceof DecoratedPotBlock)for(String mode:List.of("loot","seed"))for(long seed:List.of(0L,-123L)){
        var entity=(DecoratedPotBlockEntity)block.newBlockEntity(POS,state);prime(entity);
        var getter=mock(BlockGetter.class);doReturn(entity).when(getter).getBlockEntity(POS);
        var random=RandomSource.create(seed);var table=ResourceKey.create(Registries.LOOT_TABLE,Identifier.parse("minecraft:chests/simple_dungeon"));
        if(mode.equals("loot"))RandomizableContainer.setBlockEntityLootTable(getter,random,POS,table);else entity.setLootTable(table,seed);
        System.out.println("factory2-loot\t"+Block.getId(state)+"\t"+mode+"\t"+seed+"\t"+entity.saveWithFullMetadata(context)+"\t"+components(entity)+"\t"+random.nextLong());
      }
    }
  }
}
