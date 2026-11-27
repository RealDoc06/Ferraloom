package dev.horizonlab.parity;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.io.BufferedReader;
import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.PrintStream;
import java.io.PrintWriter;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.ArrayList;
import java.util.BitSet;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;
import java.util.UUID;
import java.util.zip.Deflater;
import java.util.zip.Inflater;
import net.minecraft.SharedConstants;
import net.minecraft.core.UUIDUtil;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.LastSeenMessages;
import net.minecraft.network.chat.contents.TranslatableContents;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.common.ClientboundDisconnectPacket;
import net.minecraft.network.protocol.common.ClientboundKeepAlivePacket;
import net.minecraft.network.protocol.common.ClientboundPingPacket;
import net.minecraft.network.protocol.common.ServerboundClientInformationPacket;
import net.minecraft.network.protocol.common.ServerboundCustomPayloadPacket;
import net.minecraft.network.protocol.common.ServerboundKeepAlivePacket;
import net.minecraft.network.protocol.common.ServerboundPongPacket;
import net.minecraft.network.protocol.common.custom.BrandPayload;
import net.minecraft.network.protocol.configuration.ClientboundCodeOfConductPacket;
import net.minecraft.network.protocol.configuration.ClientboundFinishConfigurationPacket;
import net.minecraft.network.protocol.configuration.ClientboundSelectKnownPacks;
import net.minecraft.network.protocol.configuration.ConfigurationProtocols;
import net.minecraft.network.protocol.configuration.ServerboundAcceptCodeOfConductPacket;
import net.minecraft.network.protocol.configuration.ServerboundFinishConfigurationPacket;
import net.minecraft.network.protocol.configuration.ServerboundSelectKnownPacks;
import net.minecraft.network.protocol.game.ClientboundAddEntityPacket;
import net.minecraft.network.protocol.game.ClientboundChunkBatchFinishedPacket;
import net.minecraft.network.protocol.game.ClientboundLoginPacket;
import net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket;
import net.minecraft.network.protocol.game.ClientboundPlayerPositionPacket;
import net.minecraft.network.protocol.game.ClientboundRecipeBookAddPacket;
import net.minecraft.network.protocol.game.ClientboundRemoveEntitiesPacket;
import net.minecraft.network.protocol.game.ClientboundSelectAdvancementsTabPacket;
import net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket;
import net.minecraft.network.protocol.game.ClientboundSetScorePacket;
import net.minecraft.network.protocol.game.ClientboundSystemChatPacket;
import net.minecraft.network.protocol.game.GameProtocols;
import net.minecraft.network.protocol.game.ServerboundAcceptTeleportationPacket;
import net.minecraft.network.protocol.game.ServerboundChatCommandPacket;
import net.minecraft.network.protocol.game.ServerboundChatPacket;
import net.minecraft.network.protocol.game.ServerboundChunkBatchReceivedPacket;
import net.minecraft.network.protocol.game.ServerboundClientTickEndPacket;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import net.minecraft.network.protocol.game.ServerboundPlayerLoadedPacket;
import net.minecraft.network.protocol.game.ServerboundRecipeBookSeenRecipePacket;
import net.minecraft.network.protocol.game.ServerboundSeenAdvancementsPacket;
import net.minecraft.network.protocol.handshake.ClientIntent;
import net.minecraft.network.protocol.handshake.ClientIntentionPacket;
import net.minecraft.network.protocol.handshake.HandshakeProtocols;
import net.minecraft.network.protocol.login.ClientboundLoginCompressionPacket;
import net.minecraft.network.protocol.login.ClientboundLoginDisconnectPacket;
import net.minecraft.network.protocol.login.ClientboundLoginFinishedPacket;
import net.minecraft.network.protocol.login.LoginProtocols;
import net.minecraft.network.protocol.login.ServerboundHelloPacket;
import net.minecraft.network.protocol.login.ServerboundLoginAcknowledgedPacket;
import net.minecraft.network.protocol.ping.ClientboundPongResponsePacket;
import net.minecraft.network.protocol.ping.ServerboundPingRequestPacket;
import net.minecraft.network.syncher.SynchedEntityData;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.PositionMoveRotation;
import net.minecraft.world.phys.Vec3;

/**
 * Players made of vanilla's own protocol code for the hub's player sync
 * (tools/test_hub_sync_parity.py). Each joins in offline mode (accepting a
 * code of conduct, recorded), answers keep-alives, pings, teleports and
 * chunk batches, reminds the server where it stands every second as
 * LocalPlayer.sendPosition does, and records what its client would make of the server's
 * packets: the players added to and removed from its level, the tab list's
 * hat flags, the scores it is sent, the selected advancements tab, its
 * recipe book's highlights, pongs, why it was disconnected, and on
 * {@code snapshot} the entity data it holds of each player (indices 7, 10,
 * 11, 18, 19 and 20, those the hub syncs beside Horizon's own).
 *
 * <p>{@code HubSyncClient HOST PORT OUT} takes its script on stdin:
 * {@code step LABEL}, {@code join NAME [nohat] [unlisted]}, {@code cmd NAME
 * COMMAND...}, {@code sync NAME} (prints {@code synced NAME}), {@code hat
 * NAME on|off} (ClientInformation), {@code hold NAME TICKS} (standing
 * still, a position and a tick end every 50 ms), {@code float NAME TICKS UP}
 * (the same UP blocks higher, off the ground), {@code chat NAME COUNT} and
 * {@code commands NAME COUNT} (that many at once), {@code ping NAME TIME},
 * {@code tab NAME ID} (ServerboundSeenAdvancementsPacket OPENED_TAB),
 * {@code seen NAME} (ServerboundRecipeBookSeenRecipePacket for the first
 * display of the last recipes added), {@code snapshot NAME}, {@code quit
 * NAME} and {@code end}.
 */
public final class HubSyncClient {
    private static volatile String step = "start";
    private static final Map<UUID, String> NAMES = new LinkedHashMap<>();
    private static final int[] WATCHED = {7, 10, 11, 18, 19, 20};

    public static void main(String[] args) throws Exception {
        PrintStream stdout = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
        ParityData.init();
        String host = args[0];
        int port = Integer.parseInt(args[1]);
        Path output = Path.of(args[2]);
        Map<String, Player> players = new LinkedHashMap<>();
        List<Player> all = new ArrayList<>();
        BufferedReader script = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String line;
        while ((line = script.readLine()) != null) {
            String[] w = line.trim().split(" ", 3);
            try {
                switch (w[0]) {
                    case "step" -> step = w[1];
                    case "join" -> {
                        String options = w.length > 2 ? w[2] : "";
                        Player p = new Player(w[1], !options.contains("nohat"), !options.contains("unlisted"));
                        players.put(w[1], p);
                        all.add(p);
                        NAMES.put(UUIDUtil.createOfflinePlayerUUID(w[1]), w[1]);
                        p.join(host, port);
                    }
                    case "cmd" -> players.get(w[1]).command(w[2]);
                    case "sync" -> {
                        try {
                            players.get(w[1]).sync();
                        } finally {
                            stdout.println("synced " + w[1]);
                        }
                    }
                    case "hat" -> {
                        Player p = players.get(w[1]);
                        p.hat = w[2].equals("on");
                        p.send(new ServerboundClientInformationPacket(p.information()));
                    }
                    case "hold" -> players.get(w[1]).hold(Integer.parseInt(w[2]), 0.0);
                    case "float" -> {
                        String[] a = w[2].split(" ");
                        players.get(w[1]).hold(Integer.parseInt(a[0]), Double.parseDouble(a[1]));
                    }
                    case "chat" -> {
                        Player p = players.get(w[1]);
                        for (int i = 0; i < Integer.parseInt(w[2]); i++) {
                            p.send(new ServerboundChatPacket("message " + i, Instant.now(), 0L, Optional.empty(),
                                new LastSeenMessages.Update(0, new BitSet(), (byte) 0)));
                        }
                    }
                    case "commands" -> {
                        Player p = players.get(w[1]);
                        for (int i = 0; i < Integer.parseInt(w[2]); i++) p.send(new ServerboundChatCommandPacket("list"));
                    }
                    case "ping" -> players.get(w[1]).send(new ServerboundPingRequestPacket(Long.parseLong(w[2])));
                    case "tab" -> players.get(w[1]).send(new ServerboundSeenAdvancementsPacket(
                        ServerboundSeenAdvancementsPacket.Action.OPENED_TAB, Identifier.parse(w[2])));
                    case "seen" -> {
                        Player p = players.get(w[1]);
                        p.record("seen display " + p.lastAdded);
                        p.send(new ServerboundRecipeBookSeenRecipePacket(
                            new net.minecraft.world.item.crafting.display.RecipeDisplayId(p.lastAdded)));
                    }
                    case "snapshot" -> players.get(w[1]).snapshot();
                    case "quit" -> players.remove(w[1]).close();
                    case "end" -> {
                        try (PrintWriter out = new PrintWriter(Files.newBufferedWriter(output, StandardCharsets.UTF_8))) {
                            for (Player p : all) {
                                synchronized (p.lines) {
                                    for (String l : p.lines) out.println(p.name + " " + l);
                                }
                            }
                        }
                        for (Player p : all) p.close();
                        System.exit(0);
                    }
                    default -> throw new IllegalArgumentException("unknown script line " + line);
                }
            } catch (Exception e) {
                Player p = w.length > 1 ? players.get(w[1]) : null;
                if (p != null) p.record("error " + w[0] + " " + e);
                else System.err.println("error " + line + ": " + e);
            }
        }
        System.exit(1);
    }

    /** A synced value as a line: particles by their codec, the rest as Java prints them. */
    private static String value(Object v) {
        if (v instanceof List<?> list) {
            List<String> out = new ArrayList<>();
            for (Object o : list) {
                out.add(net.minecraft.core.particles.ParticleTypes.CODEC
                    .encodeStart(ParityData.access.createSerializationContext(com.mojang.serialization.JsonOps.INSTANCE),
                        (net.minecraft.core.particles.ParticleOptions) o)
                    .getOrThrow().toString());
            }
            return out.toString();
        }
        return String.valueOf(v);
    }

    /** One connection: login and configuration in the caller's thread, then play on its own. */
    private static final class Player {
        final String name;
        final List<String> lines = new ArrayList<>();
        volatile boolean hat;
        final boolean listed;
        private Socket socket;
        private DataInputStream in;
        private OutputStream out;
        private int threshold = -1;
        private ProtocolInfo<?> inbound;
        private ProtocolInfo<?> outbound;
        private volatile PositionMoveRotation position = new PositionMoveRotation(Vec3.ZERO, Vec3.ZERO, 0, 0);
        private volatile boolean loaded;
        private volatile int id = -1;
        /** The players this one was sent, by entity id. */
        private final Map<Integer, String> seen = new HashMap<>();
        /** The watched entity data it holds, by entity id then index. */
        private final Map<Integer, Map<Integer, String>> data = new HashMap<>();
        /** The first display of the last ClientboundRecipeBookAddPacket that added recipes. */
        volatile int lastAdded = -1;
        private final Object replyLock = new Object();
        private int replies;
        private int syncs;

        Player(String name, boolean hat, boolean listed) {
            this.name = name;
            this.hat = hat;
            this.listed = listed;
        }

        ClientInformation information() {
            ClientInformation d = ClientInformation.createDefault();
            // Every skin part shown, or all but the hat (PlayerModelPart.HAT, bit 6).
            int parts = hat ? 0x7F : 0x3F;
            return new ClientInformation(d.language(), d.viewDistance(), d.chatVisibility(), d.chatColors(), parts,
                d.mainHand(), d.textFilteringEnabled(), listed, d.particleStatus());
        }

        void record(String line) {
            synchronized (lines) {
                lines.add("step=" + step + " " + line);
            }
        }

        void command(String command) throws Exception {
            synchronized (replyLock) {
                int before = replies;
                send(new ServerboundChatCommandPacket(command));
                long until = System.nanoTime() + 30_000_000_000L;
                while (replies == before) {
                    long left = (until - System.nanoTime()) / 1_000_000L;
                    if (left <= 0) {
                        record("error no reply to /" + command);
                        return;
                    }
                    replyLock.wait(left);
                }
            }
        }

        void sync() throws Exception {
            synchronized (replyLock) {
                int before = syncs;
                send(new ServerboundChatCommandPacket("time query gametime"));
                long until = System.nanoTime() + 30_000_000_000L;
                while (syncs == before) {
                    long left = (until - System.nanoTime()) / 1_000_000L;
                    if (left <= 0) {
                        record("error no reply to sync");
                        return;
                    }
                    replyLock.wait(left);
                }
            }
        }

        /** Each client tick's moves, sent whole (a position per tick). */
        private final Object moves = new Object();
        /** While set, `hold` sends the positions and the reminder waits. */
        private volatile boolean holding;

        /** LocalPlayer.sendPosition for `ticks` client ticks, `up` blocks over where the server put it. */
        void hold(int ticks, double up) throws Exception {
            holding = true;
            try {
                PositionMoveRotation at = position;
                Vec3 there = at.position().add(0, up, 0);
                // A tick of its own: the server takes one position per client tick (receivedPositionThisTick).
                send(ServerboundClientTickEndPacket.INSTANCE);
                for (int i = 0; i < ticks && !socket.isClosed(); i++) {
                    synchronized (moves) {
                        send(new ServerboundMovePlayerPacket.PosRot(there, at.yRot(), at.xRot(), up == 0.0, false));
                        send(ServerboundClientTickEndPacket.INSTANCE);
                    }
                    Thread.sleep(50);
                }
            } finally {
                holding = false;
            }
        }

        /** LocalPlayer.sendPosition's reminder: standing still, the position every 20 ticks. */
        private void remind() {
            try {
                while (!socket.isClosed()) {
                    Thread.sleep(1000);
                    if (holding || !loaded) continue;
                    PositionMoveRotation at = position;
                    synchronized (moves) {
                        send(ServerboundClientTickEndPacket.INSTANCE);
                        send(new ServerboundMovePlayerPacket.PosRot(at.position(), at.yRot(), at.xRot(), true, false));
                        send(ServerboundClientTickEndPacket.INSTANCE);
                    }
                }
            } catch (Exception e) {
                if (!socket.isClosed()) record("error remind " + e);
            }
        }

        /** The watched entity data this player's client holds of each player it knows, itself first. */
        void snapshot() {
            synchronized (data) {
                Map<String, String> shown = new TreeMap<>();
                for (var e : data.entrySet()) {
                    String who = e.getKey() == id ? "self" : seen.get(e.getKey());
                    if (who == null || e.getValue().isEmpty()) continue;
                    shown.put(who, new TreeMap<>(e.getValue()).toString());
                }
                if (shown.isEmpty()) record("data none");
                shown.forEach((who, values) -> record("data " + who + " " + values));
            }
        }

        void close() throws Exception {
            if (socket != null) socket.close();
        }

        void join(String host, int port) throws Exception {
            socket = new Socket();
            socket.connect(new InetSocketAddress(host, port), 10_000);
            socket.setTcpNoDelay(true);
            in = new DataInputStream(socket.getInputStream());
            out = socket.getOutputStream();
            outbound = HandshakeProtocols.SERVERBOUND;
            send(new ClientIntentionPacket(SharedConstants.getProtocolVersion(), host, port, ClientIntent.LOGIN));
            outbound = LoginProtocols.SERVERBOUND;
            inbound = LoginProtocols.CLIENTBOUND;
            send(new ServerboundHelloPacket(name, UUIDUtil.createOfflinePlayerUUID(name)));
            while (true) {
                Packet<?> p = read();
                if (p instanceof ClientboundLoginCompressionPacket c) {
                    threshold = c.getCompressionThreshold();
                } else if (p instanceof ClientboundLoginFinishedPacket) {
                    send(ServerboundLoginAcknowledgedPacket.INSTANCE);
                    break;
                } else if (p instanceof ClientboundLoginDisconnectPacket d) {
                    throw new IllegalStateException("login refused: " + d.reason().getString());
                }
            }
            outbound = ConfigurationProtocols.SERVERBOUND;
            inbound = ConfigurationProtocols.CLIENTBOUND;
            send(new ServerboundCustomPayloadPacket(new BrandPayload("vanilla")));
            send(new ServerboundClientInformationPacket(information()));
            while (true) {
                Packet<?> p = read();
                if (p instanceof ClientboundSelectKnownPacks k) {
                    send(new ServerboundSelectKnownPacks(k.knownPacks()));
                } else if (p instanceof ClientboundCodeOfConductPacket c) {
                    record("code of conduct " + c.codeOfConduct().replace("\n", "\\n"));
                    send(ServerboundAcceptCodeOfConductPacket.INSTANCE);
                } else if (p instanceof ClientboundKeepAlivePacket k) {
                    send(new ServerboundKeepAlivePacket(k.getId()));
                } else if (p instanceof ClientboundPingPacket k) {
                    send(new ServerboundPongPacket(k.getId()));
                } else if (p instanceof ClientboundFinishConfigurationPacket) {
                    send(ServerboundFinishConfigurationPacket.INSTANCE);
                    break;
                } else if (p instanceof ClientboundDisconnectPacket d) {
                    throw new IllegalStateException("disconnected: " + d.reason().getString());
                }
            }
            GameProtocols.Context context = new GameProtocols.Context() {
                @Override
                public boolean hasInfiniteMaterials() {
                    return false;
                }

                @Override
                public boolean canUseCommandBlocks() {
                    return false;
                }
            };
            outbound = GameProtocols.SERVERBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(ParityData.access), context);
            inbound = GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(ParityData.access));
            Thread reader = new Thread(this::play, name);
            reader.setDaemon(true);
            reader.start();
            Thread reminder = new Thread(this::remind, name + " reminder");
            reminder.setDaemon(true);
            reminder.start();
        }

        private void play() {
            try {
                while (true) {
                    Packet<?> p = read();
                    if (p instanceof ClientboundLoginPacket l) {
                        id = l.playerId();
                    } else if (p instanceof ClientboundAddEntityPacket a && a.getType() == EntityTypes.PLAYER) {
                        String who = NAMES.getOrDefault(a.getUUID(), "uuid?");
                        synchronized (data) {
                            seen.put(a.getId(), who);
                            data.remove(a.getId());
                        }
                        record("add " + who);
                    } else if (p instanceof ClientboundRemoveEntitiesPacket rm) {
                        rm.entityIds().forEach(e -> {
                            synchronized (data) {
                                String who = seen.remove(e);
                                data.remove(e);
                                if (who != null) record("remove " + who);
                            }
                        });
                    } else if (p instanceof ClientboundSetEntityDataPacket d) {
                        synchronized (data) {
                            Map<Integer, String> held = data.computeIfAbsent(d.id(), k -> new TreeMap<>());
                            for (SynchedEntityData.DataValue<?> v : d.packedItems()) {
                                for (int watched : WATCHED) {
                                    if (v.id() != watched) continue;
                                    String text = value(v.value());
                                    // Back at a default: no longer held.
                                    if (text.equals("0") || text.equals("false") || text.equals("[]") || text.equals("OptionalInt.empty")) {
                                        held.remove(watched);
                                    } else {
                                        held.put(watched, text);
                                    }
                                }
                            }
                        }
                    } else if (p instanceof ClientboundPlayerInfoUpdatePacket info) {
                        if (info.actions().contains(ClientboundPlayerInfoUpdatePacket.Action.UPDATE_HAT)) {
                            for (var e : info.entries()) {
                                record("hat " + NAMES.getOrDefault(e.profileId(), "uuid?") + " " + e.showHat()
                                    + (info.actions().contains(ClientboundPlayerInfoUpdatePacket.Action.ADD_PLAYER) ? " added" : ""));
                            }
                        }
                    } else if (p instanceof ClientboundSetScorePacket s) {
                        record("score " + s.owner() + " " + s.objectiveName() + " " + s.score());
                    } else if (p instanceof ClientboundSelectAdvancementsTabPacket t) {
                        record("select tab " + t.getTab());
                    } else if (p instanceof ClientboundRecipeBookAddPacket r) {
                        List<String> highlighted = new ArrayList<>();
                        for (var e : r.entries()) {
                            if (e.highlight()) highlighted.add(String.valueOf(e.contents().id().index()));
                        }
                        if (!r.replace() && !r.entries().isEmpty()) lastAdded = r.entries().get(0).contents().id().index();
                        record("recipe book " + (r.replace() ? "replaced" : "added") + " " + r.entries().size()
                            + " highlighted " + highlighted);
                    } else if (p instanceof ClientboundPongResponsePacket pong) {
                        record("pong " + pong.time());
                    } else if (p instanceof ClientboundSystemChatPacket c
                        && c.content().getContents() instanceof TranslatableContents t
                        && t.getKey().equals("commands.time.query.gametime")) {
                        synchronized (replyLock) {
                            syncs++;
                            replyLock.notifyAll();
                        }
                    } else if (p instanceof ClientboundSystemChatPacket) {
                        synchronized (replyLock) {
                            replies++;
                            replyLock.notifyAll();
                        }
                    } else if (p instanceof ClientboundKeepAlivePacket k) {
                        send(new ServerboundKeepAlivePacket(k.getId()));
                    } else if (p instanceof ClientboundPingPacket k) {
                        send(new ServerboundPongPacket(k.getId()));
                    } else if (p instanceof ClientboundPlayerPositionPacket t) {
                        position = PositionMoveRotation.calculateAbsolute(position, t.change(), t.relatives());
                        Vec3 at = position.position();
                        send(new ServerboundAcceptTeleportationPacket(t.id(), at.x, at.y, at.z, position.yRot(), position.xRot()));
                        if (!loaded) {
                            send(new ServerboundMovePlayerPacket.PosRot(at, position.yRot(), position.xRot(), false, false));
                            send(new ServerboundPlayerLoadedPacket());
                            loaded = true;
                        }
                    } else if (p instanceof ClientboundChunkBatchFinishedPacket) {
                        send(new ServerboundChunkBatchReceivedPacket(64.0F));
                    } else if (p instanceof ClientboundDisconnectPacket d) {
                        String reason = d.reason().getContents() instanceof TranslatableContents t ? t.getKey() : d.reason().getString();
                        record("disconnected " + reason);
                        socket.close();
                        return;
                    }
                }
            } catch (Exception e) {
                if (!socket.isClosed()) record("error play " + e);
            }
        }

        private Packet<?> read() throws Exception {
            byte[] payload = readFrame();
            ByteBuf buf = Unpooled.wrappedBuffer(payload);
            Packet<?> packet = inbound.codec().decode(buf);
            if (buf.readableBytes() > 0) {
                throw new IllegalStateException(packet.type().id() + " was larger than expected: " + buf.readableBytes() + " bytes left");
            }
            return packet;
        }

        @SuppressWarnings({"unchecked", "rawtypes"})
        synchronized void send(Packet<?> packet) throws Exception {
            ByteBuf body = Unpooled.buffer();
            ((ProtocolInfo) outbound).codec().encode(body, packet);
            byte[] raw = new byte[body.readableBytes()];
            body.readBytes(raw);
            ByteBuf frame = Unpooled.buffer();
            if (threshold >= 0) {
                ByteBuf inner = Unpooled.buffer();
                if (raw.length >= threshold) {
                    writeVarInt(inner, raw.length);
                    Deflater d = new Deflater();
                    d.setInput(raw);
                    d.finish();
                    byte[] tmp = new byte[raw.length + 64];
                    while (!d.finished()) inner.writeBytes(tmp, 0, d.deflate(tmp));
                    d.end();
                } else {
                    writeVarInt(inner, 0);
                    inner.writeBytes(raw);
                }
                writeVarInt(frame, inner.readableBytes());
                frame.writeBytes(inner);
            } else {
                writeVarInt(frame, raw.length);
                frame.writeBytes(raw);
            }
            byte[] bytes = new byte[frame.readableBytes()];
            frame.readBytes(bytes);
            out.write(bytes);
            out.flush();
        }

        private byte[] readFrame() throws Exception {
            byte[] frame = new byte[readVarInt(in)];
            in.readFully(frame);
            if (threshold < 0) return frame;
            ByteArrayInputStream header = new ByteArrayInputStream(frame);
            int dataLength = readVarInt(header);
            int skip = frame.length - header.available();
            if (dataLength == 0) return java.util.Arrays.copyOfRange(frame, skip, frame.length);
            Inflater inflater = new Inflater();
            inflater.setInput(frame, skip, frame.length - skip);
            byte[] raw = new byte[dataLength];
            int n = inflater.inflate(raw);
            inflater.end();
            if (n != dataLength) throw new IllegalStateException("decompressed " + n + " of " + dataLength + " bytes");
            return raw;
        }
    }

    private static int readVarInt(InputStream in) throws Exception {
        int value = 0;
        for (int shift = 0; shift < 35; shift += 7) {
            int b = in.read();
            if (b < 0) throw new java.io.EOFException();
            value |= (b & 0x7F) << shift;
            if ((b & 0x80) == 0) return value;
        }
        throw new IllegalStateException("VarInt too big");
    }

    private static void writeVarInt(ByteBuf buf, int value) {
        while ((value & ~0x7F) != 0) {
            buf.writeByte((value & 0x7F) | 0x80);
            value >>>= 7;
        }
        buf.writeByte(value);
    }
}
