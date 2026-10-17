import com.google.gson.*;
import com.mojang.serialization.JsonOps;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.component.DataComponentMap;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.item.*;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.shapes.VoxelShape;

/** Extracts facts directly from the pinned vanilla registries and codecs. */
public final class ExtractData {
    private static final Gson GSON = new GsonBuilder().disableHtmlEscaping().serializeNulls().create();
    private static final List<JsonArray> SHAPES = new ArrayList<>();
    private static final Map<String, Integer> SHAPE_IDS = new LinkedHashMap<>();
    private static final Map<SoundType, String> SOUND_NAMES = new IdentityHashMap<>();

    private static int shape(VoxelShape shape) {
        var boxes = new JsonArray();
        for (var box : shape.toAabbs()) {
            var coords = new JsonArray();
            for (double v : new double[]{box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ}) {
                coords.add(v == 0.0 ? 0.0 : v);
            }
            boxes.add(coords);
        }
        return SHAPE_IDS.computeIfAbsent(GSON.toJson(boxes), key -> {
            SHAPES.add(boxes);
            return SHAPES.size() - 1;
        });
    }

    private static JsonObject sound(SoundType type) {
        var out = new JsonObject();
        out.addProperty("name", Objects.requireNonNull(SOUND_NAMES.get(type), "unknown sound type"));
        out.addProperty("volume", type.getVolume());
        out.addProperty("pitch", type.getPitch());
        out.addProperty("break", BuiltInRegistries.SOUND_EVENT.getId(type.getBreakSound()));
        out.addProperty("step", BuiltInRegistries.SOUND_EVENT.getId(type.getStepSound()));
        out.addProperty("place", BuiltInRegistries.SOUND_EVENT.getId(type.getPlaceSound()));
        out.addProperty("hit", BuiltInRegistries.SOUND_EVENT.getId(type.getHitSound()));
        out.addProperty("fall", BuiltInRegistries.SOUND_EVENT.getId(type.getFallSound()));
        return out;
    }

    private static JsonObject state(BlockState state) {
        var out = new JsonObject();
        out.addProperty("id", Block.getId(state));
        out.addProperty("block", BuiltInRegistries.BLOCK.getId(state.getBlock()));
        out.addProperty("destroy_time", state.getDestroySpeed(EmptyBlockGetter.INSTANCE, BlockPos.ZERO));
        out.addProperty("light_emission", state.getLightEmission());
        out.addProperty("collision", shape(state.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO)));
        out.addProperty("outline", shape(state.getShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO)));
        out.addProperty("occlusion", shape(state.getOcclusionShape()));
        out.addProperty("context_dependent_shape", state.getBlock().hasDynamicShape() || state.hasOffsetFunction()
            || state.getBlock() instanceof LiquidBlock || state.getBlock() instanceof ScaffoldingBlock
            || state.getBlock() instanceof PowderSnowBlock || state.getBlock() instanceof LightBlock);
        out.addProperty("solid_render", state.isSolidRender());
        out.addProperty("opacity", state.getLightDampening());
        out.addProperty("propagates_skylight", state.propagatesSkylightDown());
        out.addProperty("use_shape_for_light_occlusion", state.useShapeForLightOcclusion());
        out.addProperty("map_color", state.getMapColor(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).id);
        out.addProperty("requires_correct_tool", state.requiresCorrectToolForDrops());
        out.addProperty("random_ticking", state.isRandomlyTicking());
        out.addProperty("replaceable", state.canBeReplaced());
        out.addProperty("push_reaction", state.getPistonPushReaction().name().toLowerCase(Locale.ROOT));
        out.add("sound", sound(state.getSoundType()));
        var fluid = state.getFluidState();
        var fluidOut = new JsonObject();
        fluidOut.addProperty("type", BuiltInRegistries.FLUID.getId(fluid.getType()));
        fluidOut.addProperty("amount", fluid.getAmount());
        fluidOut.addProperty("source", fluid.isSource());
        fluidOut.addProperty("empty", fluid.isEmpty());
        // Preserve every fluid property (including falling), not just its amount.
        fluidOut.add("state", net.minecraft.world.level.material.FluidState.CODEC.encodeStart(JsonOps.INSTANCE, fluid).getOrThrow());
        out.add("fluid", fluidOut);
        return out;
    }

    // DefaultAttributes only accepts living types; hasSupplier proves the cast.
    @SuppressWarnings("unchecked")
    private static JsonObject attributes(EntityType<?> type) {
        var out = new JsonObject();
        if (DefaultAttributes.hasSupplier(type)) {
            var supplier = DefaultAttributes.getSupplier((EntityType<? extends LivingEntity>) type);
            for (var entry : BuiltInRegistries.ATTRIBUTE.listElements().toList()) {
                if (supplier.hasAttribute(entry)) out.addProperty(entry.getRegisteredName(), supplier.getBaseValue(entry));
            }
        }
        return out;
    }

    private static JsonElement canonical(JsonElement data) {
        if (data.isJsonObject()) {
            var result = new JsonObject();
            data.getAsJsonObject().keySet().stream().sorted().forEach(key -> result.add(key, canonical(data.getAsJsonObject().get(key))));
            return result;
        }
        if (data.isJsonArray()) {
            var result = new JsonArray();
            data.getAsJsonArray().forEach(value -> result.add(canonical(value)));
            return result;
        }
        return data;
    }

    private static void write(Path dir, String name, JsonElement data) throws Exception {
        Files.writeString(dir.resolve(name + ".json"), GSON.toJson(canonical(data)) + "\n", StandardCharsets.UTF_8);
    }

    private static void stateTransforms(Path dir) throws Exception {
        var out = new JsonObject();
        for (var rotation : new Rotation[]{Rotation.CLOCKWISE_90, Rotation.CLOCKWISE_180, Rotation.COUNTERCLOCKWISE_90}) {
            var table = new JsonArray();
            for (var state : Block.BLOCK_STATE_REGISTRY) table.add(Block.getId(state.rotate(rotation)));
            out.add(rotation.name().toLowerCase(Locale.ROOT), table);
        }
        for (var mirror : new Mirror[]{Mirror.LEFT_RIGHT, Mirror.FRONT_BACK}) {
            var table = new JsonArray();
            for (var state : Block.BLOCK_STATE_REGISTRY) table.add(Block.getId(state.mirror(mirror)));
            out.add(mirror.name().toLowerCase(Locale.ROOT), table);
        }
        write(dir, "state_transforms", out);
    }

    // Cached, state-only BlockStateBase#getFaceOcclusionShape facts.
    private static void faceOcclusion(Path dir) throws Exception {
        var faces = new JsonArray();
        for (var state : Block.BLOCK_STATE_REGISTRY) {
            int bits = 0;
            for (var direction : Direction.values()) {
                if (Block.isShapeFullBlock(state.getFaceOcclusionShape(direction))) bits |= 1 << direction.get3DDataValue();
            }
            faces.add(bits);
        }
        write(dir, "face_occlusion", faces);
    }

    // initCache stores legacySolid, faceSturdy and isCollisionShapeFullBlock.
    // A cache exists exactly when !hasDynamicShape. The only PostProcess
    // implementations in Blocks are constant null, self, or above callbacks.
    private static void stateFacts(Path dir) throws Exception {
        var table = new JsonArray();
        for (var state : Block.BLOCK_STATE_REGISTRY) {
            int bits = (state.isSolid() ? 1 : 0) | (state.hasBlockEntity() ? 2 : 0);
            if (!state.getBlock().hasDynamicShape()) {
                bits |= 4;
                for (var direction : Direction.values()) {
                    if (state.isFaceSturdy(EmptyBlockGetter.INSTANCE, BlockPos.ZERO, direction)) bits |= 1 << (3 + direction.get3DDataValue());
                }
                if (state.isCollisionShapeFullBlock(EmptyBlockGetter.INSTANCE, BlockPos.ZERO)) bits |= 1 << 9;
            }
            var post = state.getPostProcessPos(EmptyBlockGetter.INSTANCE, BlockPos.ZERO);
            if (post != null) {
                if (post.equals(BlockPos.ZERO)) bits |= 1 << 10;
                else if (post.equals(BlockPos.ZERO.above())) bits |= 2 << 10;
                else throw new IllegalStateException("Unknown post process offset: " + state);
            }
            table.add(bits);
        }
        write(dir, "block_state_facts", table);
    }

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        if (args.length > 1 && args[1].equals("--state-facts-only")) { stateFacts(Path.of(args[0])); return; }
        if (args.length > 1 && args[1].equals("--face-occlusion-only")) { faceOcclusion(Path.of(args[0])); return; }
        if (args.length > 1 && args[1].equals("--state-transforms-only")) {
            stateTransforms(Path.of(args[0]));
            return;
        }
        for (var field : Arrays.stream(SoundType.class.getFields()).sorted(Comparator.comparing(java.lang.reflect.Field::getName)).toList()) {
            if (field.getType() == SoundType.class) SOUND_NAMES.putIfAbsent((SoundType) field.get(null), field.getName().toLowerCase(Locale.ROOT));
        }
        var dir = Path.of(args[0]);
        Files.createDirectories(dir);
        var blocks = new JsonArray();
        var states = new JsonArray();
        for (var block : BuiltInRegistries.BLOCK) {
            var out = new JsonObject();
            out.addProperty("id", BuiltInRegistries.BLOCK.getId(block));
            out.addProperty("name", BuiltInRegistries.BLOCK.getKey(block).toString());
            out.addProperty("default_state", Block.getId(block.defaultBlockState()));
            out.addProperty("explosion_resistance", block.getExplosionResistance());
            out.addProperty("friction", block.getFriction());
            out.addProperty("speed_factor", block.getSpeedFactor());
            out.addProperty("jump_factor", block.getJumpFactor());
            // State-dependent defaults are retained on every individual state below.
            out.add("default_properties", state(block.defaultBlockState()));
            blocks.add(out);
            for (var s : block.getStateDefinition().getPossibleStates()) states.add(state(s));
        }
        var blockData = new JsonObject();
        blockData.add("blocks", blocks);
        blockData.add("states", states);
        write(dir, "block_properties", blockData);
        var shapes = new JsonArray();
        SHAPES.forEach(shapes::add);
        write(dir, "shapes", shapes);
        stateTransforms(dir);
        faceOcclusion(dir);
        stateFacts(dir);

        var registries = net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(pending -> pending.apply());
        var ops = registries.createSerializationContext(JsonOps.INSTANCE);
        var items = new JsonArray();
        for (var item : BuiltInRegistries.ITEM) {
            var out = new JsonObject();
            out.addProperty("id", BuiltInRegistries.ITEM.getId(item));
            out.addProperty("name", BuiltInRegistries.ITEM.getKey(item).toString());
            out.addProperty("max_stack_size", item.getDefaultMaxStackSize());
            out.addProperty("max_damage", item.components().getOrDefault(DataComponents.MAX_DAMAGE, 0));
            out.addProperty("rarity", item.components().getOrDefault(DataComponents.RARITY, Rarity.COMMON).name().toLowerCase(Locale.ROOT));
            if (item instanceof BlockItem blockItem) out.addProperty("block", BuiltInRegistries.BLOCK.getId(blockItem.getBlock()));
            else out.add("block", JsonNull.INSTANCE);
            out.add("components", DataComponentMap.CODEC.encodeStart(ops, item.components()).getOrThrow());
            items.add(out);
        }
        write(dir, "items", items);

        var entities = new JsonArray();
        for (var type : BuiltInRegistries.ENTITY_TYPE) {
            var out = new JsonObject();
            var dimensions = type.getDimensions();
            out.addProperty("id", BuiltInRegistries.ENTITY_TYPE.getId(type));
            out.addProperty("name", BuiltInRegistries.ENTITY_TYPE.getKey(type).toString());
            out.addProperty("width", dimensions.width());
            out.addProperty("height", dimensions.height());
            out.addProperty("eye_height", dimensions.eyeHeight());
            out.addProperty("fixed_dimensions", dimensions.fixed());
            out.addProperty("category", type.getCategory().getName());
            out.addProperty("tracking_range", type.clientTrackingRange());
            out.addProperty("update_interval", type.updateInterval());
            out.addProperty("fire_immune", type.fireImmune());
            out.addProperty("summonable", type.canSummon());
            out.add("attributes", attributes(type));
            entities.add(out);
        }
        write(dir, "entity_types", entities);
    }
}
