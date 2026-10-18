import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.advancements.*;
import net.minecraft.network.chat.*;
import net.minecraft.server.*;
import net.minecraft.world.level.gamerules.GameRules;
import static org.mockito.Mockito.*;

/** Actual PlayerAdvancements award reads its rule/name after actual reward XP. */
public final class AnnouncementVectors {
    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();
        var observations = new JsonArray();
        for (boolean before : new boolean[]{false, true}) for (boolean after : new boolean[]{false, true}) {
            var input = AdvancementVectors.json("{\"display\":{\"icon\":{\"id\":\"stone\"},\"title\":\"Reward\",\"description\":\"Description\",\"background\":\"test:background\"},\"criteria\":{\"a\":{\"trigger\":\"tick\"}},\"rewards\":{\"experience\":3}}");
            var holder = new AdvancementHolder(AdvancementVectors.id("test:announcement"), Advancement.CODEC.parse(AdvancementVectors.ops, input).getOrThrow());
            var tree = new AdvancementTree(); tree.addAll(List.of(holder));
            var manager = mock(ServerAdvancementManager.class);
            doReturn(List.of(holder)).when(manager).getAllAdvancements(); doReturn(tree).when(manager).tree();
            doReturn(holder).when(manager).get(holder.id());
            var rules = new GameRules(net.minecraft.world.flag.FeatureFlags.DEFAULT_FLAGS);
            rules.set(GameRules.SHOW_ADVANCEMENT_MESSAGES, before, null);
            doReturn(rules).when(AdvancementVectors.level).getGameRules();
            doReturn(Component.literal("Before")).when(AdvancementVectors.player).getDisplayName();
            doAnswer(call -> {
                rules.set(GameRules.SHOW_ADVANCEMENT_MESSAGES, after, null);
                doReturn(Component.literal("After")).when(AdvancementVectors.player).getDisplayName();
                return null;
            }).when(AdvancementVectors.player).giveExperiencePoints(anyInt());
            var chats = new JsonArray();
            var players = mock(net.minecraft.server.players.PlayerList.class);
            doAnswer(call -> {
                chats.add(ComponentSerialization.CODEC.encodeStart(AdvancementVectors.ops, (Component)call.getArgument(0)).getOrThrow());
                return null;
            }).when(players).broadcastSystemMessage(any(Component.class), eq(false));
            var directory = Files.createTempDirectory("ferraloom-announcement");
            var progress = new PlayerAdvancements(mock(com.mojang.datafixers.DataFixer.class), players, manager, directory.resolve("missing.json"), AdvancementVectors.player);
            progress.award(holder, "a");
            var row = new JsonObject(); row.addProperty("before", before); row.addProperty("after", after);
            row.add("input", input); row.add("chats", chats); observations.add(row);
            progress.clearTriggers(); Files.delete(directory);
        }
        try (var output = new GZIPOutputStream(Files.newOutputStream(Path.of(args[0])))) {
            output.write(AdvancementVectors.JSON.toJson(observations).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        }
        System.out.println("announcement_application_time: " + observations.size());
    }
}
