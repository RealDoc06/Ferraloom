import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.arguments.selector.EntitySelectorParser;
import net.minecraft.network.chat.Component;
import net.minecraft.server.*;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.permissions.*;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Observes predicate evaluation at the real EntitySelector#findPlayers capture point. */
public final class SelectorCaptureVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var gson=new GsonBuilder().setPrettyPrinting().create();var vectors=new JsonArray();
        for(int size:new int[]{0,1,3}) for(String sort:List.of("arbitrary","nearest","furthest","random","distant")) for(boolean current:size==1&&(sort.equals("arbitrary")||sort.equals("distant"))?new boolean[]{false,true}:new boolean[]{false}) {
            String text=current?"@s[distance=.."+(sort.equals("distant")?"0.5":"100")+",name=!Skip]":sort.equals("distant")?"@a[limit=1,distance=..0.5,name=!Skip]":"@a[limit=1,sort="+sort+",name=!Skip]";
            var source=mock(CommandSourceStack.class);var server=mock(MinecraftServer.class);var list=mock(PlayerList.class);
            doReturn(server).when(source).getServer();doReturn(list).when(server).getPlayerList();doReturn(Vec3.ZERO).when(source).getPosition();
            doReturn(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(4))).when(source).permissions();
            var names=size==3?List.of("Skip","Alpha","Beta"):size==1?List.of("Alpha"):List.<String>of();
            var examined=new ArrayList<String>();var players=new ArrayList<ServerPlayer>();var identities=new IdentityHashMap<ServerPlayer,String>();
            for(int index=0;index<names.size();index++) {
                String name=names.get(index);var player=mock(ServerPlayer.class);players.add(player);identities.put(player,name);
                doAnswer(call->{examined.add(name);return name;}).when(player).getPlainTextName();
                double x=names.size()-index;doReturn(x*x).when(player).distanceToSqr(any(Vec3.class));
                doReturn(new net.minecraft.world.phys.AABB(x-0.3,0,-0.3,x+0.3,1.8,0.3)).when(player).getBoundingBox();
            }
            var level=mock(net.minecraft.server.level.ServerLevel.class);doReturn(level).when(source).getLevel();
            doAnswer(call->{
                java.util.function.Predicate<ServerPlayer> predicate=call.getArgument(0);int limit=call.getArgument(1);var result=new ArrayList<ServerPlayer>();
                for(var player:players) if(predicate.test(player)) { result.add(player);if(result.size()>=limit)break; }
                return result;
            }).when(level).getPlayers(any(),anyInt());
            doReturn(players).when(list).getPlayers();
            if(current) doReturn(players.getFirst()).when(source).getEntity();
            var selected=new EntitySelectorParser(new com.mojang.brigadier.StringReader(text),true).parse().findPlayers(source);
            var vector=new JsonObject();vector.addProperty("selector",text);vector.add("names",gson.toJsonTree(names));vector.add("examined",gson.toJsonTree(examined));vector.addProperty("count",selected.size());vector.addProperty("current_entity",current);
            if(!sort.equals("random")) vector.add("selected",gson.toJsonTree(selected.stream().map(identities::get).toList()));
            vectors.add(vector);
        }
        Files.writeString(Path.of(args[0]),gson.toJson(vectors)+"\n");
    }
}
