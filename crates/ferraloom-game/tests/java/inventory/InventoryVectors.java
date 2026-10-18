import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.core.component.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.network.*;
import net.minecraft.util.*;
import net.minecraft.world.*;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.crafting.*;
import net.minecraft.world.level.*;

/** Exercises the real menu classes and real clicked method, with only world
 * services and outbound delivery replaced. Recipe execution is a separate port. */
public final class InventoryVectors {
  static final class MenuLevel extends EntityFixture {
    MenuLevel(RegistryAccess r){super(r);}
    @Override public RecipeAccess recipeAccess(){return new RecipeAccess(){
      public RecipePropertySet propertySet(net.minecraft.resources.ResourceKey<RecipePropertySet> id){return RecipePropertySet.EMPTY;}
      public SelectableRecipe.SingleInputSet<StonecutterRecipe> stonecutterRecipes(){return SelectableRecipe.SingleInputSet.empty();}
    };}
  }
  static final class MenuPlayer extends Player {
    final boolean creative;
    final List<String> drops=new ArrayList<>();
    MenuPlayer(Level level,boolean creative){super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"inventory"));this.creative=creative;getAbilities().instabuild=creative;}
    @Override public GameType gameMode(){return creative?GameType.CREATIVE:GameType.SURVIVAL;}
    @Override public ItemEntity drop(ItemStack stack,boolean random,Prediction prediction){if(!stack.isEmpty())drops.add(item(stack));return null;}
  }
  static final class Sync implements ContainerSynchronizer {
    final List<String> events=new ArrayList<>();
    final HashedPatchMap.HashGenerator hasher;
    Sync(RegistryAccess r){var ops=r.createSerializationContext(HashOps.CRC32C_INSTANCE);hasher=c->c.encodeValue(ops).getOrThrow().asInt();}
    public RemoteSlot createSlot(){return new RemoteSlot.Synchronized(hasher);}
    public void sendInitialData(AbstractContainerMenu m,List<ItemStack> items,ItemStack carried,int[] data){events.add("I:"+m.incrementStateId());}
    public void sendSlotChange(AbstractContainerMenu m,int slot,ItemStack stack){events.add("S:"+m.incrementStateId()+":"+slot+":"+item(stack));}
    public void sendCarriedChange(AbstractContainerMenu m,ItemStack stack){events.add("C:"+item(stack));}
    public void sendDataChange(AbstractContainerMenu m,int slot,int value){events.add("D:"+slot+":"+value);}
  }
  static String item(ItemStack stack){return stack.isEmpty()?"-":BuiltInRegistries.ITEM.getId(stack.getItem())+","+stack.getCount()+","+stack.getMaxStackSize();}
  static String slots(AbstractContainerMenu menu){var result=new ArrayList<String>();for(int i=0;i<menu.slots.size();i++)if(menu.getSlot(i).hasItem())result.add(i+":"+item(menu.getSlot(i).getItem()));return String.join(";",result);}
  static ItemStack stack(Item item,int count,int max){var stack=new ItemStack(item,count);if(max!=item.components().getOrDefault(DataComponents.MAX_STACK_SIZE,1))stack.set(DataComponents.MAX_STACK_SIZE,max);return stack;}
  static AbstractContainerMenu menu(String name,MenuPlayer player){var inv=player.getInventory();return switch(name){
    case "inventory"->new InventoryMenu(inv,true,player);
    case "chest1"->ChestMenu.oneRow(7,inv);case "chest2"->ChestMenu.twoRows(7,inv);case "chest3"->ChestMenu.threeRows(7,inv);
    case "chest4"->ChestMenu.fourRows(7,inv);case "chest5"->ChestMenu.fiveRows(7,inv);case "chest6"->ChestMenu.sixRows(7,inv);
    case "dispenser"->new DispenserMenu(7,inv);case "hopper"->new HopperMenu(7,inv);case "shulker"->new ShulkerBoxMenu(7,inv);
    case "furnace"->new FurnaceMenu(7,inv);case "smoker"->new SmokerMenu(7,inv);case "blast"->new BlastFurnaceMenu(7,inv);
    default->throw new IllegalArgumentException(name);
  };}
  static void scenario(RegistryAccess r,String name,String type,boolean creative,List<int[]> clicks,boolean randomized){
    var player=new MenuPlayer(new MenuLevel(r),creative);var menu=menu(type,player);var sync=new Sync(r);
    if(randomized){
      var random=new Random(71331+name.hashCode());var items=new Item[]{Items.STONE,Items.DIRT,Items.COBBLESTONE,Items.ENDER_PEARL};
      for(int i=0;i<menu.slots.size();i++)if(menu.getSlot(i).getMaxStackSize()>1 && menu.getSlot(i).mayPlace(new ItemStack(Items.STONE)) && random.nextInt(3)==0){
        var item=items[random.nextInt(items.length)];int max=random.nextBoolean()?item.components().getOrDefault(DataComponents.MAX_STACK_SIZE,1):7;
        menu.getSlot(i).set(stack(item,1+random.nextInt(max),max));
      }
    }else{
      int start=switch(type){case "inventory"->9;case "furnace","smoker","blast"->3;default->0;};
      menu.getSlot(start).set(stack(Items.STONE,63,64));
      menu.getSlot(start+1).set(stack(Items.STONE,5,64));
      menu.getSlot(start+2).set(stack(Items.DIRT,12,64));
      menu.getSlot(start+3).set(stack(Items.STONE,4,7));
      if(type.equals("furnace")||type.equals("smoker")||type.equals("blast"))menu.getSlot(2).set(stack(Items.IRON_INGOT,64,64));
    }
    menu.setSynchronizer(sync);sync.events.clear();
    System.out.println("start\t"+name+"\t"+type+"\t"+creative+"\t"+slots(menu));
    int step=0;
    for(var click:clicks){
      int slot=click[0],button=click[1];var input=ContainerInput.values()[click[2]];
      // Alternate correct predictions, absent predictions and a mismatching state id.
      int mode=step%4;menu.suppressRemoteUpdates();menu.clicked(slot,button,input,player);
      if(mode==1){for(int i=0;i<menu.slots.size();i++)menu.setRemoteSlotUnsafe(i,HashedStack.create(menu.getSlot(i).getItem(),sync.hasher));menu.setRemoteCarried(HashedStack.create(menu.getCarried(),sync.hasher));}
      menu.resumeRemoteUpdates();if(mode==3)menu.broadcastFullState();else menu.broadcastChanges();
      System.out.println("click\t"+name+"\t"+(step++)+"\t"+slot+"\t"+button+"\t"+input.ordinal()+"\t"+slots(menu)+"\t"+item(menu.getCarried())+"\t"+menu.getStateId()+"\t"+String.join(";",sync.events)+"\t"+String.join(";",player.drops));
      sync.events.clear();player.drops.clear();
    }
  }
  public static void main(String[] args)throws Exception {
    var output=System.out;var errors=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);System.setErr(errors);
    var registries=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(DataComponentInitializers.PendingComponents::apply);
    var player=new MenuPlayer(new MenuLevel(registries),false);
    for(var type:BuiltInRegistries.MENU){
      var m=type.create(7,player.getInventory());var layout=new ArrayList<String>();
      for(var slot:m.slots)layout.add(slot.getContainerSlot()+","+slot.x+","+slot.y);
      System.out.println("layout\t"+BuiltInRegistries.MENU.getId(type)+"\t"+String.join(";",layout));
    }
    var edge=List.of(new int[]{0,1,0},new int[]{1,0,0},new int[]{2,0,0},new int[]{-999,1,0},new int[]{3,0,0},new int[]{4,0,0},new int[]{4,0,0},new int[]{5,0,5},new int[]{5,1,5},new int[]{6,1,5},new int[]{7,1,5},new int[]{-999,2,5},new int[]{8,0,0},new int[]{-999,0,0},new int[]{0,40,2},new int[]{1,0,2},new int[]{0,1,4},new int[]{2,0,3},new int[]{-999,0,0},new int[]{2,1,4});
    scenario(registries,"edges","chest1",true,edge,false);
    for(String type:List.of("inventory","chest1","chest2","chest3","chest4","chest5","chest6","dispenser","hopper","shulker","furnace","smoker","blast")){
      int boundary=switch(type){case "inventory"->9;case "hopper"->5;case "dispenser"->9;case "shulker"->27;case "furnace","smoker","blast"->3;default->Integer.parseInt(type.substring(5))*9;};
      var clicks=new ArrayList<int[]>();
      clicks.add(new int[]{type.equals("furnace")||type.equals("smoker")||type.equals("blast")?2:boundary,0,1});
      clicks.add(new int[]{boundary,1,0});
      clicks.add(new int[]{-999,0,5});for(int i=0;i<4;i++)clicks.add(new int[]{boundary+i,1,5});clicks.add(new int[]{-999,2,5});
      clicks.add(new int[]{boundary,0,0});clicks.add(new int[]{boundary+8,0,6});clicks.add(new int[]{-999,0,0});
      var random=new Random(1302);
      int total=menu(type,new MenuPlayer(new MenuLevel(registries),false)).slots.size();
      for(int i=0;i<100;i++){
        int slot=boundary+random.nextInt(total-boundary),input=random.nextInt(7),button=random.nextInt(2);
        if(input==2)button=random.nextBoolean()?40:random.nextInt(9);
        if(input==5)input=0;
        clicks.add(new int[]{slot,button,input});
      }
      scenario(registries,type+"-random",type,true,clicks,true);
    }
  }
}
