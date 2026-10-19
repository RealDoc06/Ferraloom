import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.locale.Language;
import net.minecraft.network.chat.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.animal.cow.Cow;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.*;
import static org.mockito.Mockito.*;

/** Reads real Nameable names and Language/TranslatableContents fallbacks. */
public final class LanguageVector {
    static <T> net.minecraft.core.Registry<T> copyRegistry(net.minecraft.core.HolderLookup.RegistryLookup<T> lookup) {
        var registry=new net.minecraft.core.MappedRegistry<T>((net.minecraft.resources.ResourceKey)lookup.key(),com.mojang.serialization.Lifecycle.stable());
        lookup.listElements().forEach(h->net.minecraft.core.Registry.register(registry,h.key(),h.value()));
        return registry.freeze();
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var registries=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p->p.apply());
        var level=mock(net.minecraft.world.level.Level.class);
        var builtin=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var dynamic=List.of(copyRegistry(registries.lookupOrThrow(net.minecraft.core.registries.Registries.COW_VARIANT)),copyRegistry(registries.lookupOrThrow(net.minecraft.core.registries.Registries.COW_SOUND_VARIANT)));
        var access=new RegistryAccess.ImmutableRegistryAccess(java.util.stream.Stream.concat(builtin.registries().map(e->e.value()),dynamic.stream()).toList());
        doReturn(access).when(level).registryAccess();
        var cow=new Cow(EntityTypes.COW,level);
        var item=new ItemEntity(EntityTypes.ITEM,level);item.setItem(new ItemStack(Items.STONE,1));
        var values=new LinkedHashMap<String,Component>();
        values.put("cow",cow.getName());values.put("stone",item.getName());
        cow.setCustomName(Component.literal("Named Cow").append("!"));values.put("custom",cow.getName());
        values.put("missing",Component.translatable("ferraloom.missing"));
        values.put("fallback",Component.translatableWithFallback("ferraloom.missing","Fallback"));
        values.put("format",Component.translatableWithFallback("ferraloom.format","%2$s: %1$s %%","A","B"));
        values.put("missing-argument",Component.translatableWithFallback("ferraloom.format","before %s after"));
        values.put("bad-format",Component.translatableWithFallback("ferraloom.format","%q then %s","A"));
        values.put("deprecated",Component.translatable("block.minecraft.grass"));
        var formats=new java.util.LinkedHashMap<String,String>();
        Language.loadFromJson(new java.io.ByteArrayInputStream("{\"integer\":\"%d\",\"float\":\"%.2f\",\"indexed\":\"%2$d %1$05.2f\",\"percent\":\"%%d\"}".getBytes(java.nio.charset.StandardCharsets.UTF_8)),formats::put);
        formats.forEach((key,value)->values.put("normalized-"+key,Component.translatableWithFallback("ferraloom.format",value,"A","B")));
        var vectors=new JsonArray();
        for(var entry:values.entrySet()) {
            var row=new JsonObject();row.addProperty("case",entry.getKey());
            row.add("component",ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE,entry.getValue()).getOrThrow());
            row.addProperty("text",entry.getValue().getString());vectors.add(row);
        }
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(vectors)+"\n");
    }
}
