import java.lang.reflect.*;
import java.util.*;
import java.nio.file.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.core.BlockPos;
import net.minecraft.core.SectionPos;
import net.minecraft.world.level.gamerules.GameRules;
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
        if (packet instanceof ClientboundTakeItemEntityPacket take) return "take:"+take.getItemId()+","+take.getPlayerId()+","+take.getAmount();
        if (packet instanceof ClientboundRemoveEntitiesPacket) return "remove";
        throw new AssertionError(packet.getClass());
    }
    /** Actual ChunkMap add/move/remove with a real PlayerMap and observed ticket receiver. */
    static void spectators(ServerLevel level, ChunkMap map, ArrayList<String> rows) throws Exception {
        var players = new PlayerMap();
        field(ChunkMap.class, "playerMap").set(map, players);
        var distanceField = field(ChunkMap.class, "distanceManager");
        var distance = (DistanceManager)mock(distanceField.getType());
        distanceField.set(map, distance);
        var active = new SectionPos[]{null};
        doAnswer(call -> { active[0] = call.getArgument(0); return null; })
            .when(distance).addPlayer(any(),any());
        doAnswer(call -> { active[0] = null; return null; })
            .when(distance).removePlayer(any(),any());
        var rules = mock(GameRules.class);
        doReturn(rules).when(level).getGameRules();
        var player = mock(ServerPlayer.class);
        doReturn(2).when(player).requestedViewDistance();
        var section = new SectionPos[]{SectionPos.of(0,4,0)};
        var last = new SectionPos[]{section[0]};
        doAnswer(call -> new BlockPos(section[0].x()*16,section[0].y()*16,section[0].z()*16))
            .when(player).blockPosition();
        doAnswer(call -> new ChunkPos(section[0].x(),section[0].z())).when(player).chunkPosition();
        doAnswer(call -> ChunkTrackingView.of(new ChunkPos(section[0].x(),section[0].z()),2))
            .when(player).getChunkTrackingView();
        doAnswer(call -> { last[0] = call.getArgument(0); return null; }).when(player).setLastSectionPos(any());
        doAnswer(call -> last[0]).when(player).getLastSectionPos();
        var status = ChunkMap.class.getDeclaredMethod("updatePlayerStatus",ServerPlayer.class,boolean.class);
        status.setAccessible(true);
        for (String input : List.of(
            "add,spectator,false,0,4,0", "move,spectator,false,0,4,0",
            "move,spectator,true,0,4,0", "move,spectator,true,0,5,0",
            "move,spectator,false,0,5,0", "move,creative,false,0,5,0",
            "move,creative,false,1,5,-2", "move,spectator,false,1,5,-2",
            "remove,spectator,true,1,5,-2", "add,spectator,true,2,4,3",
            "remove,spectator,false,2,4,3", "add,spectator,false,-2,4,3",
            "remove,spectator,true,-2,4,3", "add,creative,false,0,4,0",
            "move,survival,false,0,4,0", "remove,survival,false,0,4,0")) {
            var args = input.split(",");
            section[0] = SectionPos.of(Integer.parseInt(args[3]),Integer.parseInt(args[4]),Integer.parseInt(args[5]));
            doReturn(args[1].equals("spectator")).when(player).isSpectator();
            doReturn(Boolean.parseBoolean(args[2])).when(rules).get(GameRules.SPECTATORS_GENERATE_CHUNKS);
            if (args[0].equals("move")) map.move(player);
            else status.invoke(map,player,args[0].equals("add"));
            rows.add("spectator\t"+input+"\t"+players.ignoredOrUnknown(player)+"\t"+
                (active[0]==null?"-":active[0].x()+","+active[0].y()+","+active[0].z()));
        }
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
        doReturn(player).when(connection).getPlayer();
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
        // Execute the actual LivingEntity base method; ServerPlayer's following
        // menu broadcast is a separate integration boundary.
        var chunkSource = mock(ServerChunkCache.class);
        doReturn(chunkSource).when(level).getChunkSource();
        doReturn(false).when(level).isClientSide();
        doAnswer(call -> { map.sendToTrackingPlayers(call.getArgument(0),call.getArgument(1)); return null; }).when(chunkSource).sendToTrackingPlayers(any(),any());
        var collector = mock(LivingEntity.class);
        doReturn(level).when(collector).level(); doReturn(12).when(collector).getId();
        doCallRealMethod().when(collector).take(any(),anyInt());
        entries.remove(7); output.clear(); collector.take(entity,3);
        rows.add("take\tuntracked\t"+(output.isEmpty()?"-":String.join("|",output)));
        entries.put(7,tracked); output.clear(); collector.take(entity,3);
        var remove = ChunkMap.class.getDeclaredMethod("removeEntity",Entity.class); remove.setAccessible(true);
        remove.invoke(map,entity);
        rows.add("take\ttracked-discard\t"+(output.isEmpty()?"-":String.join("|",output)));
        entity.setRemoved(Entity.RemovalReason.DISCARDED);
        entries.put(7,tracked); output.clear(); collector.take(entity,3);
        rows.add("take\tremoved\t"+(output.isEmpty()?"-":String.join("|",output)));
        output.clear(); collector.take(player,1);
        rows.add("take\tnon-item\t"+(output.isEmpty()?"-":String.join("|",output)));
        var orb = new ExperienceOrb(EntityTypes.EXPERIENCE_ORB,level); orb.setId(8); orb.setPos(0,64,0);
        var orbEntry = ctor.newInstance(map,orb,64,UpdateInterval.NEVER,true); entries.put(8,orbEntry);
        update.invoke(orbEntry,player);
        output.clear(); collector.take(orb,0);
        rows.add("take\torb-zero-count\t"+(output.isEmpty()?"-":String.join("|",output)));
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
        entries.clear();
        spectators(level,map,rows);
        Files.write(Path.of(args[0]),rows);
        System.out.println(rows.size()+" native tracker observations");
    }
}
