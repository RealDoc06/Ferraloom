import com.google.gson.GsonBuilder;
import java.nio.file.*;
import java.time.Instant;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;

/** Uses native chat decorations and the server trust expiry boundary. */
public final class CommandChatLogVector {
    record Vector(String command, boolean signed, long timestamp, long nowNanos, String line) {}
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var registry = VanillaRegistries.createWorldLookup().lookupOrThrow(Registries.CHAT_TYPE);
        var vectors = new ArrayList<Vector>();
        long timestamp = 1000;
        for (var command : List.of("say", "me")) for (boolean signed : new boolean[]{false,true}) {
            for (long nowNanos : new long[]{1_000_000_000L, 300_999_999_999L, 301_000_000_000L, 301_000_000_001L}) {
                var type = registry.getOrThrow(command.equals("say") ? ChatType.SAY_COMMAND : ChatType.EMOTE_COMMAND);
                var bound = new ChatType.Bound(type, Component.literal("Doc"), Optional.empty());
                var message = new PlayerChatMessage(SignedMessageLink.unsigned(new UUID(1,2)), signed ? new MessageSignature(new byte[256]) : null,
                    new SignedMessageBody("hello \uD83D\uDC4B", Instant.ofEpochMilli(timestamp), 0, LastSeenMessages.EMPTY), null, FilterMask.PASS_THROUGH);
                var now = Instant.ofEpochSecond(0, nowNanos);
                boolean trusted = message.hasSignature() && !message.hasExpiredServer(now);
                var line = (trusted ? "" : "[Not Secure] ") + bound.decorate(message.decoratedContent()).getString();
                vectors.add(new Vector(command,signed,timestamp,nowNanos,line));
            }
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(vectors)+"\n");
    }
}
