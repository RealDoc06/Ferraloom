import java.util.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.Level;
import net.minecraft.server.level.ServerEntity;
import net.minecraft.resources.Identifier;

/** Real ServerEntity pairing and player metadata/attribute/head update gates. */
public final class PlayerTrackingVectors {
  public static void run(Level level) {
    var player=new LivingVectors(level);player.setId(9);player.setPos(8.5,64,8.5);player.setYRot(0);player.setXRot(0);player.setYHeadRot(10);
    player.getAttribute(Attributes.MAX_HEALTH).setBaseValue(30);
    player.setItemSlot(EquipmentSlot.MAINHAND,new ItemStack(Items.STONE,3));
    player.setItemSlot(EquipmentSlot.OFFHAND,new ItemStack(Items.SHIELD));
    player.setItemSlot(EquipmentSlot.CHEST,new ItemStack(Items.IRON_CHESTPLATE));
    var output=new ArrayList<String>();var tracker=new ServerEntity(null,player,UpdateInterval.periodic(2),false,TrackingVectors.synchronizer(output));
    tracker.sendPairingData(null,p->output.add(TrackingVectors.packet(p)));
    System.out.println("player-pairing\t"+String.join("|",output));
    for(int tick=0;tick<=6;tick++) {
      switch(tick) {
        case 1 -> player.setYHeadRot(23.9f);
        case 2 -> player.setHealth(7.5f);
        case 3 -> {player.setYHeadRot(90);var health=player.getAttribute(Attributes.MAX_HEALTH);health.setBaseValue(35);health.addTransientModifier(new AttributeModifier(Identifier.withDefaultNamespace("bonus"),3,AttributeModifier.Operation.ADD_VALUE));}
        case 5 -> {player.setFlyingFlag();player.setDeltaMovement(0.125,0,0);}
      }
      output.clear();tracker.sendChanges();System.out.println("player-tracking\t"+tick+"\t"+(output.isEmpty()?"-":String.join("|",output)));
    }
  }
}
