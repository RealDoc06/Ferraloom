import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.network.chat.*;
import net.minecraft.server.*;
import net.minecraft.server.level.*;
import net.minecraft.server.players.PlayerList;
import net.minecraft.server.players.NameAndId;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.gamerules.*;
import net.minecraft.world.phys.*;
import static org.mockito.Mockito.*;

/** Records real CommandSourceStack feedback routing and exact decorated components. */
public final class FeedbackVector {
    record Case(String kind,boolean accepts,boolean informs,boolean suppress,boolean always,boolean feedback,boolean log,boolean broadcast) {}
    static void record(JsonArray events,String channel,Component message) {var row=new JsonObject();row.addProperty("channel",channel);row.add("message",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,message).getOrThrow());events.add(row);}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var out=new JsonArray();
        for(var c:List.of(
            new Case("server",true,true,false,false,true,true,true),
            new Case("other",true,true,false,false,true,true,true),
            new Case("player",true,true,false,false,true,true,true),
            new Case("player",true,true,false,false,false,true,true),
            new Case("other",true,true,false,false,false,true,true),
            new Case("other",true,true,false,false,true,false,true),
            new Case("other",true,true,false,false,false,false,true),
            new Case("other",false,true,false,false,true,true,true),
            new Case("other",false,true,false,false,false,false,true),
            new Case("other",true,false,false,false,true,true,true),
            new Case("other",true,true,false,false,true,true,false),
            new Case("other",true,true,true,false,true,true,true),
            new Case("other",true,true,true,true,true,true,true))) {
            var events=new JsonArray();var server=mock(MinecraftServer.class);var level=mock(ServerLevel.class);var players=mock(PlayerList.class);var operator=mock(ServerPlayer.class);var ordinary=mock(ServerPlayer.class);
            var rules=new GameRules(FeatureFlags.DEFAULT_FLAGS);rules.set(GameRules.SEND_COMMAND_FEEDBACK,c.feedback,server);rules.set(GameRules.LOG_ADMIN_COMMANDS,c.log,server);
            doReturn(rules).when(level).getGameRules();doReturn(players).when(server).getPlayerList();doReturn(List.of(operator,ordinary)).when(players).getPlayers();
            var opIdentity=new NameAndId(UUID.fromString("00000000-0000-0000-0000-000000000001"),"Operator");when(operator.nameAndId()).thenReturn(opIdentity);when(players.isOp(opIdentity)).thenReturn(true);
            doAnswer(call->{record(events,"operator",call.getArgument(0));return null;}).when(operator).sendSystemMessage(any(Component.class));
            doAnswer(call->{record(events,"ordinary",call.getArgument(0));return null;}).when(ordinary).sendSystemMessage(any(Component.class));
            CommandSource sink=c.kind.equals("server")?server:mock(CommandSource.class);
            when(sink.acceptsSuccess()).thenReturn(c.accepts&&(!c.kind.equals("player")||c.feedback));when(sink.shouldInformAdmins()).thenReturn(c.informs);when(sink.alwaysAccepts()).thenReturn(c.always);
            doAnswer(call->{record(events,c.kind.equals("server")?"source":"server",call.getArgument(0));return null;}).when(server).sendSystemMessage(any(Component.class));
            if(sink!=server)doAnswer(call->{record(events,"source",call.getArgument(0));return null;}).when(sink).sendSystemMessage(any(Component.class));
            var source=new CommandSourceStack(sink,Vec3.ZERO,Vec2.ZERO,level,PermissionSet.NO_PERMISSIONS,Component.literal(c.kind.equals("server")?"Server":"Invoker"),server);
            if(c.suppress)source=source.withSuppressedOutput();int[] suppliers={0};source.sendSuccess(()->{suppliers[0]++;return Component.translatable("commands.save.success");},c.broadcast);
            var row=new Gson().toJsonTree(c).getAsJsonObject();row.add("events",events);row.addProperty("suppliers",suppliers[0]);out.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
    }
}
