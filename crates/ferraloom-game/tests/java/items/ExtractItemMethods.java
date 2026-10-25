import com.google.gson.*;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.*;

/** Immutable registry-class facts, never sampled from a stack, world or RNG. */
public final class ExtractItemMethods {
    static boolean virtual(Method method) {
        int m = method.getModifiers();
        return !method.isSynthetic() && !Modifier.isStatic(m) && !Modifier.isPrivate(m) && !Modifier.isFinal(m);
    }
    static String signature(Method method) {
        return method.getName() + Arrays.toString(method.getParameterTypes());
    }
    static Method implementation(Class<?> type, Method base) throws Exception {
        if (!base.getDeclaringClass().isAssignableFrom(type)) return null;
        for (var c = type; c != null; c = c.getSuperclass()) {
            try {
                var method = c.getDeclaredMethod(base.getName(), base.getParameterTypes());
                if (Modifier.isStatic(method.getModifiers()) || Modifier.isAbstract(method.getModifiers())
                    || !base.getReturnType().isAssignableFrom(method.getReturnType())) throw new AssertionError(method);
                return method;
            } catch (NoSuchMethodException missing) { }
        }
        throw new AssertionError(type.getName() + ": " + base);
    }
    static void hierarchy(Class<?> type, Set<Class<?>> result) {
        if (type == null || !result.add(type)) return;
        hierarchy(type.getSuperclass(), result);
        for (var parent : type.getInterfaces()) hierarchy(parent, result);
    }
    static String methodName(Method method) {
        return method.getDeclaringClass() == StandingAndWallBlockItem.class && method.getName().equals("canPlace")
            ? "canPlaceStandingOrWall" : method.getName();
    }
    static Object field(Object receiver, Class<?> owner, String name) throws Exception {
        var field = owner.getDeclaredField(name); field.setAccessible(true);
        return field.get(receiver);
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var methodsBySignature = new TreeMap<String, Method>();
        // The first declaration supplies the base, including BlockItem's extra
        // placement stages. An absent BlockItem method is distinct from a base.
        for (var type : List.of(Item.class, BlockItem.class)) {
            for (var method : type.getDeclaredMethods()) if (virtual(method))
                methodsBySignature.putIfAbsent(signature(method), method);
        }
        var standingCanPlace = StandingAndWallBlockItem.class.getDeclaredMethod("canPlace",
            net.minecraft.world.level.LevelReader.class, net.minecraft.world.level.block.state.BlockState.class, net.minecraft.core.BlockPos.class);
        methodsBySignature.put(signature(standingCanPlace), standingCanPlace);
        var bases = new ArrayList<>(methodsBySignature.values());
        var names = new HashSet<String>();
        for (var base : bases) if (!names.add(methodName(base))) throw new AssertionError("Overloaded virtual requires explicit name: " + base);
        var types = new TreeSet<Class<?>>(Comparator.comparing(Class::getName));
        for (var item : BuiltInRegistries.ITEM) hierarchy(item.getClass(), types);
        var typeIds = new HashMap<Class<?>, Integer>();
        int id = 0;
        for (var type : types) typeIds.put(type, id++);
        var classes = new JsonArray();
        for (var type : types) {
            var entry = new JsonObject(); entry.addProperty("name", type.getName());
            var parents = new TreeSet<Class<?>>(Comparator.comparing(Class::getName));
            hierarchy(type, parents);
            var instances = new JsonArray();
            for (var parent : parents) instances.add(typeIds.get(parent));
            entry.add("instances", instances); classes.add(entry);
        }
        var methods = new JsonArray();
        for (var method : bases) {
            var entry = new JsonObject(); entry.addProperty("name", method.getName());
            entry.addProperty("rust_name", methodName(method));
            entry.addProperty("base", typeIds.get(method.getDeclaringClass()));
            entry.addProperty("returns", method.getReturnType().getTypeName());
            var parameters = new JsonArray();
            for (var parameter : method.getParameterTypes()) parameters.add(parameter.getTypeName());
            entry.add("parameters", parameters); methods.add(entry);
        }
        var items = new JsonArray();
        for (var item : BuiltInRegistries.ITEM) {
            if (BuiltInRegistries.ITEM.getId(item) != items.size()) throw new AssertionError("item order");
            var entry = new JsonObject(); entry.addProperty("name", BuiltInRegistries.ITEM.getKey(item).toString());
            entry.addProperty("class", typeIds.get(item.getClass()));
            var owners = new JsonArray();
            for (var base : bases) {
                var method = implementation(item.getClass(), base);
                if (method == null) owners.add(JsonNull.INSTANCE); else owners.add(typeIds.get(method.getDeclaringClass()));
            }
            entry.add("declaring_classes", owners);
            if (item instanceof StandingAndWallBlockItem) {
                entry.addProperty("wall_block", BuiltInRegistries.BLOCK.getId((net.minecraft.world.level.block.Block) field(item, StandingAndWallBlockItem.class, "wallBlock")));
                entry.addProperty("attachment_direction", ((net.minecraft.core.Direction) field(item, StandingAndWallBlockItem.class, "attachmentDirection")).ordinal());
            }
            if (item instanceof SolidBucketItem) {
                entry.addProperty("place_sound", BuiltInRegistries.SOUND_EVENT.getId((net.minecraft.sounds.SoundEvent) field(item, SolidBucketItem.class, "placeSound")));
            }
            items.add(entry);
        }
        var result = new JsonObject(); result.add("classes", classes); result.add("methods", methods); result.add("items", items);
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(result) + "\n");
        System.out.println(items.size() + " native item classes/hierarchies x " + methods.size() + " virtual owners");
    }
}
