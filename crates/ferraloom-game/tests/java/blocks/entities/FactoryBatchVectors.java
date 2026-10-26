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
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.*;
import net.minecraft.world.level.storage.TagValueInput;
import static org.mockito.Mockito.*;

/** Native factories and TagValueInput codecs, including unbound banner holders. */
public final class FactoryBatchVectors {
  static final BlockPos POS=new BlockPos(8,64,8);
  static HolderLookup.Provider context;
  static Field field(Object o,String name)throws Exception {
    for(Class<?> c=o.getClass();c!=null;c=c.getSuperclass())try{var f=c.getDeclaredField(name);f.setAccessible(true);return f;}catch(NoSuchFieldException ignored){}
    throw new NoSuchFieldException(name);
  }
  static Object get(Object o,String name)throws Exception{return field(o,name).get(o);}
  static void set(Object o,String name,Object value)throws Exception{field(o,name).set(o,value);}
  static String bits(float value){return Integer.toUnsignedString(Float.floatToRawIntBits(value),16);}
  static void seed(BlockEntity e)throws Exception {
    if(e instanceof EnderChestBlockEntity){var counter=get(e,"openersCounter");set(counter,"openCount",3);set(counter,"maxInteractionRange",9.5);var lid=get(e,"chestLidController");set(lid,"openness",0.6f);set(lid,"oOpenness",0.2f);e.triggerEvent(1,1);}
    if(e instanceof EnchantingTableBlockEntity){for(String f:List.of("flip","oFlip","flipT","flipA","open","oOpen","rot","oRot","tRot"))set(e,f,0.75f);set(e,"time",123);}
    if(e instanceof BrewingStandBlockEntity){set(e,"ingredient",Items.DIAMOND);set(e,"lastPotionCount",new boolean[]{true,false,true});}
  }
  static String transientState(BlockEntity e)throws Exception {
    if(e instanceof BannerBlockEntity b)return b.getBaseColor().getName();
    if(e instanceof EnderChestBlockEntity){var c=get(e,"openersCounter");var lid=get(e,"chestLidController");return get(c,"openCount")+","+get(c,"maxInteractionRange")+","+get(lid,"shouldBeOpen")+","+bits((float)get(lid,"openness"))+","+bits((float)get(lid,"oOpenness"));}
    if(e instanceof EnchantingTableBlockEntity){var out=new ArrayList<String>();out.add(get(e,"time").toString());for(String f:List.of("flip","oFlip","flipT","flipA","open","oOpen","rot","oRot","tRot"))out.add(bits((float)get(e,f)));return String.join(",",out);}
    var ingredient=(Item)get(e,"ingredient");return (ingredient==null?"null":BuiltInRegistries.ITEM.getKey(ingredient))+","+Arrays.toString((boolean[])get(e,"lastPotionCount")).replace(" ","");
  }
  @SuppressWarnings({"rawtypes","unchecked"})
  public static void main(String[] args)throws Exception {
    var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
    var registries=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var level=mock(ServerLevel.class);doReturn(false).when(level).isClientSide();doReturn(registries).when(level).registryAccess();
    var blocks=new ArrayList<Block>();for(var block:BuiltInRegistries.BLOCK)if(block instanceof AbstractBannerBlock)blocks.add(block);blocks.addAll(List.of(Blocks.ENDER_CHEST,Blocks.ENCHANTING_TABLE,Blocks.BREWING_STAND));
    var lookup=context.lookupOrThrow(Registries.BANNER_PATTERN);
    var excluded=lookup.getOrThrow(BannerPatterns.CROSS).value();
    var filtered=HolderLookup.Provider.create(java.util.stream.Stream.concat(context.listRegistries().filter(r->r.key()!=Registries.BANNER_PATTERN),java.util.stream.Stream.of(lookup.filterElements(p->p!=excluded))));
    String maskedInput="{patterns:[{pattern:'cross',color:'blue'},{pattern:'stripe_bottom',color:'white'}]}";
    var masked=new BannerBlockEntity(POS,BuiltInRegistries.BLOCK.getValue(net.minecraft.resources.Identifier.parse("minecraft:white_banner")).defaultBlockState());
    masked.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,filtered,TagParser.parseCompoundFully(maskedInput)));
    System.out.println("factory-registry\t"+maskedInput+"\t"+masked.saveWithFullMetadata(context));
    for(var block:blocks){
      var inputs=new ArrayList<String>(List.of("@new","{}","{CustomName:'native name',components:{'minecraft:max_stack_size':7}}","{CustomName:0}","{components:{'minecraft:custom_name':'stored-only'}}","{CustomName:{text:'named',bold:true},components:{'minecraft:custom_name':'extra name'}}"));
      if(block instanceof AbstractBannerBlock)inputs.addAll(List.of(
        "{patterns:[{pattern:'minecraft:cross',color:'red'}]}",
        "{patterns:[{pattern:'test:missing',color:'red'}]}",
        "{patterns:[{pattern:'cross',color:'blue'},{pattern:'test:missing',color:'red'},{pattern:'stripe_bottom',color:'white'}]}",
        "{patterns:[{pattern:'cross',color:'wrong'},{pattern:'stripe_top',color:'lime'}]}",
        "{patterns:[{}, {pattern:{asset_id:'test:shape',translation_key:'test.shape'},color:'yellow'}]}",
        "{patterns:'wrong'}",
        "{components:{'minecraft:banner_patterns':[{pattern:'cross',color:'green'},{pattern:'test:missing',color:'red'},{pattern:'stripe_left',color:'black'}]}}",
        "{components:{'minecraft:banner_patterns':'wrong'}}",
        "{patterns:[{pattern:'cross',color:'red'},{pattern:'cross',color:'red'},{pattern:'cross',color:'red'},{pattern:'cross',color:'red'},{pattern:'cross',color:'red'},{pattern:'cross',color:'red'},{pattern:'cross',color:'red'}]}"));
      if(block==Blocks.BREWING_STAND)inputs.addAll(List.of(
        "{BrewTime:9,total_brew_time:57,Fuel:4,total_fuel:11,speed_multiplier:1.5f,Items:[{Slot:3b,id:'minecraft:nether_wart',count:4},{Slot:4b,id:'minecraft:blaze_powder',count:7}]}",
        "{BrewTime:1,Items:[{Slot:3b,id:'minecraft:stone',count:0},{Slot:0b,id:'minecraft:potion',count:100}]}",
        "{BrewTime:-7,total_brew_time:-99,Fuel:-4,total_fuel:0,speed_multiplier:-2f}",
        "{BrewTime:'wrong',total_brew_time:'wrong',Fuel:'wrong',total_fuel:'wrong',speed_multiplier:'wrong'}",
        "{BrewTime:4294967301L,Fuel:2.75f,speed_multiplier:0.1d}",
        "{Items:[{Slot:5b,id:'minecraft:stone',count:1},{Slot:-1b,id:'minecraft:dirt',count:2},{Slot:0b,id:'minecraft:diamond',count:1},{Slot:0b,id:'minecraft:stone',count:3}],lock:{predicate:{items:'minecraft:tripwire_hook'}}}"));
      for(String input:inputs){
        var state=block.defaultBlockState();var entity=((EntityBlock)block).newBlockEntity(POS,state);entity.setLevel(level);
        if(!input.equals("@new")){seed(entity);entity.loadWithComponents(TagValueInput.create(ProblemReporter.DISCARDING,context,TagParser.parseCompoundFully(input)));}
        var components=DataComponentMap.CODEC.encodeStart(context.createSerializationContext(NbtOps.INSTANCE),entity.collectComponents()).getOrThrow();
        boolean ticker=((EntityBlock)block).getTicker(level,state,(BlockEntityType)entity.getType())!=null;
        System.out.println("factory-data\t"+BuiltInRegistries.BLOCK.getKey(block)+"\t"+input+"\t"+entity.saveWithFullMetadata(context)+"\t"+entity.getUpdateTag(context)+"\t"+(entity.getUpdatePacket()!=null)+"\t"+components+"\t"+transientState(entity)+"\t"+ticker+"\t"+(entity instanceof net.minecraft.world.Nameable nameable?net.minecraft.network.chat.ComponentSerialization.CODEC.encodeStart(context.createSerializationContext(NbtOps.INSTANCE),nameable.getName()).getOrThrow():"-"));
      }
    }
  }
}
