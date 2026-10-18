import java.util.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.item.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.phys.*;

/** Real Player.aiStep contact order; earlier movement/effect phases are controlled. */
public final class ContactVectors {
  static final class ContactPlayer extends Player {
    GameType mode=GameType.SURVIVAL;
    ContactPlayer(Level level) {super(level,new com.mojang.authlib.GameProfile(new UUID(1,2),"contact"));}
    @Override public GameType gameMode() {return mode;}
    @Override public void travel(Vec3 input) {} // Isolate the native touch section.
    @Override protected void applyEffectsFromBlocks() {}
    @Override protected void pushEntities() {}
  }
  static final class TouchedItem extends ItemEntity {
    final List<Integer> calls;
    TouchedItem(Level level,List<Integer> calls) {super(EntityTypes.ITEM,level);this.calls=calls;}
    @Override public void playerTouch(Player player) {calls.add(getId());}
  }
  static final class TouchedOrb extends ExperienceOrb {
    TouchedOrb(Level level) {super(EntityTypes.EXPERIENCE_ORB,level);}
    @Override public void playerTouch(Player player) {}
  }
  public static void run(Level level) throws Exception {
    var cls=level.getClass();var add=cls.getDeclaredMethod("addContact",Entity.class);add.setAccessible(true);
    var clear=cls.getDeclaredMethod("clearContacts");clear.setAccessible(true);
    var last=cls.getDeclaredField("lastContactBounds");last.setAccessible(true);
    for(int scenario=0;scenario<6;scenario++) {
      clear.invoke(level);
      var calls=new ArrayList<Integer>();var player=new ContactPlayer(level);player.setId(1);player.setPos(15.8,64,-0.2);
      add.invoke(level,player);
      var vehicle=new PrimedTnt(EntityTypes.TNT,level);vehicle.setId(9);vehicle.setPos(18.5,64,0.5);add.invoke(level,vehicle);
      if(scenario==1||scenario==2) {player.startRiding(vehicle,true,false);if(scenario==2)vehicle.discard();}
      double[][] positions={{16.6,64,-0.2},{15.1,64,-0.2},{16.6,64.3,0.5},{17.5,64,-0.2},{15.8,66.25,-0.2},{18.4,64,1.2}};
      int id=2;
      for(double[] pos:positions) {var item=new TouchedItem(level,calls);item.setId(id++);item.setPos(pos[0],pos[1],pos[2]);add.invoke(level,item);}
      for(int orb=0;orb<3;orb++) {var entity=new TouchedOrb(level);entity.setId(10+orb);entity.setPos(15.5+orb*0.25,64,0);add.invoke(level,entity);}
      if(scenario==3)player.setHealth(0);
      if(scenario==4)player.mode=GameType.SPECTATOR;
      if(scenario==5)player.setHealth(Float.NaN);
      player.getRandom().setSeed(99);player.aiStep();
      var bounds=(AABB)last.get(level);
      String box=bounds==null?"none":TrackingVectors.vec(new Vec3(bounds.minX,bounds.minY,bounds.minZ))+";"+TrackingVectors.vec(new Vec3(bounds.maxX,bounds.maxY,bounds.maxZ));
      System.out.println("player-contact\t"+scenario+"\t"+(calls.isEmpty()?"-":String.join(",",calls.stream().map(String::valueOf).toList()))+"\t"+box+"\t"+Long.toHexString(player.getRandom().nextLong()));
    }
  }
}
