import com.google.gson.*;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.*;

/** Records virtual-method ownership by exact signature, never by sampling behavior. */
public final class ExtractBlockMethods {
    private static final List<String> NAMES = List.of(
        "canSurvive", "updateShape", "getStateForPlacement", "onPlace",
        "affectNeighborsAfterRemoval", "neighborChanged", "tick", "randomTick",
        "isRandomlyTicking", "useItemOn", "useWithoutItem", "attack", "entityInside",
        "stepOn", "fallOn", "getCollisionShape", "getShape", "rotate", "mirror",
        "getBlockSupportShape", "isCollisionShapeFullBlock", "getVisualShape",
        "getFluidState", "getOcclusionShape", "getInteractionShape",
        "updateIndirectNeighbourShapes", "propagatesSkylightDown",
        "useShapeForLightOcclusion", "isPathfindable", "getLightDampening",
        "getEntityInsideCollisionShape", "bounceOn", "triggerEvent",
        "getAnalogOutputSignal", "hasAnalogOutputSignal",
        "canPlaceLiquid", "placeLiquid", "pickupBlock", "getCloneItemStack", "canBeReplaced",
        "handlePrecipitation",
        "getSignal", "getDirectSignal", "isSignalSource", "ownSignal", "shouldRedstoneWireConnectTo",
        "getDrops", "spawnAfterBreak", "getMenuProvider",
        "setPlacedBy", "playerWillDestroy", "spawnDestroyByEntityParticles", "spawnDestroyParticles",
        "getDestroyProgress", "destroy", "playerDestroy"
    );

    private static Method baseMethod(String name) {
        var candidates = new ArrayList<Method>();
        for (Class<?> type : List.of(BlockBehaviour.class, Block.class, SimpleWaterloggedBlock.class)) {
            for (Method method : type.getDeclaredMethods()) {
                int modifiers = method.getModifiers();
                if (method.getName().equals(name) && !method.isSynthetic()
                    && !Modifier.isStatic(modifiers) && !Modifier.isPrivate(modifiers)
                    && !Modifier.isFinal(modifiers)) {
                    // The other overload takes Fluid. Placement must retain its
                    // own exact signature, including all subclass overrides.
                    if (name.equals("canBeReplaced") && method.getParameterTypes()[1]
                        != net.minecraft.world.item.context.BlockPlaceContext.class) continue;
                    candidates.add(method);
                }
            }
        }
        if (candidates.size() != 1) throw new AssertionError("Ambiguous base method: " + name + candidates);
        return candidates.getFirst();
    }

    private static Method implementation(Class<?> type, Method base) throws NoSuchMethodException {
        for (Class<?> current = type; current != null; current = current.getSuperclass()) {
            try {
                Method method = current.getDeclaredMethod(base.getName(), base.getParameterTypes());
                if (Modifier.isStatic(method.getModifiers()) || Modifier.isAbstract(method.getModifiers()))
                    throw new AssertionError("Not a concrete virtual implementation: " + method);
                if (!base.getReturnType().isAssignableFrom(method.getReturnType()))
                    throw new AssertionError("Incompatible return type: " + method);
                return method;
            } catch (NoSuchMethodException missing) {
                // Continue through the actual superclass chain, including anonymous classes.
            }
        }
        if (base.getDeclaringClass().isInterface()) {
            try {
                // getMethod resolves the maximally specific inherited default,
                // including subinterfaces, after all concrete class overrides.
                return type.getMethod(base.getName(), base.getParameterTypes());
            } catch (NoSuchMethodException absentInterface) {
                return null;
            }
        }
        throw new NoSuchMethodException(type.getName() + ": " + base);
    }

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var bases = NAMES.stream().map(ExtractBlockMethods::baseMethod).toList();
        var methods = new JsonArray();
        for (Method method : bases) {
            var item = new JsonObject();
            item.addProperty("name", method.getName());
            item.addProperty("base", method.getDeclaringClass().getName());
            item.addProperty("returns", method.getReturnType().getTypeName());
            var parameters = new JsonArray();
            for (Class<?> parameter : method.getParameterTypes()) parameters.add(parameter.getTypeName());
            item.add("parameters", parameters);
            methods.add(item);
        }
        var blocks = new JsonArray();
        var classes = new LinkedHashMap<String, Integer>();
        long survivalChecks = 0, shapeChecks = 0;
        for (Block block : BuiltInRegistries.BLOCK) {
            if (BuiltInRegistries.BLOCK.getId(block) != blocks.size()) throw new AssertionError("Block registry order");
            var item = new JsonObject();
            item.addProperty("name", BuiltInRegistries.BLOCK.getKey(block).toString());
            item.addProperty("class", block.getClass().getName());
            var owners = new JsonArray();
            for (Method base : bases) {
                Method method = implementation(block.getClass(), base);
                String owner = method == null ? "<not implemented>" : method.getDeclaringClass().getName();
                owners.add(classes.computeIfAbsent(owner, key -> classes.size()));
            }
            item.add("declaring_classes", owners);
            blocks.add(item);
            // These checks validate the two base algorithms used by worldgen. They
            // do not classify overrides: only the method identities above do that.
            boolean baseSurvival = implementation(block.getClass(), bases.get(0)).equals(bases.get(0));
            boolean baseShape = implementation(block.getClass(), bases.get(1)).equals(bases.get(1));
            for (BlockState state : block.getStateDefinition().getPossibleStates()) {
                if (baseSurvival) {
                    if (!state.canSurvive(null, BlockPos.ZERO)) throw new AssertionError(state);
                    survivalChecks++;
                }
                if (baseShape) for (Direction direction : Direction.values()) {
                    if (state.updateShape(null, null, BlockPos.ZERO, direction,
                        BlockPos.ZERO.relative(direction), Blocks.AIR.defaultBlockState(), null) != state)
                        throw new AssertionError(state);
                    shapeChecks++;
                }
            }
        }
        var result = new JsonObject();
        result.add("methods", methods);
        result.add("blocks", blocks);
        var classNames = new JsonArray();
        classes.keySet().forEach(classNames::add);
        result.add("declaring_classes", classNames);
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(result) + "\n");
        System.out.println("Extracted " + blocks.size() + " blocks x " + methods.size()
            + " method owners; checked " + survivalChecks + " base survival and " + shapeChecks + " base shape calls");
    }
}
