package dev.horizonlab.parity;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.zip.Deflater;
import java.util.zip.Inflater;
import net.minecraft.SharedConstants;
import net.minecraft.core.UUIDUtil;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.TagParser;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentContents;
import net.minecraft.network.chat.Style;
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
import net.minecraft.network.protocol.configuration.ClientboundFinishConfigurationPacket;
import net.minecraft.network.protocol.configuration.ClientboundSelectKnownPacks;
import net.minecraft.network.protocol.configuration.ConfigurationProtocols;
import net.minecraft.network.protocol.configuration.ServerboundFinishConfigurationPacket;
import net.minecraft.network.protocol.configuration.ServerboundSelectKnownPacks;
import net.minecraft.network.protocol.game.ClientboundChunkBatchFinishedPacket;
import net.minecraft.network.protocol.game.ClientboundDisguisedChatPacket;
import net.minecraft.network.protocol.game.ClientboundPlayerChatPacket;
import net.minecraft.network.protocol.game.ClientboundPlayerPositionPacket;
import net.minecraft.network.protocol.game.ClientboundRespawnPacket;
import net.minecraft.network.protocol.game.ClientboundStartConfigurationPacket;
import net.minecraft.network.protocol.game.ClientboundSystemChatPacket;
import net.minecraft.network.protocol.game.GameProtocols;
import net.minecraft.network.protocol.game.ServerboundAcceptTeleportationPacket;
import net.minecraft.network.protocol.game.ServerboundChatCommandPacket;
import net.minecraft.network.protocol.game.ServerboundChunkBatchReceivedPacket;
import net.minecraft.network.protocol.game.ServerboundClientTickEndPacket;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import net.minecraft.network.protocol.game.ServerboundPlayerLoadedPacket;
import net.minecraft.network.protocol.handshake.ClientIntent;
import net.minecraft.network.protocol.handshake.ClientIntentionPacket;
import net.minecraft.network.protocol.handshake.HandshakeProtocols;
import net.minecraft.network.protocol.login.ClientboundLoginCompressionPacket;
import net.minecraft.network.protocol.login.ClientboundLoginDisconnectPacket;
import net.minecraft.network.protocol.login.ClientboundLoginFinishedPacket;
import net.minecraft.network.protocol.login.LoginProtocols;
import net.minecraft.network.protocol.login.ServerboundHelloPacket;
import net.minecraft.network.protocol.login.ServerboundLoginAcknowledgedPacket;
import net.minecraft.server.level.ClientInformation;
import net.minecraft.world.entity.PositionMoveRotation;
import net.minecraft.world.phys.Vec3;

/**
 * A player made of vanilla's protocol code that runs a command script on a
 * dedicated server, for tools/test_command_exec_parity.py. It joins in offline
 * mode (an operator through ops.json), freezes the game (`tick freeze`: like
 * the old in-process oracle, nothing ticks between two commands but the
 * player) and runs tools/command_exec_corpus.py's operations:
 *
 * <ul>
 *   <li>{@code cmd}: the command is sent as ServerboundChatCommandPacket,
 *     wrapped as {@code execute store success storage parity:exec s byte 1
 *     store result storage parity:exec r int 1 run COMMAND} so that its
 *     CommandResultCallback can be read back ({@code data get storage}). A
 *     command that does not parse is sent again unwrapped for its messages:
 *     the wrapper would change its error's context line (parsing has no side
 *     effects). Recorded: every system message that is not an overlay and
 *     every player or disguised chat message, decorated by its chat type, as
 *     styled spans (as VanillaCommandExecOracle did), and the last result.</li>
 *   <li>{@code tick N}: {@code tick step N}, then waits until the game time
 *     moved N ticks.</li>
 *   <li>{@code settle}: waits until the 5x5 chunks around the player are
 *     loaded and entity-ticking ({@code execute if loaded}), then 5 ticks.</li>
 *   <li>{@code portal}: with {@code at}, teleports into the portal there;
 *     without, steps out of the portal the player stands in and back. Then
 *     waits for the dimension change.</li>
 * </ul>
 *
 * After each batch of commands a {@code tellraw} marker comes back once the
 * server ran everything sent before it. With PARITY_TRACE set, each operation
 * and teleport is logged to stderr.
 *
 *   CommandExecClient HOST PORT NAME SCRIPT.jsonl OUT.jsonl
 */
public final class CommandExecClient {
    private static final String STORAGE = "parity:exec";
    private static final String WRAP = "execute store success storage " + STORAGE + " s byte 1 store result storage "
        + STORAGE + " r int 1 run ";
    private static final long TIMEOUT_NANOS = 600_000_000_000L;

    public static void main(String[] args) throws Exception {
        ParityData.init();
        String host = args[0];
        int port = Integer.parseInt(args[1]);
        Path script = Path.of(args[3]);
        Path output = Path.of(args[4]);
        Player player = new Player(args[2]);
        player.join(host, port);
        player.awaitPosition();
        player.batch(List.of("tick freeze"));
        StringBuilder out = new StringBuilder();
        Gson gson = new Gson();
        for (String line : Files.readAllLines(script, StandardCharsets.UTF_8)) {
            if (line.isBlank()) continue;
            JsonObject op = JsonParser.parseString(line).getAsJsonObject();
            if (System.getenv("PARITY_TRACE") != null) System.err.println("op " + line);
            boolean record = !op.has("record") || op.get("record").getAsBoolean();
            switch (op.get("op").getAsString()) {
                case "cmd" -> {
                    String command = op.get("cmd").getAsString();
                    if (record) {
                        out.append(gson.toJson(player.recorded(command))).append('\n');
                    } else {
                        player.batch(List.of(command.substring(1)));
                    }
                }
                case "tick" -> {
                    int n = op.has("n") ? op.get("n").getAsInt() : 1;
                    boolean recordTick = op.has("record") && op.get("record").getAsBoolean();
                    List<Component> messages = player.tick(n, recordTick);
                    if (recordTick) {
                        JsonObject r = new JsonObject();
                        r.addProperty("cmd", "#tick");
                        r.add("messages", spans(messages));
                        r.add("results", new JsonArray());
                        out.append(gson.toJson(r)).append('\n');
                    }
                }
                case "settle" -> player.settle();
                case "portal" -> player.portal(op.has("at") ? op.get("at").getAsString() : null);
                default -> throw new IllegalArgumentException(line);
            }
            if (player.failure != null) {
                throw new IllegalStateException("after " + line + ": " + player.failure);
            }
        }
        Files.writeString(output, out.toString(), StandardCharsets.UTF_8);
        player.close();
        System.exit(0);
    }

    static JsonArray spans(List<Component> messages) {
        JsonArray all = new JsonArray();
        for (Component message : messages) all.add(spans(message));
        return all;
    }

    /** The message's text as spans of one resolved style, merged where neighbours share it (VanillaCommandExecOracle). */
    static JsonArray spans(Component message) {
        JsonArray out = new JsonArray();
        message.visit((style, text) -> {
            if (text.isEmpty()) return Optional.empty();
            JsonArray key = styleKey(style);
            if (!out.isEmpty()) {
                JsonArray last = out.get(out.size() - 1).getAsJsonArray();
                if (last.get(1).equals(key)) {
                    last.set(0, new com.google.gson.JsonPrimitive(last.get(0).getAsString() + text));
                    return Optional.empty();
                }
            }
            JsonArray span = new JsonArray();
            span.add(text);
            span.add(key);
            out.add(span);
            return Optional.empty();
        }, Style.EMPTY);
        return out;
    }

    static JsonArray styleKey(Style style) {
        JsonArray key = new JsonArray();
        if (style.getColor() == null) key.add(com.google.gson.JsonNull.INSTANCE);
        else key.add(style.getColor().getValue() & 0xFFFFFF);
        key.add(style.isBold());
        key.add(style.isItalic());
        key.add(style.isUnderlined());
        key.add(style.isStrikethrough());
        key.add(style.isObfuscated());
        return key;
    }

    /** A coordinate as a command reads it back exactly (no exponent). */
    static String plain(double value) {
        return java.math.BigDecimal.valueOf(value).toPlainString();
    }

    /** Whether the component, or any part of it, is translatable with this key. */
    static boolean hasKey(Component component, String key) {
        ComponentContents contents = component.getContents();
        if (contents instanceof TranslatableContents t) {
            if (t.getKey().equals(key)) return true;
            for (Object arg : t.getArgs()) {
                if (arg instanceof Component c && hasKey(c, key)) return true;
            }
        }
        for (Component sibling : component.getSiblings()) {
            if (hasKey(sibling, key)) return true;
        }
        return false;
    }

    /** The first argument of a translatable reply with this key, as text, or null. */
    static String argument(List<Component> messages, String key, int index) {
        for (Component m : messages) {
            if (m.getContents() instanceof TranslatableContents t && t.getKey().equals(key) && t.getArgs().length > index) {
                Object arg = t.getArgs()[index];
                return arg instanceof Component c ? c.getString() : String.valueOf(arg);
            }
        }
        return null;
    }

    /** One connection: login and configuration in the caller's thread, then play on its own. */
    private static final class Player {
        final String name;
        private Socket socket;
        private DataInputStream in;
        private OutputStream out;
        private int threshold = -1;
        private ProtocolInfo<?> inbound;
        private ProtocolInfo<?> outbound;
        private volatile PositionMoveRotation position = new PositionMoveRotation(Vec3.ZERO, Vec3.ZERO, 0, 0);
        /** Whether the next teleport is the first after joining or a respawn (answered as LocalPlayer does). */
        private boolean awaitingSpawn = true;
        /** Guarded by `inbox`: messages received since the last batch began, the teleports and dimension changes seen. */
        private final List<Component> inbox = new ArrayList<>();
        private int teleports;
        private int respawns;
        private int markers;
        volatile String failure;

        Player(String name) {
            this.name = name;
        }

        /**
         * Sends the commands (without their slash), each followed by a marker, and returns the messages each one
         * received before its marker came back. The first one's also holds whatever arrived since the last batch.
         */
        List<List<Component>> batch(List<String> commands) throws Exception {
            List<String> ids = new ArrayList<>();
            for (String command : commands) {
                String id = "#parity " + (++markers);
                ids.add(id);
                send(new ServerboundChatCommandPacket(command));
                send(new ServerboundChatCommandPacket("tellraw @s \"" + id + "\""));
            }
            long until = System.nanoTime() + TIMEOUT_NANOS;
            String last = ids.getLast();
            synchronized (inbox) {
                while (inbox.stream().noneMatch(m -> m.getString().equals(last))) {
                    long left = (until - System.nanoTime()) / 1_000_000L;
                    if (left <= 0 || failure != null) {
                        throw new IllegalStateException("no reply to " + commands + (failure != null ? ": " + failure : ""));
                    }
                    inbox.wait(Math.min(left, 1000));
                }
                List<List<Component>> segments = new ArrayList<>();
                List<Component> current = new ArrayList<>();
                while (segments.size() < ids.size()) {
                    Component m = inbox.removeFirst();
                    if (m.getString().equals(ids.get(segments.size()))) {
                        segments.add(current);
                        current = new ArrayList<>();
                    } else {
                        current.add(m);
                    }
                }
                return segments;
            }
        }

        /** A recorded command: its messages and the result its callback was given (none if it was not called). */
        JsonObject recorded(String command) throws Exception {
            String bare = command.substring(1);
            // What arrived before the command is not its own (the oracle cleared its messages too): the first
            // segment, the storage reset's, is dropped.
            List<List<Component>> replies = batch(List.of("data merge storage " + STORAGE + " {s:2b,r:0}", WRAP + bare,
                "data get storage " + STORAGE));
            List<Component> messages = replies.get(1);
            JsonArray results = new JsonArray();
            String stored = argument(replies.get(2), "commands.data.storage.query", 1);
            CompoundTag tag = null;
            try {
                tag = stored == null ? null : TagParser.parseCompoundFully(stored);
            } catch (Exception e) {
                // Recorded as unreadable below.
            }
            if (tag == null || !tag.contains("s") || !tag.contains("r")) {
                JsonArray r = new JsonArray();
                r.add("unreadable storage");
                r.add(replies.get(2).stream().map(Component::getString).toList().toString());
                results.add(r);
            } else if (tag.getByteOr("s", (byte) 2) != 2) {
                JsonArray r = new JsonArray();
                r.add(tag.getByteOr("s", (byte) 0) == 1);
                r.add(tag.getIntOr("r", 0));
                results.add(r);
            } else if (messages.stream().anyMatch(m -> hasKey(m, "command.context.here"))) {
                // It did not parse: its own error, from the command as typed.
                messages = batch(List.of(bare)).get(0);
            }
            JsonObject record = new JsonObject();
            record.addProperty("cmd", command);
            record.add("messages", spans(messages));
            record.add("results", results);
            return record;
        }

        long gameTime() throws Exception {
            String time = argument(batch(List.of("time query gametime")).get(0), "commands.time.query.gametime", 0);
            if (time == null) throw new IllegalStateException("no game time");
            return Long.parseLong(time);
        }

        /**
         * Steps n ticks of the frozen game (or waits for them while it runs). Recorded, it returns the messages
         * that arrived meanwhile but its own replies, after a pause in which answers computed off-thread (profile
         * lookups) reach the server's queue, as the oracle paused.
         */
        List<Component> tick(int n, boolean record) throws Exception {
            List<Component> messages = new ArrayList<>();
            if (record) {
                synchronized (inbox) {
                    inbox.clear();
                }
                Thread.sleep(300);
                messages.addAll(withoutBookkeeping(batch(List.of("time query gametime")).get(0)));
            }
            long until = gameTime() + n;
            List<List<Component>> step = batch(List.of("tick step " + n, "time query gametime"));
            messages.addAll(withoutBookkeeping(step.get(0)));
            messages.addAll(withoutBookkeeping(step.get(1)));
            long deadline = System.nanoTime() + TIMEOUT_NANOS;
            while (true) {
                List<Component> poll = batch(List.of("time query gametime")).get(0);
                String time = argument(poll, "commands.time.query.gametime", 0);
                messages.addAll(withoutBookkeeping(poll));
                if (time != null && Long.parseLong(time) >= until) return messages;
                if (System.nanoTime() > deadline) throw new IllegalStateException("the game time did not reach " + until);
                Thread.sleep(20);
            }
        }

        private static List<Component> withoutBookkeeping(List<Component> messages) {
            return messages.stream().filter(m -> !hasKey(m, "commands.time.query.gametime") && !hasKey(m, "commands.tick.step.success")
                && !hasKey(m, "commands.tick.step.fail")).toList();
        }

        /** Waits for the chunks around the player to be loaded and entity-ticking, then 5 ticks. */
        void settle() throws Exception {
            List<String> checks = new ArrayList<>();
            for (int dz = -2; dz <= 2; dz++) {
                for (int dx = -2; dx <= 2; dx++) {
                    checks.add("execute if loaded ~" + 16 * dx + " ~ ~" + 16 * dz);
                }
            }
            long deadline = System.nanoTime() + TIMEOUT_NANOS;
            while (true) {
                List<List<Component>> replies = batch(checks);
                if (replies.stream().allMatch(r -> r.stream().anyMatch(m -> hasKey(m, "commands.execute.conditional.pass")))) break;
                if (System.nanoTime() > deadline) throw new IllegalStateException("the chunks around the player did not load");
                Thread.sleep(200);
            }
            tick(5, false);
        }

        /**
         * Through a nether portal: into the one at `at`, or out of the one the player stands in and back after its
         * portal cooldown (10 ticks); then waits for the dimension change and its teleport.
         */
        void portal(String at) throws Exception {
            int before;
            synchronized (inbox) {
                before = respawns;
            }
            if (at == null) {
                Vec3 p = position.position();
                String back = "tp @s " + plain(p.x) + " " + plain(p.y) + " " + plain(p.z);
                batch(List.of("tp @s ~ ~3 ~"));
                tick(20, false);
                batch(List.of(back));
            } else {
                batch(List.of("tp @s " + at));
            }
            long until = System.nanoTime() + TIMEOUT_NANOS;
            synchronized (inbox) {
                while (respawns == before || awaitingSpawn) {
                    long left = (until - System.nanoTime()) / 1_000_000L;
                    if (left <= 0 || failure != null) {
                        throw new IllegalStateException("the portal did not take the player" + (failure != null ? ": " + failure : ""));
                    }
                    inbox.wait(Math.min(left, 1000));
                }
            }
        }

        void awaitPosition() throws Exception {
            long until = System.nanoTime() + TIMEOUT_NANOS;
            synchronized (inbox) {
                while (teleports == 0) {
                    long left = (until - System.nanoTime()) / 1_000_000L;
                    if (left <= 0 || failure != null) throw new IllegalStateException("never placed: " + failure);
                    inbox.wait(Math.min(left, 1000));
                }
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
            send(new ServerboundClientInformationPacket(ClientInformation.createDefault()));
            while (true) {
                Packet<?> p = read();
                if (p instanceof ClientboundSelectKnownPacks k) {
                    send(new ServerboundSelectKnownPacks(k.knownPacks()));
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
                    return true;
                }

                @Override
                public boolean canUseCommandBlocks() {
                    return true;
                }
            };
            outbound = GameProtocols.SERVERBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(ParityData.access), context);
            inbound = GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(ParityData.access));
            Thread reader = new Thread(this::play, name);
            reader.setDaemon(true);
            reader.start();
        }

        private void receive(Component message) {
            synchronized (inbox) {
                inbox.add(message);
                inbox.notifyAll();
            }
        }

        private void play() {
            try {
                while (true) {
                    Packet<?> p = read();
                    if (p instanceof ClientboundSystemChatPacket c) {
                        if (!c.overlay()) receive(c.content());
                    } else if (p instanceof ClientboundPlayerChatPacket c) {
                        receive(c.chatType().decorate(c.unsignedContent().orElseGet(() -> Component.literal(c.body().content()))));
                    } else if (p instanceof ClientboundDisguisedChatPacket c) {
                        receive(c.chatType().decorate(c.message()));
                    } else if (p instanceof ClientboundKeepAlivePacket k) {
                        send(new ServerboundKeepAlivePacket(k.getId()));
                    } else if (p instanceof ClientboundPingPacket k) {
                        send(new ServerboundPongPacket(k.getId()));
                    } else if (p instanceof ClientboundRespawnPacket) {
                        synchronized (inbox) {
                            respawns++;
                            awaitingSpawn = true;
                            inbox.notifyAll();
                        }
                    } else if (p instanceof ClientboundPlayerPositionPacket t) {
                        if (System.getenv("PARITY_TRACE") != null) System.err.println("teleport " + t);
                        position = PositionMoveRotation.calculateAbsolute(position, t.change(), t.relatives());
                        Vec3 at = position.position();
                        send(new ServerboundAcceptTeleportationPacket(t.id(), at.x, at.y, at.z, position.yRot(), position.xRot()));
                        synchronized (inbox) {
                            // After joining and each respawn, LocalPlayer reports where it is and that its level loaded.
                            if (awaitingSpawn) {
                                // A client tick of its own: the server takes one position per tick (receivedPositionThisTick).
                                send(new ServerboundMovePlayerPacket.PosRot(at, position.yRot(), position.xRot(), false, false));
                                send(ServerboundClientTickEndPacket.INSTANCE);
                                send(new ServerboundPlayerLoadedPacket());
                                awaitingSpawn = false;
                            }
                            teleports++;
                            inbox.notifyAll();
                        }
                    } else if (p instanceof ClientboundChunkBatchFinishedPacket) {
                        send(new ServerboundChunkBatchReceivedPacket(64.0F));
                    } else if (p instanceof ClientboundStartConfigurationPacket) {
                        fail("the server started a new configuration");
                        return;
                    } else if (p instanceof ClientboundDisconnectPacket d) {
                        fail("disconnected: " + d.reason().getString());
                        return;
                    }
                }
            } catch (Exception e) {
                if (!socket.isClosed()) fail("play: " + e);
            }
        }

        private void fail(String why) {
            synchronized (inbox) {
                failure = why;
                inbox.notifyAll();
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
