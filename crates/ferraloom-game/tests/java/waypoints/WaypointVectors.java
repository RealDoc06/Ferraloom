import java.nio.file.*;
import java.util.*;
import io.netty.buffer.Unpooled;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.*;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundTrackedWaypointPacket;
import net.minecraft.server.level.*;
import net.minecraft.server.*;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.entity.player.Player;
import com.mojang.authlib.GameProfile;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.server.waypoints.ServerWaypointManager;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.Attributes;
import net.minecraft.world.level.*;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.dimension.BuiltinDimensionTypes;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.scores.*;
import net.minecraft.world.waypoints.Waypoint;
import static org.mockito.Mockito.*;

/** Actual manager and entity connections. Only world services and packet delivery are controlled. */
public final class WaypointVectors {
    static final class Fixture {
        final ServerWaypointManager manager=new ServerWaypointManager();
        final ServerLevel level=mock(ServerLevel.class);
        final GameRules rules=mock(GameRules.class);
        final MinecraftServer server=mock(MinecraftServer.class);
        final ServerScoreboard board=new ServerScoreboard(server);
        final AttributeUpdateVectors.Receiver[] players=new AttributeUpdateVectors.Receiver[3];
        final ArrayList<String> packets=new ArrayList<>();
        boolean enabled=true;
        Fixture(boolean reciprocal,double sourceX) throws Exception {
            doReturn(AdvancementVectors.lookup).when(level).registryAccess();
            doReturn(Level.OVERWORLD).when(level).dimension();
            doReturn(new WorldBorder()).when(level).getWorldBorder();
            doReturn(AdvancementVectors.lookup.lookupOrThrow(Registries.DIMENSION_TYPE)
                .getOrThrow(BuiltinDimensionTypes.OVERWORLD).value()).when(level).dimensionType();
            doReturn(mock(PlayerList.class)).when(server).getPlayerList();
            doReturn(List.of(level)).when(server).getAllLevels();
            doReturn(board).when(level).getScoreboard();
            doReturn(manager).when(level).getWaypointManager();
            doReturn(rules).when(level).getGameRules();
            doAnswer(call->enabled).when(rules).get(GameRules.LOCATOR_BAR);
            for(int index=0;index<players.length;index++) {
                int id=index+1;
                var player=AttributeUpdateVectors.player(level,new ArrayList<>());players[index]=player;
                player.setId(id);player.setUUID(new UUID(1,id));
                AttributeUpdateVectors.field(Player.class,"gameProfile").set(player,new GameProfile(new UUID(1,id),"observer"+id));
                player.setPos(index==1?sourceX:8.5,64,8.5+index);
                player.getAttribute(Attributes.WAYPOINT_RECEIVE_RANGE).setBaseValue(reciprocal||index==0?1000000:0);
                player.getAttribute(Attributes.WAYPOINT_TRANSMIT_RANGE).setBaseValue(reciprocal||index==1?1000000:0);
                player.setChunkTrackingView(ChunkTrackingView.of(new ChunkPos(0,0),2));
                var connection=mock(ServerGamePacketListenerImpl.class);
                player.connection=connection;
                doAnswer(call->{
                    var packet=call.getArgument(0);
                    if(!(packet instanceof ClientboundTrackedWaypointPacket waypoint))throw new AssertionError(packet);
                    var buffer=new RegistryFriendlyByteBuf(Unpooled.buffer(),AdvancementVectors.lookup);
                    ClientboundTrackedWaypointPacket.STREAM_CODEC.encode(buffer,waypoint);
                    var bytes=new byte[buffer.readableBytes()];buffer.readBytes(bytes);buffer.release();
                    packets.add(id+":"+HexFormat.of().formatHex(bytes));return null;
                }).when(connection).send(any());
                AttributeUpdateVectors.field(Entity.class,"firstTick").setBoolean(player,false);
            }
        }
        void apply(String input) throws Exception {
            var a=input.split(",");
            var player=a.length>1&&a[1].matches("[123]")?players[Integer.parseInt(a[1])-1]:null;
            switch(a[0]) {
                case "add" -> manager.addPlayer(player);
                case "track" -> manager.trackWaypoint(player);
                case "update" -> manager.updateWaypoint(player);
                case "receiver" -> manager.updatePlayer(player);
                case "untrack" -> manager.untrackWaypoint(player);
                case "remove" -> manager.removePlayer(player);
                case "remake" -> manager.remakeConnections(player);
                case "move" -> player.setPos(Double.parseDouble(a[2]),Double.parseDouble(a[3]),Double.parseDouble(a[4]));
                case "view" -> {player.setChunkTrackingView(ChunkTrackingView.of(new ChunkPos(Integer.parseInt(a[2]),Integer.parseInt(a[3])),Integer.parseInt(a[4])));manager.updatePlayer(player);}
                case "transmit" -> player.getAttribute(Attributes.WAYPOINT_TRANSMIT_RANGE).setBaseValue(Double.parseDouble(a[2]));
                case "receive" -> player.getAttribute(Attributes.WAYPOINT_RECEIVE_RANGE).setBaseValue(Double.parseDouble(a[2]));
                case "spectator" -> {player.mode=Boolean.parseBoolean(a[2])?GameType.SPECTATOR:GameType.SURVIVAL;manager.remakeConnections(player);manager.updatePlayer(player);}
                case "first" -> AttributeUpdateVectors.field(Entity.class,"firstTick").setBoolean(player,Boolean.parseBoolean(a[2]));
                case "rule" -> {enabled=Boolean.parseBoolean(a[1]);if(enabled)for(var member:players)manager.updatePlayer(member);else manager.breakAllConnections();}
                case "team" -> {
                    var team=board.getPlayerTeam("waypoints");if(team==null)team=board.addPlayerTeam("waypoints");
                    team.setColor(Optional.of(TeamColor.valueOf(a[2])));board.addPlayerToTeam(player.getScoreboardName(),team);manager.remakeConnections(player);
                }
                case "icon" -> {player.waypointIcon().color=Optional.of(Integer.parseUnsignedInt(a[2],16));manager.remakeConnections(player);}
                case "vehicle" -> {AttributeUpdateVectors.field(Entity.class,"vehicle").set(players[0],players[1]);manager.updateWaypoint(players[1]);}
                case "passenger" -> {AttributeUpdateVectors.field(Entity.class,"passengers").set(players[1],com.google.common.collect.ImmutableList.of(players[0]));manager.updateWaypoint(players[1]);}
                default -> throw new AssertionError(input);
            }
        }
    }
    static void sequence(ArrayList<String> rows,String name,boolean reciprocal,double sourceX,String... inputs) throws Exception {
        var fixture=new Fixture(reciprocal,sourceX);
        for(var input:inputs) {
            fixture.packets.clear();fixture.apply(input);
            rows.add("manager\t"+name+"\t"+reciprocal+"\t"+sourceX+"\t"+input+"\t"+
                (fixture.packets.isEmpty()?"-":String.join("|",fixture.packets)));
        }
    }
    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();var rows=new ArrayList<String>();
        sequence(rows,"block",false,12.5,"add,1","track,2","track,2","move,2,13.5,64,9.5","move,2,15.5,64,9.5","move,1,9.5,64,8.5","untrack,2","untrack,2");
        sequence(rows,"chunk",false,100.5,"add,1","track,2","move,2,116.5,64,9.5","move,2,148.5,64,9.5","view,1,9,0,2","view,1,0,0,2","move,2,149.5,64,9.5");
        sequence(rows,"azimuth",false,350.5,"add,1","track,2","move,1,8.5,64,8.6","move,1,8.5,64,20.5","move,2,340.5,64,8.5","move,2,340.5001,64,8.5","view,1,21,0,2");
        sequence(rows,"range",false,136.5,"receive,1,128","transmit,2,128","add,1","track,2","move,2,136.499999,64,8.5","move,2,136.49,64,8.5","move,2,136.5,64,8.5","spectator,1,true","spectator,2,true","spectator,1,false","spectator,2,false","transmit,2,NaN","remake,2");
        sequence(rows,"first-tick",false,12.5,"first,2,true","add,1","track,2","first,2,false","update,2","passenger,1","vehicle,1");
        sequence(rows,"rules-table",true,12.5,"add,1","add,2","add,3","rule,false","rule,true","team,2,BLACK","team,2,RED","icon,2,12345678","team,2,GREEN","remove,2","remove,1","remove,3");
        for(String input:List.of("{}","{style:'minecraft:default'}","{style:'test:arrow',color:0}","{style:'test:arrow',color:-1}","{style:'test:arrow',color:4294967297L}","{style:'test:arrow',color:1.5f}","{style:'test:arrow',color:[0.5f,1.0f,0.0f]}","{style:'test:arrow',color:[-1.0f,2.0f,0.1f]}","{style:'test:arrow',color:[I;1,0,1]}","{style:'test:arrow',color:[L;1L,0L,1L]}","{style:'test:arrow',color:[B;1b,0b,1b]}","{style:'test:arrow',color:[1.0f,0.0f]}","{style:'BAD'}","{style:1}","{style:'test:arrow',color:'bad'}")) {
            var result=Waypoint.Icon.CODEC.parse(NbtOps.INSTANCE,TagParser.parseCompoundFully(input));
            var encoded=result.result().map(icon->Waypoint.Icon.CODEC.encodeStart(NbtOps.INSTANCE,icon).getOrThrow().toString()).orElse("-");
            rows.add("icon\t"+input+"\t"+encoded);
        }
        Files.write(Path.of(args[0]),rows);System.out.println(rows.size()+" native waypoint observations");
    }
}
