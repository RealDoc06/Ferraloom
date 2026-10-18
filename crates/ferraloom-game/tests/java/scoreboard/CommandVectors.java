import java.nio.file.*;
import java.util.*;
import java.util.function.Supplier;
import com.google.gson.*;
import com.mojang.brigadier.*;
import com.mojang.serialization.JsonOps;
import net.minecraft.SharedConstants;
import net.minecraft.server.*;
import net.minecraft.server.commands.*;
import net.minecraft.server.level.*;
import net.minecraft.server.permissions.*;
import net.minecraft.server.players.PlayerList;
import net.minecraft.commands.*;
import net.minecraft.network.*;
import net.minecraft.network.chat.*;
import net.minecraft.network.protocol.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import io.netty.buffer.Unpooled;
import static org.mockito.Mockito.*;

/** Real registered commands, real ServerScoreboard broadcast hooks and real packet stream codecs. */
public class CommandVectors {
  static JsonElement json(Component value) { return ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,value).getOrThrow(); }
  static JsonObject packet(Packet<?> packet) {
    var buffer = new RegistryFriendlyByteBuf(Unpooled.buffer(),RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY));
    switch (packet) {
      case ClientboundSetObjectivePacket p -> ClientboundSetObjectivePacket.STREAM_CODEC.encode(buffer,p);
      case ClientboundSetPlayerTeamPacket p -> ClientboundSetPlayerTeamPacket.STREAM_CODEC.encode(buffer,p);
      case ClientboundSetScorePacket p -> ClientboundSetScorePacket.STREAM_CODEC.encode(buffer,p);
      case ClientboundResetScorePacket p -> ClientboundResetScorePacket.STREAM_CODEC.encode(buffer,p);
      case ClientboundSetDisplayObjectivePacket p -> ClientboundSetDisplayObjectivePacket.STREAM_CODEC.encode(buffer,p);
      default -> throw new AssertionError(packet);
    }
    byte[] bytes = new byte[buffer.readableBytes()]; buffer.readBytes(bytes); buffer.release();
    var value = new JsonObject(); value.addProperty("type",packet.getClass().getSimpleName()); value.addProperty("hex",HexFormat.of().formatHex(bytes)); return value;
  }
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
    com.mojang.brigadier.exceptions.CommandSyntaxException.BUILT_IN_EXCEPTIONS = new BrigadierExceptions();
    var context = CommandBuildContext.simple(net.minecraft.data.registries.VanillaRegistries.createWorldLookup(),FeatureFlags.DEFAULT_FLAGS);
    var source = mock(CommandSourceStack.class); var server = mock(MinecraftServer.class); var players = mock(PlayerList.class); var player = mock(ServerPlayer.class);
    when(server.getPlayerList()).thenReturn(players); when(players.getPlayers()).thenReturn(List.of()); when(server.getAllLevels()).thenReturn(List.of());
    var board = new ServerScoreboard(server); when(server.getScoreboard()).thenReturn(board);
    when(source.getServer()).thenReturn(server); when(source.permissions()).thenReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(4)));
    when(source.getPlayerOrException()).thenReturn(player); when(source.getEntityOrException()).thenReturn(player); when(source.getEntity()).thenReturn(player);
    when(player.getScoreboardName()).thenReturn("Doc"); when(player.getDisplayName()).thenReturn(Component.literal("Doc"));
    when(player.getFeedbackDisplayName()).thenReturn(Component.literal("Doc").withStyle(style->style.withHoverEvent(new HoverEvent.ShowText(Component.literal("Doc")))));
    when(players.getPlayerByName("Doc")).thenReturn(player);
    var feedback = new JsonArray(); var packets = new JsonArray();
    doAnswer(call->{ var value = new JsonObject(); value.add("message",json(((Supplier<Component>)call.getArgument(0)).get())); value.addProperty("broadcast",(boolean)call.getArgument(1)); feedback.add(value); return null; }).when(source).sendSuccess(any(),anyBoolean());
    doAnswer(call->{packets.add(packet(call.getArgument(0)));return null;}).when(players).broadcastAll(any(Packet.class));
    var dispatcher = new CommandDispatcher<CommandSourceStack>(); TeamCommand.register(dispatcher,context); ScoreboardCommand.register(dispatcher,context); TriggerCommand.register(dispatcher);
    var commands = List.of(
      "team list", "team add red", "team add red", "team add blue \"Blue\"", "team join red #one", "team join red #one", "team join blue #one", "team join red #two", "team list red",
      "team leave #absent", "team leave #two", "team join red #two", "team modify red displayName \"Red Team\"", "team modify red displayName \"Red Team\"", "team modify red prefix \"[\"", "team modify red suffix \"]\"",
      "team modify red color red", "team modify red color red", "team modify red color reset", "team modify red color reset", "team modify red friendlyFire false", "team modify red friendlyFire false", "team modify red seeFriendlyInvisibles false",
      "team modify red nametagVisibility never", "team modify red nametagVisibility hideForOtherTeams", "team modify red nametagVisibility hideForOwnTeam", "team modify red nametagVisibility always", "team modify red nametagVisibility always",
      "team modify red deathMessageVisibility never", "team modify red collisionRule never", "team modify red collisionRule pushOwnTeam", "team modify red collisionRule pushOtherTeams", "team modify red collisionRule always", "team modify red collisionRule always",
      "team empty red", "team empty red", "team remove red", "team list", "team remove blue", "team list", "team list missing",
      "scoreboard objectives list", "scoreboard objectives add points dummy", "scoreboard objectives add points dummy", "scoreboard objectives add other dummy \"Other\"", "scoreboard objectives add health health", "scoreboard objectives add stat mined:stone", "scoreboard players set #one stat 99", "scoreboard objectives list",
      "scoreboard players list", "scoreboard players get #one points", "scoreboard players set #one points 0", "scoreboard players set #one points 5", "scoreboard players add #one points 2", "scoreboard players remove #one points 10", "scoreboard players list #one",
      "scoreboard objectives setdisplay sidebar points", "scoreboard objectives setdisplay list points", "scoreboard players set #one points -3", "scoreboard players set #two points 12", "scoreboard players add #two points 0",
      "scoreboard objectives modify points displayname \"New\"", "scoreboard objectives modify points displayname \"New\"", "scoreboard objectives modify points rendertype hearts", "scoreboard objectives modify points rendertype hearts", "scoreboard objectives modify points displayautoupdate true", "scoreboard objectives modify points displayautoupdate true",
      "scoreboard objectives modify points numberformat blank", "scoreboard objectives modify points numberformat fixed \"fixed\"", "scoreboard objectives modify points numberformat styled {bold:true}", "scoreboard objectives modify points numberformat",
      "scoreboard players display name #one points \"One\"", "scoreboard players display name #one points \"One\"", "scoreboard players display name #one points", "scoreboard players display numberformat #one points blank", "scoreboard players display numberformat #one points styled {italic:true}", "scoreboard players display numberformat #one points",
      "scoreboard players set #one health 3", "scoreboard players enable #one points", "scoreboard objectives add toggle trigger", "scoreboard players enable Doc toggle", "scoreboard players enable Doc toggle", "trigger toggle", "trigger toggle", "scoreboard players enable Doc toggle", "trigger toggle add -5", "scoreboard players enable Doc toggle", "trigger toggle set 7", "trigger points",
      "scoreboard players set #one points -5", "scoreboard players set #two other 2", "scoreboard players operation #one points /= #two other", "scoreboard players operation #one points %= #two other", "scoreboard players operation #one points += #two other", "scoreboard players operation #one points -= #two other", "scoreboard players operation #one points *= #two other", "scoreboard players operation #one points < #two other", "scoreboard players operation #one points > #two other", "scoreboard players operation #one points >< #two other", "scoreboard players operation #one points = #two other",
      "scoreboard players set #two other 0", "scoreboard players operation #one points /= #two other", "scoreboard players set #one points 2147483647", "scoreboard players add #one points 1", "scoreboard players remove #one points 1",
      "scoreboard players reset #one points", "scoreboard players reset #absent", "scoreboard players list", "scoreboard objectives setdisplay sidebar other", "scoreboard objectives setdisplay list", "scoreboard objectives setdisplay list", "scoreboard objectives remove points", "scoreboard objectives setdisplay sidebar", "scoreboard objectives remove other", "scoreboard objectives remove health", "scoreboard objectives remove toggle", "scoreboard objectives remove stat", "scoreboard objectives list"
    );
    var vectors = new JsonArray();
    for (String command : commands) {
      var vector = new JsonObject(); vector.addProperty("command",command);
      try { vector.addProperty("value",dispatcher.execute(command,source)); }
      catch (com.mojang.brigadier.exceptions.CommandSyntaxException error) { vector.add("error",json((Component)error.getRawMessage())); }
      vector.add("feedback",feedback.deepCopy()); vector.add("packets",packets.deepCopy()); vectors.add(vector); feedback.asList().clear(); packets.asList().clear();
    }
    // Exercise the real PlayerList join synchronizer, including an objective displayed in two slots.
    when(players.getPlayerByName("Doc")).thenReturn(null);
    var joinBoard = new ServerScoreboard(server);
    var team = joinBoard.addPlayerTeam("red"); joinBoard.addPlayerToTeam("Doc",team);
    var shown = joinBoard.addObjective("points",net.minecraft.world.scores.criteria.ObjectiveCriteria.DUMMY,Component.literal("Points"),net.minecraft.world.scores.criteria.ObjectiveCriteria.RenderType.INTEGER,false,null);
    joinBoard.addObjective("hidden",net.minecraft.world.scores.criteria.ObjectiveCriteria.DUMMY,Component.literal("Hidden"),net.minecraft.world.scores.criteria.ObjectiveCriteria.RenderType.INTEGER,false,null);
    joinBoard.getOrCreatePlayerScore(net.minecraft.world.scores.ScoreHolder.forNameOnly("Doc"),shown).set(3);
    joinBoard.setDisplayObjective(net.minecraft.world.scores.DisplaySlot.LIST,shown); joinBoard.setDisplayObjective(net.minecraft.world.scores.DisplaySlot.SIDEBAR,shown);
    var listener = mock(net.minecraft.server.network.ServerGamePacketListenerImpl.class); player.connection = listener;
    var joinPackets = new JsonArray(); doAnswer(call->{joinPackets.add(packet(call.getArgument(0)));return null;}).when(listener).send(any(Packet.class));
    var sync = mock(PlayerList.class,CALLS_REAL_METHODS);
    var update = PlayerList.class.getDeclaredMethod("updateEntireScoreboard",ServerScoreboard.class,ServerPlayer.class); update.setAccessible(true); update.invoke(sync,joinBoard,player);
    Files.writeString(Path.of(args[1]),new GsonBuilder().setPrettyPrinting().create().toJson(joinPackets)+"\n");
    var formats = new JsonArray();
    var score = joinBoard.getOrCreatePlayerScore(net.minecraft.world.scores.ScoreHolder.forNameOnly("Doc"),shown);
    var info = joinBoard.getPlayerScoreInfo(net.minecraft.world.scores.ScoreHolder.forNameOnly("Doc"),shown);
    formats.add(json(info.formatValue(net.minecraft.network.chat.numbers.StyledFormat.NO_STYLE)));
    shown.setNumberFormat(net.minecraft.network.chat.numbers.BlankFormat.INSTANCE); formats.add(json(info.formatValue(shown.numberFormatOrDefault(net.minecraft.network.chat.numbers.StyledFormat.NO_STYLE))));
    shown.setNumberFormat(new net.minecraft.network.chat.numbers.FixedFormat(Component.literal("fixed"))); formats.add(json(info.formatValue(shown.numberFormatOrDefault(net.minecraft.network.chat.numbers.StyledFormat.NO_STYLE))));
    score.numberFormatOverride(new net.minecraft.network.chat.numbers.StyledFormat(Style.EMPTY.withBold(true))); formats.add(json(info.formatValue(shown.numberFormatOrDefault(net.minecraft.network.chat.numbers.StyledFormat.NO_STYLE))));
    Files.writeString(Path.of(args[2]),new GsonBuilder().setPrettyPrinting().create().toJson(formats)+"\n");
    // Invoke the real ServerPlayer producer methods with only its level/stat ownership supplied.
    var stat = net.minecraft.stats.Stats.BLOCK_MINED.get(net.minecraft.world.level.block.Blocks.STONE);
    var producerBoard = new ServerScoreboard(server);
    var statObjective = producerBoard.addObjective("mined",stat,Component.literal("Mined"),net.minecraft.world.scores.criteria.ObjectiveCriteria.RenderType.INTEGER,false,null);
    var realPlayer = mock(ServerPlayer.class,CALLS_REAL_METHODS); var level = mock(ServerLevel.class);
    doReturn(level).when(realPlayer).level(); doReturn(producerBoard).when(level).getScoreboard(); doReturn("Doc").when(realPlayer).getScoreboardName(); doReturn(Component.literal("Doc")).when(realPlayer).getDisplayName();
    var stats = new net.minecraft.stats.ServerStatsCounter(server,Path.of(args[3]+".unused"));
    var statsField = ServerPlayer.class.getDeclaredField("stats"); statsField.setAccessible(true); statsField.set(realPlayer,stats);
    stats.setValue(realPlayer,stat,Integer.MAX_VALUE-1); producerBoard.getOrCreatePlayerScore(realPlayer,statObjective).set(100);
    var producerValues = new JsonArray(); realPlayer.awardStat(stat,5);
    producerValues.add(new Gson().toJsonTree(List.of(stats.getValue(stat),producerBoard.getPlayerScoreInfo(realPlayer,statObjective).value())));
    realPlayer.resetStat(stat); producerValues.add(new Gson().toJsonTree(List.of(stats.getValue(stat),producerBoard.getPlayerScoreInfo(realPlayer,statObjective).value())));
    Files.writeString(Path.of(args[3]),new GsonBuilder().setPrettyPrinting().create().toJson(producerValues)+"\n");
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
  }
}
