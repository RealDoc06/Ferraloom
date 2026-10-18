import com.google.gson.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.chat.Component;
import net.minecraft.world.item.*;

/** Real ItemStack accessors and copy/grow behavior with retained nonpositive-count payloads. */
public final class ItemStackCounts {
    static RegistryAccess access;
    static JsonArray keys(DataComponentMap map) {
        var names=new ArrayList<String>();
        for(var entry:map) names.add(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(entry.type()).toString());
        Collections.sort(names);var result=new JsonArray();names.forEach(result::add);return result;
    }
    static JsonObject snapshot(ItemStack stack) {
        var result=new JsonObject();
        result.addProperty("item",BuiltInRegistries.ITEM.getKey(stack.getItem()).toString());
        result.addProperty("count",stack.getCount());result.addProperty("empty",stack.isEmpty());
        result.add("components",keys(stack.getComponents()));result.add("prototype",keys(stack.getPrototype()));
        result.addProperty("max_stack_size",stack.getMaxStackSize());
        result.addProperty("damaged",stack.isDamaged());result.addProperty("stackable",stack.isStackable());
        var slots=new JsonArray();
        slots.add(stack.getItem() instanceof BannerItem);
        slots.add(!(stack.getItem() instanceof BlockItem block && block.getBlock() instanceof net.minecraft.world.level.block.ShulkerBoxBlock));
        slots.add(stack.is(Items.LAPIS_LAZULI));
        slots.add(stack.has(DataComponents.COOKING_FUEL)||stack.is(Items.BUCKET));
        slots.add(stack.is(Items.PAPER)||stack.is(Items.MAP)||stack.is(Items.GLASS_PANE));
        result.add("slots",slots);
        var buf=new RegistryFriendlyByteBuf(Unpooled.buffer(),access);
        try {
            DataComponentPatch.STREAM_CODEC.encode(buf,stack.getComponentsPatch());
            byte[] bytes=new byte[buf.readableBytes()];buf.readBytes(bytes);
            result.addProperty("patch",HexFormat.of().formatHex(bytes));
        } finally {buf.release();}
        return result;
    }
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();
        var lookup=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p->p.apply());
        access=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var rows=new JsonArray();
        for(Item item:List.of(Items.STONE,Items.DIAMOND_SWORD,Items.MILK_BUCKET,Items.BANNER.pick(DyeColor.WHITE),
                Items.SHULKER_BOX,Items.LAPIS_LAZULI,Items.BUCKET,Items.PAPER)) {
            var patch=DataComponentPatch.builder().set(DataComponents.CUSTOM_NAME,Component.literal("retained"))
                .set(DataComponents.MAX_STACK_SIZE,23).set(DataComponents.DAMAGE,7).remove(DataComponents.ITEM_NAME).build();
            for(int count:new int[]{-3,-1,0,1,2}) {
                var stack=new ItemStack(item.builtInRegistryHolder(),1,patch);stack.setCount(count);
                var row=new JsonObject();row.addProperty("source_item",BuiltInRegistries.ITEM.getKey(item).toString());
                row.addProperty("source_count",count);row.add("effective",snapshot(stack));
                row.add("copy",snapshot(stack.copyWithCount(3)));
                var zeroCopy=stack.copyWithCount(0);row.add("zero_copy",snapshot(zeroCopy));
                zeroCopy.grow(2);row.add("zero_copy_grown",snapshot(zeroCopy));
                var negativeCopy=stack.copyWithCount(-3);negativeCopy.grow(2);
                row.add("negative_copy_grown",snapshot(negativeCopy));
                row.add("transmuted",snapshot(new ItemStackTemplate(Items.DIRT).apply(stack.getComponentsPatch())));
                stack.grow(2);row.add("grown",snapshot(stack));rows.add(row);
            }
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(rows)+"\n");
        System.out.println("ItemStack counts: "+rows.size()+" zero/negative/positive cases, seven native observations each");
    }
}
