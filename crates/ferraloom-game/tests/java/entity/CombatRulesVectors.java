import java.util.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;
import static org.mockito.Mockito.*;

/** Actual CombatRules float arithmetic and DamageSource's live direct-weapon read. */
public final class CombatRulesVectors {
  static String f(float value){return Integer.toHexString(Float.floatToRawIntBits(value));}
  static String weapon(DamageSource source){var stack=source.getWeaponItem();return stack==null?"null":BuiltInRegistries.ITEM.getKey(stack.getItem()).toString();}
  public static void run(Level level)throws Exception {
    var victim=new LivingVectors(level);var attacker=new LivingVectors(level);
    ExperienceVectors.field(Entity.class,"level").set(victim,mock(ServerLevel.class));
    float[][] cases={{4,0,0},{4,20,0},{20,20,8},{100,30,20},{-1,30,8},{-0.0f,0,0},{0.000001f,7,0},{0.3f,7.25f,1.5f},{Float.NaN,20,0},{Float.POSITIVE_INFINITY,20,8},{Float.NEGATIVE_INFINITY,20,8},{Float.MAX_VALUE,30,20}};
    for(String kind:List.of("absent","empty","stone")) {
      attacker.getInventory().setSelectedItem(kind.equals("stone")?new ItemStack(Items.STONE):ItemStack.EMPTY);
      var source=kind.equals("absent")?level.damageSources().generic():level.damageSources().playerAttack(attacker);
      for(var setup:cases)System.out.println("combat-armor\t"+kind+"\t"+f(setup[0])+"\t"+f(setup[1])+"\t"+f(setup[2])+"\t"+f(CombatRules.getDamageAfterAbsorb(victim,setup[0],source,setup[1],setup[2])));
    }
    float[][] magic={{1,0},{1,-1},{1,20},{1,25},{7.3f,4},{-0.0f,20},{0,20},{-1,4},{Float.MAX_VALUE,20},{Float.POSITIVE_INFINITY,4},{Float.NaN,4},{4,Float.NaN}};
    for(var setup:magic)System.out.println("combat-magic\t"+f(setup[0])+"\t"+f(setup[1])+"\t"+f(CombatRules.getDamageAfterMagicAbsorb(setup[0],setup[1])));
    var cow=new Cow(EntityTypes.COW,level);cow.setItemSlot(EquipmentSlot.MAINHAND,new ItemStack(Items.IRON_AXE));
    var tnt=new PrimedTnt(EntityTypes.TNT,level);
    var holder=level.damageSources().generic().typeHolder();
    System.out.println("damage-weapon\tabsent\t"+weapon(new DamageSource(holder)));
    attacker.getInventory().setSelectedSlot(0);attacker.getInventory().setItem(0,new ItemStack(Items.STONE));attacker.getInventory().setItem(1,new ItemStack(Items.GOLD_INGOT));
    var direct=new DamageSource(holder,attacker,cow);
    System.out.println("damage-weapon\tselected0\t"+weapon(direct));
    attacker.getInventory().setSelectedSlot(1);System.out.println("damage-weapon\tselected1\t"+weapon(direct));
    attacker.getMainHandItem().setCount(0);System.out.println("damage-weapon\tretained-empty\t"+weapon(direct));
    System.out.println("damage-weapon\tcow-direct\t"+weapon(new DamageSource(holder,cow,attacker)));
    System.out.println("damage-weapon\ttnt-direct\t"+weapon(new DamageSource(holder,tnt,attacker)));
  }
}
