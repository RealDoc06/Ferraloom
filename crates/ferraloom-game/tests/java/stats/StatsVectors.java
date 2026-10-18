import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundAwardStatsPacket;
import net.minecraft.server.*;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.stats.*;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import static org.mockito.Mockito.*;

/** Actual typed-stat codec, counter arithmetic, dirty draining and packet bytes. */
public final class StatsVectors {
    static class Counter extends ServerStatsCounter {Counter(Path file){super(mock(MinecraftServer.class),file);} JsonElement json(){return toJson();}}
    static RegistryAccess.Frozen lookup;
    static ServerPlayer player;
    static ServerGamePacketListenerImpl connection;
    static JsonArray packets;
    static String wire(ClientboundAwardStatsPacket packet) {var buf=new RegistryFriendlyByteBuf(Unpooled.buffer(),lookup);ClientboundAwardStatsPacket.STREAM_CODEC.encode(buf,packet);var bytes=new byte[buf.readableBytes()];buf.readBytes(bytes);buf.release();return HexFormat.of().formatHex(bytes);}
    static JsonElement json(String raw){return JsonParser.parseString(raw);}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();lookup=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);player=mock(ServerPlayer.class);connection=mock(ServerGamePacketListenerImpl.class);player.connection=connection;
        packets=new JsonArray();doAnswer(call->{var p=(ClientboundAwardStatsPacket)call.getArgument(0);packets.add(wire(p));return null;}).when(connection).send(any(ClientboundAwardStatsPacket.class));
        var root=new JsonObject();var statTypes=new JsonArray();
        for(var type:BuiltInRegistries.STAT_TYPE){var row=new JsonObject();row.addProperty("name",BuiltInRegistries.STAT_TYPE.getKey(type).toString());row.addProperty("id",BuiltInRegistries.STAT_TYPE.getId(type));var values=new JsonArray();for(var key:type.getRegistry().keySet())values.add(key.toString());row.add("values",values);statTypes.add(row);}root.add("types",statTypes);
        var cases=new JsonArray();var temporary=Files.createTempDirectory("ferraloom-stats");
        for(String raw:List.of("{}","{\"stats\":{}}","{\"stats\":{\"minecraft:custom\":{\"minecraft:jump\":7,\"minecraft:missing\":12,\"minecraft:deaths\":4},\"minecraft:mined\":{\"minecraft:stone\":3}}}","{\"stats\":{\"minecraft:missing\":{\"minecraft:jump\":9},\"minecraft:custom\":{\"minecraft:jump\":1.8,\"minecraft:deaths\":\"7\",\"minecraft:play_time\":null}}}","{\"stats\":{\"minecraft:used\":{\"minecraft:stone\":2147483648,\"minecraft:apple\":-2147483649}}}","{\"stats\":null}","{\"stats\":[]}","{\"stats\":{\"minecraft:custom\":{\"minecraft:jump\":8},\"minecraft:mined\":42}}")) {
            var input=json(raw).getAsJsonObject();input.addProperty("DataVersion",5023);var counter=new Counter(temporary.resolve("missing"));var fixer=mock(com.mojang.datafixers.DataFixer.class);doAnswer(call->call.getArgument(1)).when(fixer).update(any(),any(),eq(5023),eq(5023));counter.parse(fixer,input);
            var row=new JsonObject();row.add("input",input);row.add("saved",counter.json());counter.sendStats(player);row.addProperty("initial_packet",packets.remove(packets.size()-1).getAsString());counter.markAllDirty();counter.sendStats(player);row.addProperty("all_packet",packets.remove(packets.size()-1).getAsString());counter.sendStats(player);row.addProperty("empty_packet",packets.remove(packets.size()-1).getAsString());cases.add(row);
        }
        root.add("persistence",cases);var steps=new JsonArray();var counter=new Counter(temporary.resolve("sequence"));var jump=Stats.CUSTOM.get(Stats.JUMP);var stone=Stats.BLOCK_MINED.get(Blocks.STONE);
        int[][] operations={{0,0},{1,1},{1,Integer.MAX_VALUE},{1,-1},{0,Integer.MIN_VALUE},{1,-1},{0,7},{0,7}};
        for(var op:operations){if(op[0]==0)counter.setValue(player,jump,op[1]);else counter.increment(player,jump,op[1]);counter.setValue(player,stone,3);var row=new JsonObject();row.addProperty("set",op[0]==0);row.addProperty("amount",op[1]);row.addProperty("value",counter.getValue(jump));row.add("saved",counter.json());counter.sendStats(player);row.addProperty("packet",packets.remove(packets.size()-1).getAsString());steps.add(row);}root.add("steps",steps);
        counter.save();root.add("saved_file",JsonParser.parseString(Files.readString(temporary.resolve("sequence"))));
        try(var out=new java.util.zip.GZIPOutputStream(Files.newOutputStream(Path.of(args[0])))){out.write(new GsonBuilder().serializeNulls().create().toJson(root).getBytes(java.nio.charset.StandardCharsets.UTF_8));}for(var entry:root.entrySet())System.out.println(entry.getKey()+": "+(entry.getValue().isJsonArray()?entry.getValue().getAsJsonArray().size():1));
        try(var paths=Files.walk(temporary)){for(var path:paths.sorted(Comparator.reverseOrder()).toList())Files.delete(path);}
    }
}
