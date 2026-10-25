import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.advancements.*;
import net.minecraft.advancements.triggers.CriteriaTriggers;
import net.minecraft.server.*;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Actual FALL_FROM_HEIGHT trigger observes the supplied start and current endpoint. */
public final class FallHeightVectors {
    static final JsonArray rows = new JsonArray();
    static JsonArray vector(Vec3 value) {
        var row = new JsonArray(); row.add(value.x); row.add(value.y); row.add(value.z); return row;
    }
    static void observe(String conditions, Vec3 start, Vec3 end) throws Exception {
        var criterion = new JsonObject(); criterion.addProperty("trigger", "fall_from_height");
        criterion.add("conditions", AdvancementVectors.json(conditions));
        var criteria = new JsonObject(); criteria.add("a", criterion);
        var input = new JsonObject(); input.add("criteria", criteria);
        input.add("rewards", AdvancementVectors.json("{\"experience\":1}"));
        var holder = new AdvancementHolder(AdvancementVectors.id("test:fall"),
            Advancement.CODEC.parse(AdvancementVectors.ops, input).getOrThrow());
        var tree = new AdvancementTree(); tree.addAll(List.of(holder));
        var manager = mock(ServerAdvancementManager.class);
        doReturn(List.of(holder)).when(manager).getAllAdvancements();
        doReturn(tree).when(manager).tree();
        doReturn(holder).when(manager).get(holder.id());
        var directory = Files.createTempDirectory("ferraloom-fall-height");
        var progress = new PlayerAdvancements(mock(com.mojang.datafixers.DataFixer.class),
            mock(net.minecraft.server.players.PlayerList.class), manager,
            directory.resolve("missing.json"), AdvancementVectors.player);
        doReturn(progress).when(AdvancementVectors.player).getAdvancements();
        doReturn(end).when(AdvancementVectors.player).position();
        doReturn(end.x).when(AdvancementVectors.player).getX();
        doReturn(end.y).when(AdvancementVectors.player).getY();
        doReturn(end.z).when(AdvancementVectors.player).getZ();
        int[] experience = {0};
        doAnswer(call -> { experience[0] += call.<Integer>getArgument(0); return null; })
            .when(AdvancementVectors.player).giveExperiencePoints(anyInt());
        CriteriaTriggers.FALL_FROM_HEIGHT.trigger(AdvancementVectors.player, start);
        var row = new JsonObject(); row.add("input", input);
        row.add("start", vector(start)); row.add("end", vector(end));
        row.addProperty("done", progress.getOrStartProgress(holder).isDone());
        row.addProperty("experience", experience[0]); rows.add(row);
        progress.clearTriggers(); Files.delete(directory);
    }
    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();
        var start = new Vec3(1, 74, 2); var end = new Vec3(1, 64, 2);
        observe("{}", start, end);
        observe("{\"distance\":{\"y\":10}}", start, end);
        observe("{\"distance\":{\"y\":11}}", start, end);
        String location = "{\"start_position\":{\"position\":{\"y\":{\"min\":70}}},\"distance\":{\"y\":10}}";
        observe(location, start, end);
        observe(location, new Vec3(1, 64, 2), new Vec3(1, 74, 2));
        observe("{\"distance\":{\"absolute\":13,\"horizontal\":5,\"y\":12}}", new Vec3(1, 76, 7), end);
        observe("{\"distance\":{\"y\":-10}}", start, end);
        observe("{\"player\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"cow\"}}}", start, end);
        observe("{\"distance\":{\"y\":16777216}}", new Vec3(1, 16777281, 2), end);
        try (var output = new GZIPOutputStream(Files.newOutputStream(Path.of(args[0])))) {
            output.write(AdvancementVectors.JSON.toJson(rows).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        }
        System.out.println("fall_from_height: " + rows.size());
    }
}
