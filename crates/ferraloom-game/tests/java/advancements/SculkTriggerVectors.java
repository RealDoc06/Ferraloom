import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.advancements.*;
import net.minecraft.advancements.triggers.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.*;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Actual sculk-related trigger calls; producer eligibility is deliberately external. */
public final class SculkTriggerVectors {
    static final JsonArray rows = new JsonArray();

    static void observe(String trigger, String conditions, boolean indirect) throws Exception {
        var criterion = new JsonObject(); criterion.addProperty("trigger", trigger);
        criterion.add("conditions", AdvancementVectors.json(conditions));
        var input = new JsonObject();
        var criteria = new JsonObject(); criteria.add("a", criterion);
        input.add("criteria", criteria);
        input.add("rewards", AdvancementVectors.json("{\"experience\":1}"));
        var holder = new AdvancementHolder(AdvancementVectors.id("test:sculk"),
            Advancement.CODEC.parse(AdvancementVectors.ops, input).getOrThrow());
        var tree = new AdvancementTree(); tree.addAll(List.of(holder));
        var manager = mock(ServerAdvancementManager.class);
        doReturn(List.of(holder)).when(manager).getAllAdvancements();
        doReturn(tree).when(manager).tree();
        doReturn(holder).when(manager).get(holder.id());
        var directory = Files.createTempDirectory("ferraloom-sculk-trigger");
        var progress = new PlayerAdvancements(mock(com.mojang.datafixers.DataFixer.class),
            mock(net.minecraft.server.players.PlayerList.class), manager,
            directory.resolve("missing.json"), AdvancementVectors.player);
        doReturn(progress).when(AdvancementVectors.player).getAdvancements();
        int[] experience = {0};
        doAnswer(call -> { experience[0] += call.<Integer>getArgument(0); return null; })
            .when(AdvancementVectors.player).giveExperiencePoints(anyInt());
        var types = AdvancementVectors.lookup.lookupOrThrow(Registries.DAMAGE_TYPE);
        var source = indirect
            ? new DamageSource(types.getOrThrow(DamageTypes.ARROW), AdvancementVectors.cow, AdvancementVectors.player)
            : new DamageSource(types.getOrThrow(DamageTypes.PLAYER_ATTACK), AdvancementVectors.player);
        if (trigger.equals("avoid_vibration")) {
            CriteriaTriggers.AVOID_VIBRATION.trigger(AdvancementVectors.player);
        } else {
            CriteriaTriggers.KILL_MOB_NEAR_SCULK_CATALYST.trigger(AdvancementVectors.player, AdvancementVectors.cow, source);
        }
        var row = new JsonObject(); row.add("input", input);
        row.addProperty("indirect", indirect);
        row.addProperty("damage_type", source.typeHolder().unwrapKey().orElseThrow().identifier().toString());
        row.addProperty("is_direct", source.isDirect());
        row.addProperty("done", progress.getOrStartProgress(holder).isDone());
        row.addProperty("experience", experience[0]); rows.add(row);
        progress.clearTriggers(); Files.delete(directory);
    }

    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();
        // A displaced victim checks that entity context uses the player's
        // origin and preserves the actual victim handle.
        doReturn(new Vec3(4, 64, 2)).when(AdvancementVectors.cow).position();
        doReturn(4.0).when(AdvancementVectors.cow).getX();
        observe("avoid_vibration", "{}", false);
        observe("avoid_vibration", "{\"player\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"player\"}}}", false);
        observe("avoid_vibration", "{\"player\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"cow\"}}}", false);
        String victim = "\"entity\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"cow\",\"distance\":{\"absolute\":3}}}";
        String direct = "{" + victim + ",\"killing_blow\":{\"is_direct\":true,\"source_entity\":{\"entity_type\":\"player\"},\"direct_entity\":{\"entity_type\":\"player\"}}}";
        String indirect = "{" + victim + ",\"killing_blow\":{\"is_direct\":false,\"source_entity\":{\"entity_type\":\"player\"},\"direct_entity\":{\"entity_type\":\"cow\"}}}";
        observe("kill_mob_near_sculk_catalyst", "{}", false);
        observe("kill_mob_near_sculk_catalyst", direct, false);
        observe("kill_mob_near_sculk_catalyst", direct, true);
        observe("kill_mob_near_sculk_catalyst", indirect, true);
        observe("kill_mob_near_sculk_catalyst", "{\"entity\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}}}", false);
        observe("kill_mob_near_sculk_catalyst", "{\"player\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"cow\"}}}", false);
        try (var output = new GZIPOutputStream(Files.newOutputStream(Path.of(args[0])))) {
            output.write(AdvancementVectors.JSON.toJson(rows).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        }
        System.out.println("sculk_triggers: " + rows.size());
    }
}
