import java.util.*;
import java.util.stream.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.gameevent.GameEvent;
import org.mockito.Answers;
import static org.mockito.Mockito.*;

/** Actual LivingEntity.setItemSlot/onEquipItem and Player's event-type override. */
public final class EquipVectors {
  public static void main(String[] args)throws Exception {
    var out=System.out;var err=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(err);
    var registries=NeighborVectors.load();
    var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var provider=HolderLookup.Provider.create(Stream.concat(builtins.listRegistries(),registries.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(provider).forEach(pending->pending.apply());
    var first=Entity.class.getDeclaredField("firstTick");first.setAccessible(true);
    var trace=new ArrayList<String>();
    var level=mock(Level.class,call->{
      var a=call.getArguments();
      if(call.getMethod().getName().equals("registryAccess"))return registries;
      if(call.getMethod().getName().equals("playSeededSound")&&a.length==9){
        var sound=(Holder<?>)a[4];
        trace.add("sound:"+sound.unwrapKey().orElseThrow().identifier()+":"+((SoundSource)a[5]).ordinal()+":"+Long.toHexString((long)a[8]));return null;
      }
      if(call.getMethod().getName().equals("gameEvent")){
        for(var arg:a)if(arg instanceof Holder<?> h && h.value() instanceof GameEvent){trace.add("event:"+BuiltInRegistries.GAME_EVENT.getKey((GameEvent)h.value()));return null;}
        throw new AssertionError(Arrays.toString(a));
      }
      return Answers.RETURNS_DEFAULTS.answer(call);
    });
    for(String kind:List.of("cow","player","spectator"))for(var slot:EquipmentSlot.VALUES)for(int scenario=0;scenario<6;scenario++){
      LivingEntity entity;
      if(kind.equals("cow"))entity=new Cow(EntityTypes.COW,level);
      else {var player=new SlotVectors.PlayerReceiver(level);player.mode=kind.equals("spectator")?GameType.SPECTATOR:GameType.SURVIVAL;entity=player;}
      entity.setPos(8.5,64.0,8.5);
      Item input=switch(slot){case MAINHAND,OFFHAND,HEAD->Items.IRON_HELMET;case CHEST->Items.IRON_CHESTPLATE;case LEGS->Items.IRON_LEGGINGS;case FEET->Items.IRON_BOOTS;case BODY->Items.WOLF_ARMOR;case SADDLE->Items.SADDLE;};
      if(scenario==1||scenario==5)input=Items.STONE;
      if(scenario==1)entity.setItemSlot(slot,new ItemStack(Items.STONE,2));
      if(scenario==4){entity.setItemSlot(slot,new ItemStack(Items.STONE,2));input=Items.AIR;}
      first.setBoolean(entity,scenario==2);entity.setSilent(scenario==3);entity.getRandom().setSeed(100);trace.clear();
      entity.setItemSlot(slot,input==Items.AIR?ItemStack.EMPTY:new ItemStack(input,scenario==1?7:1));
      System.out.println("slot-equip\t"+kind+"\t"+slot.getName()+"\t"+scenario+"\t"+BuiltInRegistries.ITEM.getKey(input)+"\t"+SlotVectors.describe(entity.getItemBySlot(slot))+"\t"+(trace.isEmpty()?"-":String.join(";",trace))+"\t"+Long.toHexString(entity.getRandom().nextLong()));
    }
  }
}
