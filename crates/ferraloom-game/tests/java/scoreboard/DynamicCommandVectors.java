import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import com.google.gson.*;
import com.mojang.brigadier.CommandDispatcher;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.network.chat.*;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.*;
import net.minecraft.server.commands.*;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.scores.*;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;
import static org.mockito.Mockito.*;

/** Real ComponentArgument getter order, including fixed-number-format callback precedence. */
public class DynamicCommandVectors {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
    com.mojang.brigadier.exceptions.CommandSyntaxException.BUILT_IN_EXCEPTIONS=new BrigadierExceptions();
    var source=mock(CommandSourceStack.class); var server=mock(MinecraftServer.class); var players=mock(PlayerList.class); var player=mock(ServerPlayer.class);
    when(source.getServer()).thenReturn(server); when(source.getPosition()).thenReturn(Vec3.ZERO); when(source.permissions()).thenReturn(PermissionSet.ALL_PERMISSIONS);
    when(source.getEntity()).thenReturn(player); when(player.getScoreboardName()).thenReturn("Doc"); when(player.getDisplayName()).thenReturn(Component.literal("Display Doc")); when(player.isAlive()).thenReturn(true);
    when(server.getPlayerList()).thenReturn(players); when(players.getPlayers()).thenReturn(List.of(player,player)); when(server.getAllLevels()).thenReturn(List.of());
    var board=new ServerScoreboard(server); when(server.getScoreboard()).thenReturn(board);
    var objective=board.addObjective("points",ObjectiveCriteria.DUMMY,Component.literal("points"),ObjectiveCriteria.RenderType.INTEGER,false,null);
    board.getOrCreatePlayerScore(ScoreHolder.forNameOnly("#one"),objective).set(37); board.addPlayerTeam("existing");
    var feedback=new JsonArray(); var packets=new JsonArray();
    doAnswer(call->{var value=new JsonObject(); value.add("message",CommandVectors.json(((Supplier<Component>)call.getArgument(0)).get())); value.addProperty("broadcast",(boolean)call.getArgument(1)); feedback.add(value); return null;}).when(source).sendSuccess(any(),anyBoolean());
    doAnswer(call->{packets.add(CommandVectors.packet(call.getArgument(0)));return null;}).when(players).broadcastAll(any(Packet.class));
    var context=CommandBuildContext.simple(net.minecraft.data.registries.VanillaRegistries.createWorldLookup(),FeatureFlags.DEFAULT_FLAGS);
    var dispatcher=new CommandDispatcher<CommandSourceStack>(); ScoreboardCommand.register(dispatcher,context); TeamCommand.register(dispatcher,context);
    var rows=new JsonArray();
    for(String input:List.of(
      "scoreboard objectives modify missing displayname {score:{name:'@a',objective:'points'}}",
      "team modify missing prefix {score:{name:'@a',objective:'points'}}",
      "scoreboard objectives modify missing numberformat fixed {score:{name:'@a',objective:'points'}}",
      "scoreboard players display numberformat #target missing fixed {score:{name:'@a',objective:'points'}}",
      "scoreboard players display name #target missing {score:{name:'@a',objective:'points'}}",
      "team add existing {score:{name:'@a',objective:'points'}}",
      "scoreboard objectives add points dummy {score:{name:'@a',objective:'points'}}"
    )) {
      var row=new JsonObject(); row.addProperty("command",input);
      try { row.addProperty("value",dispatcher.execute(input,source)); }
      catch(com.mojang.brigadier.exceptions.CommandSyntaxException error) {row.add("error",CommandVectors.json((Component)error.getRawMessage()));}
      row.add("feedback",feedback.deepCopy()); row.add("packets",packets.deepCopy()); rows.add(row); feedback.asList().clear(); packets.asList().clear();
    }
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(rows)+"\n");
  }
}
