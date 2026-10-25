import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.advancements.*;
import net.minecraft.advancements.triggers.CriteriaTriggers;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.*;
import net.minecraft.world.damagesource.*;
import net.minecraft.world.phys.Vec3;
import static org.mockito.Mockito.*;

/** Actual hurt triggers preserve the two damage values, blocking and entity roles. */
public final class HurtTriggerVectors {
    static final JsonArray rows = new JsonArray();
    static void observe(String trigger, String conditions, String mode, float original, float damage, boolean blocked) throws Exception {
        var criterion = new JsonObject(); criterion.addProperty("trigger", trigger);
        criterion.add("conditions", AdvancementVectors.json(conditions));
        var criteria = new JsonObject(); criteria.add("a", criterion);
        var input = new JsonObject(); input.add("criteria", criteria);
        input.add("rewards", AdvancementVectors.json("{\"experience\":1}"));
        var holder = new AdvancementHolder(AdvancementVectors.id("test:hurt"),
            Advancement.CODEC.parse(AdvancementVectors.ops, input).getOrThrow());
        var tree = new AdvancementTree(); tree.addAll(List.of(holder));
        var manager = mock(ServerAdvancementManager.class);
        doReturn(List.of(holder)).when(manager).getAllAdvancements();
        doReturn(tree).when(manager).tree(); doReturn(holder).when(manager).get(holder.id());
        var directory = Files.createTempDirectory("ferraloom-hurt-trigger");
        var progress = new PlayerAdvancements(mock(com.mojang.datafixers.DataFixer.class),
            mock(net.minecraft.server.players.PlayerList.class), manager,
            directory.resolve("missing.json"), AdvancementVectors.player);
        doReturn(progress).when(AdvancementVectors.player).getAdvancements();
        var types = AdvancementVectors.lookup.lookupOrThrow(Registries.DAMAGE_TYPE);
        var source = switch (mode) {
            case "player" -> new DamageSource(types.getOrThrow(DamageTypes.PLAYER_ATTACK), AdvancementVectors.player);
            case "mob" -> new DamageSource(types.getOrThrow(DamageTypes.MOB_ATTACK), AdvancementVectors.cow);
            case "indirect" -> new DamageSource(types.getOrThrow(DamageTypes.ARROW), AdvancementVectors.cow, AdvancementVectors.player);
            case "generic" -> new DamageSource(types.getOrThrow(DamageTypes.GENERIC));
            default -> throw new AssertionError(mode);
        };
        int[] experience = {0};
        doAnswer(call -> { experience[0] += call.<Integer>getArgument(0); return null; })
            .when(AdvancementVectors.player).giveExperiencePoints(anyInt());
        if (trigger.equals("entity_hurt_player")) {
            CriteriaTriggers.ENTITY_HURT_PLAYER.trigger(AdvancementVectors.player, source, original, damage, blocked);
        } else {
            CriteriaTriggers.PLAYER_HURT_ENTITY.trigger(AdvancementVectors.player, AdvancementVectors.cow, source, original, damage, blocked);
        }
        var row = new JsonObject(); row.add("input", input); row.addProperty("source", mode);
        row.addProperty("original", original); row.addProperty("damage", damage); row.addProperty("blocked", blocked);
        row.addProperty("damage_type", source.typeHolder().unwrapKey().orElseThrow().identifier().toString());
        row.addProperty("done", progress.getOrStartProgress(holder).isDone());
        row.addProperty("experience", experience[0]); rows.add(row);
        progress.clearTriggers(); Files.delete(directory);
    }
    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();
        doReturn(new Vec3(4, 64, 2)).when(AdvancementVectors.cow).position();
        doReturn(4.0).when(AdvancementVectors.cow).getX();
        observe("entity_hurt_player", "{}", "mob", 10, 4, true);
        String blocked = "{\"damage\":{\"dealt\":10,\"taken\":4,\"blocked\":true}}";
        observe("entity_hurt_player", blocked, "mob", 10, 4, true);
        observe("entity_hurt_player", blocked, "mob", 10, 4, false);
        String cause = "{\"damage\":{\"source_entity\":{\"entity_type\":\"cow\"}}}";
        observe("entity_hurt_player", cause, "mob", 10, 4, false);
        observe("entity_hurt_player", cause, "player", 10, 4, false);
        observe("entity_hurt_player", "{\"damage\":{\"type\":{\"is_direct\":true}}}", "generic", 10, 4, false);
        String victim = "\"entity\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"cow\",\"distance\":{\"absolute\":3}}}";
        String amount = "{" + victim + ",\"damage\":{\"dealt\":{\"min\":5},\"taken\":3,\"blocked\":false}}";
        observe("player_hurt_entity", amount, "player", 8, 3, false);
        observe("player_hurt_entity", amount, "player", 8, 4, false);
        observe("player_hurt_entity", "{\"entity\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"pig\"}}}", "player", 8, 3, false);
        String indirect = "{" + victim + ",\"damage\":{\"type\":{\"is_direct\":false,\"source_entity\":{\"entity_type\":\"player\"},\"direct_entity\":{\"entity_type\":\"cow\"}}}}";
        observe("player_hurt_entity", indirect, "indirect", 8, 3, false);
        observe("player_hurt_entity", indirect, "player", 8, 3, false);
        observe("player_hurt_entity", "{\"player\":{\"type\":\"entity_properties\",\"entity\":\"this\",\"predicate\":{\"entity_type\":\"cow\"}}}", "player", 8, 3, false);
        observe("player_hurt_entity", "{\"damage\":{\"dealt\":0.1,\"taken\":0.1}}", "player", 0.1F, 0.1F, false);
        try (var output = new GZIPOutputStream(Files.newOutputStream(Path.of(args[0])))) {
            output.write(AdvancementVectors.JSON.toJson(rows).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        }
        System.out.println("hurt_triggers: " + rows.size());
    }
}
