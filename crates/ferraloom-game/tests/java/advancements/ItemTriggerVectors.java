import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import java.util.zip.GZIPOutputStream;
import net.minecraft.advancements.*;
import net.minecraft.advancements.triggers.*;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.DataComponents;
import net.minecraft.network.chat.Component;
import net.minecraft.server.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import static org.mockito.Mockito.*;

/** Actual item trigger calls and synchronous reward reads of the original stack. */
public final class ItemTriggerVectors {
    static final JsonArray rows = new JsonArray();

    static void observe(String trigger, String conditions, ItemStack stack, BlockState state, int newDamage) throws Exception {
        var criterion = new JsonObject();
        criterion.addProperty("trigger", trigger);
        criterion.add("conditions", AdvancementVectors.json(conditions));
        var input = new JsonObject();
        var criteria = new JsonObject(); criteria.add("a", criterion);
        input.add("criteria", criteria);
        input.add("rewards", AdvancementVectors.json("{\"experience\":1}"));
        var holder = new AdvancementHolder(AdvancementVectors.id("test:item"),
            Advancement.CODEC.parse(AdvancementVectors.ops, input).getOrThrow());
        var tree = new AdvancementTree(); tree.addAll(List.of(holder));
        var manager = mock(ServerAdvancementManager.class);
        doReturn(List.of(holder)).when(manager).getAllAdvancements();
        doReturn(tree).when(manager).tree();
        doReturn(holder).when(manager).get(holder.id());
        var directory = Files.createTempDirectory("ferraloom-item-trigger");
        var progress = new PlayerAdvancements(mock(com.mojang.datafixers.DataFixer.class),
            mock(net.minecraft.server.players.PlayerList.class), manager,
            directory.resolve("missing.json"), AdvancementVectors.player);
        doReturn(progress).when(AdvancementVectors.player).getAdvancements();
        var reads = new JsonArray();
        doAnswer(call -> {
            var read = new JsonObject();
            read.addProperty("count", stack.getCount());
            read.addProperty("damage", stack.getDamageValue());
            reads.add(read);
            return null;
        }).when(AdvancementVectors.player).giveExperiencePoints(anyInt());
        var before = ItemStack.CODEC.encodeStart(AdvancementVectors.ops, stack).getOrThrow();
        switch (trigger) {
            case "placed_block" -> {
                var pos = new BlockPos(0, 0, 0);
                doReturn(state).when(AdvancementVectors.level).getBlockState(pos);
                doReturn(null).when(AdvancementVectors.level).getBlockEntity(pos);
                CriteriaTriggers.PLACED_BLOCK.trigger(AdvancementVectors.player, pos, stack);
            }
            case "item_durability_changed" -> CriteriaTriggers.ITEM_DURABILITY_CHANGED.trigger(AdvancementVectors.player, stack, newDamage);
            case "consume_item" -> CriteriaTriggers.CONSUME_ITEM.trigger(AdvancementVectors.player, stack);
            default -> throw new AssertionError(trigger);
        }
        var row = new JsonObject();
        row.add("input", input); row.add("stack", before);
        row.addProperty("block", net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString());
        row.addProperty("new_damage", newDamage);
        row.addProperty("done", progress.getOrStartProgress(holder).isDone());
        row.add("reward_reads", reads);
        row.add("after_trigger", ItemStack.CODEC.encodeStart(AdvancementVectors.ops, stack).getOrThrow());
        rows.add(row);
        progress.clearTriggers(); Files.delete(directory);
    }

    static ItemStack named(Item item, String name) {
        var stack = new ItemStack(item, 3);
        stack.set(DataComponents.CUSTOM_NAME, Component.literal(name));
        return stack;
    }

    public static void main(String[] args) throws Exception {
        AdvancementVectors.setup();
        String placed = "{\"location\":{\"type\":\"all_of\",\"terms\":[{\"type\":\"match_block\",\"blocks\":\"stone\"},{\"type\":\"match_tool\",\"predicate\":{\"items\":\"stone\",\"components\":{\"minecraft:custom_name\":\"Marker\"}}}]}}";
        observe("placed_block", placed, named(Items.STONE, "Marker"), Blocks.STONE.defaultBlockState(), 0);
        observe("placed_block", placed, named(Items.STONE, "Other"), Blocks.STONE.defaultBlockState(), 0);
        observe("placed_block", placed, named(Items.STONE, "Marker"), Blocks.DIRT.defaultBlockState(), 0);
        String durability = "{\"item\":{\"items\":\"diamond_pickaxe\",\"components\":{\"minecraft:damage\":17}},\"durability\":1538,\"delta\":-6}";
        for (int[] sample : new int[][]{{17,23},{23,23},{17,30}}) {
            var stack = new ItemStack(Items.DIAMOND_PICKAXE);
            stack.setDamageValue(sample[0]);
            observe("item_durability_changed", durability, stack, Blocks.AIR.defaultBlockState(), sample[1]);
        }
        String consume = "{\"item\":{\"items\":\"apple\",\"count\":3,\"components\":{\"minecraft:custom_name\":\"Meal\"}}}";
        observe("consume_item", consume, named(Items.APPLE, "Meal"), Blocks.AIR.defaultBlockState(), 0);
        observe("consume_item", consume, named(Items.APPLE, "Other"), Blocks.AIR.defaultBlockState(), 0);
        observe("consume_item", consume, named(Items.STONE, "Meal"), Blocks.AIR.defaultBlockState(), 0);
        try (var output = new GZIPOutputStream(Files.newOutputStream(Path.of(args[0])))) {
            output.write(AdvancementVectors.JSON.toJson(rows).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        }
        System.out.println("live_item_triggers: " + rows.size());
    }
}
