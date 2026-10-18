import com.google.gson.*;
import com.mojang.brigadier.arguments.*;
import com.mojang.brigadier.tree.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.commands.*;
import net.minecraft.commands.synchronization.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundCommandsPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.permissions.*;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.gamerules.*;

/** Runs against the pinned server jar; output is data, never decompiled code. */
public final class CommandVectors {
    static final Gson JSON = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
    static CommandSourceStack source(int level) {
        return Commands.createCompilationContext(LevelBasedPermissionSet.forLevel(PermissionLevel.byId(level)));
    }
    static void write(Path out, String name, Object value) throws Exception {
        Files.writeString(out.resolve(name), JSON.toJson(value) + "\n");
    }
    static RootCommandNode<CommandSourceStack> filter(CommandNode<CommandSourceStack> original, int level) {
        var root = new RootCommandNode<CommandSourceStack>();
        var converted = new IdentityHashMap<CommandNode<CommandSourceStack>,CommandNode<CommandSourceStack>>();
        converted.put(original, root);
        fill(original, root, source(level), converted);
        return root;
    }
    static void fill(CommandNode<CommandSourceStack> from, CommandNode<CommandSourceStack> to,
            CommandSourceStack source, Map<CommandNode<CommandSourceStack>,CommandNode<CommandSourceStack>> converted) {
        for (var child : from.getChildren()) if (child.canUse(source)) {
            var builder = child.createBuilder();
            if (builder.getRedirect() != null) builder.redirect(converted.get(builder.getRedirect()));
            var node = builder.build();
            converted.put(child, node);
            to.addChild(node);
            fill(child, node, source, converted);
        }
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var registries = VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries).forEach(p -> p.apply());
        var context = CommandBuildContext.simple(registries, FeatureFlags.DEFAULT_FLAGS);
        var commands = new Commands(Commands.CommandSelection.DEDICATED, context);
        var dispatcher = commands.getDispatcher();
        var out = Path.of(args[0]);
        Files.createDirectories(out);
        var nodes = new ArrayList<CommandNode<CommandSourceStack>>();
        var ids = new IdentityHashMap<CommandNode<CommandSourceStack>,Integer>();
        var queue = new ArrayDeque<CommandNode<CommandSourceStack>>();
        queue.add(dispatcher.getRoot());
        while (!queue.isEmpty()) {
            var node = queue.remove();
            if (ids.containsKey(node)) continue;
            ids.put(node, nodes.size()); nodes.add(node);
            queue.addAll(node.getChildren());
            if (node.getRedirect() != null) queue.add(node.getRedirect());
        }
        var tree = new JsonArray();
        for (var node : nodes) {
            var entry = new JsonObject();
            entry.addProperty("name", node.getName());
            entry.addProperty("kind", node instanceof RootCommandNode ? "root" : node instanceof LiteralCommandNode ? "literal" : "argument");
            entry.addProperty("executable", node.getCommand() != null);
            entry.addProperty("fork", node.isFork());
            entry.addProperty("restricted", !node.canUse(Commands.createCompilationContext(PermissionSet.NO_PERMISSIONS)));
            var permissions = new JsonArray();
            for (int level = 0; level <= 4; level++) permissions.add(node.canUse(source(level)));
            entry.add("permissions", permissions);
            if (node.getRedirect() != null) entry.addProperty("redirect", ids.get(node.getRedirect()));
            var children = new JsonArray();
            node.getChildren().forEach(child -> children.add(ids.get(child)));
            entry.add("children", children);
            if (node instanceof ArgumentCommandNode<CommandSourceStack,?> arg) {
                var serialized = ArgumentUtils.serializeNodeToJson(dispatcher, node);
                entry.add("parser", serialized.get("parser"));
                if (serialized.has("properties")) entry.add("properties", serialized.get("properties"));
                var examples = new JsonArray(); arg.getType().getExamples().forEach(examples::add);
                entry.add("examples", examples);
                if (arg.getCustomSuggestions() != null) entry.addProperty("suggestions", SuggestionProviders.getName(arg.getCustomSuggestions()).toString());
            }
            tree.add(entry);
        }
        write(out, "tree.json", tree);
        var argumentTypes = new LinkedHashMap<String,ArgumentType<?>>();
        var propertiesByType = new LinkedHashMap<String,JsonObject>();
        for (var node : nodes) if (node instanceof ArgumentCommandNode<CommandSourceStack,?> argument) {
            var serialized = ArgumentUtils.serializeNodeToJson(dispatcher, node);
            var parser = serialized.get("parser").getAsString();
            argumentTypes.putIfAbsent(parser, argument.getType());
            propertiesByType.putIfAbsent(parser, serialized.has("properties") ? serialized.getAsJsonObject("properties") : new JsonObject());
        }
        argumentTypes.put("brigadier:long", LongArgumentType.longArg());
        argumentTypes.put("minecraft:angle", net.minecraft.commands.arguments.AngleArgument.angle());
        argumentTypes.put("minecraft:item_slots", net.minecraft.commands.arguments.SlotsArgument.slots());
        var vectors = new JsonArray();
        for (var entry : argumentTypes.entrySet()) {
            var inputs = new LinkedHashSet<String>(entry.getValue().getExamples());
            inputs.addAll(List.of("", " ", "?", "-1", "1.5", "~ ~ ~", "0 1", "0 1 2", "^ ^ ^", "~ ^ ~", "@a", "@e[type=minecraft:player,limit=1]"));
            if (entry.getKey().equals("minecraft:entity")) inputs.addAll(List.of("@", "@z", "@a[", "@a[limit=0]", "@a[x=1,x=2]", "@a[name=!A,name=B]", "@e[type=#minecraft:undead,type=minecraft:player]", "@a[distance=3..1]", "@a[scores={foo=1..2}]", "@a[advancements={minecraft:story/root={foo=true}}]"));
            for (var input : inputs) {
                var reader = new com.mojang.brigadier.StringReader(input);
                var vector = new JsonObject();
                vector.addProperty("parser", entry.getKey());
                vector.add("properties", propertiesByType.getOrDefault(entry.getKey(), new JsonObject()));
                vector.addProperty("input", input);
                try {
                    Object value = entry.getValue().parse(reader);
                    vector.addProperty("ok", true);
                    if (value instanceof Number || value instanceof Boolean || value instanceof String) vector.add("value", JSON.toJsonTree(value));
                    if (value instanceof net.minecraft.commands.arguments.coordinates.WorldCoordinates coordinates) {
                        var coordinatesJson = new JsonArray();
                        for (var coordinate : List.of(coordinates.x(), coordinates.y(), coordinates.z())) {
                            var c = new JsonObject(); c.addProperty("relative", coordinate.relative()); c.addProperty("value", coordinate.value()); coordinatesJson.add(c);
                        }
                        vector.add("world_coordinates", coordinatesJson);
                    }
                    if (value instanceof net.minecraft.commands.arguments.coordinates.LocalCoordinates coordinates) vector.add("local_coordinates", JSON.toJsonTree(List.of(coordinates.left(), coordinates.up(), coordinates.forwards())));
                } catch (com.mojang.brigadier.exceptions.CommandSyntaxException e) {
                    vector.addProperty("ok", false); vector.addProperty("error_cursor", e.getCursor()); vector.addProperty("message", e.getRawMessage().getString());
                } catch (RuntimeException e) {
                    vector.addProperty("runtime_exception", e.getClass().getName()); vector.addProperty("runtime_message", e.getMessage());
                }
                vector.addProperty("cursor", reader.getCursor());
                vectors.add(vector);
            }
        }
        write(out, "parser-vectors.json", vectors);
        var inspector = new ClientboundCommandsPacket.NodeInspector<CommandSourceStack>() {
            public Identifier suggestionId(ArgumentCommandNode<CommandSourceStack,?> node) {
                return node.getCustomSuggestions() == null ? null : SuggestionProviders.getName(node.getCustomSuggestions());
            }
            public boolean isExecutable(CommandNode<CommandSourceStack> node) { return node.getCommand() != null; }
            public boolean isRestricted(CommandNode<CommandSourceStack> node) {
                return !node.canUse(Commands.createCompilationContext(PermissionSet.NO_PERMISSIONS));
            }
        };
        for (int level : new int[]{0,4}) {
            var buf = new FriendlyByteBuf(Unpooled.buffer());
            ClientboundCommandsPacket.STREAM_CODEC.encode(buf, new ClientboundCommandsPacket(filter(dispatcher.getRoot(), level), inspector));
            var bytes = new byte[buf.readableBytes()]; buf.readBytes(bytes); buf.release();
            Files.write(out.resolve("tree-level-" + level + ".bin"), bytes);
        }
        var rules = new JsonArray();
        for (var rule : BuiltInRegistries.GAME_RULE) {
            var entry = new JsonObject();
            entry.addProperty("name", rule.getIdentifier().toString());
            entry.addProperty("category", rule.category().id().toString());
            entry.add("default", JSON.toJsonTree(rule.defaultValue()));
            entry.addProperty("enabled", rule.isEnabled(FeatureFlags.DEFAULT_FLAGS));
            if (rule.argument() instanceof IntegerArgumentType integer) {
                entry.addProperty("min", integer.getMinimum()); entry.addProperty("max", integer.getMaximum());
            }
            rules.add(entry);
        }
        write(out, "rules.json", rules);
        write(out, "level-unions.json", List.of(
            ((LevelBasedPermissionSet)LevelBasedPermissionSet.OWNER.union(LevelBasedPermissionSet.ALL)).level().id(),
            ((LevelBasedPermissionSet)LevelBasedPermissionSet.ALL.union(LevelBasedPermissionSet.OWNER)).level().id()));
    }
}
