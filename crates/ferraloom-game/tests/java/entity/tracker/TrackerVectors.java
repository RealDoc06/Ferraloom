import java.lang.reflect.*;
import java.util.*;
import java.nio.file.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.*;
import net.minecraft.server.level.*;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.util.debug.LevelDebugSynchronizers;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.phys.Vec3;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import static org.mockito.Mockito.*;

/** Actual ChunkMap.TrackedEntity audience/pairing plus damage packet constructors. */
public final class TrackerVectors {
    static Field field(Class<?> cls, String name) throws Exception {
        var field = cls.getDeclaredField(name); field.setAccessible(true); return field;
    }
    static final class ProbeItem extends ItemEntity {
        ChunkMap map;
        ProbeItem(ServerLevel level) { super(EntityTypes.ITEM, level); }
        @Override public void updateDataBeforeSync() {
            map.sendToTrackingPlayers(this, new ClientboundEntityEventPacket(this, (byte)60));
        }
        @Override public void startSeenByPlayer(ServerPlayer player) {
            map.sendToTrackingPlayers(this, new ClientboundEntityEventPacket(this, (byte)61));
        }
        @Override public void stopSeenByPlayer(ServerPlayer player) {
            player.connection.send(new ClientboundEntityEventPacket(this, (byte)62));
        }
    }
    static String packet(Packet<?> packet) {
        if (packet instanceof ClientboundEntityEventPacket event) return "event:" + event.getEventId();
        if (packet instanceof ClientboundBundlePacket bundle) {
            var types = new ArrayList<String>();
            bundle.subPackets().forEach(child -> types.add(child.getClass().getSimpleName()));
            return "bundle:" + String.join(",", types);
        }
        if (packet instanceof ClientboundRemoveEntitiesPacket) return "remove";
        throw new AssertionError(packet.getClass());
    }
    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();
        var level = AdvancementVectors.level;
        doReturn(123).when(level).getNextEntityId();
        doReturn(mock(LevelDebugSynchronizers.class)).when(level).debugSynchronizers();
        doAnswer(call -> call.<Integer>getArgument(0)).when(AdvancementVectors.server).getScaledTrackingDistance(anyInt());
        var map = mock(ChunkMap.class, CALLS_REAL_METHODS);
        field(ChunkMap.class, "level").set(map, level);
        field(ChunkMap.class, "serverViewDistance").setInt(map, 10);
        var entries = new Int2ObjectOpenHashMap<Object>();
        field(ChunkMap.class, "entityMap").set(map, entries);
        boolean[] chunkTracked = {true};
        doAnswer(call -> chunkTracked[0]).when(map).isChunkTracked(any(), anyInt(), anyInt());
        var output = new ArrayList<String>();
        var player = mock(ServerPlayer.class);
        doReturn(2).when(player).requestedViewDistance();
        var connection = mock(ServerGamePacketListenerImpl.class);
        field(ServerPlayer.class, "connection").set(player, connection);
        doAnswer(call -> { output.add(packet(call.getArgument(0))); return null; }).when(connection).send(any());
        var entity = new ProbeItem(level); entity.map = map; entity.setId(7);
        entity.setPos(0,64,0); entity.setItem(new ItemStack(Items.STONE,3));
        var type = Class.forName("net.minecraft.server.level.ChunkMap$TrackedEntity");
        var ctor = type.getDeclaredConstructor(ChunkMap.class, Entity.class, int.class, UpdateInterval.class, boolean.class);
        ctor.setAccessible(true);
        var tracked = ctor.newInstance(map, entity, 64, UpdateInterval.periodic(3), true);
        entries.put(7, tracked);
        var update = type.getMethod("updatePlayer", ServerPlayer.class); update.setAccessible(true);
        var rows = new ArrayList<String>();
        for (double x : new double[]{32, 32, Math.nextUp(32.0), 32, 32}) {
            if (rows.size() == 3) chunkTracked[0] = false;
            if (rows.size() == 4) chunkTracked[0] = true;
            doReturn(new Vec3(x,100000,0)).when(player).position();
            output.clear(); update.invoke(tracked,player);
            rows.add("audience\t"+rows.size()+"\t"+(output.isEmpty()?"-":String.join("|",output)));
        }
        var eventRows = new ArrayList<String>();
        var sendSelf = type.getMethod("sendToTrackingPlayersAndSelf", net.minecraft.network.protocol.Packet.class);
        sendSelf.setAccessible(true);
        entries.remove(7); output.clear();
        var broadcast = ChunkMap.class.getDeclaredMethod("sendToTrackingPlayersAndSelf", Entity.class, net.minecraft.network.protocol.Packet.class);
        broadcast.setAccessible(true);
        broadcast.invoke(map, entity, new ClientboundEntityEventPacket(entity, (byte)3));
        eventRows.add("event\tmissing\t"+(output.isEmpty()?"-":String.join("|",output)));
        entries.put(7,tracked); output.clear();
        broadcast.invoke(map, entity, new ClientboundEntityEventPacket(entity, (byte)3));
        eventRows.add("event\titem\t"+(output.isEmpty()?"-":String.join("|",output)));
        var allocatorField = sun.misc.Unsafe.class.getDeclaredField("theUnsafe"); allocatorField.setAccessible(true);
        var allocator = (sun.misc.Unsafe)allocatorField.get(null);
        var playerEntry = allocator.allocateInstance(type);
        var subject = mock(ServerPlayer.class); doReturn(7).when(subject).getId();
        var selfConnection = mock(ServerGamePacketListenerImpl.class);
        field(ServerPlayer.class, "connection").set(subject,selfConnection);
        doAnswer(call -> { output.add("self:"+packet(call.getArgument(0))); return null; }).when(selfConnection).send(any());
        field(type,"entity").set(playerEntry,subject);
        var seen = java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<net.minecraft.server.network.ServerPlayerConnection,Boolean>());
        seen.add(connection); field(type,"seenBy").set(playerEntry,seen);
        output.clear(); sendSelf.invoke(playerEntry,new ClientboundEntityEventPacket(subject,(byte)3));
        eventRows.add("event\tplayer\t"+(output.isEmpty()?"-":String.join("|",output)));
        seen.clear(); output.clear(); sendSelf.invoke(playerEntry,new ClientboundEntityEventPacket(subject,(byte)3));
        eventRows.add("event\tself\t"+(output.isEmpty()?"-":String.join("|",output)));
        rows.addAll(eventRows);
        var types = AdvancementVectors.lookup.lookupOrThrow(Registries.DAMAGE_TYPE);
        doReturn(7).when(AdvancementVectors.player).getId();
        doReturn(12).when(AdvancementVectors.cow).getId();
        for (String mode : List.of("generic", "player", "indirect", "position")) {
            DamageSource source = switch(mode) {
                case "generic" -> new DamageSource(types.getOrThrow(DamageTypes.GENERIC));
                case "player" -> new DamageSource(types.getOrThrow(DamageTypes.PLAYER_ATTACK), AdvancementVectors.player);
                case "indirect" -> new DamageSource(types.getOrThrow(DamageTypes.ARROW), AdvancementVectors.cow, AdvancementVectors.player);
                case "position" -> new DamageSource(types.getOrThrow(DamageTypes.FALL), new Vec3(1.25,63,-0.0));
                default -> throw new AssertionError();
            };
            var packet = new ClientboundDamageEventPacket(entity,source);
            var registry = AdvancementVectors.lookup.lookupOrThrow(Registries.DAMAGE_TYPE);
            int number = registry.getId(packet.sourceType().value());
            rows.add("damage\t"+mode+"\t"+number+"\t"+packet.sourceCauseId()+"\t"+packet.sourceDirectId()+"\t"+packet.sourcePosition().map(v->Long.toHexString(Double.doubleToRawLongBits(v.x))+","+Long.toHexString(Double.doubleToRawLongBits(v.y))+","+Long.toHexString(Double.doubleToRawLongBits(v.z))).orElse("-"));
        }
        Files.write(Path.of(args[0]),rows);
        System.out.println(rows.size()+" native tracker observations");
    }
}
