import com.google.gson.GsonBuilder;
import com.mojang.authlib.GameProfile;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.UUID;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.player.Player;
import static org.mockito.Mockito.*;

/** Calls the real Player#getDisplayName with only identity/accessor dependencies stubbed. */
public final class DisplayNameVector {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var player = mock(Player.class, CALLS_REAL_METHODS);
        var uuid = UUID.fromString("01234567-89ab-cdef-0123-456789abcdef");
        doReturn(new GameProfile(uuid, "Doc")).when(player).getGameProfile();
        doReturn(Component.literal("Doc")).when(player).getName();
        doReturn(uuid).when(player).getUUID();
        doReturn(EntityTypes.PLAYER).when(player).getType();
        doReturn(null).when(player).getTeam();
        var component = player.getDisplayName();
        var json = ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, component).getOrThrow();
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(json) + "\n");
        var input=Component.literal("Styled").append(" tail").withStyle(style->style.withColor(net.minecraft.ChatFormatting.RED).withBold(true).withClickEvent(new ClickEvent.RunCommand("/old")).withInsertion("old"));
        var decorate=Player.class.getDeclaredMethod("decorateDisplayNameComponent",MutableComponent.class);decorate.setAccessible(true);
        var decorated=(Component)decorate.invoke(player,input);
        var decoratedJson=ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,decorated).getOrThrow();
        Files.writeString(Path.of(args[1]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(decoratedJson)+"\n");
    }
}
