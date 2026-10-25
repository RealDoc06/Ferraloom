import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.util.*;
import net.minecraft.core.*;
import net.minecraft.core.registries.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.item.*;
import net.minecraft.world.item.enchantment.*;

/** Real registry definitions, item eligibility, slot groups and overflow costs. */
public final class DefinitionVectors {
    static int slots(Enchantment value) {
        int bits=0;
        for(var slot:EquipmentSlot.values()) if(value.matchingSlot(slot)) bits|=1<<slot.ordinal();
        return bits;
    }
    public static void main(String[] args) throws Exception {
        var out=System.out;
        net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();System.setOut(out);
        var world=NeighborVectors.load();
        var builtins=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var registries=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtins.registries(),world.registries())).freeze();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var ops=registries.createSerializationContext(JsonOps.INSTANCE);
        var entries=world.lookupOrThrow(Registries.ENCHANTMENT).listElements().sorted(Comparator.comparing(e->e.key().identifier().toString())).toList();
        var samples=List.of(Items.AIR,Items.STONE,Items.DIAMOND_SWORD,Items.DIAMOND_AXE,Items.DIAMOND_PICKAXE,Items.DIAMOND_HELMET,Items.DIAMOND_CHESTPLATE,Items.DIAMOND_LEGGINGS,Items.DIAMOND_BOOTS,Items.IRON_CHESTPLATE,Items.LEATHER_BOOTS,Items.BOW,Items.CROSSBOW,Items.TRIDENT,Items.FISHING_ROD,Items.MACE,Items.ELYTRA,Items.BOOK,Items.ENCHANTED_BOOK,Items.SADDLE,Items.WOLF_ARMOR);
        for(var holder:entries) {
            var value=holder.value();var id=holder.key().identifier();
            System.out.println("definition\t"+id+"\t"+value.getWeight()+"\t"+value.getMaxLevel()+"\t"+value.getAnvilCost()+"\t"+slots(value));
            for(int level:new int[]{Integer.MIN_VALUE,-1,0,1,2,3,255,256,Integer.MAX_VALUE})
                System.out.println("cost\t"+id+"\t"+level+"\t"+value.getMinCost(level)+"\t"+value.getMaxCost(level));
            for(var item:samples) for(int count:new int[]{0,1}) {
                var stack=new ItemStack(item,count);
                System.out.println("item\t"+id+"\t"+BuiltInRegistries.ITEM.getId(item)+"\t"+count+"\t"+value.isSupportedItem(stack)+"\t"+value.isPrimaryItem(stack)+"\t"+value.canEnchant(stack));
            }
        }
        String base="{\"supported_items\":[\"minecraft:air\",\"minecraft:stone\"],\"weight\":1,\"max_level\":1,\"min_cost\":{\"base\":2,\"per_level_above_first\":3},\"max_cost\":{\"base\":7,\"per_level_above_first\":9},\"anvil_cost\":0,\"slots\":[\"any\"]}";
        var json=JsonParser.parseString(base).getAsJsonObject();
        var cases=new ArrayList<JsonObject>();cases.add(json.deepCopy());
        for(var key:List.of("weight","max_level","anvil_cost")) for(int number:new int[]{-1,0,1,255,256,1024,1025,Integer.MAX_VALUE}) {
            var v=json.deepCopy();v.addProperty(key,number);cases.add(v);
        }
        for(String field:List.of("supported_items","primary_items","slots")) for(String input:List.of("[]","\"minecraft:stone\"","[\"minecraft:missing_fixture\"]","[\"armor\",\"saddle\"]","null")) {
            var v=json.deepCopy();v.add(field,JsonParser.parseString(input));cases.add(v);
        }
        for(var v:cases) System.out.println("codec\t"+v+"\t"+Enchantment.EnchantmentDefinition.CODEC.codec().parse(ops,v).result().isPresent());
        for(var group:EquipmentSlotGroup.values()) {
            var v=json.deepCopy();var groups=new JsonArray();groups.add(group.getSerializedName());v.add("slots",groups);
            var definition=Enchantment.EnchantmentDefinition.CODEC.codec().parse(ops,v).getOrThrow();
            var value=new Enchantment(net.minecraft.network.chat.Component.literal("fixture"),definition,HolderSet.empty(),net.minecraft.core.component.DataComponentMap.EMPTY);
            System.out.println("slots\t"+v+"\t"+slots(value));
        }
        for(int baseCost:new int[]{Integer.MIN_VALUE,-17,0,19,Integer.MAX_VALUE}) for(int per:new int[]{Integer.MIN_VALUE,-3,0,7,Integer.MAX_VALUE}) for(int level:new int[]{Integer.MIN_VALUE,-1,0,1,2,255,Integer.MAX_VALUE}) {
            var cost=new Enchantment.Cost(baseCost,per);
            System.out.println("overflow\t"+baseCost+"\t"+per+"\t"+level+"\t"+cost.calculate(level));
        }
    }
}
