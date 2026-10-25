import java.util.*;
import com.mojang.authlib.GameProfile;
import io.netty.buffer.Unpooled;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.protocol.game.ClientboundSwingAnimationPacket;
import net.minecraft.server.level.*;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.effect.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.SwingAnimation;
import net.minecraft.world.level.*;
import org.mockito.Answers;
import static org.mockito.Mockito.*;

/** Actual SwingState and ServerPlayer swing/reset calls, including native
 * packet bytes and the ChunkMap tracking/self dispatch selection. */
public final class SwingVectors {
  static String f(float value){return Integer.toHexString(Float.floatToRawIntBits(value));}
  static String snapshot(LivingEntity.SwingState state)throws Exception {
    var current=(LivingEntity.SwingDescription)ExperienceVectors.field(state.getClass(),"currentSwing").get(state);
    return (current==null?"none":current.hand().ordinal()+","+current.animation().type().ordinal()+","+current.animation().duration()+","+current.durationTicks())+":"+ExperienceVectors.field(state.getClass(),"ticks").getInt(state)+":"+f(ExperienceVectors.field(state.getClass(),"oldAnimation").getFloat(state))+":"+f(ExperienceVectors.field(state.getClass(),"animation").getFloat(state));
  }
  static LivingEntity.SwingState state(LivingEntity e)throws Exception {return (LivingEntity.SwingState)ExperienceVectors.field(LivingEntity.class,"swingState").get(e);}
  static class Receiver extends ServerPlayer {Receiver(){super(null,null,null,null);}}
  static class ClockPlayer extends Player {
    ClockPlayer(Level level){super(level,new GameProfile(new UUID(3,4),"clock"));}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
    @Override public void baseTick(){}
    @Override public void aiStep(){}
    // Isolate Player's clock/snapshot slot from unrelated equipment broadcasts.
    @Override protected Map<EquipmentSlot,ItemStack> collectEquipmentChanges(Map<EquipmentSlot,ItemStack> previous){return null;}
  }
  static String item(ItemStack stack){return BuiltInRegistries.ITEM.getKey(stack.getItem())+","+stack.getCount();}
  static void effects(LivingEntity entity,int config) {
    if(config==1||config==2||config==3||config==5)entity.addEffect(new MobEffectInstance(MobEffects.HASTE,100,config==2?255:config==5?1:0));
    if(config==3||config==6)entity.addEffect(new MobEffectInstance(MobEffects.CONDUIT_POWER,100,config==3?2:0));
    if(config>=4)entity.addEffect(new MobEffectInstance(MobEffects.MINING_FATIGUE,100,config==7?255:config==5?5:2));
  }
  public static void run(Level original)throws Exception {
    int[] durations={-2,-1,0,1,2,6,12,Integer.MAX_VALUE};
    for(int duration:durations) {
      var state=new LivingEntity.SwingState();
      for(int step=0;step<16;step++) {
        boolean start=step==0||step==2||step==6||step==9||step==13;
        var hand=step>=9?InteractionHand.OFF_HAND:InteractionHand.MAIN_HAND;
        var animation=new SwingAnimation(step>=13?SwingAnimationType.STAB:SwingAnimationType.WHACK,duration);
        String result;
        if(start)result=Boolean.toString(state.startIfAble(hand,animation,duration));else{state.tick();result="-";}
        System.out.println("swing-state\t"+duration+"\t"+step+"\t"+(start?"start":"tick")+"\t"+hand.ordinal()+"\t"+animation.type().ordinal()+"\t"+result+"\t"+snapshot(state)+"\t"+f(state.getAnimation(-0.25F))+","+f(state.getAnimation(0))+","+f(state.getAnimation(0.5F))+","+f(state.getAnimation(1.5F)));
      }
    }
    var packets=new ArrayList<String>();
    var cache=mock(ServerChunkCache.class,call->{
      String method=call.getMethod().getName();
      if(method.equals("sendToTrackingPlayers")||method.equals("sendToTrackingPlayersAndSelf")) {
        var packet=(ClientboundSwingAnimationPacket)call.getArgument(1);var bytes=Unpooled.buffer();
        ClientboundSwingAnimationPacket.STREAM_CODEC.encode(bytes,packet);byte[] payload=new byte[bytes.readableBytes()];bytes.readBytes(payload);bytes.release();
        packets.add((method.equals("sendToTrackingPlayersAndSelf")?"self:":"players:")+HexFormat.of().formatHex(payload));return null;
      }
      return Answers.RETURNS_DEFAULTS.answer(call);
    });
    var server=mock(ServerLevel.class);when(server.getChunkSource()).thenReturn(cache);
    var allocatorField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");allocatorField.setAccessible(true);var allocator=(sun.misc.Unsafe)allocatorField.get(null);
    int[] ticks={0,1,3,4,6,8};
    for(int i=0;i<48;i++) {
      var seed=new LivingVectors(original);effects(seed,i%8);
      var player=(Receiver)allocator.allocateInstance(Receiver.class);DropVectors.copyFields(seed,player,Player.class);
      ExperienceVectors.field(Entity.class,"level").set(player,server);player.setId(77);
      player.swing(InteractionHand.MAIN_HAND,SwingAnimation.DEFAULT,false);
      for(int tick=0;tick<ticks[i%ticks.length];tick++)state(player).tick();
      ExperienceVectors.field(LivingEntity.class,"attackStrengthTicker").setInt(player,17+i);
      ExperienceVectors.field(LivingEntity.class,"itemSwapTicker").setInt(player,23+i);
      var hand=i%3==0?InteractionHand.OFF_HAND:InteractionHand.MAIN_HAND;
      var animation=new SwingAnimation(SwingAnimationType.values()[i%3],durations[i%durations.length]);boolean sendSelf=i%2==0;packets.clear();
      player.swingAndResetAttackStrength(hand,animation,sendSelf);
      System.out.println("swing-player\t"+i+"\t"+(i%8)+"\t"+ticks[i%ticks.length]+"\t"+hand.ordinal()+"\t"+animation.type().ordinal()+"\t"+animation.duration()+"\t"+sendSelf+"\t"+snapshot(state(player))+"\t"+ExperienceVectors.field(LivingEntity.class,"attackStrengthTicker").getInt(player)+","+ExperienceVectors.field(LivingEntity.class,"itemSwapTicker").getInt(player)+"\t"+(packets.isEmpty()?"-":String.join(";",packets)));
    }
    for(int i=0;i<8;i++) {
      var player=new ClockPlayer(original);player.setPos(8.5,64,8.5);
      var old=i==0?ItemStack.EMPTY:new ItemStack(Items.STONE,3);var current=i<2?ItemStack.EMPTY:new ItemStack(i==2?Items.DIRT:Items.STONE,i==3?5:3);
      if(i==4)current.set(net.minecraft.core.component.DataComponents.CUSTOM_NAME,net.minecraft.network.chat.Component.literal("changed"));
      if(i==5)current.setCount(0);
      if(i==6)current.setCount(-1);
      ExperienceVectors.field(Player.class,"lastItemInMainHand").set(player,old);player.setItemInHand(InteractionHand.MAIN_HAND,current);
      ExperienceVectors.field(LivingEntity.class,"attackStrengthTicker").setInt(player,i==7?Integer.MAX_VALUE:17);
      ExperienceVectors.field(LivingEntity.class,"itemSwapTicker").setInt(player,i==7?Integer.MAX_VALUE:23);
      player.tick();
      System.out.println("swing-clock\t"+i+"\t"+ExperienceVectors.field(LivingEntity.class,"attackStrengthTicker").getInt(player)+","+ExperienceVectors.field(LivingEntity.class,"itemSwapTicker").getInt(player)+"\t"+item((ItemStack)ExperienceVectors.field(Player.class,"lastItemInMainHand").get(player)));
    }
  }
}
