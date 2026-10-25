import com.google.gson.*;
import com.mojang.brigadier.CommandDispatcher;
import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.*;
import net.minecraft.server.commands.ScoreboardCommand;
import net.minecraft.server.players.PlayerList;
import net.minecraft.server.permissions.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.scores.*;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;
import static org.mockito.Mockito.*;

/** Displayed zero scores still emit packets; only set filters zero-valued feedback elements. */
public class ZeroScoreVectors {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
    var source=mock(CommandSourceStack.class);var server=mock(MinecraftServer.class);var players=mock(PlayerList.class);
    when(source.getServer()).thenReturn(server);when(server.getPlayerList()).thenReturn(players);
    when(source.permissions()).thenReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(4)));
    var board=new ServerScoreboard(server);when(server.getScoreboard()).thenReturn(board);
    var objective=board.addObjective("points",ObjectiveCriteria.DUMMY,Component.literal("Points"),ObjectiveCriteria.RenderType.INTEGER,false,null);
    board.getOrCreatePlayerScore(ScoreHolder.forNameOnly("#one"),objective).set(4);
    board.getOrCreatePlayerScore(ScoreHolder.forNameOnly("#two"),objective).set(0);
    board.setDisplayObjective(DisplaySlot.SIDEBAR,objective);
    var feedback=new JsonArray();var packets=new JsonArray();var cases=new JsonArray();
    doAnswer(call->{var message=new JsonObject();message.add("message",CommandVectors.json(((Supplier<Component>)call.getArgument(0)).get()));message.addProperty("broadcast",(boolean)call.getArgument(1));feedback.add(message);return null;}).when(source).sendSuccess(any(),anyBoolean());
    doAnswer(call->{packets.add(CommandVectors.packet(call.getArgument(0)));return null;}).when(players).broadcastAll(any(Packet.class));
    var dispatcher=new CommandDispatcher<CommandSourceStack>();
    ScoreboardCommand.register(dispatcher,CommandBuildContext.simple(net.minecraft.data.registries.VanillaRegistries.createWorldLookup(),FeatureFlags.DEFAULT_FLAGS));
    for(String command:List.of("scoreboard players set #one points 0","scoreboard players set #one points 0",
        "scoreboard players add #one points 0","scoreboard players remove #one points 0",
        "scoreboard players set #one points -2","scoreboard players set * points 0",
        "scoreboard players add * points 0","scoreboard players remove * points 0")) {
      var row=new JsonObject();row.addProperty("command",command);row.addProperty("value",dispatcher.execute(command,source));
      row.add("feedback",feedback.deepCopy());row.add("packets",packets.deepCopy());cases.add(row);
      feedback.asList().clear();packets.asList().clear();
    }
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(cases)+"\n");
  }
}
