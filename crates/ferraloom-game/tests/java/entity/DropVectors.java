import java.lang.reflect.*;
import java.util.*;
import net.minecraft.server.level.*;
import net.minecraft.stats.*;
import net.minecraft.util.Prediction;
import net.minecraft.world.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.SwingAnimation;
import net.minecraft.world.level.*;

/** Real outside-primary menu click and ServerPlayer.drop; only swing delivery,
 * world registration and stat delivery are controlled observation boundaries. */
public final class DropVectors {
  static final class ServerFixture extends ServerLevel {
    DropPlayer player;
    int nextId;
    ServerFixture(){super(null,null,null,null,null,null,false,0,List.of(),false);}
    @Override public int getNextEntityId(){return ++nextId;}
    @Override public boolean addFreshEntity(Entity entity){
      player.dropped=(ItemEntity)entity;
      if(player.menu.getCarried()!=player.held || player.dropped.getItem()!=player.held)throw new AssertionError("drop receiver changed before add");
      player.events.add("add:"+player.dropped.getItem().getCount());
      return true;
    }
  }
  static final class DropPlayer extends ServerPlayer {
    List<String> events;
    AbstractContainerMenu menu;
    ItemStack held;
    ItemEntity dropped;
    DropPlayer(){super(null,null,null,null);}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
    @Override public boolean swing(InteractionHand hand,SwingAnimation animation,boolean sendToSelf){return true;}
    @Override public void awardStat(Stat<?> stat,int count){
      if(dropped==null || dropped.getItem()!=held || menu.getCarried()!=held)throw new AssertionError("stat read outside native held-receiver window");
      if(stat==Stats.ITEM_DROPPED.get(Items.STONE))events.add("item_dropped:"+count);
      else if(stat==Stats.CUSTOM.get(Stats.DROP))events.add("drop:"+count);
      else throw new AssertionError(stat);
    }
  }
  static void copyFields(Object from,Object to,Class<?> first)throws Exception {
    for(Class<?> owner=first;owner!=Object.class;owner=owner.getSuperclass())for(Field field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())){
      field.setAccessible(true);field.set(to,field.get(from));
    }
  }
  public static void run(Level level)throws Exception {
    var unsafeField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");unsafeField.setAccessible(true);
    var allocator=(sun.misc.Unsafe)unsafeField.get(null);
    for(String scenario:List.of("carried","empty","not-thrown")){
      var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);
      copyFields(level,server,Level.class);
      var player=(DropPlayer)allocator.allocateInstance(DropPlayer.class);
      copyFields(new LivingVectors(level),player,Player.class);
      ExperienceVectors.field(Entity.class,"level").set(player,server);
      server.player=player;player.events=new ArrayList<>();
      player.menu=new InventoryMenu(player.getInventory(),true,player);
      player.held=new ItemStack(Items.STONE,scenario.equals("empty")?0:7);
      player.menu.setCarried(player.held);
      if(scenario.equals("not-thrown")){
        player.drop(player.menu.getCarried(),false,Prediction.PREDICTED);
        player.menu.setCarried(ItemStack.EMPTY);
      }else player.menu.clicked(-999,0,ContainerInput.PICKUP,player);
      boolean identity=player.dropped!=null && player.dropped.getItem()==player.held;
      System.out.println("drop-receiver\t"+scenario+"\t"+(player.events.isEmpty()?"-":String.join(",",player.events))+"\t"+identity+"\t"+(player.dropped==null?0:player.dropped.getItem().getCount())+"\t"+player.menu.getCarried().isEmpty());
    }
  }
}
