import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.RegistryAccess;
import net.minecraft.nbt.*;
import net.minecraft.network.chat.*;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.MinecraftServer;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.world.level.storage.CommandStorage;
import net.minecraft.world.scores.*;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;
import net.minecraft.server.players.PlayerList;
import net.minecraft.world.phys.Vec3;
import org.apache.commons.lang3.mutable.MutableInt;
import static org.mockito.Mockito.*;

/** Executes ComponentUtils.resolve, including native NBT text component trees. */
public final class ComponentResolutionVector {
    static final JsonArray rows = new JsonArray();
    static Component parse(String input) {return ComponentSerialization.CODEC.parse(JsonOps.INSTANCE,JsonParser.parseString(input)).getOrThrow();}
    static void add(String input, String nbt, boolean sourcePresent, int depth, int limit, boolean discard, boolean reject, int budget) throws Exception {
        var source=mock(CommandSourceStack.class); var server=mock(MinecraftServer.class); var storage=mock(CommandStorage.class);
        var player=mock(ServerPlayer.class);
        doReturn(Component.literal("Display Alice")).when(player).getDisplayName();
        doReturn("Alice").when(player).getScoreboardName();
        doReturn(true).when(player).isAlive();
        doReturn(server).when(source).getServer(); doReturn(storage).when(server).getCommandStorage();
        var scoreboard=new net.minecraft.server.ServerScoreboard(server);
        var objective=scoreboard.addObjective("o",ObjectiveCriteria.DUMMY,Component.literal("o"),ObjectiveCriteria.RenderType.INTEGER,false,null);
        scoreboard.getOrCreatePlayerScore(ScoreHolder.forNameOnly("Alice"),objective).set(37);
        scoreboard.getOrCreatePlayerScore(ScoreHolder.forNameOnly("fake holder"),objective).set(-4);
        doReturn(scoreboard).when(server).getScoreboard();
        var players=mock(PlayerList.class);doReturn(players).when(server).getPlayerList();
        doReturn(List.of(player,player)).when(players).getPlayers();
        doReturn(player).when(players).getPlayerByName("Alice");
        doReturn(player).when(source).getEntity(); doReturn(Vec3.ZERO).when(source).getPosition();
        doReturn(PermissionSet.ALL_PERMISSIONS).when(source).permissions();
        doReturn(RegistryAccess.EMPTY).when(source).registryAccess();
        doReturn(TagParser.parseCompoundFully(nbt)).when(storage).get(any());
        var context=new ResolutionContext(sourcePresent?source:null,sourcePresent?player:null,o->!reject,limit,
            discard?ResolutionContext.LimitBehavior.DISCARD_REMAINING:ResolutionContext.LimitBehavior.STOP_PROCESSING_AND_COPY_REMAINING,budget,new MutableInt());
        var row=new JsonObject(); row.add("input",JsonParser.parseString(input));row.addProperty("nbt",nbt);
        row.addProperty("source",sourcePresent);row.addProperty("depth",depth);row.addProperty("limit",limit);row.addProperty("discard",discard);row.addProperty("reject",reject);row.addProperty("budget",budget);
        try {row.add("output",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,ComponentUtils.resolve(context,parse(input),depth)).getOrThrow());}
        catch(com.mojang.brigadier.exceptions.CommandSyntaxException error) {row.addProperty("error",error.getRawMessage().getString());}
        row.addProperty("count",context.resolvedComponentCount().intValue());rows.add(row);
    }
    static void add(String input) throws Exception {add(input,"{}",true,0,100,false,false,65536);}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        add("{\"text\":\"root\",\"bold\":true,\"extra\":[{\"text\":\"child\",\"bold\":false}],\"hover_event\":{\"action\":\"show_text\",\"value\":{\"selector\":\"@s\"}}}");
        add("{\"translate\":\"test\",\"fallback\":\"%s\",\"with\":[{\"selector\":\"@s\"},4,true,\"text\"]}");
        add("{\"selector\":\"@s\",\"color\":\"red\",\"separator\":{\"selector\":\"@s\"}}");
        add("{\"selector\":\"@s\"}","{}",false,0,100,false,false,65536);
        add("{\"score\":{\"name\":\"*\",\"objective\":\"o\"}}","{}",false,0,100,false,false,65536);
        for(String object:List.of("{\"atlas\":\"minecraft:blocks\",\"sprite\":\"minecraft:stone\"}","{\"atlas\":\"example:a\",\"sprite\":\"example:b\"}","{\"player\":\"Alice\"}","{\"player\":{\"id\":[19088743,-1985229329,19088743,-1985229329]}}")) {
            for(boolean reject:new boolean[]{false,true}) {
                add(object,"{}",true,0,100,false,reject,65536);
                var withFallback=JsonParser.parseString(object).getAsJsonObject();withFallback.add("fallback",JsonParser.parseString("{\"selector\":\"@s\"}"));
                add(withFallback.toString(),"{}",true,0,100,false,reject,65536);
            }
        }
        for(String holder:List.of("*","@s","Alice","fake holder","missing","@a"))add("{\"score\":{\"name\":\""+holder+"\",\"objective\":\"o\"}}");
        add("{\"score\":{\"name\":\"Alice\",\"objective\":\"missing\"}}");
        String nested="{\"translate\":\"test\",\"with\":[{\"text\":\"n\",\"extra\":[{\"selector\":\"@s\"}]}],\"extra\":[{\"selector\":\"@s\"}],\"hover_event\":{\"action\":\"show_text\",\"value\":{\"selector\":\"@s\"}}}";
        for(int limit:new int[]{0,1,2,100})for(boolean discard:new boolean[]{false,true})add(nested,"{}",true,0,limit,discard,false,65536);
        for(int depth:new int[]{99,100,101})add("{\"selector\":\"@s\"}","{}",true,depth,100,false,false,65536);
        for(String value:List.of("1b","-2s","3","4L","-0.0f","1.25d","'hello'","'a\\\"b'","[B;1b,-2b]","[I;1,-2]","[L;1L,-2L]","[1,2]","['a',2]","{z:1,a:2,'key space':'value'}","[]","{}")) {
            for(boolean plain:new boolean[]{false,true})add("{\"nbt\":\"v\",\"storage\":\"test:data\",\"plain\":"+plain+"}","{v:"+value+"}",true,0,100,false,false,65536);
        }
        add("{\"nbt\":\"missing\",\"storage\":\"test:data\"}","{}",true,0,100,false,false,65536);
        add("{\"nbt\":\"v[]\",\"storage\":\"test:data\",\"separator\":\" | \"}","{v:[1,2,3]}",true,0,100,false,false,65536);
        add("{\"nbt\":\"v[]\",\"storage\":\"test:data\",\"interpret\":true,\"separator\":{\"selector\":\"@s\"}}","{v:[{text:'first'},{selector:'@s'},{invalid:1}]}",true,0,100,false,false,65536);
        add("{\"nbt\":\"v\",\"storage\":\"test:data\"}","{v:1}",false,0,100,false,false,65536);
        add("{\"nbt\":\"v\",\"storage\":\"test:data\",\"extra\":[{\"nbt\":\"v\",\"storage\":\"test:data\"},{\"nbt\":\"v\",\"storage\":\"test:data\"}]}","{v:1}",true,0,100,false,false,0);
        for(boolean plain:new boolean[]{false,true}) {
            String longs="[L;"+String.join(",",Collections.nCopies(129,"1L"))+"]";
            add("{\"nbt\":\"v\",\"storage\":\"test:data\",\"plain\":"+plain+"}","{v:"+longs+"}",true,0,100,false,false,65536);
            String deep="[".repeat(65)+"0"+"]".repeat(65);
            add("{\"nbt\":\"v\",\"storage\":\"test:data\",\"plain\":"+plain+"}","{v:"+deep+"}",true,0,100,false,false,65536);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(rows)+"\n");
    }
}
