import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import com.google.gson.*;
import com.mojang.authlib.GameProfile;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.network.chat.*;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.*;
import net.minecraft.server.commands.TeamCommand;
import net.minecraft.server.level.*;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.scores.*;
import static org.mockito.Mockito.*;

/** Real entity display/feedback methods are evaluated by TeamCommand after every mutation. */
public class TeamSingleFeedbackVectors {
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
    var join = TeamCommand.class.getDeclaredMethod("joinTeam",CommandSourceStack.class,PlayerTeam.class,Collection.class); join.setAccessible(true);
    var leave = TeamCommand.class.getDeclaredMethod("leaveTeam",CommandSourceStack.class,Collection.class); leave.setAccessible(true);
    var vectors = new JsonArray();
    for (String kind : List.of("player","cow")) {
      var source=mock(CommandSourceStack.class); var server=mock(MinecraftServer.class); var players=mock(PlayerList.class); var level=mock(ServerLevel.class);
      when(source.getServer()).thenReturn(server); when(server.getPlayerList()).thenReturn(players);
      when(players.getPlayers()).thenReturn(List.of()); when(server.getAllLevels()).thenReturn(List.of());
      var board=new ServerScoreboard(server); when(server.getScoreboard()).thenReturn(board); when(level.getScoreboard()).thenReturn(board);
      for (var name : List.of("red","blue")) {
        var team=board.addPlayerTeam(name); team.setColor(Optional.of(name.equals("red")?TeamColor.RED:TeamColor.BLUE));
        team.setPlayerPrefix(Component.literal("<"+name+">")); team.setPlayerSuffix(Component.literal("</"+name+">"));
      }
      Entity entity;
      var uuid=new UUID(0,1);
      if(kind.equals("player")) {
        var player=mock(ServerPlayer.class,CALLS_REAL_METHODS); doReturn(new GameProfile(uuid,"Doc")).when(player).getGameProfile();
        entity=player; doReturn(EntityTypes.PLAYER).when(entity).getType(); doReturn(Component.literal("Doc")).when(entity).getName();
      } else {
        entity=mock(Cow.class,CALLS_REAL_METHODS); doReturn(EntityTypes.COW).when(entity).getType();
        doReturn(Component.literal("Named Cow").withStyle(s->s.withBold(true))).when(entity).getName();
      }
      entity.setUUID(uuid);
      doReturn(level).when(entity).level(); doReturn(uuid).when(entity).getUUID(); doReturn(uuid.toString()).when(entity).getStringUUID();
      var feedback=new JsonArray(); var packets=new JsonArray();
      doAnswer(call->{var value=new JsonObject(); value.add("message",CommandVectors.json(((Supplier<Component>)call.getArgument(0)).get())); value.addProperty("broadcast",(boolean)call.getArgument(1)); feedback.add(value); return null;}).when(source).sendSuccess(any(),anyBoolean());
      doAnswer(call->{packets.add(CommandVectors.packet(call.getArgument(0)));return null;}).when(players).broadcastAll(any(Packet.class));
      for(String input : List.of("join red entity","join red entity","join blue entity","leave #absent entity","leave entity","join red entity entity","leave entity entity","leave entity")) {
        var words=input.split(" "); boolean joining=words[0].equals("join");
        var members=Arrays.stream(words).skip(joining?2:1).map(word->word.equals("entity")?(ScoreHolder)entity:ScoreHolder.forNameOnly(word)).toList();
        var vector=new JsonObject(); vector.addProperty("kind",kind); vector.addProperty("command",joining?"team join "+words[1]+" @a":"team leave @a");
        vector.add("members",new Gson().toJsonTree(members.stream().map(ScoreHolder::getScoreboardName).toList()));
        var before=CommandVectors.json(entity.getFeedbackDisplayName());
        int value=(Integer)(joining?join.invoke(null,source,board.getPlayerTeam(words[1]),members):leave.invoke(null,source,members));
        vector.addProperty("value",value); vector.add("feedback",feedback.deepCopy()); vector.add("packets",packets.deepCopy());
        if(value==1) {
          var message=feedback.get(0).getAsJsonObject().getAsJsonObject("message");
          var actual=message.getAsJsonArray("with").get(0); var after=CommandVectors.json(entity.getFeedbackDisplayName());
          if(!actual.equals(after))throw new AssertionError("feedback is not post-mutation: "+input);
          if(joining) {
            if(!actual.getAsJsonObject().get("color").getAsString().equals(words[1]))throw new AssertionError("wrong new team color");
            if(!actual.toString().contains("<"+words[1]+">")||!actual.toString().contains("</"+words[1]+">"))throw new AssertionError("missing new team prefix/suffix");
          }
          vector.addProperty("display_changed",!before.equals(after));
        }
        vectors.add(vector); feedback.asList().clear(); packets.asList().clear();
      }
    }
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
  }
}
