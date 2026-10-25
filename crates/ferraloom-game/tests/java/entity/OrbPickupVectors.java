import java.util.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.level.*;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.item.*;
import net.minecraft.world.item.enchantment.*;
import net.minecraft.world.level.Level;
import static org.mockito.Mockito.*;

/** Actual orb pickup, recursive Mending and XP arithmetic. Menu broadcasting is
 * the observed external subsystem boundary; the real ServerPlayer.take runs. */
public final class OrbPickupVectors {
  static final List<String> events=new ArrayList<>(),scans=new ArrayList<>();
  static boolean captureScan;
  static String damage(Player player) {
    var values=new ArrayList<String>();
    for(var slot:EquipmentSlot.values())values.add(Integer.toString(player.getItemBySlot(slot).getDamageValue()));
    return String.join(",",values);
  }
  static String callback(String phase,Player player,ExperienceOrb orb)throws Exception {
    return phase+":"+player.takeXpDelay+":"+ExperienceVectors.field(ExperienceOrb.class,"count").getInt(orb)+":"+player.totalExperience+":"+damage(player);
  }
  static final class Receiver extends ServerPlayer {
    Receiver(){super(null,null,null,null);}
    @Override public net.minecraft.world.level.GameType gameMode(){return net.minecraft.world.level.GameType.SURVIVAL;}
    @Override public ItemStack getItemBySlot(EquipmentSlot slot) {
      if(captureScan)scans.add(slot.getName());
      return super.getItemBySlot(slot);
    }
    @Override public void take(Entity entity,int count) {
      try {events.add(callback("before",this,(ExperienceOrb)entity));super.take(entity,count);events.add(callback("after",this,(ExperienceOrb)entity));}
      catch(Exception error){throw new RuntimeException(error);}
    }
  }
  static void equipment(Player player,Level level,int config) {
    var mending=level.registryAccess().lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(Enchantments.MENDING);
    var protection=level.registryAccess().lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(Enchantments.PROTECTION);
    int[] damages=switch(config){case 0->new int[]{32,0,0,0,0,0,0,0};case 1->new int[8];case 2->new int[]{9,0,0,0,0,0,0,0};case 3->new int[]{3,0,0,0,0,0,0,0};case 4->new int[]{9,7,3,0,0,1,0,0};case 5->new int[]{1,2,3,4,5,6,7,8};default->new int[]{17,0,0,0,0,0,0,0};};
    player.getInventory().setSelectedSlot(6);
    for(var slot:EquipmentSlot.values()) {
      int index=slot.ordinal();
      if(damages[index]==0 && !(config==1 && index==0))continue;
      var item=new ItemStack(Items.DIAMOND_PICKAXE);item.setDamageValue(damages[index]);
      if(config!=0)item.enchant(config==6?protection:mending,1);
      player.setItemSlot(slot,item);
    }
  }
  static Receiver player(Level original,ServerLevel server,sun.misc.Unsafe allocator,int config,long seed)throws Exception {
    var player=(Receiver)allocator.allocateInstance(Receiver.class);
    DropVectors.copyFields(new LivingVectors(original),player,Player.class);
    ExperienceVectors.field(Entity.class,"level").set(player,server);
    player.setId(1);player.setUUID(new UUID(0,1));player.setPos(8.5,64,8.5);player.tickCount=37;
    player.containerMenu=mock(AbstractContainerMenu.class);
    ExperienceVectors.field(ServerPlayer.class,"lastSentExp").setInt(player,0);
    equipment(player,original,config);player.getRandom().setSeed(seed);
    return player;
  }
  public static void run(Level original)throws Exception {
    var field=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");field.setAccessible(true);var allocator=(sun.misc.Unsafe)field.get(null);
    var server=mock(ServerLevel.class,RETURNS_DEEP_STUBS);
    when(server.registryAccess()).thenReturn(original.registryAccess());
    when(server.getRandom()).thenReturn(RandomSource.create(789));
    when(server.getServer().reloadableRegistries().lookup()).thenReturn(original.registryAccess());
    var value=ExperienceOrb.class.getDeclaredMethod("setValue",int.class);value.setAccessible(true);
    int[] configs={0,1,2,3,4,5,6,4,4,2,5,0,0,0};
    int[] amounts={7,7,3,7,5,13,7,0,-3,7,7,7,7,7};
    int[] delays={0,0,0,0,0,0,0,0,0,0,0,1,2,-1};
    int[] counts={1,1,1,1,1,1,1,1,1,3,0,1,1,1};
    for(int config=0;config<configs.length;config++)for(long seed:new long[]{0,1,2,42,12345}) {
      var player=player(original,server,allocator,configs[config],seed);player.takeXpDelay=delays[config];
      var orb=new ExperienceOrb(EntityTypes.EXPERIENCE_ORB,original);orb.setId(2);orb.setPos(8.5,64,8.5);value.invoke(orb,amounts[config]);
      ExperienceVectors.field(ExperienceOrb.class,"count").setInt(orb,counts[config]);
      events.clear();orb.playerTouch(player);
      System.out.println("orb-pickup\t"+configs[config]+"\t"+amounts[config]+"\t"+delays[config]+"\t"+counts[config]+"\t"+seed+"\t"+player.takeXpDelay+"\t"+ExperienceVectors.field(ExperienceOrb.class,"count").getInt(orb)+"\t"+orb.isRemoved()+"\t"+damage(player)+"\t"+ExperienceVectors.result(player).replace('\t',':')+"\t"+(events.isEmpty()?"-":String.join(";",events))+"\t"+Long.toHexString(player.getRandom().nextLong()));
    }
    for(int config=0;config<=6;config++)for(long seed:new long[]{0,1,2,3,4,42,12345}) {
      var player=player(original,server,allocator,config,seed);scans.clear();captureScan=true;
      var selected=EnchantmentHelper.getRandomItemWith(EnchantmentEffectComponents.REPAIR_WITH_XP,player,ItemStack::isDamaged);
      captureScan=false;
      System.out.println("orb-repair-scan\t"+config+"\t"+seed+"\t"+selected.map(item->item.inSlot().getName()).orElse("-")+"\t"+String.join(",",scans)+"\t"+Long.toHexString(player.getRandom().nextLong()));
    }
    for(int delay:new int[]{Integer.MIN_VALUE,-1,0,1,2,3,Integer.MAX_VALUE}) {
      var player=new SwingVectors.ClockPlayer(original);player.setPos(8.5,64,8.5);player.takeXpDelay=delay;player.tick();
      System.out.println("orb-delay-tick\t"+delay+"\t"+player.takeXpDelay);
    }
  }
}
