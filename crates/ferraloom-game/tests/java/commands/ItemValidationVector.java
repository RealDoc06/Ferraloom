import com.google.gson.*;
import com.mojang.brigadier.exceptions.CommandSyntaxException;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.arguments.item.ItemInput;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.chat.Component;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.ItemStack;

/** Calls ItemStack.validateStrict and ItemInput.createItemStack without copying their logic. */
public final class ItemValidationVector {
    static JsonObject stack(String id, JsonObject components) {
        var item = new JsonObject(); item.addProperty("id", "minecraft:" + id);
        if (components.size() != 0) item.add("components", components); return item;
    }
    static JsonObject components(String json) { return JsonParser.parseString(json).getAsJsonObject(); }
    static JsonObject item(String id, int count) {
        var value = stack(id,new JsonObject()); value.addProperty("count",count); return value;
    }
    static JsonObject bundle(JsonArray contents) {
        var patch = new JsonObject(); patch.add("minecraft:bundle_contents",contents); return stack("bundle",patch);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var registries=VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p->p.apply());
        var ops=registries.createSerializationContext(JsonOps.INSTANCE);
        var rows=new JsonArray(); var inputs=new ArrayList<JsonObject>();
        for (String id:List.of("stone","diamond_sword","air")) inputs.add(stack(id,new JsonObject()));
        inputs.add(stack("stone",components("{\"minecraft:max_damage\":10}")));
        inputs.add(stack("stone",components("{\"minecraft:max_damage\":10,\"minecraft:max_stack_size\":1}")));
        inputs.add(stack("diamond_sword",components("{\"!minecraft:max_stack_size\":{}}")));
        for (String kind:List.of("container","bundle_contents","charged_projectiles")) {
            var children=new JsonArray(); var over=item("diamond_sword",2);
            if (kind.equals("container")) { var slot=new JsonObject();slot.addProperty("slot",0);slot.add("item",over);children.add(slot); }
            else children.add(over);
            var patch=new JsonObject();patch.add("minecraft:"+kind,children);inputs.add(stack("stone",patch));
        }
        var order=components("{\"minecraft:charged_projectiles\":[{\"id\":\"minecraft:diamond_sword\",\"count\":3}],\"minecraft:bundle_contents\":[{\"id\":\"minecraft:stone\",\"count\":65}],\"minecraft:container\":[{\"slot\":0,\"item\":{\"id\":\"minecraft:diamond_sword\",\"count\":2}}]}");
        inputs.add(stack("stone",order));
        var primeChildren=new JsonArray();
        for (int prime:List.of(97,89,83,79,73)) {
            var patch=new JsonObject();patch.addProperty("minecraft:max_stack_size",prime);
            primeChildren.add(stack("stone",patch));inputs.add(bundle(primeChildren.deepCopy()));
        }
        JsonObject nested=stack("stone",new JsonObject());
        for (int depth=1;depth<=9;depth++) {
            var children=new JsonArray();children.add(nested);nested=bundle(children);inputs.add(nested);
        }
        for (var input:inputs) {
            var source=input.get("id").getAsString().equals("minecraft:air") ? new ItemStack(net.minecraft.world.item.Items.AIR) : ItemStack.CODEC.parse(ops,input).getOrThrow();
            for (int count:List.of(1,2,65,100)) {
                var stack=source.copyWithCount(count);
                var row=new JsonObject();row.add("input",input);row.addProperty("count",count);
                var result=ItemStack.validateStrict(stack);
                if (result.error().isPresent()) row.addProperty("reason",result.error().orElseThrow().message());
                try {
                    new ItemInput(source.typeHolder(),source.getComponentsPatch()).createItemStack(count);
                } catch(CommandSyntaxException failure) {
                    row.add("input_error",ComponentSerialization.CODEC.encodeStart(ops,(Component)failure.getRawMessage()).getOrThrow());
                }
                rows.add(row);
            }
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create().toJson(rows)+"\n");
    }
}
