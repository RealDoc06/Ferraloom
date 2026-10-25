import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.*;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundBossEventPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.bossevents.CustomBossEvent;
import net.minecraft.server.bossevents.CustomBossEvents;
import net.minecraft.nbt.*;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.world.BossEvent;
import static org.mockito.Mockito.*;

/** Actual CustomBossEvent lifecycle and packet codecs; only player connection endpoints are supplied. */
public class BossEventVectors {
  static JsonArray rows = new JsonArray();
  static Map<String,JsonArray> packets = new TreeMap<>();
  static int dirty;
  static CustomBossEvent event;
  static List<ServerPlayer> players;
  static String hex(Packet<?> packet) {
    var buffer = new RegistryFriendlyByteBuf(Unpooled.buffer(),RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY));
    ClientboundBossEventPacket.STREAM_CODEC.encode(buffer,(ClientboundBossEventPacket)packet);
    byte[] bytes = new byte[buffer.readableBytes()]; buffer.readBytes(bytes); buffer.release();
    return HexFormat.of().formatHex(bytes);
  }
  static void step(String action, String argument) {
    packets.clear(); dirty=0;
    switch(action) {
      case "add" -> event.addPlayer(players.get(Integer.parseInt(argument)));
      case "remove" -> event.removePlayer(players.get(Integer.parseInt(argument)));
      case "connect" -> event.onPlayerConnect(players.get(Integer.parseInt(argument)));
      case "disconnect" -> event.onPlayerDisconnect(players.get(Integer.parseInt(argument)));
      case "players" -> event.setPlayers(argument.isEmpty()?List.of():Arrays.stream(argument.split(",")).map(Integer::parseInt).map(players::get).toList());
      case "clear" -> event.removeAllPlayers();
      case "value" -> event.setValue(Integer.parseInt(argument));
      case "max" -> event.setMax(Integer.parseInt(argument));
      case "progress" -> event.setProgress(Float.parseFloat(argument));
      case "name" -> event.setName(Component.literal(argument));
      case "color" -> event.setColor(BossEvent.BossBarColor.valueOf(argument));
      case "overlay" -> event.setOverlay(BossEvent.BossBarOverlay.valueOf(argument));
      case "visible" -> event.setVisible(Boolean.parseBoolean(argument));
      case "darken" -> event.setDarkenScreen(Boolean.parseBoolean(argument));
      case "music" -> event.setPlayBossMusic(Boolean.parseBoolean(argument));
      case "fog" -> event.setCreateWorldFog(Boolean.parseBoolean(argument));
      default -> throw new AssertionError(action);
    }
    var row = new JsonObject(); row.addProperty("action",action); row.addProperty("argument",argument);
    row.addProperty("dirty",dirty>0); row.addProperty("value",event.value()); row.addProperty("max",event.max());
    row.addProperty("progressBits",Float.floatToIntBits(event.getProgress())); row.addProperty("visible",event.isVisible());
    row.addProperty("color",event.getColor().ordinal()); row.addProperty("overlay",event.getOverlay().ordinal());
    row.add("display",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,event.getDisplayName()).getOrThrow());
    var online=new JsonArray(); event.getPlayers().stream().map(p->p.getUUID().toString()).sorted().forEach(online::add); row.add("online",online);
    var persistent=new JsonArray(); event.pack().players().stream().map(UUID::toString).sorted().forEach(persistent::add); row.add("persistent",persistent);
    row.add("packets",new Gson().toJsonTree(packets)); rows.add(row);
  }
  public static void main(String[] args) throws Exception {
    SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
    event=new CustomBossEvent(new UUID(0,17),Identifier.parse("test:bar"),Component.literal("Boss"),()->dirty++);
    players=new ArrayList<>();
    for(int i=0;i<2;i++) {
      final String key=new UUID(0,i+1).toString();
      var player=mock(ServerPlayer.class); when(player.getUUID()).thenReturn(new UUID(0,i+1));
      var listener=mock(ServerGamePacketListenerImpl.class); player.connection=listener;
      doAnswer(call->{packets.computeIfAbsent(key,k->new JsonArray()).add(hex(call.getArgument(0))); return null;}).when(listener).send(any(Packet.class));
      players.add(player);
    }
    step("add","0"); step("add","0"); step("add","1");
    for(String value:List.of("0","50","50","200","-1")) step("value",value);
    step("max","0"); step("value","0"); step("progress","NaN"); step("max","100");
    step("progress","-0.0"); step("progress","0.0");
    step("name","Renamed"); step("name","Renamed");
    for(var color:BossEvent.BossBarColor.values()) step("color",color.name());
    for(var overlay:BossEvent.BossBarOverlay.values()) step("overlay",overlay.name());
    for(String property:List.of("darken","music","fog")) for(String value:List.of("true","true","false")) step(property,value);
    step("visible","false"); step("value","30"); step("remove","1"); step("add","1"); step("visible","true");
    step("disconnect","1"); step("connect","1"); step("players","0,0"); step("disconnect","0");
    step("players",""); step("connect","0"); step("add","0"); step("disconnect","0"); step("clear","");
    Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(rows)+"\n");
    var codecRows=new JsonArray();
    for(String input:List.of("{}", "{'test:bar':{Name:'Boss'}}", "{'test:bar':{Name:'Boss',Visible:1b,Value:35,Max:80,Color:'red',Overlay:'notched_12',DarkenScreen:1b,PlayBossMusic:1b,CreateWorldFog:1b,Players:[[I;0,0,0,1],[I;0,0,0,1]]}}",
        "{'test:bar':{Name:'Boss',Visible:0.5d,Value:-0.5d,Max:0}}", "{'test:bar':{Name:'Boss',Players:[[0b,0b,0b,2b]]}}",
        "{'test:bar':{Name:'Boss',Players:[[L;0,0,0,2]]}}", "{'test:bar':{Name:'Boss',Players:['00000000-0000-0000-0000-000000000001']}}",
        "{'test:bar':{Name:'Boss',Color:'invalid'}}", "{'test:bar':{Value:4}}", "{'test:bar':{Name:'Boss',Visible:'yes'}}")) {
      var row=new JsonObject(); row.addProperty("input",input);
      var decoded=CustomBossEvents.TYPE.codec().parse(NbtOps.INSTANCE,TagParser.parseCompoundFully(input));
      row.addProperty("valid",decoded.result().isPresent());
      if(decoded.result().isPresent()) {
        var value=decoded.result().orElseThrow(); row.addProperty("dirty",value.isDirty());
        row.addProperty("output",CustomBossEvents.TYPE.codec().encodeStart(NbtOps.INSTANCE,value).getOrThrow().toString());
      }
      codecRows.add(row);
    }
    Files.writeString(Path.of(args[1]),new GsonBuilder().setPrettyPrinting().create().toJson(codecRows)+"\n");
    var owner=new CustomBossEvents();var random=net.minecraft.util.RandomSource.create(1);
    var inputIds=new JsonArray();
    // "an" and "c0" have the same String hash; sixteen concatenations force tree bins.
    for(int i=0;i<16;i++) {
      String path="";for(int bit=0;bit<4;bit++)path+=((i>>bit)&1)==0?"an":"c0";
      var id=Identifier.parse("test:"+path);inputIds.add(id.toString());owner.create(random,id,Component.literal(path));
    }
    var order=new JsonObject();order.add("input",inputIds);var ordered=new JsonArray();owner.getIds().forEach(id->ordered.add(id.toString()));order.add("ordered",ordered);
    owner.setDirty(false);owner.onPlayerConnect(players.getFirst());order.addProperty("dirtyAfterUnrelatedConnect",owner.isDirty());
    owner.onPlayerDisconnect(players.getFirst());order.addProperty("dirtyAfterUnrelatedDisconnect",owner.isDirty());
    Files.writeString(Path.of(args[2]),new GsonBuilder().setPrettyPrinting().create().toJson(order)+"\n");
  }
}
