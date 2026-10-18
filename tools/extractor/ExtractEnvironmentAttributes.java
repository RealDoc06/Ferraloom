import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.attribute.*;

/** Immutable registry metadata only; does not sample clocks, positions or weather. */
public final class ExtractEnvironmentAttributes {
    static <T> JsonObject entry(EnvironmentAttribute<T> attribute, com.mojang.serialization.DynamicOps<JsonElement> ops, Map<Object,String> fields) {
        var row=new JsonObject();
        row.addProperty("id",BuiltInRegistries.ENVIRONMENT_ATTRIBUTE.getId(attribute));
        row.addProperty("name",BuiltInRegistries.ENVIRONMENT_ATTRIBUTE.getKey(attribute).toString());
        row.addProperty("field",fields.get(attribute));
        row.addProperty("type",BuiltInRegistries.ATTRIBUTE_TYPE.getId(attribute.type()));
        row.addProperty("type_name",BuiltInRegistries.ATTRIBUTE_TYPE.getKey(attribute.type()).toString());
        row.add("default",attribute.valueCodec().encodeStart(ops,attribute.defaultValue()).getOrThrow());
        row.addProperty("positional",attribute.isPositional());
        row.addProperty("spatially_interpolated",attribute.isSpatiallyInterpolated());
        row.addProperty("full_resolution_biomes",attribute.isFullResolutionBiomes());
        row.addProperty("syncable",attribute.isSyncable());
        var modifiers=new JsonArray();modifiers.add("override");
        attribute.type().modifierLibrary().keySet().stream().map(op->op.getSerializedName()).sorted().forEach(modifiers::add);
        row.add("modifiers",modifiers);
        row.add("float_min",JsonNull.INSTANCE);row.add("float_max",JsonNull.INSTANCE);
        if(attribute.type()==AttributeTypes.FLOAT || attribute.type()==AttributeTypes.ANGLE_DEGREES) {
            @SuppressWarnings("unchecked") var scalar=(EnvironmentAttribute<Float>)attribute;
            float min=scalar.sanitizeValue(Float.NEGATIVE_INFINITY),max=scalar.sanitizeValue(Float.POSITIVE_INFINITY);
            if(Float.isFinite(min))row.addProperty("float_min",min);
            if(Float.isFinite(max))row.addProperty("float_max",max);
        }
        return row;
    }
    public static void main(String[] args) throws Exception {
        var output=System.out;SharedConstants.tryDetectVersion();Bootstrap.bootStrap();System.setOut(output);
        var fields=new IdentityHashMap<Object,String>();
        for(var field:EnvironmentAttributes.class.getFields())if(field.getType()==EnvironmentAttribute.class)fields.put(field.get(null),field.getName());
        var ops=RegistryOps.create(JsonOps.INSTANCE,RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY));
        var entries=new JsonArray();
        for(var attribute:BuiltInRegistries.ENVIRONMENT_ATTRIBUTE)entries.add(entry(attribute,ops,fields));
        var dir=Path.of(args[0]);Files.createDirectories(dir);
        Files.writeString(dir.resolve("environment_attributes.json"),new GsonBuilder().disableHtmlEscaping().serializeNulls().create().toJson(entries)+"\n");
    }
}
