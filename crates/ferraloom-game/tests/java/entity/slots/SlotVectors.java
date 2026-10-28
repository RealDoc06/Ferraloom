import java.util.*;
import java.util.stream.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.EnderChestBlockEntity;
import static org.mockito.Mockito.*;

/** Real Entity#getSlot/SlotAccess getters, validation, setters and live receivers. */
public final class SlotVectors {
  static final class PlayerReceiver extends Player {
    PlayerReceiver(Level level){super(level,new GameProfile(new UUID(1,2),"slot_owner"));}
    GameType mode=GameType.SURVIVAL;
    @Override public GameType gameMode(){return mode;}
  }
  static final class RemovingCow extends Cow {
    boolean capture;
    String observed;
    RemovingCow(Level level){super(EntityTypes.COW,level);}
    @Override public void onEquipItem(EquipmentSlot slot,ItemStack oldStack,ItemStack incoming){
      if(!capture){super.onEquipItem(slot,oldStack,incoming);return;}
      capture=false;setItemSlot(slot,new ItemStack(Items.DIAMOND));capture=true;
      discard();incoming.grow(3);
      observed=describe(oldStack)+"\t"+describe(incoming)+"\t"+describe(getItemBySlot(slot))+"\t"+isRemoved();
    }
  }
  static ItemStack stack(int count){return new ItemStack(Items.STONE,count);}
  static String describe(ItemStack s){return BuiltInRegistries.ITEM.getKey(s.getItem())+":"+s.getCount();}
  static String equipment(LivingEntity e){return EquipmentSlot.VALUES.stream().map(s->s.getName()+"="+describe(e.getItemBySlot(s))).collect(Collectors.joining(";"));}
  static Entity create(String kind,Level level) {
    Entity entity=switch(kind){
      case "player" -> new PlayerReceiver(level);
      case "cow" -> new Cow(EntityTypes.COW,level);
      case "item" -> new ItemEntity(EntityTypes.ITEM,level);
      case "falling" -> new FallingBlockEntity(EntityTypes.FALLING_BLOCK,level);
      case "tnt" -> new PrimedTnt(EntityTypes.TNT,level);
      case "orb" -> new ExperienceOrb(EntityTypes.EXPERIENCE_ORB,level);
      default -> throw new AssertionError(kind);
    };
    if(entity instanceof Player player){
      for(int i=0;i<36;i++)player.getInventory().setItem(i,stack(i+1));
      player.getInventory().setSelectedSlot(4);
      for(int i=0;i<27;i++)player.getEnderChestInventory().setItem(i,stack(i+1));
      for(int i=0;i<4;i++)player.inventoryMenu.getCraftSlots().setItem(i,stack(56+i));
      player.containerMenu.setCarried(stack(55));
    }
    if(entity instanceof LivingEntity living)for(var slot:EquipmentSlot.VALUES)living.setItemSlot(slot,stack(40+slot.ordinal()));
    if(entity instanceof ItemEntity item)item.setItem(stack(9));
    return entity;
  }
  public static void main(String[] args)throws Exception {
    var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
    var registries=NeighborVectors.load();
    var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var provider=HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(provider).forEach(pending->pending.apply());
    var level=mock(Level.class);when(level.registryAccess()).thenReturn(registries);
    var slots=new TreeSet<Integer>();
    for(var range:List.of(new int[]{-1,44},new int[]{97,108},new int[]{199,228},new int[]{498,505}))IntStream.range(range[0],range[1]).forEach(slots::add);
    slots.add(Integer.MIN_VALUE);slots.add(Integer.MAX_VALUE);
    for(String kind:List.of("player","cow","item","falling","tnt","orb")) {
      var entity=create(kind,level);
      for(int slot:slots){var access=entity.getSlot(slot);System.out.println("slot-read\t"+kind+"\t"+slot+"\t"+(access==null?"-":describe(access.get())));}
    }
    Item[] inputs={Items.AIR,Items.STONE,Items.IRON_HELMET,Items.IRON_CHESTPLATE,Items.IRON_LEGGINGS,Items.IRON_BOOTS,Items.WOLF_ARMOR,Items.SADDLE};
    int[] targets={-1,0,8,9,35,36,98,99,100,101,102,103,104,105,106,200,226,227,499,500,501,502,503,504};
    for(String kind:List.of("player","cow","item"))for(int slot:targets)for(Item input:inputs) {
      var entity=create(kind,level);var access=entity.getSlot(slot);var value=input==Items.AIR?ItemStack.EMPTY:new ItemStack(input,input==Items.STONE?77:1);
      String before=access==null?"-":describe(access.get());
      boolean result=access!=null&&access.set(value);
      System.out.println("slot-set\t"+kind+"\t"+slot+"\t"+BuiltInRegistries.ITEM.getKey(input)+"\t"+before+"\t"+result+"\t"+(access==null?"-":describe(access.get())));
    }
    var player=(Player)create("player",level);
    var hand=player.getSlot(98);player.getInventory().setSelectedSlot(6);String before=describe(hand.get());hand.set(new ItemStack(Items.DIAMOND,3));
    System.out.println("slot-dynamic-hand\t"+before+"\t"+describe(player.getInventory().getItem(4))+"\t"+describe(player.getInventory().getItem(6)));
    var cursor=player.getSlot(499);var original=player.containerMenu;player.containerMenu=ChestMenu.threeRows(7,player.getInventory());player.containerMenu.setCarried(new ItemStack(Items.GOLD_INGOT,2));
    before=describe(cursor.get());cursor.set(new ItemStack(Items.DIAMOND,3));
    System.out.println("slot-dynamic-cursor\t"+before+"\t"+describe(original.getCarried())+"\t"+describe(player.containerMenu.getCarried()));
    var ender=player.getSlot(200);var chest=new EnderChestBlockEntity(BlockPos.ZERO,Blocks.ENDER_CHEST.defaultBlockState());player.getEnderChestInventory().setActiveChest(chest);
    player.discard();boolean accepted=ender.set(new ItemStack(Items.DIAMOND,7));
    System.out.println("slot-retired-ender\t"+player.isRemoved()+"\t"+accepted+"\t"+describe(ender.get())+"\t"+player.getEnderChestInventory().isActiveChest(chest));
    var receiver=new RemovingCow(level);receiver.setItemSlot(EquipmentSlot.FEET,new ItemStack(Items.STONE,4));receiver.capture=true;
    boolean replaced=receiver.getSlot(100).set(new ItemStack(Items.IRON_BOOTS,2));
    System.out.println("slot-reentrant\t"+replaced+"\t"+receiver.observed);
  }
}
