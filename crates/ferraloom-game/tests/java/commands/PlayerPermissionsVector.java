import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.dedicated.DedicatedServer;
import net.minecraft.server.dedicated.DedicatedPlayerList;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.players.*;
import net.minecraft.server.permissions.*;
import net.minecraft.server.notifications.NotificationService;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.level.GameType;
import static org.mockito.Mockito.*;

/** Real dedicated profile permission lookup, ServerPlayer override and abilities gate. */
public final class PlayerPermissionsVector {
    static void field(Class<?> owner,Object receiver,String name,Object value) throws Exception {
        var field=owner.getDeclaredField(name);field.setAccessible(true);field.set(receiver,value);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var identity=new NameAndId(new UUID(1,2),"metadata");
        var directory=Files.createTempDirectory("profile-permission-vectors");
        var rows=new JsonArray();int index=0;
        try {
            for(String entryCase:List.of("nonop","missing","-2","0","1","2","3","4","9"))for(boolean creative:List.of(false,true))for(boolean instabuild:List.of(false,true)) {
                var server=mock(DedicatedServer.class);var list=mock(DedicatedPlayerList.class);var player=mock(ServerPlayer.class);
                var file=directory.resolve("ops"+(index++)+".json");
                var ops=new ServerOpList(file.toFile(),mock(NotificationService.class));
                JsonObject input=null;
                if(!entryCase.equals("nonop")) {
                    input=new JsonObject();input.addProperty("uuid",identity.id().toString());input.addProperty("name",identity.name());
                    if(!entryCase.equals("missing"))input.addProperty("level",Integer.parseInt(entryCase));
                    ops.add(new ServerOpListEntry(input));
                }
                field(PlayerList.class,list,"ops",ops);field(PlayerList.class,list,"server",server);
                doReturn(list).when(server).getPlayerList();doReturn(ops).when(list).getOps();
                doCallRealMethod().when(list).isOp(any());doCallRealMethod().when(server).isSingleplayerOwner(any());
                doCallRealMethod().when(server).getProfilePermissions(any());
                doThrow(new AssertionError("dedicated operator-without-entry fallback is unreachable")).when(server).operatorUserPermissions();
                field(ServerPlayer.class,player,"server",server);doReturn(identity).when(player).nameAndId();
                var abilities=new Abilities();abilities.instabuild=instabuild;field(Player.class,player,"abilities",abilities);
                doReturn(creative?GameType.CREATIVE:GameType.SURVIVAL).when(player).gameMode();
                doCallRealMethod().when(player).permissions();doCallRealMethod().when(player).canUseGameMasterBlocks();
                var permissions=player.permissions();
                var levels=new JsonArray();for(var level:PermissionLevel.values())levels.add(permissions.hasPermission(new Permission.HasCommandLevel(level)));
                var row=new JsonObject();row.add("entry",input);row.addProperty("creative",creative);row.addProperty("instabuild",instabuild);
                row.addProperty("is_op",list.isOp(identity));row.add("levels",levels);
                row.addProperty("selectors",permissions.hasPermission(Permissions.COMMANDS_ENTITY_SELECTORS));
                row.addProperty("can_use_game_master_blocks",player.canUseGameMasterBlocks());
                row.add("saved",Files.exists(file)?JsonParser.parseString(Files.readString(file)).getAsJsonArray().get(0):JsonNull.INSTANCE);
                rows.add(row);
            }
            Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().serializeNulls().create().toJson(rows)+"\n");
        } finally {
            try(var files=Files.list(directory)){for(var file:files.toList())Files.delete(file);}
            Files.delete(directory);
        }
    }
}
