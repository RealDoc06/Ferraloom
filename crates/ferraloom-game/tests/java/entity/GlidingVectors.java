import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.core.Holder;
import net.minecraft.core.component.DataComponents;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.effect.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.equipment.Equippable;
import net.minecraft.world.level.*;
import net.minecraft.world.level.gameevent.GameEvent;

/** Native eligibility and real updateFallFlying calls. The controlled ordinary
 * Level makes ItemStack.hurtAndBreak's ServerLevel branch inactive, matching the
 * approved Warn durability seam while retaining its preceding RNG and reads. */
public final class GlidingVectors {
  static ItemStack stack(int config,EquipmentSlot slot) {
    var stack=new ItemStack(Items.ELYTRA);
    stack.set(DataComponents.EQUIPPABLE,Equippable.builder(slot).build());
    switch(config) {
      case 1 -> stack.remove(DataComponents.GLIDER);
      case 2 -> stack.remove(DataComponents.EQUIPPABLE);
      case 3 -> stack.set(DataComponents.EQUIPPABLE,Equippable.builder(EquipmentSlot.CHEST).build());
      case 4 -> stack.set(DataComponents.DAMAGE,431);
      case 5 -> stack.set(DataComponents.DAMAGE,432);
      case 6 -> stack.set(DataComponents.DAMAGE,-7);
      case 7 -> {stack.set(DataComponents.DAMAGE,432);stack.set(DataComponents.UNBREAKABLE,net.minecraft.util.Unit.INSTANCE);}
      case 8 -> stack.remove(DataComponents.MAX_DAMAGE);
      case 9 -> stack.remove(DataComponents.DAMAGE);
      case 10 -> stack.setCount(0);
      case 11 -> stack.setCount(-1);
      default -> {}
    }
    return stack;
  }
  static final class Receiver extends Player {
    final EnumMap<EquipmentSlot,ItemStack> items=new EnumMap<>(EquipmentSlot.class);
    EquipmentSlot lastRead;int events;
    Receiver(Level level){super(level,new GameProfile(new UUID(3,4),"glide"));}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
    @Override public ItemStack getItemBySlot(EquipmentSlot slot){lastRead=slot;return items==null?ItemStack.EMPTY:items.getOrDefault(slot,ItemStack.EMPTY);}
    @Override public void gameEvent(Holder<GameEvent> event){if(event==GameEvent.ELYTRA_GLIDE)events++;else throw new AssertionError(event);}
    boolean eligible(){return canGlide();}
    void update(){updateFallFlying();}
    void flag(){setSharedFlag(7,true);}
  }
  public static void run(Level level)throws Exception {
    for(int config=0;config<12;config++)for(EquipmentSlot slot:EquipmentSlot.VALUES) {
      var stack=stack(config,slot);
      System.out.println("glider-eligibility\t"+config+"\t"+slot.getName()+"\t"+LivingEntity.canGlideUsing(stack,slot));
    }
    int[] ticks={Integer.MIN_VALUE,-21,-20,-11,-1,0,8,9,18,19,20,29,39,Integer.MAX_VALUE};
    double[] ys={0,-0.5,Math.nextUp(-0.5),-0.0};
    var randomField=ExperienceVectors.field(Entity.class,"random");
    for(int i=0;i<98;i++) {
      var entity=new Receiver(level);
      entity.items.put(EquipmentSlot.MAINHAND,stack(0,EquipmentSlot.MAINHAND));
      entity.items.put(EquipmentSlot.CHEST,stack(0,EquipmentSlot.CHEST));
      entity.items.put(EquipmentSlot.HEAD,stack(0,EquipmentSlot.HEAD));
      int kind=i%7;
      if(kind==1)entity.setOnGround(true);
      if(kind==2)entity.getAbilities().flying=true;
      if(kind==3)entity.addEffect(new MobEffectInstance(MobEffects.LEVITATION,100));
      if(kind==4)entity.items.clear();
      if(kind==5)ExperienceVectors.field(Entity.class,"vehicle").set(entity,new Receiver(level));
      if(kind==6){entity.items.clear();entity.items.put(EquipmentSlot.CHEST,stack(4,EquipmentSlot.CHEST));}
      ExperienceVectors.field(LivingEntity.class,"fallFlyTicks").setInt(entity,ticks[(i/7)%ticks.length]);
      entity.fallDistance=10.0;entity.setDeltaMovement(0,ys[i%ys.length],0);entity.flag();
      long seed=12345L+i;var random=net.minecraft.util.RandomSource.create(seed);randomField.set(entity,random);
      boolean eligible=entity.eligible();entity.lastRead=null;
      entity.update();
      System.out.println("glider-cadence\t"+i+"\t"+kind+"\t"+ticks[(i/7)%ticks.length]+"\t"+TravelVectors.d(ys[i%ys.length])+"\t"+seed+"\t"+eligible+"\t"+entity.isFallFlying()+"\t"+TravelVectors.d(entity.fallDistance)+"\t"+entity.events+"\t"+(entity.lastRead==null?"-":entity.lastRead.getName())+"\t"+random.nextLong());
    }
  }
}
