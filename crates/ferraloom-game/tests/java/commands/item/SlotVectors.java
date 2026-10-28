import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.SlotAccess;
import net.minecraft.world.item.*;
import net.minecraft.world.item.slot.*;
import org.apache.commons.lang3.mutable.MutableInt;

/** Real SlotCollection, ItemProvider, SlotSelector and CountingModifier calls. */
public final class SlotVectors {
    static final Gson JSON=new GsonBuilder().setPrettyPrinting().create();
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var registries=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();net.minecraft.core.registries.BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p->p.apply());var rows=new JsonArray();
        for(String action:List.of("copies","replace1","replace2","replace4","replace6","identity","clear","double")) {
            var row=new JsonObject();row.addProperty("action",action);var trace=new JsonArray();
            var values=new ArrayList<ItemStack>(List.of(new ItemStack(Items.STONE,2),new ItemStack(Items.STONE,3),ItemStack.EMPTY));
            var slots=new ArrayList<SlotAccess>();
            for(int index:List.of(0,1,0,2)) slots.add(new SlotAccess(){
                public ItemStack get(){trace.add("get:"+index+":"+values.get(index).getCount());return values.get(index);}
                public boolean set(ItemStack stack){trace.add("set:"+index+":"+stack.getCount());if(index==1)return false;values.set(index,stack);return true;}
            });
            var collection=SlotCollection.of(slots);int selected=0,updated=0;
            if(action.equals("copies"))row.add("copies",JSON.toJsonTree(collection.itemCopies().map(ItemStack::getCount).toList()));
            else if(action.startsWith("replace")) {
                int count=Integer.parseInt(action.substring(7));var items=new ArrayList<ItemStack>();for(int i=0;i<count;i++)items.add(new ItemStack(Items.STONE,10+i));
                var tracking=new MutableInt();updated=collection.replaceSlotItems(ItemProvider.of(items),SlotSelector.tracking(tracking));selected=tracking.intValue();
            } else {
                var tracking=new MutableInt();var modifier=new CountingModifier(stack->{trace.add("modify:"+stack.getCount());if(action.equals("clear"))return ItemStack.EMPTY;if(action.equals("double"))stack.setCount(stack.getCount()*2);return stack;});
                collection.modifySlots(modifier,SlotSelector.tracking(SlotSelector.NON_EMPTY_SLOTS,tracking));selected=tracking.intValue();updated=modifier.updatedCount();
            }
            row.addProperty("size",collection.size());row.addProperty("selected",selected);row.addProperty("updated",updated);row.add("final",JSON.toJsonTree(values.stream().map(ItemStack::getCount).toList()));row.add("trace",trace);rows.add(row);
        }
        Files.writeString(Path.of(args[0]),JSON.toJson(rows)+"\n");System.out.println("Captured "+rows.size()+" native slot collection cases");
    }
}
