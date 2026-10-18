import java.util.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.UseEffects;
import net.minecraft.world.level.Level;

/** Real idle/active stopUsingItem state, data flags and use vibration selection. */
public final class UseItemVectors extends net.minecraft.world.entity.player.Player {
  UseItemVectors(Level level) {super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"using"));}
  @Override public net.minecraft.world.level.GameType gameMode() {return net.minecraft.world.level.GameType.SURVIVAL;}
  void seed(ItemStack stack,int ticks,int flags) {
    useItem=stack;useItemRemaining=ticks;entityData.set(DATA_LIVING_ENTITY_FLAGS,(byte)flags);
  }
  String state() {return isUsingItem()+","+entityData.get(DATA_LIVING_ENTITY_FLAGS)+","+getUseItem().isEmpty()+","+getUseItemRemainingTicks();}
  @SuppressWarnings("unchecked") public static void run(Level level) throws Exception {
    var capture=level.getClass().getDeclaredField("captureEntitySounds");capture.setAccessible(true);capture.setBoolean(level,true);
    var field=level.getClass().getDeclaredField("gameEvents");field.setAccessible(true);var events=(List<String>)field.get(level);
    for(String name:List.of("idle","stored_without_flag","active_empty","active_default","vibration_off","vibration_on","empty_payload","other_flags","signed_inactive")) {
      var player=new UseItemVectors(level);player.setId(1);player.setPos(8.5,64,8.5);
      ItemStack stack=switch(name) {case "idle","active_empty","signed_inactive"->ItemStack.EMPTY;default->new ItemStack(Items.STICK);};
      if(name.equals("vibration_off")||name.equals("vibration_on")||name.equals("empty_payload")||name.equals("other_flags"))stack.set(DataComponents.USE_EFFECTS,new UseEffects(true,!name.equals("vibration_off"),1f));
      if(name.equals("empty_payload"))stack.setCount(0);
      int flags=switch(name) {case "idle"->0;case "stored_without_flag"->2;case "active_empty","vibration_off"->3;case "vibration_on"->7;case "other_flags"->-1;case "signed_inactive"->-128;default->1;};
      player.seed(stack,name.equals("idle")?0:name.equals("other_flags")?-5:17,flags);
      String before=player.state();player.getEntityData().packDirty();events.clear();player.stopUsingItem();
      System.out.println("stop-use\t"+name+"\t"+before+"\t"+player.state()+","+(player.getEntityData().packDirty()!=null)+"\t"+(events.isEmpty()?"-":String.join("|",events)));
    }
  }
}
