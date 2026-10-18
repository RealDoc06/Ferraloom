import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.dedicated.DedicatedServer;
import net.minecraft.server.dedicated.DedicatedPlayerList;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.notifications.NotificationService;
import net.minecraft.server.permissions.*;
import net.minecraft.server.players.*;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.level.storage.LevelData;
import static org.mockito.Mockito.*;

/** Calls the native protection and interaction methods, including real op membership. */
public final class SpawnProtectionVector {
    record Case(String dimension, String spawnDimension, int radius, boolean hasOps, int playerOpLevel,
                int[] spawn, int[] pos, boolean player, double borderSize) {}
    static Case c(String dimension,String spawnDimension,int radius,boolean hasOps,int op,
                  int sx,int sz,int x,int y,int z,boolean player,double border) {
        return new Case(dimension,spawnDimension,radius,hasOps,op,new int[]{sx,64,sz},new int[]{x,y,z},player,border);
    }
    static void field(Class<?> type,Object object,String name,Object value) throws Exception {
        var field=type.getDeclaredField(name); field.setAccessible(true); field.set(object,value);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var cases=new ArrayList<Case>();
        for (int[] pos:List.of(new int[]{0,64,0},new int[]{16,64,16},new int[]{-16,64,-16},
             new int[]{17,64,0},new int[]{0,64,-17},new int[]{0,Integer.MIN_VALUE,0}))
            cases.add(c("overworld","overworld",16,true,-1,0,0,pos[0],pos[1],pos[2],true,100));
        for (int op:new int[]{0,1,2,3,4}) cases.add(c("overworld","overworld",16,true,op,0,0,0,64,0,true,100));
        for (int radius:new int[]{-1,0,1,16}) cases.add(c("overworld","overworld",radius,false,-1,0,0,0,64,0,true,100));
        for (int radius:new int[]{-1,0,1}) cases.add(c("overworld","overworld",radius,true,-1,0,0,1,64,1,true,100));
        cases.add(c("the_nether","overworld",16,true,-1,0,0,0,64,0,true,100));
        cases.add(c("the_nether","the_nether",16,true,-1,0,0,0,64,0,true,100));
        cases.add(c("overworld","the_nether",16,true,-1,0,0,0,64,0,true,100));
        for (int x:new int[]{-5,4,5}) cases.add(c("overworld","overworld",16,true,0,0,0,x,64,0,true,10));
        cases.add(c("overworld","overworld",16,true,-1,0,0,0,64,0,false,10));
        cases.add(c("overworld","overworld",16,true,-1,0,0,100,64,100,false,10));
        cases.add(c("overworld","overworld",16,true,-1,100,200,116,64,216,true,1000));
        cases.add(c("overworld","overworld",16,true,-1,0,0,Integer.MIN_VALUE,64,0,true,100));
        cases.add(c("overworld","overworld",16,true,-1,Integer.MAX_VALUE,0,Integer.MIN_VALUE,64,0,true,100));
        var result=new JsonArray(); var gson=new GsonBuilder().setPrettyPrinting().create();
        var directory=Files.createTempDirectory("spawn-protection-vectors");
        try {
            int index=0;
            for (var test:cases) {
                var server=mock(DedicatedServer.class); var level=mock(ServerLevel.class);
                var player=mock(Player.class); var list=mock(DedicatedPlayerList.class);
                var identity=new NameAndId(new UUID(1,2),"Player");
                var ops=new ServerOpList(directory.resolve("ops"+(index++)+".json").toFile(),mock(NotificationService.class));
                if(test.hasOps) ops.add(new ServerOpListEntry(new NameAndId(new UUID(3,4),"Other"),LevelBasedPermissionSet.forLevel(PermissionLevel.OWNERS),false));
                if(test.playerOpLevel>=0) ops.add(new ServerOpListEntry(identity,LevelBasedPermissionSet.forLevel(PermissionLevel.byId(test.playerOpLevel)),false));
                field(PlayerList.class,list,"ops",ops); field(PlayerList.class,list,"server",server);
                field(ServerLevel.class,level,"server",server);
                doReturn(list).when(server).getPlayerList(); doReturn(test.radius).when(server).spawnProtectionRadius();
                doReturn(ops).when(list).getOps(); doCallRealMethod().when(list).isOp(any());
                doReturn(identity).when(player).nameAndId();
                var dimension=test.dimension.equals("overworld")?Level.OVERWORLD:Level.NETHER;
                var spawnDimension=test.spawnDimension.equals("overworld")?Level.OVERWORLD:Level.NETHER;
                doReturn(dimension).when(level).dimension();
                doReturn(new LevelData.RespawnData(GlobalPos.of(spawnDimension,new BlockPos(test.spawn[0],test.spawn[1],test.spawn[2])),0,0)).when(level).getRespawnData();
                var border=new WorldBorder(); border.setSize(test.borderSize);
                doReturn(border).when(level).getWorldBorder();
                doCallRealMethod().when(server).isUnderSpawnProtection(any(),any(),any());
                doCallRealMethod().when(level).mayInteract(any(),any());
                var pos=new BlockPos(test.pos[0],test.pos[1],test.pos[2]);
                var vector=gson.toJsonTree(test).getAsJsonObject();
                vector.addProperty("protected",server.isUnderSpawnProtection(level,pos,player));
                vector.addProperty("mayInteract",level.mayInteract(test.player?player:mock(Entity.class),pos));
                result.add(vector);
            }
            Files.writeString(Path.of(args[0]),gson.toJson(result)+"\n");
        } finally {
            try(var files=Files.list(directory)){for(var file:files.toList()) Files.delete(file);}
            Files.delete(directory);
        }
    }
}
