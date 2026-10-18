import java.lang.reflect.*;
import java.util.*;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.*;
import net.minecraft.network.chat.Component;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.*;
import net.minecraft.world.scores.*;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;
import static org.mockito.Mockito.*;

/** Executes real ServerPlayer.doTick and ServerScoreboard broadcasts, capturing
 * two audiences: the player receiving health/XP and a second global recipient. */
public final class PlayerSyncVectors {
  static String packet(Packet<?> packet) {
    return switch(packet) {
      case ClientboundSetHealthPacket p -> "health:"+TravelVectors.f(p.getHealth())+":"+p.getFood()+":"+TravelVectors.f(p.getSaturation());
      case ClientboundSetScorePacket p -> "score:"+p.objectiveName()+":"+p.score();
      case ClientboundSetExperiencePacket p -> "xp:"+p.getExperienceLevel()+":"+p.getTotalExperience()+":"+TravelVectors.f(p.getExperienceProgress());
      default -> throw new AssertionError(packet);
    };
  }
  public static void run(Level sourceLevel)throws Exception {
    var a=new ArrayList<String>();var b=new ArrayList<String>();
    var server=mock(MinecraftServer.class);var players=mock(PlayerList.class);when(server.getPlayerList()).thenReturn(players);
    var board=new ServerScoreboard(server);when(server.getAllLevels()).thenReturn(List.of());
    doAnswer(call->{String event=packet(call.getArgument(0));a.add(event);b.add(event);return null;}).when(players).broadcastAll(any(Packet.class));
    var criteria=List.of(ObjectiveCriteria.HEALTH,ObjectiveCriteria.FOOD,ObjectiveCriteria.AIR,ObjectiveCriteria.ARMOR,ObjectiveCriteria.EXPERIENCE,ObjectiveCriteria.LEVEL);
    // Suppress setup packet capture, then track all six native objectives.
    var nativePackets=new ArrayList<Packet<?>>();doAnswer(call->{nativePackets.add(call.getArgument(0));return null;}).when(players).broadcastAll(any(Packet.class));
    for(int i=0;i<criteria.size();i++){var criterion=criteria.get(i);var obj=board.addObjective(criterion.getName(),criterion,Component.literal(criterion.getName()),ObjectiveCriteria.RenderType.INTEGER,false,null);board.setDisplayObjective(DisplaySlot.values()[i],obj);}
    doAnswer(call->{String event=packet(call.getArgument(0));a.add(event);b.add(event);return null;}).when(players).broadcastAll(any(Packet.class));
    var level=mock(ServerLevel.class);when(level.getScoreboard()).thenReturn(board);
    var player=mock(ServerPlayer.class,CALLS_REAL_METHODS);var source=new TravelVectors(sourceLevel);
    for(Class<?> owner=Player.class;owner!=Object.class;owner=owner.getSuperclass())for(var field:owner.getDeclaredFields())if(!Modifier.isStatic(field.getModifiers())){field.setAccessible(true);field.set(player,field.get(source));}
    doReturn(level).when(player).level();doReturn(GameType.SPECTATOR).when(player).gameMode();doReturn(true).when(player).touchingUnloadedChunk();
    var listener=mock(ServerGamePacketListenerImpl.class);player.connection=listener;doAnswer(call->{a.add(packet(call.getArgument(0)));return null;}).when(listener).send(any(Packet.class));
    player.tickCount=1;player.setHealth(17.25f);player.getAttribute(Attributes.MAX_ABSORPTION).setBaseValue(10);player.setAbsorptionAmount(2.125f);player.getAttribute(Attributes.ARMOR).setBaseValue(2.5);player.totalExperience=123;player.experienceLevel=7;player.experienceProgress=0.25f;
    for(int stage=0;stage<5;stage++) {
      if(stage==2)player.getFoodData().setSaturation(2.5f);
      if(stage==3){player.totalExperience++;player.experienceProgress=0.5f;}
      if(stage==4){player.setHealth(12);player.setAbsorptionAmount(0);player.getFoodData().setFoodLevel(7);player.getFoodData().setSaturation(0);player.setAirSupply(8);player.getAttribute(Attributes.ARMOR).setBaseValue(5.99);player.totalExperience=24;player.experienceLevel=8;}
      player.doTick();System.out.println("player-sync-order\t"+stage+"\t"+(a.isEmpty()?"-":String.join(";",a))+"\t"+(b.isEmpty()?"-":String.join(";",b)));a.clear();b.clear();
    }
  }
}
