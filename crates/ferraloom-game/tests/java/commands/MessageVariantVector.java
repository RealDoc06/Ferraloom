import com.google.gson.GsonBuilder;
import com.mojang.brigadier.CommandDispatcher;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicReference;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.commands.arguments.MessageArgument;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.util.TaskChainer;
import static org.mockito.Mockito.*;

/** Exercises real MessageArgument resolution and OutgoingChatMessage selection. */
public final class MessageVariantVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var server = mock(MinecraftServer.class);
        doReturn(ChatDecorator.PLAIN).when(server).getChatDecorator();
        var variants = new LinkedHashMap<String,String>();
        var uuid = UUID.fromString("01234567-89ab-cdef-0123-456789abcdef");
        for (boolean signedContext : new boolean[]{false,true}) {
            var source = mock(CommandSourceStack.class);
            doReturn(server).when(source).getServer();
            doReturn(PermissionSet.ALL_PERMISSIONS).when(source).permissions();
            doReturn(TaskChainer.immediate(Runnable::run)).when(source).getChatMessageChainer();
            CommandSigningContext signing = signedContext
                ? new CommandSigningContext.SignedArguments(Map.of("message", PlayerChatMessage.unsigned(uuid,"hello")))
                : CommandSigningContext.ANONYMOUS;
            doReturn(signing).when(source).getSigningContext();
            var result = new AtomicReference<PlayerChatMessage>();
            var dispatcher = new CommandDispatcher<CommandSourceStack>();
            dispatcher.register(Commands.literal("say").then(Commands.argument("message", MessageArgument.message()).executes(c -> {
                MessageArgument.resolveChatMessage(c,"message",result::set); return 1;
            })));
            dispatcher.execute("say hello",source);
            variants.put(signedContext ? "present_unsigned_argument" : "absent_argument", OutgoingChatMessage.create(result.get()).getClass().getSimpleName());
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(variants)+"\n");
    }
}
