import java.util.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;

/** Real freeze getters, armor eligibility and protected frost attribute methods. */
public final class FreezingVectors extends Player {
  GameType mode=GameType.SURVIVAL;
  BlockState supporting=Blocks.AIR.defaultBlockState();int reads;
  FreezingVectors(Level level){super(level,new GameProfile(new UUID(0,1),"freeze"));}
  @Override public GameType gameMode(){return mode;}
  @Override protected BlockState getBlockStateOnLegacy(){reads++;return supporting;}
  public static void run(Level level) {
    var base=new PrimedTnt(EntityTypes.TNT,level);
    int[] frozen={Integer.MIN_VALUE,-141,-1,0,1,70,139,140,141,Integer.MAX_VALUE};
    for(int ticks:frozen) {
      base.setTicksFrozen(ticks);
      System.out.println("freeze-counter\t"+ticks+"\t"+Integer.toHexString(Float.floatToRawIntBits(base.getPercentFrozen()))+"\t"+base.isFullyFrozen()+"\t"+base.getTicksRequiredToFreeze());
    }
    for(boolean spectator:List.of(false,true))for(EquipmentSlot slot:EquipmentSlot.VALUES)for(int count:new int[]{1,0,-1}) {
      var player=new FreezingVectors(level);player.mode=spectator?GameType.SPECTATOR:GameType.SURVIVAL;
      var stack=new ItemStack(Items.LEATHER_BOOTS);stack.setCount(count);player.setItemSlot(slot,stack);
      System.out.println("freeze-armor\t"+spectator+"\t"+slot.getSerializedName()+"\t"+count+"\t"+player.canFreeze());
    }
    for(boolean air:List.of(false,true))for(int ticks:frozen) {
      var player=new FreezingVectors(level);player.setTicksFrozen(ticks);
      player.supporting=air?Blocks.AIR.defaultBlockState():Blocks.STONE.defaultBlockState();
      player.removeFrost();player.tryAddFrost();
      var speed=player.getAttribute(Attributes.MOVEMENT_SPEED);var modifier=speed.getModifier(Identifier.withDefaultNamespace("powder_snow"));
      System.out.println("freeze-frost\t"+air+"\t"+ticks+"\t"+player.reads+"\t"+
        (modifier==null?"none":Long.toHexString(Double.doubleToRawLongBits(modifier.amount())))+"\t"+Long.toHexString(Double.doubleToRawLongBits(speed.getValue())));
      player.removeFrost();
      if(speed.getModifier(Identifier.withDefaultNamespace("powder_snow"))!=null)throw new AssertionError();
    }
  }
}
