import java.lang.reflect.*;
import java.util.*;
import net.minecraft.server.level.*;
import net.minecraft.stats.*;
import net.minecraft.util.Prediction;
import net.minecraft.util.RandomSource;
import net.minecraft.world.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.SwingAnimation;
import net.minecraft.world.level.*;
import org.mockito.Answers;
import static org.mockito.Mockito.*;

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
  static String vector(net.minecraft.world.phys.Vec3 value) {
    return TravelVectors.d(value.x)+","+TravelVectors.d(value.y)+","+TravelVectors.d(value.z);
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
    creation(level,allocator);
  }
  /** Actual unregistered creation, with only RandomSource.create's seed controlled. */
  static void creation(Level level,sun.misc.Unsafe allocator)throws Exception {
    long[] playerSeeds={0,123,-11,Long.MAX_VALUE},entitySeeds={42,-1,991,Long.MIN_VALUE};
    float[][] rotations={{0,0},{37.25f,-28.125f},{90,180},{-721.25f,123.5f}};
    for(int scenario=0;scenario<4;scenario++)for(String mode:List.of("normal","fake","leftover"))for(int count:new int[]{0,3}) {
      var server=(ServerFixture)allocator.allocateInstance(ServerFixture.class);
      copyFields(level,server,Level.class);
      var player=(DropPlayer)allocator.allocateInstance(DropPlayer.class);
      copyFields(new LivingVectors(level),player,Player.class);
      ExperienceVectors.field(Entity.class,"level").set(player,server);
      player.setPos(8.5,64,8.5);player.setYRot(rotations[scenario][0]);player.setXRot(rotations[scenario][1]);
      player.getRandom().setSeed(playerSeeds[scenario]);server.getRandom().setSeed(555);server.nextId=1;
      var constructorRandom=RandomSource.create(entitySeeds[scenario]);
      int[] creations={0};
      ItemEntity item;
      try(var factory=mockStatic(RandomSource.class,Answers.CALLS_REAL_METHODS)) {
        factory.when(RandomSource::create).thenAnswer(call->{creations[0]++;return constructorRandom;});
        item=player.createItemStackToDrop(new ItemStack(Items.STONE,count),scenario>=2,scenario%2!=0);
      }
      String state="-\t-\t-\t-\t-\t-\t-\t-\t-\t0";
      if(item!=null) {
        if(mode.equals("fake"))item.makeFakeItem();
        if(mode.equals("leftover")){item.setNoPickUpDelay();item.setTarget(player.getUUID());}
        var thrower=(EntityReference<?>)ExperienceVectors.field(ItemEntity.class,"thrower").get(item);
        var target=(UUID)ExperienceVectors.field(ItemEntity.class,"target").get(item);
        state=vector(item.position())+"\t"+vector(item.getDeltaMovement())+"\t"+item.getId()+"\t"+item.getUUID()+"\t"+ExperienceVectors.f(item.bobOffs)+","+ExperienceVectors.f(item.getYRot())+"\t"+item.getAge()+"\t"+ExperienceVectors.field(ItemEntity.class,"pickupDelay").getInt(item)+"\t"+(thrower==null?"-":thrower.getUUID())+"\t"+(target==null?"-":target)+"\t"+item.getItem().getCount();
      }
      if(player.dropped!=null)throw new AssertionError("createItemStackToDrop registered an entity");
      System.out.println("drop-create\t"+scenario+"\t"+mode+"\t"+count+"\t"+playerSeeds[scenario]+"\t"+entitySeeds[scenario]+"\t"+ExperienceVectors.f(rotations[scenario][0])+","+ExperienceVectors.f(rotations[scenario][1])+"\t"+state+"\t"+creations[0]+"\t"+Long.toHexString(player.getRandom().nextLong())+"\t"+Long.toHexString(constructorRandom.nextLong())+"\t"+Long.toHexString(server.getRandom().nextLong())+"\t"+server.getNextEntityId());
    }
  }
}
