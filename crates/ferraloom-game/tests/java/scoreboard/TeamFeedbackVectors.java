import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import com.google.gson.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.network.chat.Component;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.*;
import net.minecraft.server.commands.TeamCommand;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.scores.*;
import static org.mockito.Mockito.*;

/** Native team feedback must never ask count-only entity holders for their display names. */
public class TeamFeedbackVectors {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
    var source = mock(CommandSourceStack.class); var server = mock(MinecraftServer.class); var players = mock(PlayerList.class);
    when(source.getServer()).thenReturn(server); when(server.getPlayerList()).thenReturn(players);
    when(players.getPlayers()).thenReturn(List.of()); when(server.getAllLevels()).thenReturn(List.of());
    var board = new ServerScoreboard(server); when(server.getScoreboard()).thenReturn(board);
    var teams = Map.of("count",board.addPlayerTeam("count"),"other",board.addPlayerTeam("other"));
    var holders = new HashMap<String,ScoreHolder>();
    for (String name : List.of("entity-one","entity-two")) {
      var holder = mock(ScoreHolder.class); when(holder.getScoreboardName()).thenReturn(name);
      when(holder.getFeedbackDisplayName()).thenThrow(new AssertionError("count-only feedback read entity display: "+name));
      holders.put(name,holder);
    }
    holders.put("#fake",ScoreHolder.forNameOnly("#fake"));
    var feedback = new JsonArray(); var packets = new JsonArray();
    doAnswer(call->{var value=new JsonObject(); value.add("message",CommandVectors.json(((Supplier<Component>)call.getArgument(0)).get())); value.addProperty("broadcast",(boolean)call.getArgument(1)); feedback.add(value); return null;}).when(source).sendSuccess(any(),anyBoolean());
    doAnswer(call->{packets.add(CommandVectors.packet(call.getArgument(0)));return null;}).when(players).broadcastAll(any(Packet.class));
    var join = TeamCommand.class.getDeclaredMethod("joinTeam",CommandSourceStack.class,PlayerTeam.class,Collection.class); join.setAccessible(true);
    var leave = TeamCommand.class.getDeclaredMethod("leaveTeam",CommandSourceStack.class,Collection.class); leave.setAccessible(true);
    var vectors = new JsonArray();
    for (String input : List.of(
      "join count entity-one entity-two",
      "join count entity-one entity-two",
      "join other entity-one entity-one",
      "join count entity-one #fake",
      "leave entity-one entity-two",
      "leave entity-one #fake",
      "leave entity-one entity-two",
      "join other entity-one entity-two",
      "leave #fake entity-one entity-one entity-two")) {
      var words = input.split(" "); boolean joining = words[0].equals("join");
      var names = Arrays.asList(words).subList(joining?2:1,words.length);
      var members = names.stream().map(holders::get).toList();
      var vector = new JsonObject(); vector.addProperty("command",joining?"team join "+words[1]+" @a":"team leave @a");
      vector.add("members",new Gson().toJsonTree(names));
      vector.addProperty("value",(Integer)(joining?join.invoke(null,source,teams.get(words[1]),members):leave.invoke(null,source,members)));
      vector.add("feedback",feedback.deepCopy()); vector.add("packets",packets.deepCopy()); vectors.add(vector);
      feedback.asList().clear(); packets.asList().clear();
    }
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
  }
}
