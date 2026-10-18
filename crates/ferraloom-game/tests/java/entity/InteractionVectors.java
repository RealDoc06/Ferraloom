import java.lang.reflect.*;
import java.util.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.DataComponents;
import net.minecraft.resources.Identifier;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.UseCooldown;
import net.minecraft.world.level.Level;

/** Real hand mapping, eye-to-AABB range checks and ItemCooldowns callbacks. */
public final class InteractionVectors {
  static final class Probe extends ItemCooldowns {
    final List<String> calls=new ArrayList<>();
    @Override protected void onCooldownStarted(Identifier group,int duration) {calls.add(group+":"+duration);}
    @Override protected void onCooldownEnded(Identifier group) {calls.add(group+":0");}
  }
  static String percent(float value) {return Float.isNaN(value)?"nan":Integer.toHexString(Float.floatToRawIntBits(value));}
  static void snapshot(String name,Probe cooldowns,List<ItemStack> stacks) {
    var result=new ArrayList<String>();
    for(var stack:stacks) for(float partial:new float[]{0,0.5f,-0.0f,Float.NaN,Float.POSITIVE_INFINITY}) result.add(percent(cooldowns.getCooldownPercent(stack,partial)));
    System.out.println("cooldown\t"+name+"\t"+String.join(",",result)+"\t"+(cooldowns.calls.isEmpty()?"-":String.join("|",cooldowns.calls)));
    cooldowns.calls.clear();
  }
  public static void run(Level level) throws Exception {
    for(var mode:net.minecraft.world.level.GameType.values())for(boolean flying:List.of(false,true)) {
      var abilityPlayer=new LivingVectors(level);var abilities=abilityPlayer.getAbilities();abilities.flying=flying;
      mode.updatePlayerAbilities(abilities);boolean applied=abilities.instabuild;abilities.instabuild=!applied;
      var packed=net.minecraft.world.entity.player.Abilities.Packed.CODEC.encodeStart(net.minecraft.nbt.NbtOps.INSTANCE,abilities.pack()).getOrThrow();
      boolean saved=((net.minecraft.nbt.CompoundTag)packed).getBooleanOr("instabuild",false);
      boolean packet=new net.minecraft.network.protocol.game.ClientboundPlayerAbilitiesPacket(abilities).canInstabuild();
      boolean prevents=abilityPlayer.preventsBlockDrops();
      var tag=new net.minecraft.nbt.CompoundTag();tag.put("abilities",packed);
      abilityPlayer.load(net.minecraft.world.level.storage.TagValueInput.create(net.minecraft.util.ProblemReporter.DISCARDING,level.registryAccess(),tag));
      mode.updatePlayerAbilities(abilities);
      System.out.println("player-instabuild\t"+mode.getId()+"\t"+flying+"\t"+applied+","+prevents+","+saved+","+packet+","+abilities.instabuild);
    }
    var player=new LivingVectors(level);player.setPos(0,64,0);
    player.getInventory().setSelectedSlot(5);
    player.setItemInHand(InteractionHand.MAIN_HAND,new ItemStack(Items.STONE,3));
    player.setItemInHand(InteractionHand.OFF_HAND,new ItemStack(Items.SHIELD));
    for(boolean shift:List.of(false,true)) {
      player.setShiftKeyDown(shift);
      System.out.println("player-interaction\t"+shift+"\t"+player.isSecondaryUseActive()+","+player.getInventory().getSelectedSlot()+","+player.getMainHandItem().getCount()+","+player.getOffhandItem().is(Items.SHIELD));
    }
    double[] ranges={4.5,5,5.000000000000001,0,Double.NaN};
    for(int i=0;i<ranges.length;i++) {
      player.getAttribute(Attributes.BLOCK_INTERACTION_RANGE).setBaseValue(ranges[i]);
      for(double buffer:new double[]{0,1,4,-1}) for(BlockPos pos:List.of(new BlockPos(5,65,0),new BlockPos(0,70,0),new BlockPos(0,65,0))) {
        System.out.println("player-range\t"+i+"\t"+buffer+"\t"+pos.getX()+","+pos.getY()+","+pos.getZ()+"\t"+player.isWithinBlockInteractionRange(pos,buffer));
      }
    }
    var stone=new ItemStack(Items.STONE);var shield=new ItemStack(Items.SHIELD);var dirt=new ItemStack(Items.DIRT);
    var group=Identifier.withDefaultNamespace("shared");
    shield.set(DataComponents.USE_COOLDOWN,new UseCooldown(1,Optional.of(group)));
    dirt.set(DataComponents.USE_COOLDOWN,new UseCooldown(1,Optional.of(group)));
    var empty=shield.copy();empty.setCount(0);
    var stacks=List.of(stone,shield,dirt,empty);
    var cooldowns=new Probe();
    for(int i=0;i<stacks.size();i++) System.out.println("cooldown-group\t"+i+"\t"+cooldowns.getCooldownGroup(stacks.get(i)));
    snapshot("fresh",cooldowns,stacks);
    cooldowns.addCooldown(stone,3);cooldowns.addCooldown(shield,2);snapshot("added",cooldowns,stacks);
    cooldowns.tick();snapshot("tick1",cooldowns,stacks);
    cooldowns.addCooldown(dirt,4);snapshot("replace",cooldowns,stacks);
    cooldowns.tick();snapshot("tick2",cooldowns,stacks);
    cooldowns.tick();snapshot("tick3",cooldowns,stacks);
    cooldowns.removeCooldown(group);cooldowns.removeCooldown(group);snapshot("remove-absent",cooldowns,stacks);
    cooldowns.addCooldown(stone,0);cooldowns.addCooldown(shield,-1);snapshot("nonpositive",cooldowns,stacks);
    cooldowns.tick();snapshot("nonpositive-expire",cooldowns,stacks);
    for(int i=0;i<24;i++) cooldowns.addCooldown(Identifier.withDefaultNamespace("group_"+i),1);
    snapshot("rehash",cooldowns,stacks);cooldowns.tick();snapshot("iterator-expire",cooldowns,stacks);
    var tick=ItemCooldowns.class.getDeclaredField("tickCount");tick.setAccessible(true);
    tick.setInt(cooldowns,Integer.MAX_VALUE-1);cooldowns.addCooldown(stone,2);snapshot("overflow-add",cooldowns,stacks);
    cooldowns.tick();snapshot("overflow-end",cooldowns,stacks);
    cooldowns.addCooldown(shield,2);cooldowns.tick();snapshot("wrap-tick",cooldowns,stacks);
  }
}
