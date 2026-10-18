import com.google.gson.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.world.food.FoodProperties;
import net.minecraft.world.item.*;
import net.minecraft.world.item.component.CustomModelData;

/** Native component map/patch equality across insertion orders, with raw patch stream bytes. */
public final class ComponentMapEquality {
    static final int[][] ORDERS={{0,1,2},{0,2,1},{1,0,2},{1,2,0},{2,0,1},{2,1,0}};
    static final int[] BITS={0,0x80000000,0x3f800000,0x7fc00000,0x7f800001,0xffc01234};
    record Row(int bits,DataComponentMap map,DataComponentPatch patch,ItemStack stack,ItemStackTemplate template) {}
    static <T> void add(DataComponentMap.Builder map,DataComponentPatch.Builder patch,TypedDataComponent<T> entry) {
        map.set(entry.type(),entry.value());patch.set(entry.type(),entry.value());
    }
    static Row row(int bits,int[] order,int removals) {
        float value=Float.intBitsToFloat(bits);
        List<TypedDataComponent<?>> entries=List.of(
            new TypedDataComponent<>(DataComponents.FOOD,new FoodProperties(3,value,false)),
            new TypedDataComponent<>(DataComponents.DAMAGE,4),
            new TypedDataComponent<>(DataComponents.CUSTOM_MODEL_DATA,new CustomModelData(List.of(value),List.of(),List.of(),List.of())));
        var map=DataComponentMap.builder();var patch=DataComponentPatch.builder();
        for(int index:order) add(map,patch,entries.get(index));
        switch(removals) {
            case 0 -> {patch.remove(DataComponents.RARITY);patch.remove(DataComponents.ITEM_NAME);}
            case 1 -> {patch.remove(DataComponents.ITEM_NAME);patch.remove(DataComponents.RARITY);}
            case 2 -> patch.remove(DataComponents.RARITY);
            case 3 -> {}
            default -> throw new AssertionError();
        }
        var builtMap=map.build();var builtPatch=patch.build();
        int sum=0;for(var entry:builtMap) sum+=entry.type().hashCode()^entry.value().hashCode();
        if(sum!=builtMap.hashCode()) throw new AssertionError("Map hash is not the entry sum");
        return new Row(bits,builtMap,builtPatch,new ItemStack(Items.STONE.builtInRegistryHolder(),1,builtPatch),new ItemStackTemplate(Items.STONE,builtPatch));
    }
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();
        var lookup=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup).forEach(p->p.apply());
        var access=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var values=new ArrayList<Row>();
        for(int bits:BITS) {
            for(int[] order:ORDERS) for(int removal=0;removal<2;removal++) values.add(row(bits,order,removal));
            values.add(row(bits,ORDERS[0],2));values.add(row(bits,ORDERS[0],3));
        }
        var rows=new JsonArray();
        for(var value:values) {
            var row=new JsonObject();row.addProperty("bits",Integer.toHexString(value.bits()));
            var buf=new RegistryFriendlyByteBuf(Unpooled.buffer(),access);
            try {
                DataComponentPatch.STREAM_CODEC.encode(buf,value.patch());
                byte[] bytes=new byte[buf.readableBytes()];buf.readBytes(bytes);
                row.addProperty("wire",HexFormat.of().formatHex(bytes));
            } finally {buf.release();}
            var maps=new StringBuilder();var patches=new StringBuilder();var stacks=new StringBuilder();var same=new StringBuilder();var templates=new StringBuilder();
            for(var other:values) {
                boolean map=value.map().equals(other.map()),patch=value.patch().equals(other.patch());
                if(map&&value.map().hashCode()!=other.map().hashCode()) throw new AssertionError("Map order changed hash");
                if(patch&&value.patch().hashCode()!=other.patch().hashCode()) throw new AssertionError("Patch order changed hash");
                maps.append(map?'1':'0');patches.append(patch?'1':'0');
                stacks.append(ItemStack.matches(value.stack(),other.stack())?'1':'0');
                same.append(ItemStack.isSameItemSameComponents(value.stack(),other.stack().copyWithCount(7))?'1':'0');
                templates.append(value.template().equals(other.template())?'1':'0');
            }
            row.addProperty("map",maps.toString());row.addProperty("patch",patches.toString());
            row.addProperty("stack",stacks.toString());row.addProperty("same_components",same.toString());
            row.addProperty("template",templates.toString());rows.add(row);
        }
        var json=new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
        Files.writeString(Path.of(args[0]),json.toJson(rows)+"\n");
        System.out.println("Component order vectors: "+values.size()+" encodings, "+values.size()*values.size()+" pairs for each of five equality paths; native map/patch hash invariance checked");
    }
}
