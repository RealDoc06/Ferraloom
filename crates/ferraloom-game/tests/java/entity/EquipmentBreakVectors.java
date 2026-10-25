import java.util.*;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.NbtOps;
import net.minecraft.server.level.*;
import net.minecraft.stats.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.ItemAttributeModifiers;
import net.minecraft.world.level.*;
import org.mockito.Answers;
import static org.mockito.Mockito.*;

/** Real ServerPlayer equipment-break parent/stat calls; only outbound delivery is observed. */
public final class EquipmentBreakVectors {
  static final class Receiver extends ServerPlayer {
    List<String> events;
    Receiver(){super(null,null,null,null);}
    @Override public GameType gameMode(){return GameType.SURVIVAL;}
    @Override public void awardStat(Stat<?> stat,int count) {
      if(stat.getType()!=Stats.ITEM_BROKEN)throw new AssertionError(stat);
      events.add("stat:"+BuiltInRegistries.ITEM.getKey((Item)stat.getValue())+":"+count);
    }
  }
  static String saved(AttributeMap attributes) {
    return AttributeInstance.Packed.CODEC.listOf().encodeStart(NbtOps.INSTANCE,attributes.pack()).getOrThrow().toString();
  }
  public static void run(Level level)throws Exception {
    var unsafeField=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");unsafeField.setAccessible(true);
    var allocator=(sun.misc.Unsafe)unsafeField.get(null);
    for(var slot:EquipmentSlot.VALUES)for(String scenario:List.of("absent","installed","other-slot","empty")) {
      var player=(Receiver)allocator.allocateInstance(Receiver.class);
      DropVectors.copyFields(new LivingVectors(level),player,Player.class);
      player.events=new ArrayList<>();
      var attributes=new AttributeMap(DefaultAttributes.getSupplier(EntityTypes.PLAYER));
      ExperienceVectors.field(LivingEntity.class,"attributes").set(player,attributes);
      var modifier=AttributeVectors.modifier("broken",0.5,AttributeModifier.Operation.ADD_VALUE);
      if(scenario.equals("installed"))attributes.getInstance(Attributes.SUBMERGED_MINING_SPEED).addTransientModifier(modifier);
      attributes.getAttributesToSync().clear();attributes.getAttributesToUpdate().clear();
      var server=mock(ServerLevel.class,call->{
        if(call.getMethod().getName().equals("broadcastEntityEvent")) {
          player.events.add("event:"+call.getArgument(1)+":"+attributes.hasModifier(Attributes.SUBMERGED_MINING_SPEED,modifier.id()));
          return null;
        }
        return Answers.RETURNS_DEFAULTS.answer(call);
      });
      ExperienceVectors.field(Entity.class,"level").set(player,server);
      var group=EquipmentSlotGroup.bySlot(scenario.equals("other-slot")?(slot==EquipmentSlot.HEAD?EquipmentSlot.CHEST:EquipmentSlot.HEAD):slot);
      var broken=new ItemStack(Items.STONE);
      broken.set(DataComponents.ATTRIBUTE_MODIFIERS,new ItemAttributeModifiers(List.of(new ItemAttributeModifiers.Entry(Attributes.SUBMERGED_MINING_SPEED,modifier,group))));
      if(scenario.equals("empty"))broken.setCount(0);
      player.onEquippedItemBroken(broken,slot);
      System.out.println("equipment-break\t"+slot.getSerializedName()+"\t"+scenario+"\t"+String.join(";",player.events)+"\t"+saved(attributes)+"\t"+attributes.getAttributesToSync().size()+"\t"+attributes.getAttributesToUpdate().size());
    }
  }
}
