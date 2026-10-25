import java.nio.file.Path;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.stats.*;
import net.minecraft.util.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.*;
import net.minecraft.world.item.enchantment.*;
import net.minecraft.world.level.*;
import static org.mockito.Mockito.*;

/** Actual ItemStack overloads, canonical Unbreaking and real player break hooks. */
public final class HurtAndBreakVectors {
  static final List<String> events=new ArrayList<>();
  static final class Receiver extends ServerPlayer {
    ServerLevel world; PlayerAdvancements tracker; GameType mode;
    Receiver(){super(null,null,null,null);}
    @Override public ServerLevel level(){return world;}
    @Override public GameType gameMode(){return mode;}
    @Override public PlayerAdvancements getAdvancements(){return tracker;}
    @Override public void awardStat(Stat<?> stat,int amount){super.awardStat(stat,amount);events.add("stat:"+stat.getName()+":"+amount);}
  }
  static ItemStack stack(int kind,int enchantment,RegistryAccess registry) {
    var item=kind==5?Items.DIAMOND_CHESTPLATE:kind==7?Items.STONE:Items.IRON_AXE;
    var stack=new ItemStack(item,kind==9?2:1);
    if(kind!=7){stack.set(DataComponents.MAX_DAMAGE,7);stack.set(DataComponents.DAMAGE,6);}
    if(kind==1)stack.set(DataComponents.UNBREAKABLE,Unit.INSTANCE);
    if(kind==2)stack.remove(DataComponents.DAMAGE);
    if(kind==3)stack.remove(DataComponents.MAX_DAMAGE);
    if(kind==4)stack.set(DataComponents.MAX_DAMAGE,0);
    if(kind==8)stack.set(DataComponents.MAX_DAMAGE,-3);
    if(enchantment>0)stack.enchant(registry.lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(Enchantments.UNBREAKING),enchantment);
    if(kind==6)stack.setCount(0);
    return stack;
  }
  static String snapshot(ItemStack stack) {
    return BuiltInRegistries.ITEM.getKey(stack.getItem())+","+stack.getCount()+","+stack.getDamageValue()+","+stack.getMaxDamage()+","+stack.isDamageableItem()+","+stack.isBroken();
  }
  static Receiver player(Level original,ServerLevel server,sun.misc.Unsafe allocator)throws Exception {
    var player=(Receiver)allocator.allocateInstance(Receiver.class);
    DropVectors.copyFields(new LivingVectors(original),player,Player.class);
    ExperienceVectors.field(Entity.class,"level").set(player,server);player.world=server;player.tracker=mock(PlayerAdvancements.class);player.mode=GameType.SURVIVAL;
    ExperienceVectors.field(ServerPlayer.class,"stats").set(player,new ServerStatsCounter(mock(MinecraftServer.class),Path.of("unused-durability-stats.json")));
    return player;
  }
  static void run(Level original)throws Exception {
    var field=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");field.setAccessible(true);var allocator=(sun.misc.Unsafe)field.get(null);
    var random=RandomSource.create(0);var server=mock(ServerLevel.class,RETURNS_DEEP_STUBS);
    when(server.registryAccess()).thenReturn(original.registryAccess());when(server.getRandom()).thenReturn(random);
    when(server.getServer().reloadableRegistries().lookup()).thenReturn(original.registryAccess());
    when(server.getScoreboard()).thenReturn(new ServerScoreboard(mock(MinecraftServer.class)));
    doAnswer(call->{events.add("event:"+call.getArgument(1));return null;}).when(server).broadcastEntityEvent(any(Entity.class),anyByte());
    for(int kind=0;kind<10;kind++)for(int unbreaking:new int[]{0,1,3})for(int amount:new int[]{-3,0,1,7,129})for(long seed:new long[]{0,1,42})for(String mode:List.of("none","survival","infinite","creative","slot")) {
      var player=(Receiver)allocator.allocateInstance(Receiver.class);
      DropVectors.copyFields(new LivingVectors(original),player,Player.class);
      ExperienceVectors.field(Entity.class,"level").set(player,server);player.world=server;player.tracker=mock(PlayerAdvancements.class);
      player.mode=mode.equals("creative")?GameType.CREATIVE:GameType.SURVIVAL;
      player.getAbilities().instabuild=mode.equals("infinite")||mode.equals("creative");
      ExperienceVectors.field(ServerPlayer.class,"stats").set(player,new ServerStatsCounter(mock(MinecraftServer.class),Path.of("unused-durability-stats.json")));
      var stack=stack(kind,unbreaking,original.registryAccess());var before=snapshot(stack);var callbacks=new ArrayList<String>();events.clear();random.setSeed(seed);
      if(mode.equals("slot"))stack.hurtAndBreak(amount,player,kind==5?EquipmentSlot.CHEST:EquipmentSlot.MAINHAND);
      else stack.hurtAndBreak(amount,server,mode.equals("none")?null:player,broken->callbacks.add(snapshot(broken)+"#"+snapshot(stack)+"#"+(broken==stack)));
      System.out.println("hurt-break\t"+kind+"\t"+mode+"\t"+amount+"\t"+unbreaking+"\t"+seed+"\t"+before+"\t"+snapshot(stack)+"\t"+String.join(";",callbacks)+"\t"+String.join(";",events)+"\t"+random.nextLong());
    }
    for(String enchantment:List.of("minecraft:unbreaking","minecraft:swift_sneak","minecraft:soul_speed")) {
      var slot=enchantment.endsWith("swift_sneak")?EquipmentSlot.LEGS:enchantment.endsWith("soul_speed")?EquipmentSlot.FEET:EquipmentSlot.MAINHAND;
      var holder=original.registryAccess().lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(net.minecraft.resources.ResourceKey.create(Registries.ENCHANTMENT,net.minecraft.resources.Identifier.parse(enchantment)));
      System.out.println("break-cleanup\t"+enchantment+"\t"+slot.getName()+"\t"+holder.value().matchingSlot(slot)+"\t"+holder.value().effects().has(EnchantmentEffectComponents.ATTRIBUTES)+"\t"+holder.value().effects().has(EnchantmentEffectComponents.LOCATION_CHANGED));
    }
    for(long seed:new long[]{0,1,42}) {
      var entity=new GlidingVectors.Receiver(original);
      entity.items.put(EquipmentSlot.MAINHAND,GlidingVectors.stack(0,EquipmentSlot.MAINHAND));
      entity.items.put(EquipmentSlot.CHEST,GlidingVectors.stack(0,EquipmentSlot.CHEST));
      entity.items.put(EquipmentSlot.HEAD,GlidingVectors.stack(0,EquipmentSlot.HEAD));
      ExperienceVectors.field(Entity.class,"level").set(entity,server);
      ExperienceVectors.field(LivingEntity.class,"fallFlyTicks").setInt(entity,19);entity.getRandom().setSeed(seed);random.setSeed(123);entity.flag();
      entity.update();
      var damage=new ArrayList<String>();for(var slot:EquipmentSlot.VALUES)damage.add(Integer.toString(entity.getItemBySlot(slot).getDamageValue()));
      System.out.println("glider-wear\t"+seed+"\t"+String.join(",",damage)+"\t"+entity.events+"\t"+entity.getRandom().nextLong()+"\t"+random.nextLong());
    }
    var hurt=LivingEntity.class.getDeclaredMethod("doHurtEquipment",net.minecraft.world.damagesource.DamageSource.class,float.class,EquipmentSlot[].class);hurt.setAccessible(true);
    for(float damage:new float[]{0,1,3.9f,4,16,Float.NaN})for(long seed:new long[]{0,1,42})for(int unbreaking:new int[]{0,3})for(boolean infinite:new boolean[]{false,true}) {
      var player=player(original,server,allocator);player.getAbilities().instabuild=infinite;
      var stack=stack(5,unbreaking,original.registryAccess());player.setItemSlot(EquipmentSlot.CHEST,stack);events.clear();random.setSeed(seed);
      hurt.invoke(player,original.damageSources().generic(),damage,new EquipmentSlot[]{EquipmentSlot.CHEST});
      System.out.println("equipment-wear\t"+Float.floatToRawIntBits(damage)+"\t"+seed+"\t"+unbreaking+"\t"+infinite+"\t"+snapshot(stack)+"\t"+String.join(";",events)+"\t"+random.nextLong());
    }
  }
  public static void main(String[] args)throws Exception {
    var out=System.out;var errors=System.err;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(out);System.setErr(errors);
    var registry=NeighborVectors.load();var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    var context=HolderLookup.Provider.create(java.util.stream.Stream.concat(builtins.listRegistries(),registry.listRegistries()));
    BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(context).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
    run(new EntityFixture(registry));
  }
}
